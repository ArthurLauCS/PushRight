use crate::ops::safe;
use crate::{decode, Repo, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug, Clone)]
pub struct Commit {
    pub id: String,
    pub author: String,
    pub author_email: String,
    pub time: i64,
    pub subject: String,
    /// 文件在这个提交里的路径（重命名之前是旧名字）；搜索结果里为空
    pub path: String,
}

/// 每条记录以 NUL 开头，字段以 0x1f 分隔；`--name-only` 的路径跟在记录后面的行里
const FORMAT: &str = "--format=%x00%H%x1f%an%x1f%ae%x1f%at%x1f%s";

fn parse(out: &[u8], path: &str, patches: bool) -> Vec<Commit> {
    decode(out)
        .split('\0')
        .skip(1)
        .filter_map(|rec| {
            let (head, rest) = rec.split_once('\n').unwrap_or((rec, ""));
            let mut f = head.split('\x1f');
            Some(Commit {
                id: f.next()?.to_owned(),
                author: f.next()?.to_owned(),
                author_email: f.next()?.to_owned(),
                time: f.next()?.parse().ok()?,
                subject: f.next()?.to_owned(),
                path: if patches { rest.lines().find_map(|l| l.strip_prefix("+++ ")) } else { rest.lines().find(|l| !l.is_empty()) }.unwrap_or(path).to_owned(),
            })
        })
        .collect()
}

/// 改过 `path` 的提交，新的在前，跟踪重命名。
pub fn file(repo: &Repo, path: &str, limit: usize) -> Result<Vec<Commit>> {
    file_page(repo, path, limit, 0)
}

pub fn file_page(repo: &Repo, path: &str, limit: usize, skip: usize) -> Result<Vec<Commit>> {
    // Git --skip suppresses rename processing with --follow. Walk the prefix before slicing.
    // ponytail: later pages revisit metadata; use a commit/path cursor if very long file histories warrant it.
    let n = limit.saturating_add(skip).to_string();
    let args = ["-c", "core.quotepath=false", "log", "--follow", "-n", &n, FORMAT, "--name-only", "--", path];
    Ok(parse(&repo.git(&args)?, path, false).into_iter().skip(skip).collect())
}

/// 改过 `path` 第 `start`..=`end` 行（从 1 开始）的提交。
pub fn lines(repo: &Repo, path: &str, start: u32, end: u32, limit: usize) -> Result<Vec<Commit>> {
    // -L 的行号属于 HEAD；本地行号变化时不能把另一段代码的历史显示为选区历史。
    repo.git(&["diff", "--quiet", "HEAD", "--", path]).map_err(|_| "PR_LINE_HISTORY_CHANGED")?;
    let n = limit.to_string();
    let range = format!("-L{start},{end}:{path}");
    let out = repo.git_limited(&["-c", "core.quotepath=false", "log", "-n", &n, FORMAT, "--no-prefix", "--no-color", "--no-ext-diff", "--no-textconv", &range], 4 * 1024 * 1024)?;
    if out.len() > 4 * 1024 * 1024 { return Err("PR_DIFF_TOO_LARGE".into()); }
    Ok(parse(&out, path, true))
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Message,
    Author,
    /// 改动内容里增删过这段文字的提交（git log -S）
    Content,
    File,
    Id,
}

/// 在所有分支里搜索提交。
pub fn search(repo: &Repo, kind: SearchKind, query: &str, limit: usize) -> Result<Vec<Commit>> {
    search_page(repo, kind, query, limit, 0)
}

pub fn search_page(repo: &Repo, kind: SearchKind, query: &str, limit: usize, skip: usize) -> Result<Vec<Commit>> {
    let n = limit.to_string();
    let skip = format!("--skip={skip}");
    let pattern;
    let mut args = vec!["log", "-n", &n, &skip, FORMAT];
    match kind {
        SearchKind::Message => {
            pattern = format!("--grep={query}");
            args.extend(["--all", "-i", "--fixed-strings", &pattern]);
        }
        SearchKind::Author => {
            pattern = format!("--author={query}");
            args.extend(["--all", "-i", "--fixed-strings", &pattern]);
        }
        SearchKind::Content => {
            pattern = format!("-S{query}");
            args.extend(["--all", &pattern]);
        }
        SearchKind::File => args.extend(["--all", "--", query]),
        SearchKind::Id => args.extend(["--no-walk", safe(query)?]),
    }
    Ok(parse(&repo.git(&args)?, "", false))
}

/// 文件在某个版本的内容；`rev` 为空表示暂存区。
pub fn show(repo: &Repo, rev: &str, path: &str) -> Result<Option<String>> {
    show_limit(repo, rev, path, 4 << 20)
}

pub fn show_limit(repo: &Repo, rev: &str, path: &str, limit: usize) -> Result<Option<String>> {
    let Some(bytes) = blob(repo, rev, path, limit)? else { return Ok(None) };
    // 与 Git 判断二进制的规则一致：只看前 8000 字节里有没有 NUL，后面夹着一个 NUL 的源码仍按文本打开
    if bytes[..bytes.len().min(8000)].contains(&0) { return Err("PR_NOT_TEXT_FILE".into()); }
    Ok(Some(decode(&bytes)))
}

