//! 工作树总览：几个仓库的工作树按仓库分组列出，和清理已合并的工作树。

use crate::ops::{self, Log, Op};
use crate::refs::{self, Worktree};
use crate::{err, git_at, Repo, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Serialize, Clone)]
pub struct Group {
    /// 主工作树的路径
    pub path: String,
    /// 第一个是主工作树
    pub worktrees: Vec<Worktree>,
}

/// 按传入顺序读出各仓库的工作树；同一个仓库的几个工作树（共用一个 Git 目录）只读一次。
/// 打不开或读失败的路径跳过（文件夹已删除、不再是仓库）。各仓库并行读。
pub fn groups(paths: &[String]) -> Vec<Group> {
    let mut seen = HashSet::new();
    let repos: Vec<Repo> = paths
        .iter()
        .filter_map(|p| Repo::open(Path::new(p)).ok())
        .filter(|r| seen.insert(std::fs::canonicalize(r.git_dirs().1).ok()))
        .collect();
    std::thread::scope(|s| {
        let jobs: Vec<_> = repos.iter().map(|r| s.spawn(move || refs::worktrees(r, true))).collect();
        jobs.into_iter()
            .filter_map(|j| j.join().ok()?.ok())
            .filter_map(|worktrees| Some(Group { path: worktrees.first()?.path.clone(), worktrees }))
            .collect()
    })
}

/// 清理一个已合并的工作树：删除它的文件夹（不加 --force，有改动就失败）和本地分支，远程分支不动。
/// 列出之后又有了新提交就不删（`PR_WORKTREE_CHANGED`），免得删掉没推送的分支。
/// git 删不掉文件夹里的目录联接（如借主检出 node_modules 的），工作树注销后文件夹会留下，再用 remove_dir_all 删：
/// 它遇到联接只删联接本身，不进入指向的目录。
pub fn clean(main: &str, w: &Worktree) -> Result<Vec<Log>> {
    let head = git_at(Path::new(&w.path), &["rev-parse", "HEAD"], &[])?;
    if String::from_utf8_lossy(&head).trim() != w.head {
        return Err("PR_WORKTREE_CHANGED".into());
    }
    let repo = Repo::open(Path::new(main))?;
    let removed = ops::run(&repo, Op::WorktreeRemove { path: w.path.clone(), force: false })?;
    if !removed.ok {
        return Ok(vec![removed]);
    }
    let mut logs = vec![removed];
    if Path::new(&w.path).exists() {
        if let Err(e) = std::fs::remove_dir_all(&w.path) {
            logs.push(Log { command: format!("remove {}", w.path), output: err(e), ok: false });
        }
    }
    // 已确认合入主线，但不一定合进本地分支，-d 会拒绝
    if let Some(name) = w.branch.as_deref().and_then(|b| b.strip_prefix("refs/heads/")) {
        logs.push(ops::run(&repo, Op::DeleteBranch { name: name.to_owned(), force: true })?);
    }
    Ok(logs)
}
