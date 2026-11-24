//! 改动分组：把工作区的改动按文件分进几个有名字的组，每组单独提交。
//! 存在本工作树的 Git 目录里（`pushright/groups.json`），不进提交；各工作树各有一份。
//! 组里的文件只有未暂存的改动：被暂存或不再有改动就离开组，这样提交框提交的始终是「已暂存」里看得到的内容。

use crate::status::{self, Entry};
use crate::{err, Repo, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Group {
    /// 组名，也是提交说明（可多行）
    pub name: String,
    pub paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    groups: Vec<Group>,
}

fn file(repo: &Repo) -> PathBuf {
    PathBuf::from(repo.git_dirs().0).join("pushright").join("groups.json")
}

/// 文件不存在或损坏都当作没有分组
fn read(repo: &Repo) -> Vec<Group> {
    std::fs::read(file(repo)).ok().and_then(|b| serde_json::from_slice::<File>(&b).ok()).unwrap_or_default().groups
}

/// 读出分组，并去掉已暂存、冲突或不再有改动的文件；有去掉的就写回。`entries` 是同一次刷新读到的状态
pub fn load(repo: &Repo, entries: &[Entry]) -> Vec<Group> {
    let mut groups = read(repo);
    let mut pruned = false;
    for g in &mut groups {
        let before = g.paths.len();
        g.paths.retain(|p| entries.iter().any(|e| e.path == *p && e.staged.is_none() && e.unstaged.is_some() && !e.conflicted));
        pruned |= g.paths.len() != before;
    }
    if pruned {
        // 写不进去下次刷新会再清理一遍，不影响显示
        let _ = save(repo, &groups);
    }
    groups
}

pub fn save(repo: &Repo, groups: &[Group]) -> Result<()> {
    let path = file(repo);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
    // 先写临时文件再换名，写到一半被结束也不会留下半个文件
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&File { groups: groups.to_vec() }).unwrap()).map_err(err)?;
    std::fs::rename(&tmp, &path).map_err(err)
}

/// 把 `paths` 移到第 `target` 个组；None 表示移出分组。一个文件只在一个组里
pub fn assign(groups: &mut [Group], paths: &[String], target: Option<usize>) {
    for g in groups.iter_mut() {
        g.paths.retain(|p| !paths.contains(p));
    }
    if let Some(g) = target.and_then(|i| groups.get_mut(i)) {
        g.paths.extend(paths.iter().cloned());
    }
}

/// 用组名作提交说明，只提交组里的文件；成功后删掉这个组
pub fn commit(repo: &Repo, group: &Group) -> Result<()> {
    status::commit_paths(repo, &group.name, &group.paths)?;
    let mut groups = read(repo);
    groups.retain(|g| g != group);
    save(repo, &groups)
}