pub fn blob(repo: &Repo, rev: &str, path: &str, limit: usize) -> Result<Option<Vec<u8>>> {
    if rev.starts_with('-') {
        return Err(format!("PR_INVALID_NAME: {rev:?}"));
    }
    let spec = format!("{rev}:{path}");
    // 先查类型和大小，不把大对象读进进程。不存在的版本用于显示新增/删除的空白一侧。
    let info = repo.git_in(&["cat-file", "--batch-check=%(objecttype) %(objectsize)", "-z"], format!("{spec}\0").as_bytes())?;
    let info = String::from_utf8_lossy(&info);
    if info.ends_with(" missing\n") { return Ok(None); }
    let size = info.strip_prefix("blob ").and_then(|s| s.trim().parse::<usize>().ok()).ok_or("PR_NOT_TEXT_FILE")?;
    let limit = limit.min(64 << 20);
    if size > limit { return Err("PR_FILE_TOO_LARGE".into()); }
    let bytes = repo.git_limited(&["show", &spec], limit)?;
    if bytes.len() > limit { return Err("PR_FILE_TOO_LARGE".into()); }
    Ok(Some(bytes))
}

/// Map unchanged lines in the editor buffer back to HEAD, without touching the index or worktree.
pub fn lines_page(repo: &Repo, path: &str, start: u32, end: u32, limit: usize, skip: usize, contents: Option<&str>) -> Result<Vec<Commit>> {
    let (start, end) = if let Some(contents) = contents { map_lines(repo, path, start, end, contents)? } else {
        repo.git(&["diff", "--quiet", "HEAD", "--", path]).map_err(|_| "PR_LINE_HISTORY_CHANGED")?;
        (start, end)
    };
    let range = format!("-L{start},{end}:{path}");
    // Keep only commit metadata and +++ paths; streaming preserves rename paths without retaining patches.
    use std::io::{BufReader, Read};
    let mut child = crate::git_command(&repo.path).args(["-c", "core.quotepath=false", "log", &format!("--max-count={}", limit.min(201)), &format!("--skip={skip}"), FORMAT, "--no-prefix", "--no-color", "--no-ext-diff", "--no-textconv", &range]).stdin(std::process::Stdio::null()).spawn().map_err(crate::err)?;
    let mut stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || { let mut out = Vec::new(); let _ = stderr.by_ref().take(65536).read_to_end(&mut out); let _ = std::io::copy(&mut stderr, &mut std::io::sink()); out });
    let read = (|| -> Result<Vec<u8>> {
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut kept = Vec::new();
        loop {
            let (mut line, clipped) = crate::preview_line(&mut reader)?;
            if line.is_empty() { break; }
            if clipped { line.push(b'\n'); }
            if line.starts_with(b"\0") || line.starts_with(b"+++ ") { kept.extend(line); }
        }
        Ok(kept)
    })();
    if read.is_err() { let _ = child.kill(); }
    let status = child.wait().map_err(crate::err)?;
    let errors = errors.join().unwrap();
    if !status.success() { return Err(decode(&errors)); }
    Ok(parse(&read?, path, true))
}

fn map_lines(repo: &Repo, path: &str, start: u32, end: u32, contents: &str) -> Result<(u32, u32)> {
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};
    if start == 0 || end < start || end as usize > contents.lines().count() || contents.len() > 64 << 20 { return Err("PR_LINE_HISTORY_NEW".into()); }
    let head = show_limit(repo, "HEAD", path, 64 << 20)?.ok_or("PR_LINE_HISTORY_NEW")?;
    if head == contents { return Ok((start, end)); }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(crate::err)?.as_nanos();
    let temp = std::env::temp_dir().join(format!("pushright-lines-{}-{stamp}", std::process::id()));
    fs::create_dir(&temp).map_err(crate::err)?;
    let result = (|| {
        fs::write(temp.join("head"), head).map_err(crate::err)?;
        fs::write(temp.join("buffer"), contents).map_err(crate::err)?;
        let out = crate::git_command(&temp).args(["diff", "--no-index", "--unified=0", "--ignore-cr-at-eol", "--no-color", "--no-ext-diff", "--no-textconv", "--", "head", "buffer"]).output().map_err(crate::err)?;
        if out.status.code().is_none_or(|c| c > 1) { return Err(decode(&out.stderr)); }
        let (mut first, mut last) = (start as i64, end as i64);
        for line in decode(&out.stdout).lines().filter(|l| l.starts_with("@@ ")) {
            let fields: Vec<_> = line.split(' ').collect();
            let range = |s: &str| -> Result<(i64, i64)> {
                let (at, n) = s[1..].split_once(',').unwrap_or((&s[1..], "1"));
                Ok((at.parse().map_err(crate::err)?, n.parse().map_err(crate::err)?))
            };
            let (_, old_count) = range(fields[1])?;
            let (new_at, new_count) = range(fields[2])?;
            if new_count > 0 && start as i64 <= new_at + new_count - 1 && end as i64 >= new_at { return Err("PR_LINE_HISTORY_NEW".into()); }
            let after = if new_count == 0 { new_at + 1 } else { new_at + new_count };
            if start as i64 >= after { first += old_count - new_count; }
            if end as i64 >= after { last += old_count - new_count; }
        }
        Ok((first as u32, last as u32))
    })();
    let _ = fs::remove_dir_all(temp);
    result
}
