mod common;

use common::{git, init};
use engine::{diff, refs, status, Repo};

fn lines(from: u32, to: u32) -> String {
    (from..=to).map(|i| format!("l{i}\n")).collect()
}

fn brief(repo: &Repo) -> Vec<(String, Option<char>, Option<char>)> {
    status::status(repo).unwrap().into_iter().map(|e| (e.path, e.staged, e.unstaged)).collect()
}

fn index_content(dir: &std::path::Path, path: &str) -> String {
    let out = std::process::Command::new("git").current_dir(dir).args(["show", &format!(":{path}")]).output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn stage_lines_commit_and_amend() {
    let dir = init("working");
    std::fs::write(dir.join("f.txt"), lines(1, 20)).unwrap();
    git(&dir, 0, &["add", "."]);
    git(&dir, 1, &["commit", "-q", "-m", "base"]);
    let repo = Repo::open(&dir).unwrap();
    let f = "f.txt".to_owned();

    // 两处改动相隔足够远，形成两个区块；第一个区块里既有替换也有纯新增
    let edited = lines(1, 20).replace("l2\n", "L2\nnew\n").replace("l18\n", "L18\n");
    std::fs::write(dir.join("f.txt"), &edited).unwrap();
    std::fs::write(dir.join("中文.txt"), [0xc4, 0xe3, 0xba, 0xc3, b'\n']).unwrap(); // GBK 的「你好」
    assert_eq!(brief(&repo), [("f.txt".into(), None, Some('M')), ("中文.txt".into(), None, Some('?'))]);

    let d = diff::worktree(&repo, &f, false, false).unwrap();
    assert_eq!(d.hunks.len(), 2);
    let h = &d.hunks[0];
    let shape: Vec<_> = h.lines.iter().map(|l| (l.kind, l.text.as_str(), l.old_no, l.new_no)).collect();
    assert_eq!(shape[..4], [(' ', "l1", Some(1), Some(1)), ('-', "l2", Some(2), None), ('+', "L2", None, Some(2)), ('+', "new", None, Some(3))]);

    // 只暂存「new」这一行：暂存区里 l2 保持原样，后面多一行 new
    assert!(diff::apply_lines(&repo, &f, false, 0, "@@ 过期的区块头 @@", &[3]).is_err());
    diff::apply_lines(&repo, &f, false, 0, &h.header, &[3]).unwrap();
    assert_eq!(index_content(&dir, "f.txt"), lines(1, 20).replace("l2\n", "l2\nnew\n"));
    assert_eq!(brief(&repo)[0], ("f.txt".into(), Some('M'), Some('M')));

    // 再把它从暂存区按行撤回
    let staged = diff::worktree(&repo, &f, true, false).unwrap();
    let pos = staged.hunks[0].lines.iter().position(|l| l.kind == '+').unwrap();
    diff::apply_lines(&repo, &f, true, 0, &staged.hunks[0].header, &[pos]).unwrap();
    assert_eq!(index_content(&dir, "f.txt"), lines(1, 20));

    // 暂存整个第二个区块
    let d = diff::worktree(&repo, &f, false, false).unwrap();
    let all: Vec<_> = (0..d.hunks[1].lines.len()).collect();
    diff::apply_lines(&repo, &f, false, 1, &d.hunks[1].header, &all).unwrap();
    assert_eq!(index_content(&dir, "f.txt"), lines(1, 20).replace("l18\n", "L18\n"));

    // 整文件暂存 / 取消暂存
    status::stage(&repo, &[f.clone()]).unwrap();
    assert_eq!(brief(&repo)[0], ("f.txt".into(), Some('M'), None));
    status::unstage(&repo, &[f.clone()]).unwrap();
    assert_eq!(brief(&repo)[0], ("f.txt".into(), None, Some('M')));

    // 未跟踪的 GBK 文件：解码显示，暂存后提交
    let gbk = diff::worktree(&repo, "中文.txt", false, true).unwrap();
    assert_eq!((gbk.hunks[0].lines[0].kind, gbk.hunks[0].lines[0].text.as_str()), ('+', "你好"));
    status::stage(&repo, &["中文.txt".into()]).unwrap();
    status::commit(&repo, "提交信息\n\n正文", false).unwrap();
    assert_eq!(brief(&repo), [("f.txt".into(), None, Some('M'))]);
    assert_eq!(status::last_message(&repo).unwrap(), "提交信息\n\n正文");
    let head = refs::list(&repo).unwrap().head_id.unwrap();
    let shown = diff::commit(&repo, &head, "中文.txt").unwrap();
    assert_eq!(shown.hunks[0].lines[0].text, "你好");

    status::commit(&repo, "改过的信息", true).unwrap();
    assert_eq!(status::last_message(&repo).unwrap(), "改过的信息");
    assert!(status::commit(&repo, "没有暂存内容", false).is_err());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn untracked_directories_list_preview_and_stage_individual_files() {
    let dir = init("untracked-directory");
    std::fs::write(dir.join(".gitignore"), "*.ignored\n").unwrap();
    git(&dir, 0, &["add", ".gitignore"]);
    git(&dir, 1, &["commit", "-q", "-m", "base"]);
    std::fs::create_dir_all(dir.join("新增 目录/nested")).unwrap();
    let paths = ["新增 目录/a.txt", "新增 目录/nested/b.txt"];
    for path in paths {
        std::fs::write(dir.join(path), format!("{path}\n")).unwrap();
    }
    std::fs::write(dir.join("新增 目录/skip.ignored"), "ignored\n").unwrap();
    let repo = Repo::open(&dir).unwrap();
    let expected: Vec<_> = paths.iter().map(|p| ((*p).into(), None, Some('?'))).collect();
    assert_eq!(brief(&repo), expected);
    // 用户配置隐藏未跟踪文件时，仍按文件列出。
    git(&dir, 0, &["config", "status.showUntrackedFiles", "no"]);
    assert_eq!(brief(&repo), expected);
    for path in paths {
        let shown = diff::worktree(&repo, path, false, true).unwrap();
        assert_eq!(shown.hunks[0].lines[0].text, path);
    }
    status::stage(&repo, &[paths[0].into()]).unwrap();
    assert_eq!(brief(&repo), [(paths[0].into(), Some('A'), None), (paths[1].into(), None, Some('?'))]);
    assert_eq!(diff::worktree(&repo, paths[0], true, false).unwrap().hunks[0].lines[0].text, paths[0]);
    status::unstage(&repo, &[paths[0].into()]).unwrap();
    assert_eq!(brief(&repo), expected);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unstage_before_first_commit_and_binary() {
    let dir = init("unborn");
    let repo = Repo::open(&dir).unwrap();
    std::fs::write(dir.join("b.bin"), [0u8, 1, 2]).unwrap();
    assert!(diff::worktree(&repo, "b.bin", false, true).unwrap().binary);
    status::stage(&repo, &["b.bin".into()]).unwrap();
    assert_eq!(brief(&repo), [("b.bin".into(), Some('A'), None)]);
    assert!(diff::worktree(&repo, "b.bin", true, false).unwrap().binary);
    status::unstage(&repo, &["b.bin".into()]).unwrap();
    assert_eq!(brief(&repo), [("b.bin".into(), None, Some('?'))]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn oversized_diffs_are_bounded_and_cannot_apply_stale_lines() {
    let dir = init("large-diff");
    let repo = Repo::open(&dir).unwrap();
    let large = "a line of text\n".repeat(400_000);
    std::fs::write(dir.join("large.txt"), &large).unwrap();
    assert!(diff::worktree(&repo, "large.txt", false, true).unwrap().too_large);
    status::stage(&repo, &["large.txt".into()]).unwrap();
    assert!(diff::worktree(&repo, "large.txt", true, false).unwrap().too_large);
    status::commit(&repo, "large file", false).unwrap();
    let id = refs::list(&repo).unwrap().head_id.unwrap();
    assert!(diff::commit(&repo, &id, "large.txt").unwrap().too_large);
    std::fs::write(dir.join("large.txt"), large.replace('a', "b")).unwrap();
    assert!(diff::worktree(&repo, "large.txt", false, false).unwrap().too_large);
    assert!(diff::apply_lines(&repo, "large.txt", false, 0, "@@ -1 +1 @@", &[0]).is_err());
    assert_eq!(index_content(&dir, "large.txt"), large);
    assert!(diff::commit(&repo, &"a".repeat(40), "large.txt").is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn display_options_change_the_view_without_breaking_partial_staging() {
    let dir = init("diff-options");
    std::fs::write(dir.join("f.txt"), lines(1, 20)).unwrap();
    std::fs::write(dir.join("gone.txt"), lines(1, 3)).unwrap();
    git(&dir, 0, &["add", "."]);
    git(&dir, 1, &["commit", "-q", "-m", "base"]);
    let repo = Repo::open(&dir).unwrap();
    let with = |context, ignore_ws| diff::Options { context, ignore_ws };
    let view = |opts| diff::worktree_with(&repo, "f.txt", false, false, opts).unwrap();

    // 文件头、中间、文件尾各改一处，另有一行只动了缩进
    let edited = lines(1, 20).replace("l1\n", "L1\n").replace("l10\n", "L10\nnew\n").replace("l15\n", "  l15\n").replace("l20\n", "L20\n");
    std::fs::write(dir.join("f.txt"), &edited).unwrap();

    assert_eq!(view(with(None, false)).hunks.len(), 2);
    let changed = |d: &diff::Diff| d.hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind != ' ').map(|l| l.text.clone()).collect::<Vec<_>>();
    assert!(changed(&view(with(None, false))).contains(&"  l15".to_owned()));
    assert!(!changed(&view(with(None, true))).iter().any(|t| t.contains("l15")));

    // 上下文 1 行：四处改动各成一个区块，文件头尾的行号照常
    let narrow = view(with(Some(1), false));
    assert_eq!(narrow.hunks.len(), 4);
    assert_eq!((narrow.hunks[0].lines[0].kind, narrow.hunks[0].lines[0].old_no), ('-', Some(1)));
    assert_eq!(narrow.hunks[3].lines.last().unwrap().new_no, Some(21));
    // 按界面显示的上下文暂存「new」一行；不带上下文参数时区块标题对不上
    let pos = narrow.hunks[1].lines.iter().position(|l| l.text == "new").unwrap();
    assert!(diff::apply_lines(&repo, "f.txt", false, 1, &narrow.hunks[1].header, &[pos]).is_err());
    diff::apply_lines_with(&repo, "f.txt", false, 1, &narrow.hunks[1].header, &[pos], Some(1)).unwrap();
    assert_eq!(index_content(&dir, "f.txt"), lines(1, 20).replace("l10\n", "l10\nnew\n"));

    // 整个文件：只有一个区块，修改标记还在，照样能按行暂存
    let whole = view(with(Some(1_000_000), false));
    assert_eq!(whole.hunks.len(), 1);
    assert_eq!(whole.hunks[0].lines.iter().filter(|l| l.kind == ' ').count(), 17);
    let tail: Vec<_> = whole.hunks[0].lines.iter().enumerate().filter(|(_, l)| l.text.ends_with("20")).map(|(i, _)| i).collect();
    diff::apply_lines_with(&repo, "f.txt", false, 0, &whole.hunks[0].header, &tail, Some(1_000_000)).unwrap();
    assert_eq!(index_content(&dir, "f.txt"), lines(1, 20).replace("l10\n", "l10\nnew\n").replace("l20\n", "L20\n"));

    // 没有上下文行的部分补丁不可靠，直接拒绝
    let bare = view(with(Some(0), false));
    assert_eq!(diff::apply_lines_with(&repo, "f.txt", false, 0, &bare.hunks[0].header, &[0], Some(0)).unwrap_err(), "PR_DIFF_FILTERED");

    // 新增和删除的文件
    std::fs::remove_file(dir.join("gone.txt")).unwrap();
    let gone = diff::worktree_with(&repo, "gone.txt", false, false, with(Some(1_000_000), false)).unwrap();
    assert_eq!(gone.hunks[0].lines.iter().map(|l| (l.kind, l.old_no)).collect::<Vec<_>>(), [('-', Some(1)), ('-', Some(2)), ('-', Some(3))]);
    std::fs::write(dir.join("added.txt"), lines(1, 2)).unwrap();
    git(&dir, 2, &["add", "added.txt"]);
    let added = diff::worktree_with(&repo, "added.txt", true, false, with(Some(0), true)).unwrap();
    assert_eq!(added.hunks[0].lines.iter().map(|l| (l.kind, l.new_no)).collect::<Vec<_>>(), [('+', Some(1)), ('+', Some(2))]);
}
