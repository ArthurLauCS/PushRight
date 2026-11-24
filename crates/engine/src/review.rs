//! 已审阅：工作区里把文件标为已审阅时记下它当时的内容（写进 Git 对象库），之后能只看审阅后的新改动。
//! 存在本工作树的 Git 目录里（`pushright/review.json`），不进提交；各工作树各有一份。
//! 快照对象没有提交引用，Git 回收时（默认两周后）可能删掉，删掉后只能看全部改动。

use crate::diff::{self, Diff, Options};
use crate::status::Entry;
use crate::{decode, err, Repo, Result};
use gix::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    /// 审阅时内容的对象号；文件当时不存在就是空内容的对象
    blob: String,
    /// 审阅时文件原样字节的 SHA-1，文件不存在为空：刷新时在进程内比对，不用每次起 git
    sum: String,
}

fn file(repo: &Repo) -> PathBuf {
    PathBuf::from(repo.git_dirs().0).join("pushright").join("review.json")
}

/// 文件不存在或损坏都当作没有审阅记录
fn read(repo: &Repo) -> BTreeMap<String, Snapshot> {
    std::fs::read(file(repo)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(repo: &Repo, files: &BTreeMap<String, Snapshot>) -> Result<()> {
    let path = file(repo);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
    // 先写临时文件再换名，写到一半被结束也不会留下半个文件
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(files).unwrap()).map_err(err)?;
    std::fs::rename(&tmp, &path).map_err(err)
}

/// 文件现在原样字节的 SHA-1；读不到（不存在、是目录）为空
fn sum(repo: &Repo, path: &str) -> String {
    let Ok(bytes) = std::fs::read(repo.path.join(path)) else { return String::new() };
    let mut h = gix::hash::hasher(gix::hash::Kind::Sha1);
    h.update(&bytes);
    h.try_finalize().map(|id| id.to_string()).unwrap_or_default()
}

/// 把文件现在的内容写进对象库（和 `git add` 一样经过换行转换），返回对象号；文件不存在时写空内容
fn write_blob(repo: &Repo, path: &str) -> Result<String> {
    let out = if repo.path.join(path).is_file() { repo.git(&["hash-object", "-w", "--", path])? } else { repo.git_in(&["hash-object", "-w", "--stdin"], b"")? };
    Ok(decode(&out).trim().to_owned())
}

/// 已审阅的文件 → 审阅后有没有再改过。去掉不再有改动的文件（已提交、已丢弃），有去掉的就写回。`entries` 是同一次刷新读到的状态
pub fn load(repo: &Repo, entries: &[Entry]) -> HashMap<String, bool> {
    let mut files = read(repo);
    let before = files.len();
    files.retain(|p, _| entries.iter().any(|e| e.path == *p));
    if files.len() != before {
        // 写不进去下次刷新会再清理一遍，不影响显示
        let _ = save(repo, &files);
    }
    files.into_iter().map(|(p, s)| {
        let changed = sum(repo, &p) != s.sum;
        (p, changed)
    }).collect()
}

/// `reviewed` 为 true 时给文件现在的内容记快照（已有的换成新的），false 时去掉审阅记录
pub fn set(repo: &Repo, path: &str, reviewed: bool) -> Result<()> {
    let mut files = read(repo);
    if reviewed {
        // 先算字节再写对象：中间文件又被改了，下次刷新会显示有新改动
        let sum = sum(repo, path);
        files.insert(path.to_owned(), Snapshot { blob: write_blob(repo, path)?, sum });
    } else {
        files.remove(path);
    }
    save(repo, &files)
}

/// 审阅后的改动：快照 → 文件现在的内容。没有审阅记录或快照对象已被回收时为 None
pub fn diff(repo: &Repo, path: &str, opts: Options) -> Result<Option<Diff>> {
    let Some(snapshot) = read(repo).remove(path) else { return Ok(None) };
    let gix = repo.gix();
    if !ObjectId::from_hex(snapshot.blob.as_bytes()).is_ok_and(|id| gix.has_object(id)) {
        return Ok(None);
    }
    let now = write_blob(repo, path)?;
    diff::blobs(repo, &snapshot.blob, &now, opts).map(Some)
}
