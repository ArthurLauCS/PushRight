use crate::{Repo, Result};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Identity {
    /// 用于编辑的有效 user 配置（含全局配置和 include）。
    pub name: String,
    pub email: String,
    /// Git 实际使用的身份，包含 author/committer 配置和环境变量覆盖。
    pub author: Option<String>,
    pub committer: Option<String>,
}

pub fn read(repo: &Repo) -> Result<Identity> {
    let config = |key| {
        repo.git(&["config", "--default", "", "--get", key])
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
    };
    let actual = |key| {
        let out = repo.git(&["var", key]).ok()?;
        let out = String::from_utf8_lossy(&out);
        let end = out.rfind('>')?;
        Some(out[..=end].to_owned())
    };
    // 四个 git 进程并行跑：Windows 上每次启动约 45 毫秒
    std::thread::scope(|s| {
        let name = s.spawn(|| config("user.name"));
        let email = s.spawn(|| config("user.email"));
        let author = s.spawn(|| actual("GIT_AUTHOR_IDENT"));
        let committer = actual("GIT_COMMITTER_IDENT");
        Ok(Identity { name: name.join().unwrap()?, email: email.join().unwrap()?, author: author.join().unwrap(), committer })
    })
}

/// 只写仓库配置；关联的工作树共享它，不改全局配置或已有提交。
pub fn set(repo: &Repo, name: &str, email: &str) -> Result<Identity> {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() || email.is_empty()
        || name.chars().any(|c| c.is_control() || c == '<' || c == '>')
        || email.chars().any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>')
    {
        return Err("PR_INVALID_IDENTITY".into());
    }
    repo.git(&["config", "--local", "--replace-all", "--", "user.name", name])?;
    repo.git(&["config", "--local", "--replace-all", "--", "user.email", email])?;
    read(repo)
}
