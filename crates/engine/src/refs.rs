use crate::{err, git_at, Repo, Result};
use gix::ObjectId;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize)]
pub struct Ref {
    /// 完整引用名，如 refs/heads/main
    pub name: String,
    /// 剥到提交后的 id
    pub id: String,
    /// 本地分支的上游，完整引用名如 refs/remotes/origin/main
    pub upstream: Option<String>,
}

#[derive(Serialize)]
pub struct Refs {
    /// HEAD 指向的完整引用名；游离 HEAD 时为 None
    pub head: Option<String>,
    pub head_id: Option<String>,
    /// 当前分支相对上游的 (领先, 落后) 提交数；没有上游时为 None
    pub ahead_behind: Option<(u32, u32)>,
    /// 进行到一半的操作：merge / rebase / cherry-pick / revert
    pub in_progress: Option<&'static str>,
    pub revert_disabled: bool,
    pub refs: Vec<Ref>,
}

#[derive(Serialize)]
pub struct Stash {
    pub name: String,
    pub id: String,
    pub subject: String,
}

#[derive(Serialize, Default)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    /// 完整引用名；游离 HEAD 或裸仓库时为 None
    pub branch: Option<String>,
    /// 是不是当前打开的这个工作树
    pub current: bool,
    /// 有改动的文件数
    pub changes: usize,
    /// 相对上游的 (领先, 落后)；没有上游时为 None
    pub ahead_behind: Option<(u32, u32)>,
    /// 最后一次提交的标题和时间
    pub subject: String,
    pub time: i64,
}

/// 分支选择器里每个分支下面显示的最新提交
#[derive(Serialize, Debug, PartialEq)]
pub struct Tip {
    /// 完整引用名
    pub name: String,
    pub short_id: String,
    pub author: String,
    pub time: i64,
    pub subject: String,
}

/// 本地分支相对各自上游的同步状态
#[derive(Serialize, Debug, PartialEq)]
pub struct Track {
    /// 完整引用名
    pub name: String,
    pub ahead: u32,
    pub behind: u32,
    /// 上游分支在远程已经被删除
    pub gone: bool,
}

fn commits(gix: &gix::Repository) -> Result<Vec<(String, ObjectId)>> {
    let mut out = Vec::new();
    for r in gix.references().map_err(err)?.all().map_err(err)? {
        let Ok(r) = r else { continue };
        let name = r.name().as_bstr().to_string();
        // 贮藏在侧栏单独列出；它内部的几个提交不进提交图
        if name == "refs/stash" {
            continue;
        }
        // 标签可以指向 tree/blob（内核的 v2.6.11），这些不进提交图
        let Ok(id) = r.into_fully_peeled_id() else { continue };
        if id.header().is_ok_and(|h| h.kind() == gix::object::Kind::Commit) {
            out.push((name, id.detach()));
        }
    }
    Ok(out)
}

/// 提交图的起点：所有引用加 HEAD（游离 HEAD 不在引用里）。
pub(crate) fn tips(gix: &gix::Repository) -> Result<Vec<ObjectId>> {
    let mut tips: Vec<_> = commits(gix)?.into_iter().map(|(_, id)| id).collect();
    tips.extend(gix.head_id().ok().map(|id| id.detach()));
    Ok(tips)
}

