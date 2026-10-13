mod common;
use common::{git, init};
use engine::{diff, history, ops::{self, Op}, rebase, tools, Repo};
use std::fs;

fn commit(dir: &std::path::Path, n: u32, path: &str, text: &str) {
    fs::write(dir.join(path), text).unwrap();
    git(dir, n, &["add", "."]);
    git(dir, n, &["commit", "-q", "-m", &format!("change {n}")]);
}
fn run(repo: &Repo, op: Op) { let log = ops::run(repo, op).unwrap(); assert!(log.ok, "{}", log.output); }

#[test]
fn compare_remotes_and_reflog_recovery() {
    let dir = init("tools-compare");
    commit(&dir, 1, "old.txt", "one\n");
    git(&dir, 0, &["branch", "other"]);
    git(&dir, 0, &["mv", "old.txt", "new.txt"]);
    git(&dir, 2, &["commit", "-qm", "rename"]);
    let repo = Repo::open(&dir).unwrap();
    let comparison = tools::compare(&repo, "other", "HEAD", false).unwrap();
    assert_eq!(comparison.files[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(comparison.files[0].path, "new.txt");
    assert!(tools::compare(&repo, "--all", "HEAD", false).is_err());
    git(&dir, 0, &["checkout", "-q", "other"]);
    commit(&dir, 3, "other.txt", "branch\n");
    assert_eq!(tools::compare(&repo, "other", "main", true).unwrap().files.len(), 1);
    run(&repo, Op::RemoteAdd { name: "test".into(), url: "https://example.invalid/old.git".into() });
    run(&repo, Op::RemoteUrl { name: "test".into(), url: "https://example.invalid/new.git".into(), push: false });
    run(&repo, Op::RemoteUrl { name: "test".into(), url: "https://example.invalid/push.git".into(), push: true });
    run(&repo, Op::RemoteRename { old: "test".into(), new: "renamed".into() });
    let remote = tools::remotes(&repo).unwrap().remove(0);
    assert_eq!(remote.name, "renamed"); assert!(remote.fetch.ends_with("new.git")); assert!(remote.push.ends_with("push.git"));
    run(&repo, Op::RemoteRemove { name: "renamed".into() });
    assert!(tools::remotes(&repo).unwrap().is_empty());
    let lost = tools::revision(&repo, "HEAD").unwrap();
    git(&dir, 0, &["reset", "--hard", "HEAD~1"]);
    assert!(tools::reflog(&repo, 0, 201).unwrap().iter().any(|r| r.id == lost));
    run(&repo, Op::CreateBranch { name: "recovered".into(), start: lost.clone(), checkout: false });
    assert_eq!(tools::revision(&repo, "recovered").unwrap(), lost);
    assert_eq!(history::show(&repo, "recovered", "other.txt").unwrap().as_deref(), Some("branch\n"));
}

#[test]
fn clone_init_and_nonempty_destinations() {
    let dir = init("tools-create");
    commit(&dir, 1, "a", "source\n");
    let dest = dir.join("clone space's");
    tools::create(dest.to_str().unwrap(), Some(dir.to_str().unwrap())).unwrap();
    assert_eq!(fs::read_to_string(dest.join("a")).unwrap().replace("\r\n", "\n"), "source\n");
    assert!(tools::create(dest.to_str().unwrap(), Some(dir.to_str().unwrap())).is_err());
    let empty = dir.join("fresh"); fs::create_dir(&empty).unwrap();
    tools::create(empty.to_str().unwrap(), None).unwrap();
    assert!(empty.join(".git").is_dir());
    let occupied = dir.join("occupied"); fs::create_dir(&occupied).unwrap(); fs::write(occupied.join("keep"), "keep").unwrap();
    assert!(tools::create(occupied.to_str().unwrap(), Some(dir.to_str().unwrap())).is_err());
    assert_eq!(fs::read_to_string(occupied.join("keep")).unwrap(), "keep");
}

#[test]
fn native_rebase_reorder_reword_squash_and_edit() {
    let dir = init("rebase space's");
    commit(&dir, 1, "base", "base\n");
    commit(&dir, 2, "a", "a\n");
    commit(&dir, 3, "b", "b\n");
    commit(&dir, 4, "c", "c\n");
    let repo = Repo::open(&dir).unwrap();
    let mut plan = rebase::plan(&repo, "HEAD~3").unwrap();
    let old_head = plan.head.clone();
    plan.steps.swap(0, 1);
    plan.steps[0].action = "reword".into();
    plan.steps[0].message = "new message ' $() ` ;\n\nbody".into();
    plan.steps[2].action = "squash".into();
    plan.steps[2].message = "combined".into();
    let result = rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).unwrap();
    assert!(result.ok, "{}", result.output);
    let history = history::file(&repo, "b", 10).unwrap();
    assert_eq!(history[0].subject, "new message ' $() ` ;");
    assert_eq!(engine::detail::detail(&repo, &tools::revision(&repo, "HEAD").unwrap()).unwrap().message, "combined");
    for p in ["a", "b", "c"] { assert!(dir.join(p).exists()); }
    let backup = result.output.lines().next().unwrap();
    assert_eq!(tools::revision(&repo, backup).unwrap(), old_head);
    assert!(rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).is_err(), "stale plan must be rejected");
    let mut plan = rebase::plan(&repo, "HEAD~1").unwrap();
    plan.steps[0].action = "edit".into();
    assert!(rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).unwrap().ok);
    assert!(engine::refs::list(&repo).unwrap().in_progress.is_some());
    assert!(rebase::continue_rebase(&repo).unwrap().ok);
    assert!(engine::refs::list(&repo).unwrap().in_progress.is_none());
}

