//! GitHub 拉取请求和 GitLab 合并请求。从远程地址认出托管平台，经系统自带的 curl 调 REST API：
//! 令牌和请求体写进 curl 从 stdin 读的配置，不出现在命令行里；curl 用系统证书库和环境变量里的代理，
//! 自建服务器用公司内部证书也能连。

use crate::diff::Line;
use crate::{decode, err, git_at, Repo, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    GitHub,
    GitLab,
}

/// 仓库在托管平台上对应的项目
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Forge {
    pub kind: Kind,
    /// 主机名，令牌按它存
    pub host: String,
    /// 网页根地址，如 `https://github.com`
    pub web: String,
    /// REST API 根地址
    pub api: String,
    /// `owner/repo`；GitLab 可以有多级群组
    pub project: String,
    /// 取 PR 的提交、检出时用的远程
    pub remote: String,
}

/// 一个拉取请求 / 合并请求
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Request {
    /// GitHub 的编号 / GitLab 的 iid
    pub number: u64,
    pub title: String,
    pub body: String,
    /// open / closed / merged
    pub state: String,
    pub draft: bool,
    pub author: String,
    pub source: String,
    pub target: String,
    /// 源分支最新的提交
    pub head: String,
    /// 对比的起点：GitHub 是目标分支的提交，GitLab 是合并基础；列表里没有时为空
    pub base: String,
    /// 源分支在别的仓库（fork）里
    pub fork: bool,
    pub reviewers: Vec<String>,
    pub url: String,
    /// ISO 8601
    pub updated: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Check {
    pub name: String,
    /// success / failure / pending / neutral
    pub state: String,
    pub url: String,
}

/// 详情页要的状态：CI、审批、能否合并和合并选项的默认值
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub checks: Vec<Check>,
    pub approved: Vec<String>,
    /// 要求修改的人（只有 GitHub 有）
    pub changes_requested: Vec<String>,
    /// 服务器给的合并状态原词，如 clean / dirty / blocked、mergeable / conflict / not_approved
    pub mergeable: String,
    /// 合并方式：GitHub 是仓库允许的 merge / squash / rebase；GitLab 只有 merge
    pub methods: Vec<String>,
    pub squash: bool,
    pub remove_source: bool,
    pub base: String,
    /// GitLab diff 的起点（start_sha），发行评论要用；GitHub 为空
    pub start: String,
}

/// diff 里的一条行评论；所在行被后来的提交改掉了（已过时）的不算
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub path: String,
    /// 落在旧文件那一侧（删除行），否则是新文件
    pub old: bool,
    /// 多行评论是最后一行
    pub line: u32,
    pub author: String,
    pub body: String,
    /// ISO 8601
    pub created: String,
}

/// diff 里的一行在新旧文件里的位置。增删行只在一侧有行号，另一侧记它插在哪（GitLab 的 line_code 要用）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spot {
    /// '+' / '-' / ' '
    pub kind: char,
    pub old: u32,
    pub new: u32,
}

/// 行评论落在哪：文件和起止两行，单行时两行相同
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anchor {
    pub path: String,
    /// 改名前的路径，没改名时和 path 相同
    pub old_path: String,
    pub from: Spot,
    pub to: Spot,
}

/// 创建表单要的项目信息
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Project {
    pub default_branch: String,
    /// GitLab 项目设置里新建合并请求时默认勾选的两项
    pub remove_source: bool,
    pub squash: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NewRequest {
    pub source: String,
    pub target: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub remove_source: bool,
    pub squash: bool,
    /// GitLab 的指派人、审核者的用户 id，0 是不指定；社区版各只能有一个
    pub assignee: u64,
    pub reviewer: u64,
}

/// 能访问项目的用户，选指派人和审核者用（只有 GitLab）
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Member {
    pub id: u64,
    pub username: String,
    pub name: String,
}

/// 远程地址 → (网页根地址, 主机名, 项目路径)。认 `https://host[:port]/path`、`git@host:path`、`ssh://git@host[:port]/path`；
/// SSH 的端口和网页无关，按 https 默认端口算
pub fn parse_url(url: &str) -> Option<(String, String, String)> {
    let url = url.trim();
    let (scheme, authority, path) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let (a, p) = rest.split_once('/')?;
            (scheme, a, p)
        }
        // scp 写法 git@host:group/repo.git
        None => {
            let (a, p) = url.split_once(':')?;
            ("ssh", a, p)
        }
    };
    let authority = authority.rsplit('@').next()?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => (h, Some(p)),
        _ => (authority, None),
    };
    let path = path.trim_matches('/').trim_end_matches(".git").trim_end_matches('/');
    if host.is_empty() || !path.contains('/') {
        return None;
    }
    let web = match (scheme, port) {
        ("http" | "https", Some(p)) => format!("{scheme}://{host}:{p}"),
        ("http", None) => format!("http://{host}"),
        _ => format!("https://{host}"),
    };
    Some((web, host.to_lowercase(), path.to_owned()))
}

