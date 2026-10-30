use crate::{decode, err, Repo, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Default, Clone)]
pub struct Conflict {
    /// 二进制文件没法逐块选，只能整个文件取一边
    pub binary: bool,
    /// 子模块冲突：唯一的冲突块是两边各指向的提交，只能整个取一边
    pub submodule: bool,
    /// 这一边没有这个文件（删除或改名了），Git 不写冲突标记：唯一的冲突块是两边的全文，只能整个文件取一边
    pub ours_deleted: bool,
    pub theirs_deleted: bool,
    pub blocks: Vec<Block>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    /// 两边一致的部分；长的只保留头尾几行供定位
    Text { text: String },
    Conflict { ours: String, theirs: String, ours_label: String, theirs_label: String },
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Ours,
    Theirs,
    /// 先我方后对方，两段都保留
    Both,
    /// 用户手动改出来的内容，整段替换这个冲突块
    Custom(String),
}

/// 手动编辑的内容来自界面（UTF-8、换行是 \n），写回时跟文件原来的编码和换行保持一致。
fn encode_like(text: &str, file: &[u8]) -> Vec<u8> {
    let mut text = text.replace("\r\n", "\n");
    // 冲突块后面的内容要另起一行
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if file.windows(2).any(|w| w == b"\r\n") {
        text = text.replace('\n', "\r\n");
    }
    match std::str::from_utf8(file) {
        Ok(_) => text.into_bytes(),
        Err(_) => encoding_rs::GB18030.encode(&text).0.into_owned(),
    }
}

/// 按原始字节切开：写回文件时原样拼接，保持文件原来的编码和换行。
enum Raw<'a> {
    Text(Vec<&'a [u8]>),
    Conflict { ours: Vec<&'a [u8]>, theirs: Vec<&'a [u8]>, ours_label: &'a [u8], theirs_label: &'a [u8] },
}

fn split<'a>(bytes: &'a [u8]) -> Result<Vec<Raw<'a>>> {
    #[derive(PartialEq)]
    enum In {
        Text,
        Ours,
        Base,
        Theirs,
    }
    let mut state = In::Text;
    let mut blocks = vec![Raw::Text(Vec::new())];
    let (mut open, mut ours, mut theirs, mut ours_label) = (&b""[..], Vec::new(), Vec::new(), &b""[..]);
    let text = |blocks: &mut Vec<Raw<'a>>, line: &'a [u8]| match blocks.last_mut() {
        Some(Raw::Text(lines)) => lines.push(line),
        _ => blocks.push(Raw::Text(vec![line])),
    };
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        let body = body.strip_suffix(b"\r").unwrap_or(body);
        let label = |marker: &[u8]| body.strip_prefix(marker).map(|l| l.strip_prefix(b" ").unwrap_or(l));
        match state {
            In::Text => match label(b"<<<<<<<") {
                Some(l) => (state, open, ours_label) = (In::Ours, line, l),
                None => text(&mut blocks, line),
            },
            // diff3 风格多一段共同祖先，界面不展示
            In::Ours if label(b"|||||||").is_some() => state = In::Base,
            In::Ours | In::Base if body == b"=======" => state = In::Theirs,
            // 我方还没结束又遇到开头：前一个开头是正文里本来就有的一行，连同收进来的内容还回正文
            In::Ours if body.starts_with(b"<<<<<<<") => {
                for l in std::iter::once(open).chain(ours.drain(..)) {
                    text(&mut blocks, l);
                }
                (open, ours_label) = (line, label(b"<<<<<<<").unwrap_or_default());
            }
            In::Ours => ours.push(line),
            In::Base => {}
            In::Theirs => match label(b">>>>>>>") {
                Some(theirs_label) => {
                    blocks.push(Raw::Conflict { ours: std::mem::take(&mut ours), theirs: std::mem::take(&mut theirs), ours_label, theirs_label });
                    state = In::Text;
                }
                None => theirs.push(line),
            },
        }
    }
    match state {
        In::Text => {}
        // 到文件末尾都没有分隔线：那个开头也是正文
        In::Ours => std::iter::once(open).chain(ours).for_each(|l| text(&mut blocks, l)),
        _ => return Err("PR_CONFLICT_MARKERS".into()),
    }
    Ok(blocks)
}

