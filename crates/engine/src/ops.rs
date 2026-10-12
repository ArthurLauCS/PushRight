use crate::{Repo, Result};
use serde::{Deserialize, Serialize};

/// 一次写操作实际执行的命令和它的输出，界面的命令日志原样展示。
#[derive(Serialize, Debug)]
pub struct Log {
    pub command: String,
    pub output: String,
    pub ok: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
    /// 像硬重置一样移动分支，但会丢失未提交改动时拒绝执行；撤销操作用它
    Keep,
}

/// 进行到一半、可以继续或中止的操作
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum InProgress {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// 检出分支、标签或提交
    Checkout { target: String },
    /// 检出远程分支：建立同名本地分支并跟踪它
    Track { remote_branch: String },
    CreateBranch { name: String, start: String, checkout: bool },
    DeleteBranch { name: String, force: bool },
    RenameBranch { old: String, new: String },
    Merge { target: String },
    Rebase { onto: String },
    CherryPick { id: String },
    Revert { id: String },
    SetRevertDisabled { disabled: bool },
    Reset { target: String, mode: ResetMode },
    /// `paths` 非空时只贮藏这些文件
    StashPush { message: String, include_untracked: bool, #[serde(default)] paths: Vec<String> },
    /// 丢弃这些路径的改动（空列表表示全部）。改动进贮藏列表而不是直接删除，后悔了可以取回
    Discard { paths: Vec<String> },
    StashApply { name: String, pop: bool },
    StashDrop { name: String },
    /// 在 `path` 新建工作树：检出已有分支，或（给了 `new_branch`）从 `start` 新建分支
    WorktreeAdd { path: String, start: String, new_branch: Option<String> },
    WorktreeRemove { path: String, force: bool },
    Fetch { #[serde(default)] background: bool },
    /// `autostash`：先收起未提交的改动，拉取完再放回
    Pull { rebase: bool, #[serde(default)] autostash: bool },
    /// 把本地分支快进到它的上游；分叉时拒绝，不合并也不变基
    FastForward { branch: String },
    /// `tags`：同时推送所有本地标签
    Push { remote: String, branch: String, remote_branch: String, force: bool, set_upstream: bool, #[serde(default)] tags: bool },
    CreateTag { name: String, target: String, message: String },
    DeleteTag { name: String },
    RemoteAdd { name: String, url: String },
    RemoteUrl { name: String, url: String, push: bool },
    RemoteRemove { name: String },
    RemoteRename { old: String, new: String },
    Continue { what: InProgress },
    Abort { what: InProgress },
}

/// 名字来自界面输入，以 `-` 开头会被 git 当成选项
pub(crate) fn safe(s: &str) -> Result<&str> {
    if s.is_empty() || s.starts_with('-') {
        return Err(format!("PR_INVALID_NAME: {s:?}"));
    }
    Ok(s)
}

pub fn revert_disabled(repo: &Repo) -> Result<bool> {
    let out = repo.git(&["config", "--local", "--type=bool", "--default=false", "--get", "pushright.disableRevert"])?;
    Ok(out == b"true\n")
}

/// 当前分支用 `merge --ff-only`（工作区文件跟着更新）；其他分支用本地 fetch 只挪引用。
/// 两条命令在分叉时都会拒绝，后者在分支被别的工作树检出时也拒绝。
fn fast_forward(repo: &Repo, branch: &str) -> Result<Log> {
    let name = format!("refs/heads/{}", safe(branch)?);
    let upstream = String::from_utf8_lossy(&repo.git(&["for-each-ref", "--format=%(upstream)", &name])?).trim().to_owned();
    if upstream.is_empty() {
        return Err("PR_NO_UPSTREAM".into());
    }
    let current = repo.gix().head_name().map_err(crate::err)?.is_some_and(|head| head.as_bstr() == name.as_str());
    let args = if current {
        vec!["merge".to_owned(), "--ff-only".into(), upstream]
    } else {
        // 不写 FETCH_HEAD：那是用户上一次真正 fetch / pull 的记录。fetch.prune 也不能作用到本地分支上
        vec!["fetch".to_owned(), "--no-write-fetch-head".into(), "--no-prune".into(), ".".into(), format!("{upstream}:{name}")]
    };
    Ok(repo.git_log(&args))
}

pub fn run(repo: &Repo, op: Op) -> Result<Log> {
    // 每次执行都重读，不能依赖窗口中可能过期的设置；中止仍可用于退出冲突。
    if matches!(op, Op::Revert { .. } | Op::Continue { what: InProgress::Revert }) && revert_disabled(repo)? {
        return Err("PR_REVERT_DISABLED".into());
    }
    let mut args: Vec<&str> = Vec::new();
    let refspec;
    match &op {
        Op::Checkout { target } => args.extend(["checkout", safe(target)?]),
        Op::Track { remote_branch } => args.extend(["checkout", "--track", safe(remote_branch)?]),
        Op::CreateBranch { name, start, checkout } => {
            args.extend(if *checkout { &["checkout", "-b"][..] } else { &["branch"] });
            args.extend([safe(name)?, safe(start)?]);
        }
        Op::DeleteBranch { name, force } => args.extend(["branch", if *force { "-D" } else { "-d" }, safe(name)?]),
        Op::RenameBranch { old, new } => args.extend(["branch", "-m", safe(old)?, safe(new)?]),
        Op::Merge { target } => args.extend(["merge", "--no-edit", safe(target)?]),
        Op::Rebase { onto } => args.extend(["rebase", safe(onto)?]),
        Op::CherryPick { id } => args.extend(["cherry-pick", safe(id)?]),
        Op::Revert { id } => args.extend(["revert", "--no-edit", safe(id)?]),
        Op::SetRevertDisabled { disabled } => args.extend(["config", "--local", "--type=bool", "--replace-all", "pushright.disableRevert", if *disabled { "true" } else { "false" }]),
        Op::Reset { target, mode } => {
            let mode = match mode {
                ResetMode::Soft => "--soft",
                ResetMode::Mixed => "--mixed",
                ResetMode::Hard => "--hard",
                ResetMode::Keep => "--keep",
            };
            args.extend(["reset", mode, safe(target)?]);
        }
        Op::StashPush { message, include_untracked, paths } => {
            args.extend(["stash", "push"]);
            if *include_untracked {
                args.push("--include-untracked");
            }
            if !message.is_empty() {
                args.extend(["-m", message]);
            }
            if !paths.is_empty() {
                args.push("--");
                args.extend(paths.iter().map(String::as_str));
            }
        }
        Op::Discard { paths } => {
            args.extend(["stash", "push", "--include-untracked", "-m", "PushRight: discarded changes", "--"]);
            args.extend(paths.iter().map(String::as_str));
        }
        Op::StashApply { name, pop } => args.extend(["stash", if *pop { "pop" } else { "apply" }, safe(name)?]),
        Op::StashDrop { name } => args.extend(["stash", "drop", safe(name)?]),
        Op::WorktreeAdd { path, start, new_branch } => {
            args.extend(["worktree", "add"]);
            if let Some(b) = new_branch {
                args.extend(["-b", safe(b)?]);
            }
            args.extend([safe(path)?, safe(start)?]);
        }
        Op::WorktreeRemove { path, force } => {
            args.extend(["worktree", "remove"]);
            if *force {
                args.push("--force");
            }
            args.push(safe(path)?);
        }
        // 多个远程并行获取
        Op::Fetch { background } => {
            if *background { args.extend(["-c", "credential.interactive=never"]); }
            args.extend(["fetch", "--all", "--prune", "--jobs=8"]);
            // 后台获取不能覆盖用户手动 fetch/pull 留下的 FETCH_HEAD。
            if *background { args.push("--no-write-fetch-head"); }
        }
        Op::Pull { rebase, autostash } => {
            // A pull must not rewrite other local branches; uncommitted work is only stashed when asked.
            args.extend(["-c", "rebase.updateRefs=false", "pull", if *rebase { "--rebase" } else { "--no-rebase" }, if *autostash { "--autostash" } else { "--no-autostash" }, "--no-edit"]);
        }
        Op::FastForward { branch } => return fast_forward(repo, branch),
        Op::Push { remote, branch, remote_branch, force, set_upstream, tags } => {
            args.push("push");
            if *tags {
                args.push("--tags");
            }
            if *force {
                // 后台 fetch 更新远程引用后，也不能覆盖尚未在本地整合过的提交。
                args.extend(["--force-with-lease", "--force-if-includes"]);
            }
            if *set_upstream {
                args.push("--set-upstream");
            }
            refspec = format!("refs/heads/{}:refs/heads/{}", safe(branch)?, safe(remote_branch)?);
            args.extend([safe(remote)?, &refspec]);
        }
        Op::CreateTag { name, target, message } => {
            args.push("tag");
            if !message.is_empty() {
                args.extend(["-a", "-m", message]);
            }
            args.extend([safe(name)?, safe(target)?]);
        }
        Op::DeleteTag { name } => args.extend(["tag", "-d", safe(name)?]),
        Op::RemoteAdd { name, url } => args.extend(["remote", "add", safe(name)?, safe(url)?]),
        Op::RemoteUrl { name, url, push } => {
            args.extend(["remote", "set-url"]);
            if *push { args.push("--push"); }
            args.extend([safe(name)?, safe(url)?]);
        }
        Op::RemoteRemove { name } => args.extend(["remote", "remove", safe(name)?]),
        Op::RemoteRename { old, new } => args.extend(["remote", "rename", safe(old)?, safe(new)?]),
        Op::Continue { what: InProgress::Rebase } => return crate::rebase::continue_rebase(repo),
        Op::Continue { what } | Op::Abort { what } => {
            let cmd = match what {
                InProgress::Merge => "merge",
                InProgress::Rebase => "rebase",
                InProgress::CherryPick => "cherry-pick",
                InProgress::Revert => "revert",
            };
            // 继续时沿用已有的提交信息，不弹编辑器
            let flag = if matches!(op, Op::Abort { .. }) { "--abort" } else { "--continue" };
            args.extend(["-c", "core.editor=true", cmd, flag]);
        }
    }
    if matches!(op, Op::Reset { mode: ResetMode::Keep, .. }) {
        // 文件被改过又改回原样时，索引里的时间戳是旧的，reset --keep 会误判为有改动而拒绝
        let _ = repo.git(&["update-index", "-q", "--refresh"]);
    }
    let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
    Ok(repo.git_log(&args))
}