fn guess(host: &str) -> Option<Kind> {
    if host == "github.com" || host.ends_with(".ghe.com") {
        Some(Kind::GitHub)
    } else if host.contains("gitlab") {
        Some(Kind::GitLab)
    } else {
        None
    }
}

/// 认不出的自建主机：GitLab 的 /api/v4/version 未登录时回 401 加 JSON；GitHub Enterprise 的 /api/v3/meta 公开可读
fn probe(web: &str) -> Option<Kind> {
    let json = |body: &str| serde_json::from_str::<Value>(body).ok();
    if let Ok((code, body)) = curl("GET", &format!("{web}/api/v4/version"), &[], None) {
        if matches!(code, 200 | 401) && json(&body).is_some_and(|v| v.get("message").or(v.get("version")).is_some()) {
            return Some(Kind::GitLab);
        }
    }
    match curl("GET", &format!("{web}/api/v3/meta"), &[], None) {
        Ok((200, body)) if json(&body).is_some_and(|v| v.get("installed_version").is_some()) => Some(Kind::GitHub),
        _ => None,
    }
}

/// 仓库设置里手动指定的（平台 github / gitlab, 网页根地址），空表示按远程地址自动识别
pub fn settings(repo: &Repo) -> (String, String) {
    let get = |key: &str| repo.git(&["config", "--local", "--get", key]).map(|o| decode(&o).trim().to_owned()).unwrap_or_default();
    (get("pushright.forge"), get("pushright.forgeUrl"))
}

pub fn set_settings(repo: &Repo, kind: &str, web: &str) -> Result<()> {
    for (key, value) in [("pushright.forge", kind), ("pushright.forgeUrl", web.trim().trim_end_matches('/'))] {
        if value.is_empty() {
            let _ = repo.git(&["config", "--local", "--unset-all", key]);
        } else {
            repo.git(&["config", "--local", key, value])?;
        }
    }
    Ok(())
}