#[test]
fn rebase_conflicts_abort_and_invalid_plan_preserve_data() {
    let dir = init("rebase-conflict");
    commit(&dir, 1, "a", "base\n"); commit(&dir, 2, "a", "second\n"); commit(&dir, 3, "a", "third\n");
    let repo = Repo::open(&dir).unwrap();
    let mut plan = rebase::plan(&repo, "HEAD~2").unwrap();
    fs::write(dir.join("private"), "keep").unwrap();
    assert!(rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).is_err());
    fs::remove_file(dir.join("private")).unwrap();
    plan.steps[0].action = "exec".into();
    assert!(rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).is_err());
    plan.steps[0].action = "pick".into(); plan.steps.swap(0, 1);
    let log = rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).unwrap();
    assert!(!log.ok); assert!(engine::refs::list(&repo).unwrap().in_progress.is_some());
    run(&repo, Op::Abort { what: engine::ops::InProgress::Rebase });
    assert_eq!(tools::revision(&repo, "HEAD").unwrap(), plan.head);
    assert_eq!(fs::read_to_string(dir.join("a")).unwrap(), "third\n");
    let result = rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).unwrap();
    assert!(!result.ok);
    fs::write(dir.join("a"), "resolved third\n").unwrap();
    git(&dir, 0, &["add", "a"]);
    let result = rebase::continue_rebase(&repo).unwrap();
    if !result.ok {
        fs::write(dir.join("a"), "resolved second\n").unwrap();
        git(&dir, 0, &["add", "a"]);
        assert!(rebase::continue_rebase(&repo).unwrap().ok);
    }
    assert!(engine::refs::list(&repo).unwrap().in_progress.is_none());
}

#[test]
fn history_pages_and_dirty_line_mapping() {
    let dir = init("tools-history");
    commit(&dir, 1, "a", "one\ntwo\nthree\n");
    commit(&dir, 2, "a", "one\ntwo\nTHREE\n");
    git(&dir, 0, &["mv", "a", "renamed"]); git(&dir, 3, &["commit", "-qm", "rename"]);
    let repo = Repo::open(&dir).unwrap();
    assert_eq!(history::file_page(&repo, "renamed", 1, 1).unwrap()[0].subject, "change 2");
    assert_eq!(history::search_page(&repo, history::SearchKind::Message, "change", 1, 1).unwrap()[0].subject, "change 1");
    let lines = history::lines_page(&repo, "renamed", 4, 4, 201, 0, Some("inserted\none\ntwo\nTHREE\n")).unwrap();
    assert_eq!(lines[0].subject, "change 2");
    assert_eq!(lines[0].path, "a");
    assert_eq!(history::lines_page(&repo, "renamed", 4, 4, 1, 1, Some("inserted\none\ntwo\nTHREE\n")).unwrap()[0].subject, "change 1");
    assert!(history::lines_page(&repo, "renamed", 1, 1, 201, 0, Some("inserted\none\ntwo\nTHREE\n")).is_err());
    assert!(history::lines_page(&repo, "renamed", 2, 2, 201, 0, Some("one\nchanged\nTHREE\n")).is_err());
    assert_eq!(fs::read_to_string(dir.join("renamed")).unwrap(), "one\ntwo\nTHREE\n");
}

