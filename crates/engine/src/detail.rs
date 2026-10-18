use crate::{err, Repo, Result};
use gix::ObjectId;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Detail {
    pub id: String,
    pub parents: Vec<String>,
    pub author: String,
    pub author_email: String,
    pub author_time: i64,
    pub committer: String,
    pub committer_time: i64,
    pub message: String,
    pub files: Vec<FileChange>,
}

#[derive(Serialize, Clone)]
pub struct FileChange {
    /// git 的状态字母：A M D R C T
    pub status: char,
    pub path: String,
    /// 重命名/复制的来源路径
    pub old_path: Option<String>,
}

/// 单个提交的完整信息和改动文件列表；合并提交对比第一个父提交。
pub fn detail(repo: &Repo, id: &str) -> Result<Detail> {
    // id 来自界面，先解析成对象 id 再拼进命令行
    let oid = ObjectId::from_hex(id.as_bytes()).map_err(err)?;
    let gix = repo.gix();
    let commit = gix.find_commit(oid).map_err(err)?;
    let c = commit.decode().map_err(err)?;
    let (author, committer) = (c.author().map_err(err)?, c.committer().map_err(err)?);
    // ponytail: 提交信息按 UTF-8 有损解码，未处理 encoding 头（GBK 提交）；M2 做编码探测时一并处理
    let detail = Detail {
        id: oid.to_string(),
        parents: c.parents().map(|p| p.to_string()).collect(),
        author: author.name.to_string(),
        author_email: author.email.to_string(),
        author_time: author.seconds(),
        committer: committer.name.to_string(),
        committer_time: committer.seconds(),
        message: c.message.to_string().trim_end().to_owned(),
        files: Vec::new(),
    };
    let out = repo.git(&["show", "--format=", "--name-status", "-z", "-M", "--diff-merges=first-parent", &oid.to_string()])?;
    Ok(Detail { files: parse_name_status(&String::from_utf8_lossy(&out)), ..detail })
}

/// 贮藏的完整信息。普通改动在贮藏提交里，`-u` 保存的未跟踪文件在第三个父提交里。
pub fn stash(repo: &Repo, name: &str) -> Result<Detail> {
    let id = crate::tools::revision(repo, name)?;
    let mut result = detail(repo, &id)?;
    let gix = repo.gix();
    let commit = gix.find_commit(ObjectId::from_hex(id.as_bytes()).map_err(err)?).map_err(err)?;
    if let Some(untracked) = commit.decode().map_err(err)?.parents().nth(2) {
        for file in detail(repo, &untracked.to_string())?.files {
            if !result.files.iter().any(|existing| existing.path == file.path) { result.files.push(file); }
        }
    }
    Ok(result)
}

/// `--name-status -z` 的格式：状态\0路径\0，重命名/复制是 状态\0旧路径\0新路径\0
pub(crate) fn parse_name_status(out: &str) -> Vec<FileChange> {
    let mut fields = out.split('\0');
    let mut files = Vec::new();
    while let Some(status) = fields.next().and_then(|s| s.chars().next()) {
        let Some(first) = fields.next() else { break };
        let renamed = matches!(status, 'R' | 'C');
        files.push(FileChange {
            status,
            old_path: renamed.then(|| first.to_owned()),
            path: if renamed { fields.next().unwrap_or_default() } else { first }.to_owned(),
        });
    }
    files
}
