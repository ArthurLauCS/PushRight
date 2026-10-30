mod common;

use common::{git, init};
use engine::ops::{self, InProgress, Op, ResetMode};
use engine::{refs, status, Repo};
use std::path::Path;

fn ok(repo: &Repo, op: Op) {
    let log = ops::run(repo, op).unwrap();
    assert!(log.ok, "{}\n{}", log.command, log.output);
}

fn commit_file(dir: &Path, time: u32, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, 0, &["add", "."]);
    git(dir, time, &["commit", "-q", "-m", name]);
}

fn id_of(repo: &Repo, name: &str) -> Option<String> {
    refs::list(repo).unwrap().refs.into_iter().find(|r| r.name == name).map(|r| r.id)
}

#[test]
fn repository_revert_policy_is_shared_persistent_and_checked_at_execution() {
    let dir = init("revert-policy");
    commit_file(&dir, 1, "a.txt", "base\n");
    commit_file(&dir, 2, "a.txt", "second\n");
    let repo = Repo::open(&dir).unwrap();
    let second = refs::list(&repo).unwrap().head_id.unwrap();
    commit_file(&dir, 3, "a.txt", "third\n");
    let head = refs::list(&repo).unwrap().head_id;
    assert!(!refs::list(&repo).unwrap().revert_disabled);
    let linked_path = dir.join("linked");
    git(&dir, 0, &["worktree", "add", "-q", "-b", "linked", linked_path.to_str().unwrap()]);
    let linked = Repo::open(&linked_path).unwrap();
    let other = Repo::open(&init("revert-policy-other")).unwrap();

    ok(&repo, Op::SetRevertDisabled { disabled: true });
    assert!(refs::list(&Repo::open(&dir).unwrap()).unwrap().revert_disabled);
    assert!(refs::list(&linked).unwrap().revert_disabled);
    assert!(!refs::list(&other).unwrap().revert_disabled);
    for r in [&repo, &linked] {
        assert_eq!(ops::run(r, Op::Revert { id: "HEAD".into() }).unwrap_err(), "PR_REVERT_DISABLED");
        assert_eq!(ops::run(r, Op::Continue { what: InProgress::Revert }).unwrap_err(), "PR_REVERT_DISABLED");
    }
    assert_eq!(refs::list(&repo).unwrap().head_id, head);
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "third\n");

    // 在另一工作树恢复设置后，原会话立即生效；中途禁用不能阻止安全中止。
    ok(&linked, Op::SetRevertDisabled { disabled: false });
    assert!(!refs::list(&repo).unwrap().revert_disabled);
    assert!(!ops::run(&repo, Op::Revert { id: second }).unwrap().ok);
    assert_eq!(refs::list(&repo).unwrap().in_progress, Some("revert"));
    ok(&repo, Op::SetRevertDisabled { disabled: true });
    assert_eq!(ops::run(&repo, Op::Continue { what: InProgress::Revert }).unwrap_err(), "PR_REVERT_DISABLED");
    ok(&repo, Op::Abort { what: InProgress::Revert });
    assert_eq!(refs::list(&repo).unwrap().head_id, head);
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "third\n");
    assert!(refs::list(&repo).unwrap().in_progress.is_none());

    git(&dir, 0, &["config", "--local", "pushright.disableRevert", "invalid"]);
    assert!(ops::run(&repo, Op::Revert { id: "HEAD".into() }).is_err());
    ok(&repo, Op::SetRevertDisabled { disabled: false });
    ok(&repo, Op::Revert { id: "HEAD".into() });
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "second\n");
}

