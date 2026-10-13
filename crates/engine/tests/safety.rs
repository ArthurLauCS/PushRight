mod common;

use common::{git, init};
use engine::conflict::{self, Block, Side};
use engine::ops::{self, Op, ResetMode};
use engine::{diff, refs, status, Repo};
use std::path::Path;

fn ok(repo: &Repo, op: Op) {
    let log = ops::run(repo, op).unwrap();
    assert!(log.ok, "{}\n{}", log.command, log.output);
}

fn commit_file(dir: &Path, time: u32, name: &str, content: &[u8]) {
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, 0, &["add", "."]);
    git(dir, time, &["commit", "-q", "-m", name]);
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

#[test]
fn discard_is_recoverable() {
    let dir = init("safety-discard");
    let base: String = (1..=20).map(|i| format!("l{i}\n")).collect();
    commit_file(&dir, 1, "f.txt", base.as_bytes());
    commit_file(&dir, 2, "g.txt", b"g\n");
    let repo = Repo::open(&dir).unwrap();

    // 丢弃一行：只有这一行回到原样，其余改动还在；备份进了贮藏
    std::fs::write(dir.join("f.txt"), base.replace("l2\n", "L2\n").replace("l18\n", "L18\n")).unwrap();
    let d = diff::worktree(&repo, "f.txt", false, false).unwrap();
    let plus = d.hunks[1].lines.iter().position(|l| l.kind == '+').unwrap();
    let minus = d.hunks[1].lines.iter().position(|l| l.kind == '-').unwrap();
    diff::discard_lines(&repo, "f.txt", 1, &d.hunks[1].header, &[minus, plus]).unwrap();
    assert_eq!(read(&dir, "f.txt"), base.replace("l2\n", "L2\n"));
    assert_eq!(refs::stashes(&repo).unwrap().len(), 1);

    // 丢弃整个文件 + 一个未跟踪文件：工作区干净，改动在贮藏里，弹出后回来
    std::fs::write(dir.join("new.txt"), "n\n").unwrap();
    std::fs::write(dir.join("g.txt"), "changed\n").unwrap();
    ok(&repo, Op::Discard { paths: vec!["g.txt".into(), "new.txt".into()] });
    assert_eq!(read(&dir, "g.txt"), "g\n");
    assert!(!dir.join("new.txt").exists());
    assert_eq!(status::status(&repo).unwrap().len(), 1); // f.txt 的改动没被牵连
    ok(&repo, Op::StashApply { name: "stash@{0}".into(), pop: true });
    assert_eq!(read(&dir, "g.txt"), "changed\n");
    assert!(dir.join("new.txt").exists());

    // 全部丢弃
    ok(&repo, Op::Discard { paths: vec![] });
    assert!(status::status(&repo).unwrap().is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reset_keep_refuses_to_lose_changes() {
    let dir = init("safety-keep");
    commit_file(&dir, 1, "a.txt", b"1\n");
    commit_file(&dir, 2, "a.txt", b"2\n");
    let repo = Repo::open(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "dirty\n").unwrap();
    assert!(!ops::run(&repo, Op::Reset { target: "HEAD~1".into(), mode: ResetMode::Keep }).unwrap().ok);
    assert_eq!(read(&dir, "a.txt"), "dirty\n");
    std::fs::write(dir.join("a.txt"), "2\n").unwrap();
    ok(&repo, Op::Reset { target: "HEAD~1".into(), mode: ResetMode::Keep });
    assert_eq!(read(&dir, "a.txt"), "1\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// main 和 other 都改了同一个文件的两处，合并后产生两个冲突块
fn conflicted(name: &str, main: &[u8], other: &[u8], base: &[u8], file: &str) -> (std::path::PathBuf, Repo) {
    let dir = init(name);
    commit_file(&dir, 1, file, base);
    let repo = Repo::open(&dir).unwrap();
    ok(&repo, Op::CreateBranch { name: "other".into(), start: "main".into(), checkout: true });
    commit_file(&dir, 2, file, other);
    ok(&repo, Op::Checkout { target: "main".into() });
    commit_file(&dir, 3, file, main);
    assert!(!ops::run(&repo, Op::Merge { target: "other".into() }).unwrap().ok);
    (dir, repo)
}

#[test]
fn resolve_conflict_block_by_block() {
    let lines = |a: &str, b: &str| format!("{a}\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n{b}\n").into_bytes();
    let (dir, repo) = conflicted("safety-conflict", &lines("main-top", "main-end"), &lines("other-top", "other-end"), &lines("1", "14"), "a.txt");

    let c = conflict::read(&repo, "a.txt").unwrap();
    let conflicts: Vec<_> = c
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Conflict { ours, theirs, ours_label, theirs_label } => Some((ours.as_str(), theirs.as_str(), ours_label.as_str(), theirs_label.as_str())),
            Block::Text { .. } => None,
        })
        .collect();
    assert_eq!(conflicts, [("main-top\n", "other-top\n", "HEAD", "other"), ("main-end\n", "other-end\n", "HEAD", "other")]);
    // 中间 12 行一致的内容被折叠
    assert!(c.blocks.iter().any(|b| matches!(b, Block::Text { text } if text.contains('⋯'))));

    assert!(conflict::resolve(&repo, "a.txt", &[Side::Ours]).is_err());
    conflict::resolve(&repo, "a.txt", &[Side::Theirs, Side::Both]).unwrap();
    assert_eq!(read(&dir, "a.txt"), "other-top\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\nmain-end\nother-end\n");
    let st = status::status(&repo).unwrap();
    assert!(!st[0].conflicted && st[0].staged == Some('M'));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn binary_conflict_takes_one_side() {
    let (dir, repo) = conflicted("safety-binary", &[0, 1, b'm'], &[0, 1, b'o'], &[0, 1, b'b'], "place.rbxl");
    assert!(conflict::read(&repo, "place.rbxl").unwrap().binary);
    conflict::take(&repo, "place.rbxl", true).unwrap();
    assert_eq!(std::fs::read(dir.join("place.rbxl")).unwrap(), [0, 1, b'o']);
    assert!(!status::status(&repo).unwrap().iter().any(|e| e.conflicted));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn hand_edited_resolution_keeps_the_file_encoding_and_line_endings() {
    // GBK 编码、CRLF 换行的文件：「中」是 D6 D0，「文」是 CE C4
    let file = |top: &[u8]| [top, b"\r\n2\r\n3\r\n"].concat();
    let (dir, repo) = conflicted("safety-custom", &file(b"\xd6\xd0 main"), &file(b"\xce\xc4 other"), &file(b"base"), "a.txt");
    let c = conflict::read(&repo, "a.txt").unwrap();
    assert!(matches!(&c.blocks[0], Block::Conflict { ours, theirs, .. } if ours.starts_with("中 main") && theirs.starts_with("文 other")));

    // 界面给的是 UTF-8、\n 换行、结尾没有换行；一个选择对应一个冲突块，其余仍可以选一边
    conflict::resolve(&repo, "a.txt", &[Side::Custom("中文 merged\nsecond line".into())]).unwrap();
    assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), b"\xd6\xd0\xce\xc4 merged\r\nsecond line\r\n2\r\n3\r\n");
    assert!(!status::status(&repo).unwrap()[0].conflicted);
    std::fs::remove_dir_all(&dir).unwrap();

    // 清空内容等于把这一块删掉
    let (dir, repo) = conflicted("safety-custom-empty", b"main\nkeep\n", b"other\nkeep\n", b"base\nkeep\n", "a.txt");
    conflict::resolve(&repo, "a.txt", &[Side::Custom(String::new())]).unwrap();
    assert_eq!(read(&dir, "a.txt"), "keep\n");
    std::fs::remove_dir_all(&dir).unwrap();
}