#[test]
fn histories_continue_beyond_two_hundred_commits() {
    use std::io::Write;
    let dir = init("history-205");
    let mut input = String::new();
    for i in 0..205 {
        let message = format!("revision {i}");
        let content = format!("line {i}\n");
        input.push_str(&format!("commit refs/heads/main\ncommitter t <t@t> {} +0000\ndata {}\n{}\nM 100644 inline file.txt\ndata {}\n{}\n", 1_700_000_000 + i, message.len(), message, content.len(), content));
    }
    input.push_str("done\n");
    let mut child = std::process::Command::new("git").arg("-C").arg(&dir).args(["fast-import", "--quiet"]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());
    git(&dir, 0, &["reset", "--hard", "HEAD"]);
    let repo = Repo::open(&dir).unwrap();
    assert_eq!(history::file_page(&repo, "file.txt", 201, 0).unwrap().len(), 201);
    let next = history::file_page(&repo, "file.txt", 201, 200).unwrap();
    assert_eq!(next.len(), 5); assert_eq!(next[4].subject, "revision 0");
    assert_eq!(history::search_page(&repo, history::SearchKind::Message, "revision", 201, 200).unwrap().len(), 5);
    assert_eq!(history::lines_page(&repo, "file.txt", 1, 1, 201, 200, None).unwrap().len(), 5);
}

#[test]
fn rebase_fixup_drop_and_drop_all() {
    let dir = init("rebase-drop");
    commit(&dir, 1, "base", "base\n"); commit(&dir, 2, "a", "a\n"); commit(&dir, 3, "b", "b\n"); commit(&dir, 4, "c", "c\n");
    let repo = Repo::open(&dir).unwrap();
    let mut plan = rebase::plan(&repo, "HEAD~3").unwrap();
    plan.steps[1].action = "fixup".into(); plan.steps[2].action = "drop".into();
    let result = rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).unwrap();
    assert!(result.ok, "{}", result.output);
    assert!(dir.join("a").exists() && dir.join("b").exists() && !dir.join("c").exists());
    assert_eq!(rebase::plan(&repo, &plan.base).unwrap().steps.len(), 1);
    let mut next = rebase::plan(&repo, &plan.base).unwrap();
    next.steps[0].action = "drop".into();
    let result = rebase::run(&repo, &next.base, &next.head, &next.steps, false).unwrap();
    assert!(result.ok, "{}", result.output);
    assert_eq!(tools::revision(&repo, "HEAD").unwrap(), plan.base);
}

#[test]
fn images_and_large_diff_pages() {
    let dir = init("tools-diff");
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jvWoAAAAASUVORK5CYII=").unwrap();
    fs::write(dir.join("a.png"), &png).unwrap();
    commit(&dir, 1, "large.txt", "small\n");
    let repo = Repo::open(&dir).unwrap();
    let image = diff::commit(&repo, &tools::revision(&repo, "HEAD").unwrap(), "a.png").unwrap();
    assert!(image.images.unwrap()[1].as_ref().unwrap().starts_with("data:image/png;base64,"));
    let text = (0..3600).map(|i| format!("{i} {}\n", "x".repeat(1400))).collect::<String>();
    fs::write(dir.join("large.txt"), &text).unwrap();
    assert!(diff::worktree(&repo, "large.txt", false, false).unwrap().too_large);
    let mut offset = 0;
    let mut added = 0;
    loop {
        let page = diff::page(&repo, "unstaged", "", "", "large.txt", None, offset).unwrap();
        assert!(page.paged);
        added += page.hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind == '+').count();
        if let Some(next) = page.next { assert!(next > offset); offset = next; } else { break; }
    }
    assert_eq!(added, 3600);
    git(&dir, 2, &["commit", "-qam", "large"]);
    assert!(history::show(&repo, "HEAD", "large.txt").is_err());
    assert_eq!(history::show_limit(&repo, "HEAD", "large.txt", 8 << 20).unwrap().unwrap(), text);
    fs::write(dir.join("long.txt"), "x".repeat(5 << 20)).unwrap();
    let preview = diff::page(&repo, "untracked", "", "", "long.txt", None, 0).unwrap();
    assert!(preview.clipped);
    assert!(preview.hunks[0].lines[0].text.len() <= 65536);
    assert!(diff::page(&repo, "untracked", "", "", "../outside", None, 0).is_err());
}