/// Auto 包含 HEAD、上游和基准远程分支；筛选改变遍历起点，不改变检出的分支。
/// 其余取值是引用全名，多个以换行分隔（引用名里不会有换行），提交取并集。
/// 以 `^` 开头的一行是「相比」的基准：它和它的祖先都不显示（同 `git log ^基准 分支`）。
/// 返回 (起点, 不显示的起点)。
pub(crate) fn graph_tips(repo: &Repo, scope: &str) -> Result<(Vec<ObjectId>, Vec<ObjectId>)> {
    if scope == "all" { return Ok((tips(&repo.gix())?, Vec::new())); }
    if scope != "auto" {
        let refs = list_with(repo, false)?;
        let ids = |hidden: bool| refs.refs.iter().filter(|r| scope.split('\n').any(|name| name.strip_prefix('^').map_or(!hidden && name == r.name, |name| hidden && name == r.name)))
            .map(|r| ObjectId::from_hex(r.id.as_bytes()).map_err(err)).collect::<Result<Vec<_>>>();
        // 已经删掉的引用跳过；一个都不在才报错
        let shown = ids(false)?;
        if shown.is_empty() { return Err("PR_GRAPH_REF_GONE".into()); }
        return Ok((shown, ids(true)?));
    }
    let head = repo.gix().head_name().map_err(err)?.map(|n| n.as_bstr().to_string());
    let Some(head) = head.as_deref() else { return Ok((repo.gix().head_id().ok().map(|id| id.detach()).into_iter().collect(), Vec::new())) };
    let branch = head.trim_start_matches("refs/heads/");
    // 引用、配置、远程、reflog 各是一个 git 进程，并行跑
    let (refs, configured, remotes, from_reflog) = std::thread::scope(|s| {
        let refs = s.spawn(|| list_with(repo, false));
        let configured = s.spawn(|| repo.git(&["config", "--get", &format!("branch.{branch}.vscode-merge-base")]).ok()
            .map(|b| format!("refs/remotes/{}", String::from_utf8_lossy(&b).trim())));
        let remotes = s.spawn(|| remotes(repo));
        // 与原生 Git 一样，优先找创建分支时的来源；只接受仍存在的远程分支。
        let from_reflog = repo.git(&["reflog", "show", "--format=%gs", "--max-count=2", "--grep-reflog=^branch: Created from ", head]).ok()
            .and_then(|out| {
                let text = String::from_utf8_lossy(&out);
                let mut lines = text.lines();
                let source = lines.next()?.strip_prefix("branch: Created from ")?;
                if lines.next().is_some() { return None; }
                if source != "HEAD" { return Some(source.to_owned()); }
                let out = repo.git(&["reflog", "show", "--format=%gs", "--fixed-strings", &format!("--grep-reflog= to {branch}"), "HEAD"]).ok()?;
                String::from_utf8_lossy(&out).lines().filter_map(|line| line.strip_prefix("checkout: moving from ")?.strip_suffix(&format!(" to {branch}"))).last().map(str::to_owned)
            });
        (refs.join().unwrap(), configured.join().unwrap(), remotes.join().unwrap(), from_reflog)
    });
    let (refs, remotes) = (refs?, remotes?);
    let mut ids = refs.head_id.into_iter().collect::<Vec<_>>();
    {
        let upstream = refs.refs.iter().find(|r| r.name == head).and_then(|r| r.upstream.as_deref());
        ids.extend(refs.refs.iter().filter(|r| Some(r.name.as_str()) == upstream).map(|r| r.id.clone()));
        let from_reflog = from_reflog.as_ref().and_then(|name| refs.refs.iter().find(|r| r.name == *name || r.name == format!("refs/heads/{name}") || r.name == format!("refs/remotes/{name}")))
            .and_then(|r| if r.name.starts_with("refs/remotes/") { Some(r) } else { refs.refs.iter().find(|up| Some(&up.name) == r.upstream.as_ref()) });
        let remote = remotes.iter().filter(|r| upstream.is_some_and(|u| u.starts_with(&format!("refs/remotes/{r}/"))))
            .max_by_key(|r| r.len()).or_else(|| remotes.iter().find(|r| *r == "origin")).or_else(|| remotes.first());
        let base = configured.as_ref().and_then(|name| refs.refs.iter().find(|r| &r.name == name))
            .or(from_reflog)
            .or_else(|| remote.and_then(|remote| ["HEAD", "main", "master"].into_iter()
                .find_map(|name| refs.refs.iter().find(|r| r.name == format!("refs/remotes/{remote}/{name}")))));
        ids.extend(base.map(|r| r.id.clone()));
    }
    ids.sort();
    ids.dedup();
    Ok((ids.iter().map(|id| ObjectId::from_hex(id.as_bytes()).map_err(err)).collect::<Result<_>>()?, Vec::new()))
}

pub fn list(repo: &Repo) -> Result<Refs> {
    list_with(repo, true)
}