#[test]
fn branches_tags_stash_reset() {
    let dir = init("ops-local");
    commit_file(&dir, 1, "a.txt", "a\n");
    let repo = Repo::open(&dir).unwrap();
    let base = id_of(&repo, "refs/heads/main").unwrap();

    ok(&repo, Op::CreateBranch { name: "feat".into(), start: "main".into(), checkout: true });
    assert_eq!(refs::list(&repo).unwrap().head.as_deref(), Some("refs/heads/feat"));
    commit_file(&dir, 2, "b.txt", "b\n");
    let feat = id_of(&repo, "refs/heads/feat").unwrap();

    ok(&repo, Op::Checkout { target: "main".into() });
    ok(&repo, Op::Merge { target: "feat".into() });
    assert_eq!(id_of(&repo, "refs/heads/main"), Some(feat.clone()));

    ok(&repo, Op::RenameBranch { old: "feat".into(), new: "feat2".into() });
    ok(&repo, Op::DeleteBranch { name: "feat2".into(), force: false });
    assert_eq!(id_of(&repo, "refs/heads/feat2"), None);

    ok(&repo, Op::CreateTag { name: "v1".into(), target: base.clone(), message: String::new() });
    ok(&repo, Op::CreateTag { name: "v2".into(), target: "HEAD".into(), message: "带说明".into() });
    assert_eq!(id_of(&repo, "refs/tags/v1"), Some(base.clone()));
    assert_eq!(id_of(&repo, "refs/tags/v2"), Some(feat.clone()));
    ok(&repo, Op::DeleteTag { name: "v1".into() });
    assert_eq!(id_of(&repo, "refs/tags/v1"), None);

    // revert 生成新提交，cherry-pick 到新分支上再把它带回来
    ok(&repo, Op::Revert { id: feat.clone() });
    assert!(!dir.join("b.txt").exists());
    ok(&repo, Op::CherryPick { id: feat.clone() });
    assert!(dir.join("b.txt").exists());

    ok(&repo, Op::Reset { target: base.clone(), mode: ResetMode::Hard });
    assert_eq!(id_of(&repo, "refs/heads/main"), Some(base.clone()));
    assert!(!dir.join("b.txt").exists());

    // 贮藏：含未跟踪文件，弹出后恢复
    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
    std::fs::write(dir.join("new.txt"), "n\n").unwrap();
    ok(&repo, Op::StashPush { message: "半成品".into(), include_untracked: true, paths: vec![] });
    assert!(status::status(&repo).unwrap().is_empty());
    let stashes = refs::stashes(&repo).unwrap();
    assert!(stashes[0].subject.contains("半成品"));
    ok(&repo, Op::StashApply { name: stashes[0].name.clone(), pop: true });
    assert_eq!(status::status(&repo).unwrap().len(), 2);
    assert!(refs::stashes(&repo).unwrap().is_empty());

    // 以 - 开头的名字不会被拼进命令行
    assert!(ops::run(&repo, Op::Checkout { target: "--orphan".into() }).is_err());
    // git 自己拒绝的操作：日志里带失败输出
    let log = ops::run(&repo, Op::DeleteBranch { name: "不存在".into(), force: false }).unwrap();
    assert!(!log.ok && log.command == "git branch -d 不存在" && !log.output.is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn conflict_state_and_abort() {
    let dir = init("ops-conflict");
    commit_file(&dir, 1, "a.txt", "base\n");
    let repo = Repo::open(&dir).unwrap();
    ok(&repo, Op::CreateBranch { name: "other".into(), start: "main".into(), checkout: true });
    commit_file(&dir, 2, "a.txt", "other\n");
    ok(&repo, Op::Checkout { target: "main".into() });
    commit_file(&dir, 3, "a.txt", "main\n");

    let log = ops::run(&repo, Op::Merge { target: "other".into() }).unwrap();
    assert!(!log.ok);
    assert_eq!(refs::list(&repo).unwrap().in_progress, Some("merge"));
    assert!(status::status(&repo).unwrap()[0].conflicted);
    ok(&repo, Op::Abort { what: InProgress::Merge });
    assert_eq!(refs::list(&repo).unwrap().in_progress, None);

    // 解决冲突后继续
    assert!(!ops::run(&repo, Op::Rebase { onto: "other".into() }).unwrap().ok);
    assert_eq!(refs::list(&repo).unwrap().in_progress, Some("rebase"));
    std::fs::write(dir.join("a.txt"), "resolved\n").unwrap();
    status::stage(&repo, &["a.txt".into()]).unwrap();
    ok(&repo, Op::Continue { what: InProgress::Rebase });
    assert_eq!(refs::list(&repo).unwrap().in_progress, None);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn background_fetch_preserves_local_work_and_force_push_rejects_unintegrated_updates() {
    let origin = init("background-origin");
    commit_file(&origin, 1, "a.txt", "base\n");
    git(&origin, 0, &["config", "receive.denyCurrentBranch", "ignore"]);
    let dir = init("background-clone");
    git(&dir, 0, &["remote", "add", "origin", origin.to_str().unwrap()]);
    let repo = Repo::open(&dir).unwrap();
    ok(&repo, Op::Fetch { background: false });
    git(&dir, 0, &["checkout", "-q", "-B", "main", "origin/main"]);
    let head = refs::list(&repo).unwrap().head_id;
    std::fs::write(dir.join("a.txt"), "staged\n").unwrap();
    status::stage(&repo, &["a.txt".into()]).unwrap();
    std::fs::write(dir.join("a.txt"), "unstaged\n").unwrap();
    std::fs::write(dir.join("untracked.txt"), "untracked\n").unwrap();
    let index = std::fs::read(dir.join(".git/index")).unwrap();
    std::fs::write(dir.join(".git/FETCH_HEAD"), "manual-fetch-sentinel\n").unwrap();
    commit_file(&origin, 2, "remote.txt", "colleague\n");
    let remote = Repo::open(&origin).unwrap();
    let remote_head = refs::list(&remote).unwrap().head_id;

    let log = ops::run(&repo, Op::Fetch { background: true }).unwrap();
    assert!(log.ok, "{}", log.output);
    assert!(log.command.contains("credential.interactive=never"));
    assert!(log.command.contains("--no-write-fetch-head"));
    assert_eq!(id_of(&repo, "refs/remotes/origin/main"), remote_head);
    assert_eq!(refs::list(&repo).unwrap().head_id, head);
    assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index);
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "unstaged\n");
    assert_eq!(std::fs::read_to_string(dir.join("untracked.txt")).unwrap(), "untracked\n");
    assert_eq!(std::fs::read_to_string(dir.join(".git/FETCH_HEAD")).unwrap(), "manual-fetch-sentinel\n");

    let push = || Op::Push { remote: "origin".into(), branch: "main".into(), remote_branch: "main".into(), force: true, set_upstream: false, tags: false };
    let log = ops::run(&repo, push()).unwrap();
    assert!(!log.ok, "auto-fetch must not make an unseen remote commit safe to overwrite");
    assert_eq!(refs::list(&remote).unwrap().head_id, remote_head);
    // includes 检查比较 reflog 时间；与实际 push 使用同一时钟。
    let now = || (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 1_700_000_000) as u32;
    git(&dir, now(), &["add", "."]);
    git(&dir, now(), &["commit", "-q", "-m", "local work"]);
    git(&dir, now(), &["rebase", "origin/main"]);
    ok(&repo, Op::Push { remote: "origin".into(), branch: "main".into(), remote_branch: "main".into(), force: false, set_upstream: false, tags: false });
    git(&dir, now(), &["commit", "-q", "--amend", "-m", "rewritten local work"]);
    ok(&repo, push());
    assert_eq!(refs::list(&remote).unwrap().head_id, refs::list(&repo).unwrap().head_id);

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&origin).unwrap();
}