#[test]
fn rebase_can_carry_dependent_branches_and_reports_their_old_positions() {
    let dir = init("rebase-update-refs");
    commit(&dir, 1, "base", "base\n"); commit(&dir, 2, "a", "a\n");
    git(&dir, 0, &["branch", "part-a"]);
    git(&dir, 0, &["branch", "pushright-backup/old"]);
    commit(&dir, 3, "b", "b\n");
    git(&dir, 0, &["branch", "part-b"]);
    let busy = dir.with_extension("busy");
    let _ = fs::remove_dir_all(&busy);
    git(&dir, 0, &["worktree", "add", "-q", "-b", "elsewhere", busy.to_str().unwrap()]);
    commit(&dir, 4, "c", "c\n"); commit(&dir, 5, "d", "d\n");
    let repo = Repo::open(&dir).unwrap();
    let id = |rev: &str| tools::revision(&repo, rev).unwrap();
    let (old_a, old_b) = (id("part-a"), id("part-b"));

    // 备份分支和别的工作树里检出的分支不在可跟随的范围内
    let mut plan = rebase::plan(&repo, "HEAD~4").unwrap();
    assert_eq!(plan.steps.iter().map(|s| s.branches.join(",")).collect::<Vec<_>>(), ["refs/heads/part-a", "refs/heads/part-b", "", ""]);

    // 不勾选：其他分支原地不动
    plan.steps[0].action = "reword".into(); plan.steps[0].message = "a reworded".into();
    assert!(rebase::run(&repo, &plan.base, &plan.head, &plan.steps, false).unwrap().ok);
    assert_eq!((id("part-a"), id("part-b")), (old_a.clone(), old_b.clone()));
    run(&repo, Op::Reset { target: plan.head.clone(), mode: engine::ops::ResetMode::Hard });

    // 勾选：分支落在对应的新提交上；c 并入 b 时 part-b 跟到合并后的提交
    plan.steps[2].action = "fixup".into();
    let result = rebase::run(&repo, &plan.base, &plan.head, &plan.steps, true).unwrap();
    assert!(result.ok, "{}", result.output);
    assert!(result.output.contains(&format!("refs/heads/part-a was {old_a}")) && result.output.contains(&format!("refs/heads/part-b was {old_b}")));
    assert_eq!(id("part-a"), id("HEAD~2"));
    assert_eq!(id("part-b"), id("HEAD~1"));
    assert_ne!(id("part-a"), old_a);
    assert_eq!(id("elsewhere"), old_b);
    assert_eq!(id("pushright-backup/old"), old_a);
    fs::remove_dir_all(&busy).unwrap();
}

