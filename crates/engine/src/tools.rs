use crate::{decode, err, git_at, ops::safe, Repo, Result};
use crate::detail::{parse_name_status, FileChange};
use serde::Serialize;
use std::path::Path;

/// Resolve user input once so comparison and recovery keep referring to the same objects.
pub fn revision(repo: &Repo, rev: &str) -> Result<String> {
    Ok(decode(&repo.git(&["rev-parse", "--verify", "--end-of-options", &format!("{}^{{commit}}", safe(rev)?)])?).trim().to_owned())
}

#[derive(Serialize)]
pub struct Comparison {
    pub left: String,
    pub right: String,
    pub files: Vec<FileChange>,
}

pub fn compare(repo: &Repo, left: &str, right: &str, common_base: bool) -> Result<Comparison> {
    let mut left = revision(repo, left)?;
    let right = revision(repo, right)?;
    if common_base { left = decode(&repo.git(&["merge-base", &left, &right])?).trim().to_owned(); }
    let out = repo.git(&["diff", "--name-status", "-z", "-M", &left, &right, "--"])?;
    Ok(Comparison { left, right, files: parse_name_status(&decode(&out)) })
}

#[derive(Serialize)]
pub struct Remote { pub name: String, pub fetch: String, pub push: String }

pub fn remotes(repo: &Repo) -> Result<Vec<Remote>> {
    crate::refs::remotes(repo)?.into_iter().map(|name| {
        let fetch = decode(&repo.git(&["remote", "get-url", &name])?).trim().to_owned();
        let push = decode(&repo.git(&["remote", "get-url", "--push", &name])?).trim().to_owned();
        Ok(Remote { name, fetch, push })
    }).collect()
}

#[derive(Serialize)]
pub struct Reflog { pub id: String, pub selector: String, pub subject: String }

pub fn reflog(repo: &Repo, skip: usize, limit: usize) -> Result<Vec<Reflog>> {
    let out = repo.git(&["reflog", "show", "--all", "--format=%H%x00%gD%x00%gs", &format!("--skip={skip}"), &format!("--max-count={}", limit.min(201))])?;
    Ok(decode(&out).lines().filter_map(|line| {
        let mut fields = line.splitn(3, '\0');
        Some(Reflog { id: fields.next()?.into(), selector: fields.next()?.into(), subject: fields.next()?.into() })
    }).collect())
}

/// Never delete or replace the destination, including on a failed clone.
pub fn create(path: &str, url: Option<&str>) -> Result<String> {
    let path = Path::new(path);
    if !path.is_absolute() { return Err("PR_INVALID_PATH".into()); }
    if path.join(".git").exists() { return Err("PR_REPO_EXISTS".into()); }
    if let Some(url) = url {
        safe(url)?;
        if path.exists() && std::fs::read_dir(path).map_err(err)?.next().is_some() { return Err("PR_DEST_NOT_EMPTY".into()); }
        let parent = path.parent().ok_or("PR_INVALID_PATH")?;
        git_at(parent, &["-c", "credential.interactive=false", "clone", "--", url, &path.to_string_lossy()], &[])?;
    } else {
        git_at(path, &["init", "-b", "main"], &[])?;
    }
    Ok(Repo::open(path)?.path.to_string_lossy().into_owned())
}