#[test]
fn remote_fetch_push_pull_and_upstream() {
    let origin = init("ops-origin");
    commit_file(&origin, 1, "a.txt", "a\n");
    git(&origin, 0, &["config", "receive.denyCurrentBranch", "ignore"]);
    let dir = std::env::temp_dir().join(format!("gitgod-test-ops-clone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git(&origin, 0, &["clone", "-q", ".", dir.to_str().unwrap()]);
    for kv in [["user.name", "t"], ["user.email", "t@t"]] {
        git(&dir, 0, &["config", kv[0], kv[1]]);
    }
    let repo = Repo::open(&dir).unwrap();
    assert_eq!(refs::remotes(&repo).unwrap(), ["origin"]);
    let upstream_of = |name: &str| refs::list(&repo).unwrap().refs.into_iter().find(|r| r.name == name).unwrap().upstream;
    assert_eq!(upstream_of("refs/heads/main").as_deref(), Some("refs/remotes/origin/main"));
    assert_eq!(refs::list(&repo).unwrap().ahead_behind, Some((0, 0)));

    // 新分支没有上游；推到同名远程分支并设置上游
    ok(&repo, Op::CreateBranch { name: "feat".into(), start: "main".into(), checkout: true });
    assert_eq!(upstream_of("refs/heads/feat"), None);
    commit_file(&dir, 2, "b.txt", "b\n");
    ok(&repo, Op::Push { remote: "origin".into(), branch: "feat".into(), remote_branch: "feat".into(), force: false, set_upstream: true, tags: false });
    assert_eq!(upstream_of("refs/heads/feat").as_deref(), Some("refs/remotes/origin/feat"));
    assert!(id_of(&Repo::open(&origin).unwrap(), "refs/heads/feat").is_some());

    // 本地领先 1 个
    commit_file(&dir, 3, "c.txt", "c\n");
    assert_eq!(refs::list(&repo).unwrap().ahead_behind, Some((1, 0)));

    assert_eq!(
        refs::tracking(&repo).unwrap(),
        [
            refs::Track { name: "refs/heads/feat".into(), ahead: 1, behind: 0, gone: false },
            refs::Track { name: "refs/heads/main".into(), ahead: 0, behind: 0, gone: false },
        ]
    );

    // 远程前进后：fetch 看到落后，pull 跟上
    ok(&repo, Op::Checkout { target: "main".into() });
    commit_file(&origin, 4, "d.txt", "d\n");
    ok(&repo, Op::Fetch { background: false });
    assert_eq!(refs::list(&repo).unwrap().ahead_behind, Some((0, 1)));
    ok(&repo, Op::Pull { rebase: true, autostash: false });
    assert_eq!(refs::list(&repo).unwrap().ahead_behind, Some((0, 0)));

    // 检出远程分支：建立跟踪它的本地分支
    git(&origin, 0, &["branch", "remote-only"]);
    ok(&repo, Op::Fetch { background: false });
    ok(&repo, Op::Track { remote_branch: "origin/remote-only".into() });
    assert_eq!(upstream_of("refs/heads/remote-only").as_deref(), Some("refs/remotes/origin/remote-only"));

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&origin).unwrap();
}

#[test]
fn pull_explicit_mode_and_conflict_recovery() {
    let origin = init("pull-origin");
    commit_file(&origin, 1, "a.txt", "base\n");
    let dir = init("pull-local");
    git(&dir, 0, &["remote", "add", "origin", origin.to_str().unwrap()]);
    git(&dir, 0, &["fetch", "-q", "origin"]);
    git(&dir, 0, &["checkout", "-B", "main", "origin/main"]);
    let repo = Repo::open(&dir).unwrap();
    commit_file(&dir, 2, "local.txt", "local\n");
    let before = id_of(&repo, "refs/heads/main").unwrap();
    git(&dir, 0, &["branch", "keep-original"]);
    commit_file(&origin, 3, "remote.txt", "remote\n");
    // Explicit rebase wins over merge preferences and does not move sibling branches.
    git(&dir, 0, &["config", "pull.rebase", "false"]);
    git(&dir, 0, &["config", "branch.main.rebase", "false"]);
    git(&dir, 0, &["config", "rebase.updateRefs", "true"]);
    git(&dir, 0, &["config", "pull.ff", "only"]);
    let log = ops::run(&repo, Op::Pull { rebase: true, autostash: false }).unwrap();
    assert!(log.ok, "{}", log.output);
    assert!(log.command.contains("pull --rebase"));
    assert_ne!(id_of(&repo, "refs/heads/main"), Some(before.clone()));
    assert_eq!(id_of(&repo, "refs/heads/keep-original"), Some(before));
    git(&dir, 0, &["merge-base", "--is-ancestor", "origin/main", "HEAD"]);
    let parents = std::process::Command::new("git").current_dir(&dir)
        .args(["rev-list", "--parents", "-1", "HEAD"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&parents.stdout).split_whitespace().count(), 2);

    // Unchecking rebase must override a repository that prefers rebase.
    git(&dir, 0, &["config", "pull.rebase", "true"]);
    git(&dir, 0, &["config", "branch.main.rebase", "true"]);
    git(&dir, 0, &["config", "pull.ff", "true"]);
    commit_file(&origin, 4, "remote2.txt", "remote2\n");
    ok(&repo, Op::Pull { rebase: false, autostash: false });
    let parents = std::process::Command::new("git").current_dir(&dir)
        .args(["rev-list", "--parents", "-1", "HEAD"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&parents.stdout).split_whitespace().count(), 3);

    // A conflicting pull pauses; Abort restores the local tip, Continue keeps the resolution.
    commit_file(&dir, 5, "a.txt", "local\n");
    let before_conflict = id_of(&repo, "refs/heads/main").unwrap();
    commit_file(&origin, 6, "a.txt", "remote\n");
    assert!(!ops::run(&repo, Op::Pull { rebase: true, autostash: false }).unwrap().ok);
    assert_eq!(refs::list(&repo).unwrap().in_progress, Some("rebase"));
    ok(&repo, Op::Abort { what: InProgress::Rebase });
    assert_eq!(id_of(&repo, "refs/heads/main"), Some(before_conflict));
    assert!(!ops::run(&repo, Op::Pull { rebase: true, autostash: false }).unwrap().ok);
    std::fs::write(dir.join("a.txt"), "resolved\n").unwrap();
    status::stage(&repo, &["a.txt".into()]).unwrap();
    ok(&repo, Op::Continue { what: InProgress::Rebase });
    assert_eq!(refs::list(&repo).unwrap().in_progress, None);
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "resolved\n");
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&origin).unwrap();
}