/// 当前分支跟踪的远程（没有就用 origin，再没有用第一个）对应的平台项目。网页和远程不在同一个主机端口上时
/// （如 SSH 走 22222、网页是 http://host:8888），用仓库设置里填的网页根地址
pub fn detect(repo: &Repo) -> Result<Forge> {
    let config = |key: &str| repo.git(&["config", "--get", key]).map(|o| decode(&o).trim().to_owned()).ok().filter(|s| !s.is_empty());
    let remotes = crate::refs::remotes(repo)?;
    let head = decode(&repo.git(&["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default()).trim().to_owned();
    let remote = config(&format!("branch.{head}.remote"))
        .filter(|r| remotes.contains(r))
        .or_else(|| remotes.iter().find(|r| *r == "origin").cloned())
        .or_else(|| remotes.first().cloned())
        .ok_or("PR_FORGE_NO_REMOTE")?;
    // get-url 会套用 url.*.insteadOf 改写
    let url = decode(&repo.git(&["remote", "get-url", "--", &remote])?).trim().to_owned();
    let (web, host, project) = parse_url(&url).ok_or_else(|| format!("PR_FORGE_UNKNOWN: {url}"))?;
    let (kind, custom) = settings(repo);
    let web = if custom.is_empty() { web } else { custom };
    let kind = match kind.as_str() {
        "github" => Kind::GitHub,
        "gitlab" => Kind::GitLab,
        _ => guess(&host).or_else(|| probe(&web)).ok_or_else(|| format!("PR_FORGE_UNKNOWN: {host}"))?,
    };
    let api = match kind {
        Kind::GitHub if host == "github.com" => "https://api.github.com".to_owned(),
        Kind::GitHub if host.ends_with(".ghe.com") => format!("https://api.{host}"),
        Kind::GitHub => format!("{web}/api/v3"),
        Kind::GitLab => format!("{web}/api/v4"),
    };
    Ok(Forge { kind, host, web, api, project, remote })
}

/// 别的工具里现成的令牌，按顺序：环境变量、gh 命令行、git 凭据。用之前先拿 `user` 验证，通不过就试下一个：
/// GCM 给 GitLab 存的令牌通常只有读写仓库的权限，调不了 API
pub fn found_tokens(repo: &Repo, f: &Forge) -> Vec<String> {
    let env = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let mut out: Vec<String> = Vec::new();
    match f.kind {
        Kind::GitHub => {
            let keys = if f.host == "github.com" || f.host.ends_with(".ghe.com") { ["GH_TOKEN", "GITHUB_TOKEN"] } else { ["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"] };
            out.extend(keys.iter().filter_map(|k| env(k)));
            out.extend(run("gh", &["auth", "token", "--hostname", &f.host]));
        }
        // GITLAB_TOKEN 不带主机名，GITLAB_HOST 指向别的服务器时不用
        Kind::GitLab if env("GITLAB_HOST").is_none_or(|h| h.contains(&f.host)) => out.extend(env("GITLAB_TOKEN")),
        Kind::GitLab => {}
    }
    let input = format!("url={}\n\n", f.web);
    // 不许弹登录窗口：没存过凭据就当没有
    if let Ok(o) = git_at(&repo.path, &["-c", "credential.interactive=never", "credential", "fill"], input.as_bytes()) {
        out.extend(decode(&o).lines().find_map(|l| l.strip_prefix("password=")).map(str::to_owned).filter(|p| !p.is_empty()));
    }
    out.dedup();
    out
}

/// 跑一个外部命令取一行输出；命令不存在或失败时为 None
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    Some(decode(&out.stdout).trim().to_owned()).filter(|s| !s.is_empty())
}

/// 发一个请求，返回（状态码, 响应体）。地址、请求头和请求体都写进 curl 从 stdin 读的配置
fn curl(method: &str, url: &str, headers: &[String], body: Option<&Value>) -> Result<(u16, String)> {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut config = format!("url = {}\nrequest = {}\nheader = \"User-Agent: PushRight\"\n", quote(url), quote(method));
    for h in headers {
        config += &format!("header = {}\n", quote(h));
    }
    if let Some(b) = body {
        config += &format!("header = \"Content-Type: application/json\"\ndata-binary = {}\n", quote(&b.to_string()));
    }
    let mut cmd = Command::new("curl");
    cmd.args(["--silent", "--show-error", "--connect-timeout", "10", "--max-time", "30", "--config", "-", "--write-out", "\n%{http_code}"]);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| format!("PR_FORGE_CURL: {e}"))?;
    let _ = child.stdin.take().unwrap().write_all(config.as_bytes());
    let out = child.wait_with_output().map_err(err)?;
    if !out.status.success() {
        return Err(format!("PR_FORGE_NETWORK: {}", decode(&out.stderr).trim()));
    }
    let text = decode(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    Ok((code.trim().parse().map_err(err)?, body.to_owned()))
}

fn call(f: &Forge, token: &str, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
    let headers = match f.kind {
        Kind::GitHub => vec![format!("Authorization: Bearer {token}"), "Accept: application/vnd.github+json".into(), "X-GitHub-Api-Version: 2022-11-28".into()],
        Kind::GitLab => vec![format!("PRIVATE-TOKEN: {token}")],
    };
    let (code, text) = curl(method, &format!("{}{path}", f.api), &headers, body.as_ref())?;
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if code == 401 {
        return Err("PR_FORGE_AUTH".into());
    }
    if code >= 400 {
        // GitHub 的说明在 message 里；GitLab 是 message 或 error，message 还可能是对象
        let msg = value.get("message").or(value.get("error")).map(|m| m.as_str().map_or_else(|| m.to_string(), str::to_owned)).unwrap_or_else(|| text.chars().take(200).collect());
        return Err(format!("PR_FORGE_HTTP: {code} {msg}"));
    }
    Ok(value)
}

/// GitLab 的项目 id 可以用 URL 编码后的完整路径
fn encoded(project: &str) -> String {
    project.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn path(f: &Forge, rest: &str) -> String {
    match f.kind {
        Kind::GitHub => format!("/repos/{}{rest}", f.project),
        Kind::GitLab => format!("/projects/{}{rest}", encoded(&f.project)),
    }
}

fn s(v: &Value, ptr: &str) -> String {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn names(v: &Value, list: &str, key: &str) -> Vec<String> {
    v.pointer(list).and_then(Value::as_array).map(|a| a.iter().map(|x| s(x, key)).filter(|n| !n.is_empty()).collect()).unwrap_or_default()
}

fn request(f: &Forge, v: &Value) -> Request {
    match f.kind {
        Kind::GitHub => Request {
            number: v["number"].as_u64().unwrap_or(0),
            title: s(v, "/title"),
            body: s(v, "/body"),
            state: if !v["merged_at"].is_null() { "merged".into() } else { s(v, "/state") },
            draft: v["draft"].as_bool().unwrap_or(false),
            author: s(v, "/user/login"),
            source: s(v, "/head/ref"),
            target: s(v, "/base/ref"),
            head: s(v, "/head/sha"),
            base: s(v, "/base/sha"),
            fork: s(v, "/head/repo/full_name") != s(v, "/base/repo/full_name"),
            reviewers: names(v, "/requested_reviewers", "/login"),
            url: s(v, "/html_url"),
            updated: s(v, "/updated_at"),
        },
        Kind::GitLab => Request {
            number: v["iid"].as_u64().unwrap_or(0),
            title: s(v, "/title"),
            body: s(v, "/description"),
            state: match v["state"].as_str() {
                Some("opened") => "open".into(),
                Some("merged") => "merged".into(),
                _ => "closed".into(),
            },
            draft: v["draft"].as_bool().or(v["work_in_progress"].as_bool()).unwrap_or(false),
            author: s(v, "/author/username"),
            source: s(v, "/source_branch"),
            target: s(v, "/target_branch"),
            head: s(v, "/sha"),
            base: s(v, "/diff_refs/base_sha"),
            fork: v["source_project_id"] != v["target_project_id"],
            reviewers: names(v, "/reviewers", "/username"),
            url: s(v, "/web_url"),
            updated: s(v, "/updated_at"),
        },
    }
}

/// 令牌对应的用户名，顺带验证令牌
pub fn user(f: &Forge, token: &str) -> Result<String> {
    let v = call(f, token, "GET", "/user", None)?;
    Ok(s(&v, if f.kind == Kind::GitHub { "/login" } else { "/username" }))
}

/// 最近更新的 100 个；`open_only` 为 false 时含已关闭和已合并的
pub fn list(f: &Forge, token: &str, open_only: bool) -> Result<Vec<Request>> {
    let query = match (f.kind, open_only) {
        (Kind::GitHub, open) => format!("/pulls?state={}&sort=updated&direction=desc&per_page=100", if open { "open" } else { "all" }),
        (Kind::GitLab, open) => format!("/merge_requests?state={}&order_by=updated_at&per_page=100", if open { "opened" } else { "all" }),
    };
    let v = call(f, token, "GET", &path(f, &query), None)?;
    Ok(v.as_array().map(|a| a.iter().map(|x| request(f, x)).collect()).unwrap_or_default())
}

pub fn project(f: &Forge, token: &str) -> Result<Project> {
    let v = call(f, token, "GET", &path(f, ""), None)?;
    Ok(Project {
        default_branch: s(&v, "/default_branch"),
        remove_source: v["remove_source_branch_after_merge"].as_bool().unwrap_or(false),
        squash: matches!(v["squash_option"].as_str(), Some("always" | "default_on")),
    })
}

pub fn status(f: &Forge, token: &str, r: &Request) -> Result<Status> {
    let n = r.number;
    match f.kind {
        Kind::GitHub => {
            let pr = call(f, token, "GET", &path(f, &format!("/pulls/{n}")), None)?;
            let repo = call(f, token, "GET", &path(f, ""), None)?;
            // 每人只算最后一次表态（批准或要求修改）
            let reviews = call(f, token, "GET", &path(f, &format!("/pulls/{n}/reviews?per_page=100")), None)?;
            let mut last: Vec<(String, String)> = Vec::new();
            for rv in reviews.as_array().into_iter().flatten() {
                let (who, state) = (s(rv, "/user/login"), s(rv, "/state"));
                if matches!(state.as_str(), "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED") {
                    last.retain(|(w, _)| *w != who);
                    last.push((who, state));
                }
            }
            let who = |state: &str| last.iter().filter(|(_, s)| s == state).map(|(w, _)| w.clone()).collect();
            let head = s(&pr, "/head/sha");
            let mut checks = Vec::new();
            let runs = call(f, token, "GET", &path(f, &format!("/commits/{head}/check-runs?per_page=100")), None)?;
            for c in runs["check_runs"].as_array().into_iter().flatten() {
                let state = match (c["status"].as_str(), c["conclusion"].as_str()) {
                    (Some("completed"), Some("success")) => "success",
                    (Some("completed"), Some("failure" | "timed_out" | "action_required" | "startup_failure")) => "failure",
                    (Some("completed"), _) => "neutral",
                    _ => "pending",
                };
                checks.push(Check { name: s(c, "/name"), state: state.into(), url: s(c, "/html_url") });
            }
            let statuses = call(f, token, "GET", &path(f, &format!("/commits/{head}/status")), None)?;
            for c in statuses["statuses"].as_array().into_iter().flatten() {
                let state = match c["state"].as_str() {
                    Some("success") => "success",
                    Some("pending") => "pending",
                    _ => "failure",
                };
                checks.push(Check { name: s(c, "/context"), state: state.into(), url: s(c, "/target_url") });
            }
            let methods = [("merge", "allow_merge_commit"), ("squash", "allow_squash_merge"), ("rebase", "allow_rebase_merge")];
            Ok(Status {
                checks,
                approved: who("APPROVED"),
                changes_requested: who("CHANGES_REQUESTED"),
                mergeable: s(&pr, "/mergeable_state"),
                methods: methods.iter().filter(|(_, k)| repo[*k].as_bool().unwrap_or(true)).map(|(m, _)| (*m).to_owned()).collect(),
                squash: false,
                remove_source: repo["delete_branch_on_merge"].as_bool().unwrap_or(false),
                base: s(&pr, "/base/sha"),
                start: String::new(),
            })
        }
        Kind::GitLab => {
            let mr = call(f, token, "GET", &path(f, &format!("/merge_requests/{n}")), None)?;
            let approvals = call(f, token, "GET", &path(f, &format!("/merge_requests/{n}/approvals")), None)?;
            let mut checks = Vec::new();
            if let Some(id) = mr["head_pipeline"]["id"].as_u64() {
                let jobs = call(f, token, "GET", &path(f, &format!("/pipelines/{id}/jobs?per_page=100")), None)?;
                for j in jobs.as_array().into_iter().flatten() {
                    let state = match j["status"].as_str() {
                        Some("success") => "success",
                        Some("failed") if j["allow_failure"].as_bool() == Some(true) => "neutral",
                        Some("failed") => "failure",
                        Some("canceled" | "skipped" | "manual") => "neutral",
                        _ => "pending",
                    };
                    checks.push(Check { name: s(j, "/name"), state: state.into(), url: s(j, "/web_url") });
                }
            }
            let mergeable = s(&mr, "/detailed_merge_status");
            Ok(Status {
                checks,
                approved: names(&approvals, "/approved_by", "/user/username"),
                changes_requested: Vec::new(),
                mergeable: if mergeable.is_empty() { s(&mr, "/merge_status") } else { mergeable },
                methods: vec!["merge".into()],
                squash: mr["squash"].as_bool().unwrap_or(false),
                remove_source: mr["force_remove_source_branch"].as_bool().unwrap_or(false),
                base: s(&mr, "/diff_refs/base_sha"),
                start: s(&mr, "/diff_refs/start_sha"),
            })
        }
    }
}

pub fn create(f: &Forge, token: &str, n: &NewRequest) -> Result<Request> {
    let v = match f.kind {
        Kind::GitHub => call(f, token, "POST", &path(f, "/pulls"), Some(json!({ "title": n.title, "head": n.source, "base": n.target, "body": n.body, "draft": n.draft })))?,
        // GitLab 没有草稿参数，标题以 Draft: 开头就是草稿
        Kind::GitLab => {
            let title = if n.draft { format!("Draft: {}", n.title) } else { n.title.clone() };
            call(f, token, "POST", &path(f, "/merge_requests"), Some(json!({ "source_branch": n.source, "target_branch": n.target, "title": title, "description": n.body, "remove_source_branch": n.remove_source, "squash": n.squash, "assignee_ids": ids(n.assignee), "reviewer_ids": ids(n.reviewer) })))?
        }
    };
    Ok(request(f, &v))
}

/// GitLab 的用户 id 列表参数，0 是一个也不要
fn ids(id: u64) -> Vec<u64> {
    (id > 0).then_some(id).into_iter().collect()
}

/// 能访问 GitLab 项目的活跃用户，含从群组继承来的；一页最多 100 个，取到最后一页
pub fn members(f: &Forge, token: &str) -> Result<Vec<Member>> {
    let mut all = Vec::new();
    for page in 1.. {
        let v = call(f, token, "GET", &path(f, &format!("/users?per_page=100&page={page}")), None)?;
        let a = v.as_array().map(Vec::as_slice).unwrap_or_default();
        all.extend(a.iter().filter(|x| x["state"] == "active").map(|x| Member { id: x["id"].as_u64().unwrap_or(0), username: s(x, "/username"), name: s(x, "/name") }));
        if a.len() < 100 {
            break;
        }
    }
    Ok(all)
}

/// 换掉 GitLab 合并请求的审核者，0 是去掉。没有权限时服务器不报错、原样返回，所以对比返回的审核者
pub fn set_reviewer(f: &Forge, token: &str, n: u64, reviewer: u64) -> Result<()> {
    let v = call(f, token, "PUT", &path(f, &format!("/merge_requests/{n}")), Some(json!({ "reviewer_ids": ids(reviewer) })))?;
    let now: Vec<u64> = v["reviewers"].as_array().map(|a| a.iter().filter_map(|x| x["id"].as_u64()).collect()).unwrap_or_default();
    if now != ids(reviewer) {
        return Err("PR_FORGE_REVIEWER".into());
    }
    Ok(())
}

/// 带上看到的源分支提交：这期间有人推了新提交时服务器拒绝合并，不会合进没看过的代码
pub fn merge(f: &Forge, token: &str, r: &Request, method: &str, squash: bool, remove_source: bool) -> Result<()> {
    let n = r.number;
    match f.kind {
        Kind::GitHub => call(f, token, "PUT", &path(f, &format!("/pulls/{n}/merge")), Some(json!({ "merge_method": method, "sha": r.head })))?,
        Kind::GitLab => call(f, token, "PUT", &path(f, &format!("/merge_requests/{n}/merge")), Some(json!({ "sha": r.head, "squash": squash, "should_remove_source_branch": remove_source })))?,
    };
    Ok(())
}

pub fn approve(f: &Forge, token: &str, n: u64) -> Result<()> {
    match f.kind {
        Kind::GitHub => call(f, token, "POST", &path(f, &format!("/pulls/{n}/reviews")), Some(json!({ "event": "APPROVE" })))?,
        Kind::GitLab => call(f, token, "POST", &path(f, &format!("/merge_requests/{n}/approve")), None)?,
    };
    Ok(())
}

pub fn close(f: &Forge, token: &str, n: u64) -> Result<()> {
    match f.kind {
        Kind::GitHub => call(f, token, "PATCH", &path(f, &format!("/pulls/{n}")), Some(json!({ "state": "closed" })))?,
        Kind::GitLab => call(f, token, "PUT", &path(f, &format!("/merge_requests/{n}")), Some(json!({ "state_event": "close" })))?,
    };
    Ok(())
}

/// 行评论，最多取 100 条（GitLab 是 100 个讨论）
pub fn notes(f: &Forge, token: &str, r: &Request) -> Result<Vec<Note>> {
    let n = r.number;
    let mut out = Vec::new();
    match f.kind {
        Kind::GitHub => {
            let v = call(f, token, "GET", &path(f, &format!("/pulls/{n}/comments?per_page=100")), None)?;
            for c in v.as_array().into_iter().flatten() {
                // 已过时的没有 line
                let Some(line) = c["line"].as_u64() else { continue };
                out.push(Note { path: s(c, "/path"), old: s(c, "/side") == "LEFT", line: line as u32, author: s(c, "/user/login"), body: s(c, "/body"), created: s(c, "/created_at") });
            }
        }
        Kind::GitLab => {
            let v = call(f, token, "GET", &path(f, &format!("/merge_requests/{n}/discussions?per_page=100")), None)?;
            for d in v.as_array().into_iter().flatten() {
                let notes = d["notes"].as_array().map(Vec::as_slice).unwrap_or_default();
                let Some(p) = notes.first().map(|x| &x["position"]) else { continue };
                // 还对得上的评论服务器会挪到最新版本上，对不上的留在旧版本，算已过时
                if p["position_type"] != "text" || s(p, "/head_sha") != r.head {
                    continue;
                }
                let (old, line) = match (p["new_line"].as_u64(), p["old_line"].as_u64()) {
                    (Some(l), _) => (false, l),
                    (None, Some(l)) => (true, l),
                    _ => continue,
                };
                for x in notes.iter().filter(|x| x["system"] != true) {
                    out.push(Note { path: s(p, "/new_path"), old, line: line as u32, author: s(x, "/author/username"), body: s(x, "/body"), created: s(x, "/created_at") });
                }
            }
        }
    }
    Ok(out)
}

/// 区块里第 i 行的位置。另一侧照 GitLab 解析 diff 时的计数：前面最近一行的行号 + 1，前面没有取后面最近的，都没有是 0。
/// 上下文为 0 行时，纯新增 / 纯删除区块的另一侧会和服务器算的不同，只影响 GitLab 多行范围的标记，评论照常发出
pub fn spot(lines: &[Line], i: usize) -> Spot {
    let at = |no: fn(&Line) -> Option<u32>| no(&lines[i]).or_else(|| lines[..i].iter().rev().find_map(no).map(|n| n + 1)).or_else(|| lines[i..].iter().find_map(no)).unwrap_or(0);
    Spot { kind: lines[i].kind, old: at(|l| l.old_no), new: at(|l| l.new_no) }
}

/// 发一条行评论，立即可见
pub fn comment(f: &Forge, token: &str, r: &Request, st: &Status, a: &Anchor, body: &str) -> Result<()> {
    let n = r.number;
    match f.kind {
        Kind::GitHub => {
            // 删除行在旧文件（LEFT），其余在新文件（RIGHT）
            let side = |p: &Spot| if p.kind == '-' { ("LEFT", p.old) } else { ("RIGHT", p.new) };
            let (to_side, to_line) = side(&a.to);
            let mut b = json!({ "body": body, "commit_id": r.head, "path": a.path, "side": to_side, "line": to_line });
            if a.from != a.to {
                let (from_side, from_line) = side(&a.from);
                b["start_side"] = from_side.into();
                b["start_line"] = from_line.into();
            }
            call(f, token, "POST", &path(f, &format!("/pulls/{n}/comments")), Some(b))?
        }
        Kind::GitLab => {
            // 新增行只给新行号，删除行只给旧行号，上下文行两个都给
            let old_line = |p: &Spot| (p.kind != '+').then_some(p.old);
            let new_line = |p: &Spot| (p.kind != '-').then_some(p.new);
            let mut position = json!({ "position_type": "text", "base_sha": st.base, "start_sha": st.start, "head_sha": r.head, "old_path": a.old_path, "new_path": a.path, "old_line": old_line(&a.to), "new_line": new_line(&a.to) });
            if a.from != a.to {
                // line_code 是「路径的 SHA-1_旧位置_新位置」
                let mut h = gix::hash::hasher(gix::hash::Kind::Sha1);
                h.update(a.path.as_bytes());
                let file = h.try_finalize().map(|id| id.to_string()).unwrap_or_default();
                let end = |p: &Spot| json!({ "line_code": format!("{file}_{}_{}", p.old, p.new), "type": if p.kind == '+' { "new" } else { "old" }, "old_line": old_line(p), "new_line": new_line(p) });
                position["line_range"] = json!({ "start": end(&a.from), "end": end(&a.to) });
            }
            call(f, token, "POST", &path(f, &format!("/merge_requests/{n}/discussions")), Some(json!({ "body": body, "position": position })))?
        }
    };
    Ok(())
}

/// 服务器上 PR 头部的引用：fork 来的 PR 只能从这里取
pub fn head_ref(f: &Forge, n: u64) -> String {
    match f.kind {
        Kind::GitHub => format!("pull/{n}/head"),
        Kind::GitLab => format!("merge-requests/{n}/head"),
    }
}

/// 本地缺 PR 的提交时从远程取（不建分支，只要对象），返回对比用的（起点, 终点）
pub fn fetch(repo: &Repo, f: &Forge, r: &Request, base: &str) -> Result<(String, String)> {
    let has = |id: &str| !id.is_empty() && repo.git(&["cat-file", "-e", &format!("{id}^{{commit}}")]).is_ok();
    if !has(&r.head) || !has(base) {
        let target = format!("refs/heads/{}", r.target);
        repo.git(&["-c", "credential.interactive=never", "fetch", "--no-tags", "--no-write-fetch-head", "--", &f.remote, &head_ref(f, r.number), &target])?;
    }
    let base = if has(base) { base.to_owned() } else { format!("refs/remotes/{}/{}", f.remote, r.target) };
    Ok((base, r.head.clone()))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Commit {
    pub id: String,
    pub author: String,
    pub subject: String,
}

/// 从合并基础到 PR 头部的提交，新的在前，最多 250 个（和 GitHub 网页一致）
pub fn commits(repo: &Repo, base: &str, head: &str) -> Result<Vec<Commit>> {
    let out = repo.git(&["log", "-n", "250", "--format=%H%x00%an%x00%s", &format!("{base}..{head}"), "--"])?;
    Ok(decode(&out)
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '\0');
            Some(Commit { id: it.next()?.to_owned(), author: it.next()?.to_owned(), subject: it.next().unwrap_or_default().to_owned() })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_urls() {
        let p = |u: &str| parse_url(u).map(|(w, h, p)| format!("{w} {h} {p}"));
        assert_eq!(p("https://github.com/owner/repo.git").as_deref(), Some("https://github.com github.com owner/repo"));
        assert_eq!(p("git@gitlab.example.com:group/sub/proj.git").as_deref(), Some("https://gitlab.example.com gitlab.example.com group/sub/proj"));
        assert_eq!(p("ssh://git@gitlab.example.com:2222/group/proj.git").as_deref(), Some("https://gitlab.example.com gitlab.example.com group/proj"));
        assert_eq!(p("https://user:pw@git.example.com:8443/team/proj/").as_deref(), Some("https://git.example.com:8443 git.example.com team/proj"));
        assert_eq!(p("http://Host/a/b").as_deref(), Some("http://Host host a/b"));
        assert_eq!(p("C:\\repos\\local"), None);
        assert_eq!(p("file:///c/repos/x"), None);
        assert_eq!(p("https://github.com/onlyowner"), None);
    }

    #[test]
    fn guesses_known_hosts() {
        assert_eq!(guess("github.com"), Some(Kind::GitHub));
        assert_eq!(guess("acme.ghe.com"), Some(Kind::GitHub));
        assert_eq!(guess("gitlab.example.com"), Some(Kind::GitLab));
        assert_eq!(guess("git.example.com"), None);
        assert_eq!(encoded("group/sub proj"), "group%2Fsub%20proj");
    }

    #[test]
    fn spots_count_the_other_side_like_gitlab() {
        let l = |kind: char, old_no: Option<u32>, new_no: Option<u32>| Line { kind, text: String::new(), old_no, new_no };
        let hunk = [l(' ', Some(1), Some(1)), l('-', Some(2), None), l('+', None, Some(2)), l('+', None, Some(3)), l(' ', Some(3), Some(4))];
        let at = |i| { let p = spot(&hunk, i); (p.kind, p.old, p.new) };
        assert_eq!(at(1), ('-', 2, 2));
        assert_eq!(at(2), ('+', 3, 2));
        assert_eq!(at(3), ('+', 3, 3));
        assert_eq!(at(4), (' ', 3, 4));
        // 区块开头就是新增：取后面第一个旧行号；新文件没有旧行，是 0
        assert_eq!(spot(&[l('+', None, Some(1)), l(' ', Some(1), Some(2))], 0), Spot { kind: '+', old: 1, new: 1 });
        assert_eq!(spot(&[l('+', None, Some(1))], 0), Spot { kind: '+', old: 0, new: 1 });
    }
}