/// 冲突文件在索引里的各版本，每个是「模式 对象 阶段」：阶段 2 是我方、3 是对方，缺哪个就是那边删除或改名了。
fn stages(repo: &Repo, path: &str) -> Result<Vec<String>> {
    let out = repo.git(&["--literal-pathspecs", "ls-files", "-u", "-z", "--", path])?;
    Ok(out.split(|&b| b == 0).filter_map(|r| r.split(|&b| b == b'\t').next()).filter(|h| !h.is_empty()).map(decode).collect())
}

pub fn read(repo: &Repo, path: &str) -> Result<Conflict> {
    let stages = stages(repo, path)?;
    let side = |n: &str| stages.iter().find(|s| s.ends_with(n));
    let (ours, theirs) = (side(" 2"), side(" 3"));
    // 索引里已经没有冲突（刚被别处解决）时按普通冲突读
    let (ours_deleted, theirs_deleted) = (!stages.is_empty() && ours.is_none(), !stages.is_empty() && theirs.is_none());
    let whole = |ours: String, theirs: String| vec![Block::Conflict { ours, theirs, ours_label: String::new(), theirs_label: String::new() }];
    if stages.iter().any(|s| s.starts_with("160000")) {
        let id = |s: Option<&String>| s.and_then(|s| s.split(' ').nth(1)).unwrap_or_default().to_owned();
        return Ok(Conflict { submodule: true, ours_deleted, theirs_deleted, blocks: whole(id(ours), id(theirs)), ..Default::default() });
    }
    // 两边都删了（或留下的那一版被手动删了）时工作区里没有这个文件
    let file = repo.path.join(path);
    let bytes = if (ours_deleted || theirs_deleted) && !file.exists() { Vec::new() } else { std::fs::read(file).map_err(err)? };
    // 跟 Git 一样只看前 8000 字节有没有 NUL
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return Ok(Conflict { binary: true, ours_deleted, theirs_deleted, ..Default::default() });
    }
    // 工作区里是留下的那一边
    if ours_deleted || theirs_deleted {
        let kept = decode(&bytes);
        let blocks = if ours_deleted { whole(String::new(), kept) } else { whole(kept, String::new()) };
        return Ok(Conflict { ours_deleted, theirs_deleted, blocks, ..Default::default() });
    }
    let text = |lines: &[&[u8]]| decode(&lines.concat());
    let blocks = split(&bytes)?
        .iter()
        .filter_map(|b| match b {
            Raw::Text(lines) if lines.is_empty() => None,
            Raw::Text(lines) if lines.len() > 8 => {
                Some(Block::Text { text: format!("{}⋯\n{}", text(&lines[..3]), text(&lines[lines.len() - 3..])) })
            }
            Raw::Text(lines) => Some(Block::Text { text: text(lines) }),
            Raw::Conflict { ours, theirs, ours_label, theirs_label } => Some(Block::Conflict {
                ours: text(ours),
                theirs: text(theirs),
                ours_label: decode(ours_label),
                theirs_label: decode(theirs_label),
            }),
        })
        .collect();
    Ok(Conflict { blocks, ..Default::default() })
}

/// 按 `choices`（每个冲突块一个）写回文件并暂存，即标记为已解决。
pub fn resolve(repo: &Repo, path: &str, choices: &[Side]) -> Result<()> {
    let file = repo.path.join(path);
    let bytes = std::fs::read(&file).map_err(err)?;
    let blocks = split(&bytes)?;
    if blocks.iter().filter(|b| matches!(b, Raw::Conflict { .. })).count() != choices.len() {
        return Err("PR_FILE_CHANGED".into());
    }
    let mut choices = choices.iter();
    let mut out = Vec::with_capacity(bytes.len());
    for b in &blocks {
        match b {
            Raw::Text(lines) => out.extend(lines.concat()),
            Raw::Conflict { ours, theirs, .. } => match choices.next().unwrap() {
                Side::Ours => out.extend(ours.concat()),
                Side::Theirs => out.extend(theirs.concat()),
                Side::Both => out.extend([ours.concat(), theirs.concat()].concat()),
                Side::Custom(text) => out.extend(encode_like(text, &bytes)),
            },
        }
    }
    std::fs::write(&file, out).map_err(err)?;
    repo.git(&["add", "--", path]).map(drop)
}