#[test]
fn patch_bisect_submodules_and_lfs() {
    use engine::ops::Verdict;
    let dir = init("tools-repo");
    for n in 1..=6 { commit(&dir, n, "a.txt", &format!("line {n}\n")); }
    let repo = Repo::open(&dir).unwrap();

    // 补丁：整体生效；对不上时什么都不改
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-line 6\n+patched";
    run(&repo, Op::ApplyPatch { text: patch.replace('\n', "\r\n"), index: true });
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "patched\n");
    assert!(engine::status::status(&repo).unwrap().iter().any(|e| e.path == "a.txt" && e.staged.is_some()), "--index stages the result");
    assert!(!ops::run(&repo, Op::ApplyPatch { text: patch.into(), index: false }).unwrap().ok, "a patch that no longer matches is refused");
    assert!(!dir.join(".git/pushright-patch.diff").exists());
    git(&dir, 0, &["reset", "-q", "--hard"]);

    // 二分查找：HEAD 有问题、HEAD~5 没问题，「line 4」起算有问题
    assert!(tools::bisect_log(&repo).is_none());
    run(&repo, Op::BisectStart { bad: "HEAD".into(), good: "HEAD~5".into() });
    assert!(tools::bisect_log(&repo).unwrap().contains("git bisect start"));
    let mut last = String::new();
    for _ in 0..6 {
        let n: u32 = fs::read_to_string(dir.join("a.txt")).unwrap().trim()[5..].parse().unwrap();
        let log = ops::run(&repo, Op::BisectMark { verdict: if n >= 4 { Verdict::Bad } else { Verdict::Good } }).unwrap();
        assert!(log.ok, "{}", log.output);
        last = log.output;
        if last.contains("is the first") { break; }
    }
    assert!(last.contains("is the first") && last.contains("change 4"), "{last}");
    run(&repo, Op::BisectReset);
    assert!(tools::bisect_log(&repo).is_none());
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "line 6\n");

    // 子模块：路径里带空格和括号也能解析
    let sub = init("tools-repo-sub");
    commit(&sub, 1, "s.txt", "sub\n");
    git(&dir, 0, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", sub.to_str().unwrap(), "libs/my lib (v1)"]);
    git(&dir, 7, &["commit", "-qm", "add submodule"]);
    let list = tools::submodules(&repo).unwrap();
    assert_eq!((list[0].path.as_str(), list[0].state, list[0].id.len()), ("libs/my lib (v1)", "ok", 40));
    // 子模块自己的工作树清单：主工作树是它的文件夹，不是父仓库 .git/modules 下的 git 目录
    let inner = engine::refs::worktrees(&Repo::open(&dir.join("libs/my lib (v1)")).unwrap()).unwrap();
    assert!(inner[0].current && inner[0].path.ends_with("libs/my lib (v1)"), "{}", inner[0].path);
    git(&dir, 0, &["submodule", "deinit", "-q", "-f", "--all"]);
    assert_eq!(tools::submodules(&repo).unwrap()[0].state, "missing");
    assert_eq!(tools::submodules(&repo).unwrap()[0].path, "libs/my lib (v1)");

    // LFS：没装 git-lfs 的机器上跳过
    if tools::lfs(&repo).version.is_some() {
        run(&repo, Op::LfsInstall);
        run(&repo, Op::LfsTrack { pattern: "*.rbxl".into() });
        let lfs = tools::lfs(&repo);
        assert!(lfs.installed && lfs.patterns == ["*.rbxl"], "{lfs:?}");
        run(&repo, Op::LfsUntrack { pattern: "*.rbxl".into() });
        assert!(tools::lfs(&repo).patterns.is_empty());
    }
    fs::remove_dir_all(&dir).unwrap();
    fs::remove_dir_all(&sub).unwrap();
}

#[test]
fn stats_count_commits_authors_times_and_files() {
    let dir = init("tools-stats");
    let at = |date: &str, author: &str, path: &str, text: &str| {
        fs::write(dir.join(path), text).unwrap();
        git(&dir, 0, &["add", "."]);
        git(&dir, 0, &["commit", "-q", "-m", "c", &format!("--date={date}T00:15:00+08:00"), &format!("--author={author} <a@example.invalid>")]);
    };
    at("2026-01-10", "Ann", "中文 名.txt", "1\n");
    at("2026-01-20", "Bob", "b.txt", "1\n");
    at("2026-04-02", "Ann", "中文 名.txt", "2\n");
    let stats = tools::stats(&Repo::open(&dir).unwrap()).unwrap();
    let pairs = |list: &[tools::Count]| list.iter().map(|c| (c.name.clone(), c.count)).collect::<Vec<_>>();
    assert_eq!(stats.commits, 3);
    assert_eq!(stats.times, [1_775_060_100, 1_768_839_300, 1_767_975_300]);
    assert_eq!(pairs(&stats.authors), [("Ann".to_owned(), 2), ("Bob".to_owned(), 1)]);
    assert_eq!(pairs(&stats.files), [("中文 名.txt".to_owned(), 2), ("b.txt".to_owned(), 1)]);
    fs::remove_dir_all(&dir).unwrap();
}