/// 提交图只要引用本身，不需要领先/落后计数。
fn list_with(repo: &Repo, count: bool) -> Result<Refs> {
    let gix = repo.gix();
    let head = gix.head_name().map_err(err)?.map(|n| n.as_bstr().to_string());
    // 上游信息走 CLI：gix 的配置是打开仓库时的快照，push -u 之后不会更新。计数与它并行跑，没有上游时计数命令自己会失败
    let (out, ahead_behind, revert_disabled) = std::thread::scope(|s| {
        let revert_disabled = s.spawn(|| crate::ops::revert_disabled(repo));
        let ahead_behind = (count && head.is_some()).then(|| s.spawn(|| {
            let out = repo.git(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]).ok()?;
            let out = String::from_utf8_lossy(&out);
            let mut n = out.split_whitespace().map(|n| n.parse().ok());
            Some((n.next()??, n.next()??))
        }));
        (repo.git(&["for-each-ref", "--format=%(refname)%00%(upstream)", "refs/heads"]), ahead_behind.and_then(|t| t.join().unwrap()), revert_disabled.join().unwrap())
    });
    let out = out?;
    let out = String::from_utf8_lossy(&out);
    let upstreams: HashMap<_, _> = out.lines().filter_map(|l| l.split_once('\0')).filter(|(_, u)| !u.is_empty()).collect();
    let dir = gix.git_dir();
    let in_progress = [
        ("merge", "MERGE_HEAD"),
        ("rebase", "rebase-merge"),
        ("rebase", "rebase-apply"),
        ("cherry-pick", "CHERRY_PICK_HEAD"),
        ("revert", "REVERT_HEAD"),
    ]
    .into_iter()
    .find(|(_, marker)| dir.join(marker).exists())
    .map(|(what, _)| what);
    Ok(Refs {
        head_id: gix.head_id().ok().map(|id| id.to_string()),
        ahead_behind,
        in_progress,
        revert_disabled: revert_disabled?,
        refs: commits(&gix)?
            .into_iter()
            .map(|(name, id)| Ref { upstream: upstreams.get(name.as_str()).map(|u| u.to_string()), name, id: id.to_string() })
            .collect(),
        head,
    })
}

pub fn remotes(repo: &Repo) -> Result<Vec<String>> {
    Ok(String::from_utf8_lossy(&repo.git(&["remote"])?).lines().map(str::to_owned).collect())
}

pub fn stashes(repo: &Repo) -> Result<Vec<Stash>> {
    let out = repo.git(&["stash", "list", "--format=%gd%x00%H%x00%gs"])?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\0');
            Some(Stash { name: f.next()?.into(), id: f.next()?.into(), subject: f.next()?.into() })
        })
        .collect())
}

pub fn worktrees(repo: &Repo) -> Result<Vec<Worktree>> {
    let out = repo.git(&["worktree", "list", "--porcelain", "-z"])?;
    let mut list = Vec::new();
    let mut cur: Option<Worktree> = None;
    // 每条记录以空字段结束
    for field in String::from_utf8_lossy(&out).split('\0') {
        match field.split_once(' ').unwrap_or((field, "")) {
            ("worktree", path) => cur = Some(Worktree { path: path.into(), ..Default::default() }),
            ("HEAD", id) => cur.as_mut().unwrap().head = id.into(),
            ("branch", name) => cur.as_mut().unwrap().branch = Some(name.into()),
            ("", _) => list.extend(cur.take()),
            _ => {}
        }
    }
    // 子模块的主工作树在 git 的清单里是它的 git 目录（父仓库的 .git/modules/…），真正的文件夹记在 core.worktree 里
    if let Some(main) = list.first_mut() {
        if let Ok(dir) = git_at(Path::new(&main.path), &["config", "--get", "core.worktree"], &[]) {
            if let Ok(real) = std::fs::canonicalize(Path::new(&main.path).join(String::from_utf8_lossy(&dir).trim())) {
                // canonicalize 在 Windows 上带 \\?\ 前缀；和 git 给的其他路径一样用正斜杠
                main.path = real.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
            }
        }
    }
    // 每个工作树要跑三条 git 命令，各开一个线程并行
    let here = std::fs::canonicalize(&repo.path).ok();
    std::thread::scope(|s| {
        for w in &mut list {
            let here = &here;
            s.spawn(move || {
                let dir = Path::new(&w.path);
                let run = |args: &[&str]| git_at(dir, args, &[]).map(|o| String::from_utf8_lossy(&o).into_owned());
                w.current = here.is_some() && std::fs::canonicalize(dir).ok() == *here;
                w.changes = run(&["status", "--porcelain=v2", "-z"]).map_or(0, |o| crate::status::parse(&o).len());
                w.ahead_behind = run(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]).ok().and_then(|o| {
                    let mut n = o.split_whitespace().map(|n| n.parse().ok());
                    Some((n.next()??, n.next()??))
                });
                if let Some((subject, time)) = run(&["log", "-1", "--format=%s%x00%ct"]).ok().as_deref().and_then(|o| o.trim().split_once('\0')) {
                    w.subject = subject.to_owned();
                    w.time = time.parse().unwrap_or(0);
                }
            });
        }
    });
    Ok(list)
}