#[test]
fn worktree_add_overview_remove() {
    let dir = init("ops-wt");
    commit_file(&dir, 1, "a.txt", "a\n");
    let repo = Repo::open(&dir).unwrap();
    let other = std::env::temp_dir().join(format!("gitgod-test-ops-wt-other-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&other);
    let path = other.to_str().unwrap().to_owned();

    ok(&repo, Op::WorktreeAdd { path: path.clone(), start: "main".into(), new_branch: Some("side".into()) });
    std::fs::write(other.join("new.txt"), "n\n").unwrap();
    std::fs::write(other.join("a.txt"), "changed\n").unwrap();

    let list = refs::worktrees(&repo).unwrap();
    assert_eq!(list.len(), 2);
    let brief: Vec<_> = list.iter().map(|w| (w.branch.as_deref(), w.current, w.changes, w.subject.as_str())).collect();
    assert_eq!(brief, [(Some("refs/heads/main"), true, 0, "a.txt"), (Some("refs/heads/side"), false, 2, "a.txt")]);
    assert_eq!(list[1].time, 1_700_000_001);
    // 从另一个工作树打开时，current 跟着变
    let from_other = refs::worktrees(&Repo::open(&other).unwrap()).unwrap();
    assert_eq!(from_other.iter().map(|w| w.current).collect::<Vec<_>>(), [false, true]);

    // 分支已在别的工作树检出：git 拒绝，日志里是失败
    assert!(!ops::run(&repo, Op::Checkout { target: "side".into() }).unwrap().ok);
    // 有改动的工作树不强制就删不掉
    assert!(!ops::run(&repo, Op::WorktreeRemove { path: path.clone(), force: false }).unwrap().ok);
    ok(&repo, Op::WorktreeRemove { path, force: true });
    assert_eq!(refs::worktrees(&repo).unwrap().len(), 1);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn operation_reports_git_message_commit_and_original_conflicts() {
    let dir = init("ops-operation");
    commit_file(&dir, 1, "中文 名.txt", "base\n");
    commit_file(&dir, 1, "b.txt", "base\n");
    let repo = Repo::open(&dir).unwrap();
    assert_eq!(engine::conflict::operation(&repo).unwrap(), None);
    ok(&repo, Op::CreateBranch { name: "other".into(), start: "main".into(), checkout: true });
    commit_file(&dir, 2, "中文 名.txt", "other\n");
    commit_file(&dir, 2, "b.txt", "other\n");
    let other = id_of(&repo, "refs/heads/other");
    ok(&repo, Op::Checkout { target: "main".into() });
    commit_file(&dir, 3, "中文 名.txt", "main\n");
    commit_file(&dir, 3, "b.txt", "main\n");

    assert!(!ops::run(&repo, Op::Merge { target: "other".into() }).unwrap().ok);
    let info = engine::conflict::operation(&repo).unwrap().unwrap();
    // 信息保留 Git 的原文：主题在第一行，冲突清单是后面的注释行
    assert!(info.message.starts_with("Merge branch 'other'\n\n# Conflicts:\n#\tb.txt\n"), "{}", info.message);
    assert_eq!(info.id, other);
    assert_eq!(info.conflicts, ["b.txt", "中文 名.txt"]);
    assert_eq!(info.step, None);
    // 解决一个之后，清单仍是开始时的全部冲突，界面据此算进度
    engine::conflict::take(&repo, "b.txt", true).unwrap();
    assert_eq!(engine::conflict::operation(&repo).unwrap().unwrap().conflicts.len(), 2);
    assert_eq!(status::status(&repo).unwrap().iter().filter(|e| e.conflicted).count(), 1);
    ok(&repo, Op::Abort { what: InProgress::Merge });
    assert_eq!(engine::conflict::operation(&repo).unwrap(), None);

    // 变基：正在重放的提交和它的说明，外加步数
    assert!(!ops::run(&repo, Op::Rebase { onto: "other".into() }).unwrap().ok);
    let info = engine::conflict::operation(&repo).unwrap().unwrap();
    assert_eq!(info.message.lines().next(), Some("中文 名.txt"));
    assert_eq!(info.step, Some((1, 2)));
    assert_eq!(info.conflicts, ["中文 名.txt"]);
    ok(&repo, Op::Abort { what: InProgress::Rebase });

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn modify_delete_conflict_shows_the_kept_side_and_takes_either() {
    use engine::conflict::{read, take, Block};
    let dir = init("ops-modify-delete");
    commit_file(&dir, 1, "kept.txt", "base\n");
    commit_file(&dir, 1, "gone.txt", "base\n");
    let repo = Repo::open(&dir).unwrap();
    ok(&repo, Op::CreateBranch { name: "other".into(), start: "main".into(), checkout: true });
    git(&dir, 0, &["rm", "-q", "kept.txt", "gone.txt"]);
    git(&dir, 2, &["commit", "-q", "-m", "delete"]);
    ok(&repo, Op::Checkout { target: "main".into() });
    commit_file(&dir, 3, "kept.txt", "main\n");
    commit_file(&dir, 3, "gone.txt", "main\n");
    assert!(!ops::run(&repo, Op::Merge { target: "other".into() }).unwrap().ok);

    // 对方删了、我方改了：Git 不写冲突标记，唯一的冲突块是我方全文对空
    let c = read(&repo, "kept.txt").unwrap();
    assert!(c.theirs_deleted && !c.ours_deleted && !c.binary);
    assert!(matches!(&c.blocks[..], [Block::Conflict { ours, theirs, .. }] if ours == "main\n" && theirs.is_empty()));
    // 取删掉的那边就是删除文件，取另一边就是保留
    take(&repo, "gone.txt", true).unwrap();
    take(&repo, "kept.txt", false).unwrap();
    assert!(!dir.join("gone.txt").exists());
    assert_eq!(std::fs::read_to_string(dir.join("kept.txt")).unwrap(), "main\n");
    assert!(status::status(&repo).unwrap().iter().all(|e| !e.conflicted));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fast_forward_moves_only_when_no_commit_is_lost() {
    let origin = init("ff-origin");
    commit_file(&origin, 1, "a.txt", "a\n");
    git(&origin, 0, &["branch", "side"]);
    git(&origin, 0, &["branch", "busy"]);
    let dir = std::env::temp_dir().join(format!("gitgod-test-ff-clone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git(&origin, 0, &["clone", "-q", ".", dir.to_str().unwrap()]);
    for kv in [["user.name", "t"], ["user.email", "t@t"]] {
        git(&dir, 0, &["config", kv[0], kv[1]]);
    }
    let repo = Repo::open(&dir).unwrap();
    ok(&repo, Op::Track { remote_branch: "origin/side".into() });
    ok(&repo, Op::Checkout { target: "main".into() });
    let busy = std::env::temp_dir().join(format!("gitgod-test-ff-busy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&busy);
    git(&dir, 0, &["worktree", "add", "-q", "--track", "-b", "busy", busy.to_str().unwrap(), "origin/busy"]);
    ok(&repo, Op::CreateBranch { name: "local-only".into(), start: "main".into(), checkout: false });
    assert_eq!(ops::run(&repo, Op::FastForward { branch: "local-only".into() }).unwrap_err(), "PR_NO_UPSTREAM");

    // 远程三个分支都前进
    commit_file(&origin, 2, "b.txt", "b\n");
    for branch in ["side", "busy"] {
        git(&origin, 0, &["checkout", "-q", branch]);
        commit_file(&origin, 3, &format!("{branch}.txt"), "x\n");
    }
    git(&origin, 0, &["checkout", "-q", "main"]);
    ok(&repo, Op::Fetch { background: false });
    let remote = |name: &str| id_of(&repo, &format!("refs/remotes/origin/{name}"));

    // 没检出的分支：只挪引用，FETCH_HEAD 不动
    let fetch_head = std::fs::read(dir.join(".git/FETCH_HEAD")).unwrap();
    ok(&repo, Op::FastForward { branch: "side".into() });
    assert_eq!(id_of(&repo, "refs/heads/side"), remote("side"));
    assert_eq!(std::fs::read(dir.join(".git/FETCH_HEAD")).unwrap(), fetch_head);
    assert_eq!(refs::list(&repo).unwrap().head.as_deref(), Some("refs/heads/main"));

    // 当前分支：工作区文件跟着更新，不相干的未提交改动保留
    std::fs::write(dir.join("a.txt"), "local edit\n").unwrap();
    ok(&repo, Op::FastForward { branch: "main".into() });
    assert_eq!(id_of(&repo, "refs/heads/main"), remote("main"));
    assert!(dir.join("b.txt").exists());
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "local edit\n");
    git(&dir, 0, &["checkout", "-q", "--", "a.txt"]);

    // 被别的工作树检出的分支：拒绝，不动它
    let before = id_of(&repo, "refs/heads/busy");
    assert!(!ops::run(&repo, Op::FastForward { branch: "busy".into() }).unwrap().ok);
    assert_eq!(id_of(&repo, "refs/heads/busy"), before);

    // 分叉：当前分支和非当前分支都拒绝，不合并、不变基
    commit_file(&dir, 4, "local.txt", "local\n");
    git(&dir, 0, &["branch", "-f", "side", "HEAD"]);
    commit_file(&origin, 5, "c.txt", "c\n");
    git(&origin, 0, &["checkout", "-q", "side"]);
    commit_file(&origin, 6, "side2.txt", "x\n");
    git(&origin, 0, &["checkout", "-q", "main"]);
    ok(&repo, Op::Fetch { background: false });
    for branch in ["main", "side"] {
        let before = id_of(&repo, &format!("refs/heads/{branch}"));
        assert!(!ops::run(&repo, Op::FastForward { branch: branch.into() }).unwrap().ok);
        assert_eq!(id_of(&repo, &format!("refs/heads/{branch}")), before);
    }
    assert_eq!(refs::list(&repo).unwrap().in_progress, None);

    std::fs::remove_dir_all(&busy).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&origin).unwrap();
}

#[test]
fn pull_autostash_single_file_stash_push_tags_and_patch_text() {
    let origin = init("extras-origin");
    commit_file(&origin, 1, "a.txt", "a\n");
    git(&origin, 0, &["config", "receive.denyCurrentBranch", "ignore"]);
    let dir = std::env::temp_dir().join(format!("gitgod-test-extras-clone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git(&origin, 0, &["clone", "-q", ".", dir.to_str().unwrap()]);
    for kv in [["user.name", "t"], ["user.email", "t@t"], ["core.autocrlf", "false"]] {
        git(&dir, 0, &["config", kv[0], kv[1]]);
    }
    let repo = Repo::open(&dir).unwrap();

    // 有未提交改动时：不勾选自动贮藏的变基拉取被拒绝，勾选后拉取成功且改动放回
    commit_file(&origin, 2, "b.txt", "b\n");
    std::fs::write(dir.join("a.txt"), "local edit\n").unwrap();
    assert!(!ops::run(&repo, Op::Pull { rebase: true, autostash: false }).unwrap().ok);
    ok(&repo, Op::Pull { rebase: true, autostash: true });
    assert!(dir.join("b.txt").exists());
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "local edit\n");

    // 补丁文本：未暂存、已暂存、提交三种来源
    let unstaged = engine::local_files::patch(&repo, "unstaged", "", "a.txt").unwrap();
    assert!(unstaged.contains("+local edit") && unstaged.starts_with("diff --git a/a.txt b/a.txt"));
    assert_eq!(engine::local_files::patch(&repo, "staged", "", "a.txt").unwrap(), "");
    let head = refs::list(&repo).unwrap().head_id.unwrap();
    assert!(engine::local_files::patch(&repo, "commit", &head, "b.txt").unwrap().contains("+b"));
    assert!(engine::local_files::patch(&repo, "commit", "--output=x", "b.txt").is_err());

    // 只贮藏一个文件：另一个文件的改动留在工作区
    std::fs::write(dir.join("b.txt"), "other edit\n").unwrap();
    ok(&repo, Op::StashPush { message: "one file".into(), include_untracked: false, paths: vec!["a.txt".into()] });
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "a\n");
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "other edit\n");
    git(&dir, 0, &["checkout", "-q", "--", "b.txt"]);

    // 推送时带上所有标签
    ok(&repo, Op::CreateTag { name: "v1".into(), target: "HEAD".into(), message: String::new() });
    ok(&repo, Op::Push { remote: "origin".into(), branch: "main".into(), remote_branch: "main".into(), force: false, set_upstream: false, tags: false });
    assert!(id_of(&Repo::open(&origin).unwrap(), "refs/tags/v1").is_none());
    ok(&repo, Op::Push { remote: "origin".into(), branch: "main".into(), remote_branch: "main".into(), force: false, set_upstream: false, tags: true });
    assert!(id_of(&Repo::open(&origin).unwrap(), "refs/tags/v1").is_some());

    // 定位文件只接受仓库内的路径
    assert_eq!(engine::local_files::reveal(&repo, "../outside.txt", false).unwrap_err(), "PR_INVALID_PATH");
    assert_eq!(engine::local_files::reveal(&repo, "missing.txt", true).unwrap_err(), "PR_INVALID_PATH");

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&origin).unwrap();
}

#[test]
fn binary_overlap_reports_binary_files_changed_on_both_sides_before_pulling() {
    let origin = init("overlap-origin");
    let bin = |tag: u8| [&[0u8, 1, 2, tag][..], b"binary"].concat();
    for name in ["place.rbxl", "icon.png", "untouched.bin"] {
        std::fs::write(origin.join(name), bin(0)).unwrap();
    }
    commit_file(&origin, 1, "notes.txt", "text\n");
    let dir = std::env::temp_dir().join(format!("gitgod-test-overlap-clone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git(&origin, 0, &["clone", "-q", ".", dir.to_str().unwrap()]);
    for kv in [["user.name", "t"], ["user.email", "t@t"]] {
        git(&dir, 0, &["config", kv[0], kv[1]]);
    }
    let repo = Repo::open(&dir).unwrap();
    assert!(refs::binary_overlap(&repo).unwrap().is_empty());

    // 远程改了两个二进制文件和一个文本文件
    for name in ["place.rbxl", "icon.png"] {
        std::fs::write(origin.join(name), bin(9)).unwrap();
    }
    commit_file(&origin, 2, "notes.txt", "remote text\n");
    // 本地：一个二进制文件未提交、一个已提交未推送，文本文件也改了
    std::fs::write(dir.join("icon.png"), bin(7)).unwrap();
    git(&dir, 3, &["commit", "-q", "-am", "local icon"]);
    std::fs::write(dir.join("place.rbxl"), bin(8)).unwrap();
    std::fs::write(dir.join("notes.txt"), "local text\n").unwrap();
    assert!(refs::binary_overlap(&repo).unwrap().is_empty(), "nothing is known before fetching");
    ok(&repo, Op::Fetch { background: false });
    let mut overlap = refs::binary_overlap(&repo).unwrap();
    overlap.sort();
    assert_eq!(overlap, ["icon.png", "place.rbxl"], "text files can be merged and are not reported");

    // 没有上游的分支不报错
    ok(&repo, Op::Discard { paths: vec![] });
    ok(&repo, Op::CreateBranch { name: "solo".into(), start: "HEAD".into(), checkout: true });
    assert!(refs::binary_overlap(&repo).unwrap().is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&origin).unwrap();
}