/// 整个文件取一边并暂存。二进制文件（.rbxl、贴图）、子模块、一边删除了的文件只能这样解决。
pub fn take(repo: &Repo, path: &str, theirs: bool) -> Result<()> {
    match stages(repo, path)?.iter().find(|s| s.ends_with(if theirs { " 3" } else { " 2" })) {
        // 选的那边没有这个文件：解决结果就是删除
        None => repo.git(&["--literal-pathspecs", "rm", "-q", "--", path]).map(drop),
        // 子模块只记下选中的提交，它的工作目录不动
        Some(s) if s.starts_with("160000") => {
            let id = s.split(' ').nth(1).unwrap_or_default();
            repo.git(&["update-index", "--cacheinfo", &format!("160000,{id},{path}")]).map(drop)
        }
        Some(_) => {
            repo.git(&["checkout", if theirs { "--theirs" } else { "--ours" }, "--", path])?;
            repo.git(&["add", "--", path]).map(drop)
        }
    }
}

/// 进行中的合并 / 变基 / 摘取 / 撤销：Git 自己记下的提交信息、正在处理的提交和冲突清单。
#[derive(Serialize, Default, Debug, PartialEq, Clone)]
pub struct Operation {
    /// Git 准备好的提交信息原文，`# Conflicts:` 清单也在里面；界面预填到提交框
    pub message: String,
    /// 正在并入或重放的提交
    pub id: Option<String>,
    /// 操作停下来时冲突的全部文件，已经解决的也在内
    pub conflicts: Vec<String>,
    /// 变基进行到第几步、共几步
    pub step: Option<(u32, u32)>,
}

/// Git 给含特殊字符或非 ASCII 的路径加的 C 风格引号。
// ponytail: 只还原八进制、\n、\t 和原样转义（\" \\）；路径里有响铃之类的控制字符时显示不准
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else { return s.to_owned() };
    let mut out = Vec::new();
    let mut bytes = inner.bytes().peekable();
    while let Some(b) = bytes.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match bytes.next() {
            Some(d @ b'0'..=b'7') => {
                let mut n = (d - b'0') as u32;
                for _ in 0..2 {
                    if let Some(d) = bytes.next_if(|d| (b'0'..=b'7').contains(d)) {
                        n = n * 8 + (d - b'0') as u32;
                    }
                }
                out.push(n as u8);
            }
            Some(b'n') => out.push(b'\n'),
            Some(b't') => out.push(b'\t'),
            Some(c) => out.push(c),
            None => {}
        }
    }
    decode(&out)
}

/// 没有进行中的操作时返回 None。只读 Git 目录里的状态文件，不启动 git 进程。
// ponytail: 冲突清单按默认的注释符 `#` 识别；改过 core.commentChar 的仓库里 `conflicts` 为空，进度只按还没解决的文件算
pub fn operation(repo: &Repo) -> Result<Option<Operation>> {
    let dir = repo.gix().git_dir().to_owned();
    let read = |name: &str| std::fs::read(dir.join(name)).ok().map(|b| decode(&b));
    let Some(id) = ["MERGE_HEAD", "REBASE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"].into_iter().find_map(read) else {
        let rebasing = dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists();
        return Ok(rebasing.then(Operation::default));
    };
    let text = read("MERGE_MSG").unwrap_or_default();
    let (mut conflicts, mut listing) = (Vec::new(), false);
    for line in text.lines() {
        match line.strip_prefix('#') {
            None => listing = false,
            Some(rest) if rest.trim() == "Conflicts:" => listing = true,
            Some(rest) if listing && rest.starts_with('\t') => conflicts.push(unquote(rest.trim())),
            Some(_) => listing = false,
        }
    }
    let number = |name: &str| read(name)?.trim().parse::<u32>().ok();
    let step = number("rebase-merge/msgnum").zip(number("rebase-merge/end")).or_else(|| number("rebase-apply/next").zip(number("rebase-apply/last")));
    Ok(Some(Operation { message: text.trim().to_owned(), id: id.lines().next().map(str::to_owned), conflicts, step }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_marker_lines_stay_text() {
        let blocks = split(b"<<<<<<< doc\n<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n<<<<<<< tail\n").unwrap();
        assert_eq!(blocks.iter().filter(|b| matches!(b, Raw::Conflict { .. })).count(), 1);
        // 选我方写回：正文里本来就有的两行原样保留
        let out: Vec<u8> = blocks.iter().flat_map(|b| match b {
            Raw::Text(lines) => lines.concat(),
            Raw::Conflict { ours, .. } => ours.concat(),
        }).collect();
        assert_eq!(out, b"<<<<<<< doc\na\n<<<<<<< tail\n");
    }
}