/// 本地和远程分支各自的最新提交；只在打开分支选择器时取，不拖慢 `list`。
pub fn branch_tips(repo: &Repo) -> Result<Vec<Tip>> {
    let out = repo.git(&["for-each-ref", "--format=%(refname)%00%(objectname:short)%00%(authorname)%00%(committerdate:unix)%00%(subject)", "refs/heads", "refs/remotes"])?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(5, '\0');
            Some(Tip { name: f.next()?.into(), short_id: f.next()?.into(), author: f.next()?.into(), time: f.next()?.parse().ok()?, subject: f.next()?.into() })
        })
        .collect())
}

/// 所有有上游的本地分支的同步状态。分支多时要算一两秒（内核仓库 1000 个分支 1.5 秒），界面应当异步取。
pub fn tracking(repo: &Repo) -> Result<Vec<Track>> {
    let out = repo.git(&["for-each-ref", "--format=%(refname)%00%(upstream)%00%(upstream:track,nobracket)", "refs/heads"])?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\0');
            let (name, upstream, track) = (f.next()?, f.next()?, f.next()?);
            if upstream.is_empty() {
                return None;
            }
            // track 形如 "ahead 1, behind 2" / "behind 2" / "gone" / 空
            let count = |key: &str| track.split(", ").find_map(|p| p.strip_prefix(key)?.trim().parse().ok()).unwrap_or(0);
            Some(Track { name: name.to_owned(), ahead: count("ahead"), behind: count("behind"), gone: track == "gone" })
        })
        .collect())
}

/// 已经完全合入 `base` 的本地分支（完整引用名），「清理分支」用。
/// 不算在内：`base` 自己、任何工作树里检出着的分支（含当前分支）、和 `base` 指向同一个提交的分支（多半是刚建还没提交的）。
pub fn merged_branches(repo: &Repo, base: &str) -> Result<Vec<String>> {
    let tip = crate::tools::revision(repo, base)?;
    let out = repo.git(&["branch", "--merged", &tip, "--format=%(refname)%00%(objectname)%00%(worktreepath)"])?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\0');
            let (name, id, worktree) = (f.next()?, f.next()?, f.next()?);
            (name.starts_with("refs/heads/") && id != tip && worktree.is_empty() && name != base && name.strip_prefix("refs/heads/") != Some(base)).then(|| name.to_owned())
        })
        .collect())
}

/// 上游改了、本地也改着（未提交或已提交未推送）的二进制文件。
/// 二进制文件没法合并，两边都改就必有一方白做，所以拉取之前就要提示。没有上游时返回空。
pub fn binary_overlap(repo: &Repo) -> Result<Vec<String>> {
    // numstat 里二进制文件的增删行数都是 "-"；三个点表示「上游相对共同祖先的改动」
    let Ok(theirs) = repo.git(&["diff", "--numstat", "--no-renames", "-z", "HEAD...@{upstream}"]) else { return Ok(Vec::new()) };
    let theirs = String::from_utf8_lossy(&theirs);
    let binary: Vec<&str> = theirs.split('\0').filter_map(|rec| rec.strip_prefix("-\t-\t")).collect();
    if binary.is_empty() {
        return Ok(Vec::new());
    }
    let committed = repo.git(&["diff", "--name-only", "--no-renames", "-z", "@{upstream}...HEAD"])?;
    let committed = String::from_utf8_lossy(&committed);
    let working = crate::status::status(repo)?;
    let mine: std::collections::HashSet<&str> = committed.split('\0').chain(working.iter().map(|e| e.path.as_str())).collect();
    Ok(binary.into_iter().filter(|path| mine.contains(path)).map(str::to_owned).collect())
}
