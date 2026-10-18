use crate::{Repo, Result};
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Entry {
    pub path: String,
    /// 重命名/复制的来源路径
    pub old_path: Option<String>,
    /// 暂存区相对 HEAD 的状态字母（A M D R C T），无变化为 None
    pub staged: Option<char>,
    /// 工作区相对暂存区的状态字母，未跟踪文件为 '?'
    pub unstaged: Option<char>,
    pub conflicted: bool,
}

// 差异预览和暂存都以文件为单位，不能让 Git 把未跟踪目录折叠成文件条目。
pub fn status(repo: &Repo) -> Result<Vec<Entry>> {
    Ok(parse(&String::from_utf8_lossy(&repo.git(&["status", "--porcelain=v2", "--untracked-files=all", "-z"])?)))
}

/// porcelain v2 的记录以 NUL 分隔；重命名记录后面多跟一个来源路径字段。
pub(crate) fn parse(out: &str) -> Vec<Entry> {
    let state = |c: Option<char>| c.filter(|&c| c != '.');
    let mut fields = out.split('\0');
    let mut entries = Vec::new();
    while let Some(rec) = fields.next() {
        let (kind, rest) = rec.split_once(' ').unwrap_or((rec, ""));
        // 路径前的定宽字段数：普通 7 个，重命名多一个相似度，冲突 9 个
        let skip = match kind {
            "1" => 7,
            "2" => 8,
            "u" => 9,
            "?" => 0,
            _ => continue,
        };
        let path = rest.splitn(skip + 1, ' ').last().unwrap_or_default().to_owned();
        let mut xy = rest.chars();
        entries.push(match kind {
            "?" => Entry { path, old_path: None, staged: None, unstaged: Some('?'), conflicted: false },
            "u" => Entry { path, old_path: None, staged: None, unstaged: Some('U'), conflicted: true },
            _ => Entry {
                path,
                old_path: (kind == "2").then(|| fields.next().unwrap_or_default().to_owned()),
                staged: state(xy.next()),
                unstaged: state(xy.next()),
                conflicted: false,
            },
        });
    }
    entries
}

/// 路径走 stdin 传给 git，不受命令行长度限制。
fn with_paths(repo: &Repo, args: &[&str], paths: &[String]) -> Result<()> {
    let args = [args, &["--pathspec-from-file=-", "--pathspec-file-nul"]].concat();
    repo.git_in(&args, paths.join("\0").as_bytes()).map(drop)
}

pub fn stage(repo: &Repo, paths: &[String]) -> Result<()> {
    with_paths(repo, &["add", "-A"], paths)
}

pub fn unstage(repo: &Repo, paths: &[String]) -> Result<()> {
    with_paths(repo, &["reset", "-q"], paths)
}

pub fn commit(repo: &Repo, message: &str, amend: bool) -> Result<()> {
    let mut args = vec!["commit", "-q", "-F", "-"];
    if amend {
        args.push("--amend");
    }
    repo.git_in(&args, message.as_bytes()).map(drop)
}

/// 上一次提交的完整信息，amend 时预填用。
pub fn last_message(repo: &Repo) -> Result<String> {
    Ok(crate::decode(&repo.git(&["log", "-1", "--format=%B"])?).trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2() {
        let out = [
            "1 .M N... 100644 100644 100644 aaa bbb 带 空格.txt",
            "1 A. N... 000000 100644 100644 aaa bbb new.txt",
            "2 R. N... 100644 100644 100644 aaa bbb R100 新名.txt",
            "旧名.txt",
            "u UU N... 100644 100644 100644 100644 a b c conflict.txt",
            "? untracked dir/",
            "",
        ]
        .join("\0");
        let e = parse(&out);
        let brief: Vec<_> = e.iter().map(|e| (e.path.as_str(), e.staged, e.unstaged)).collect();
        assert_eq!(
            brief,
            [
                ("带 空格.txt", None, Some('M')),
                ("new.txt", Some('A'), None),
                ("新名.txt", Some('R'), None),
                ("conflict.txt", None, Some('U')),
                ("untracked dir/", None, Some('?')),
            ]
        );
        assert_eq!(e[2].old_path.as_deref(), Some("旧名.txt"));
        assert!(e[3].conflicted);
    }
}