/// 二分查找进行中时返回它的记录（`git bisect log` 的内容），否则 None。
pub fn bisect_log(repo: &Repo) -> Option<String> {
    std::fs::read(repo.gix().git_dir().join("BISECT_LOG")).ok().map(|b| decode(&b))
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Submodule {
    pub path: String,
    /// 子模块当前检出的提交；还没初始化时是主仓库记录的提交
    pub id: String,
    /// ok：和主仓库记录的一致；missing：还没初始化；moved：检出的提交和记录的不同；conflict：有合并冲突
    pub state: &'static str,
}

pub fn submodules(repo: &Repo) -> Result<Vec<Submodule>> {
    let out = decode(&repo.git(&["submodule", "status", "--recursive"])?);
    Ok(out.lines().filter_map(|line| {
        let state = match line.get(..1)? { "-" => "missing", "+" => "moved", "U" => "conflict", _ => "ok" };
        let (id, rest) = line[1..].split_once(' ')?;
        // 已初始化的后面跟着「 (git describe 的结果)」
        let path = if state != "missing" && rest.ends_with(')') { rest.rsplit_once(" (").map_or(rest, |(p, _)| p) } else { rest };
        Some(Submodule { path: path.to_owned(), id: id.to_owned(), state })
    }).collect())
}

#[derive(Serialize, Debug)]
pub struct Lfs {
    /// 机器上装的 git-lfs 版本；没装时为 None
    pub version: Option<String>,
    /// LFS 的过滤器在这个仓库里是否生效（`git lfs install` 做过）
    pub installed: bool,
    /// 交给 LFS 管理的文件模式（.gitattributes 里的）
    pub patterns: Vec<String>,
}

pub fn lfs(repo: &Repo) -> Lfs {
    let Ok(version) = repo.git(&["lfs", "version"]) else { return Lfs { version: None, installed: false, patterns: Vec::new() } };
    let listing = decode(&repo.git(&["lfs", "track"]).unwrap_or_default());
    // 输出是「Listing tracked patterns」、缩进的「模式 (来源文件)」若干行，然后是排除的模式
    let patterns = listing.lines().skip(1).take_while(|l| l.starts_with(' ')).filter_map(|l| l.trim().rsplit_once(" (").map(|(p, _)| p.to_owned())).collect();
    Lfs { version: Some(decode(&version).trim().to_owned()), installed: repo.git(&["config", "--get", "filter.lfs.process"]).is_ok(), patterns }
}

/// 服务器上的文件锁清单，原样返回 `git lfs locks` 的输出；要联网。
pub fn lfs_locks(repo: &Repo) -> Result<String> {
    Ok(decode(&repo.git(&["lfs", "locks"])?).trim().to_owned())
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Count { pub name: String, pub count: u32 }

#[derive(Serialize, Debug)]
pub struct Stats {
    /// 当前分支历史里的提交数，含合并提交
    pub commits: u32,
    /// 作者时间戳；界面按用户选择的时区计算日期与月份。
    pub times: Vec<i64>,
    /// 按提交数从多到少
    pub authors: Vec<Count>,
    /// 被改动次数最多的 100 个文件；合并提交不计
    pub files: Vec<Count>,
}

/// 当前分支的提交统计。只数文件名不数增删行数：实测 5938 个提交的仓库上 `--name-only` 约 0.3 秒，`--numstat` 约 5.6 秒。
pub fn stats(repo: &Repo) -> Result<Stats> {
    use std::collections::HashMap;
    let out = decode(&repo.git(&["-c", "core.quotepath=false", "log", "--format=%x01%at%x09%aN", "--name-only", "--no-renames", "HEAD"])?);
    let (mut authors, mut files) = (HashMap::<&str, u32>::new(), HashMap::<&str, u32>::new());
    let mut times = Vec::new();
    for line in out.lines() {
        if let Some((time, author)) = line.strip_prefix('\x01').and_then(|l| l.split_once('\t')) {
            times.push(time.parse::<i64>().map_err(err)?);
            *authors.entry(author).or_default() += 1;
        } else if !line.is_empty() {
            *files.entry(line).or_default() += 1;
        }
    }
    let ranked = |map: HashMap<&str, u32>| {
        let mut list: Vec<_> = map.into_iter().map(|(name, count)| Count { name: name.to_owned(), count }).collect();
        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        list
    };
    let mut files = ranked(files);
    files.truncate(100);
    Ok(Stats { commits: times.len() as u32, times, authors: ranked(authors), files })
}
