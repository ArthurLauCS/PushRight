//! GitHub 拉取请求，通过 GitHub CLI（gh）调接口。
//! gh 没登录时改用 Git 凭据管理器里存的令牌（用 HTTPS 推送过就有），不必另外登录。

use crate::{err, Repo, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Output, Stdio};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// 源分支名（不带远程名）
    pub head: String,
    pub base: String,
    pub author: String,
    pub draft: bool,
    /// APPROVED / CHANGES_REQUESTED / REVIEW_REQUIRED，没有要求审阅时为空
    pub review: String,
    /// 检查汇总：success / failure / pending，没有检查时为空
    pub checks: String,
}

/// 拉取请求所在的 GitHub 远程
#[derive(Clone, Debug, PartialEq)]
pub struct Remote {
    /// 本地的远程名，如 origin
    pub name: String,
    pub host: String,
    /// owner/repo
    pub repo: String,
}

/// 从远程地址取出 GitHub 的主机和 owner/repo；不是 GitHub 的返回 None
fn parse_url(url: &str) -> Option<(String, String)> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
    // scp 写法 git@github.com:owner/repo 用冒号分隔主机和路径
    let (host, path) = rest.split_once(['/', ':'])?;
    // ssh://host:22/owner/repo 里的端口号
    let path = if url.contains("://") && rest.as_bytes()[host.len()] == b':' { path.split_once('/')?.1 } else { path };
    let host = host.to_lowercase();
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (host.contains("github") && parts.next().is_none() && !owner.is_empty() && !name.is_empty()).then(|| (host, format!("{owner}/{name}")))
}

/// 用哪个远程：优先 origin，否则第一个 GitHub 远程
pub fn remote(repo: &Repo) -> Result<Remote> {
    let out = repo.git(&["config", "--get-regexp", r"^remote\..*\.url$"]).unwrap_or_default();
    let mut found: Vec<Remote> = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let (key, url) = l.split_once(' ')?;
            let name = key.strip_prefix("remote.")?.strip_suffix(".url")?;
            let (host, repo) = parse_url(url)?;
            Some(Remote { name: name.to_owned(), host, repo })
        })
        .collect();
    let i = found.iter().position(|r| r.name == "origin").unwrap_or(0);
    if found.is_empty() {
        return Err("PR_NO_GITHUB".into());
    }
    Ok(found.swap_remove(i))
}

/// 凭据管理器里这台主机的令牌；不弹登录窗口，没有就返回 None
fn stored_token(repo: &Repo, host: &str) -> Option<String> {
    let mut cmd = crate::git_command(&repo.path);
    cmd.args(["credential", "fill"]).env("GCM_INTERACTIVE", "never");
    let mut child = cmd.spawn().ok()?;
    let _ = child.stdin.take()?.write_all(format!("protocol=https\nhost={host}\n\n").as_bytes());
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    out.status.success().then(|| text.lines().find_map(|l| l.strip_prefix("password=").map(str::to_owned))).flatten()
}

fn run(repo: &Repo, args: &[&str], stdin: &[u8], token: Option<&str>) -> Result<Output> {
    let mut cmd = Command::new("gh");
    cmd.current_dir(&repo.path).args(args);
    cmd.env("GH_PROMPT_DISABLED", "1").env("GH_NO_UPDATE_NOTIFIER", "1").env("NO_COLOR", "1").env("GIT_TERMINAL_PROMPT", "0");
    if let Some(token) = token {
        cmd.env("GH_TOKEN", token).env("GH_ENTERPRISE_TOKEN", token);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "PR_GH_MISSING".to_owned() } else { err(e) })?;
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().map_err(err)
}

/// 跑一条 gh 命令；gh 报需要登录（退出码 4）时用凭据管理器的令牌再试一次
fn gh(repo: &Repo, remote: &Remote, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>> {
    let slug = format!("{}/{}", remote.host, remote.repo);
    let args: Vec<&str> = args.iter().copied().chain(["-R", &slug]).collect();
    let mut out = run(repo, &args, stdin, None)?;
    if out.status.code() == Some(4) {
        let Some(token) = stored_token(repo, &remote.host) else { return Err("PR_GH_AUTH".into()) };
        out = run(repo, &args, stdin, Some(&token))?;
        if out.status.code() == Some(4) {
            return Err("PR_GH_AUTH".into());
        }
    }
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPr {
    number: u64,
    title: String,
    url: String,
    head_ref_name: String,
    base_ref_name: String,
    author: Option<Login>,
    is_draft: bool,
    review_decision: Option<String>,
    status_check_rollup: Option<Vec<Check>>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

/// CheckRun 有 status / conclusion，老式的 StatusContext 只有 state
#[derive(Deserialize)]
struct Check {
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
}

fn summarize(checks: &[Check]) -> String {
    let failed = |c: &Check| matches!(c.conclusion.as_deref().or(c.state.as_deref()), Some("FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE"));
    let pending = |c: &Check| matches!(c.state.as_deref(), Some("PENDING" | "EXPECTED")) || c.status.as_deref().is_some_and(|s| s != "COMPLETED");
    if checks.is_empty() {
        ""
    } else if checks.iter().any(failed) {
        "failure"
    } else if checks.iter().any(pending) {
        "pending"
    } else {
        "success"
    }
    .to_owned()
}

fn parse_list(json: &[u8]) -> Result<Vec<Pr>> {
    let raw: Vec<RawPr> = serde_json::from_slice(json).map_err(err)?;
    Ok(raw
        .into_iter()
        .map(|r| Pr {
            checks: summarize(r.status_check_rollup.as_deref().unwrap_or_default()),
            number: r.number,
            title: r.title,
            url: r.url,
            head: r.head_ref_name,
            base: r.base_ref_name,
            author: r.author.map(|a| a.login).unwrap_or_default(),
            draft: r.is_draft,
            review: r.review_decision.unwrap_or_default(),
        })
        .collect())
}

/// 新建拉取请求时预填的标题：分支最新提交的标题
pub fn default_title(repo: &Repo, branch: &str) -> String {
    repo.git(&["log", "-1", "--format=%s", &format!("refs/heads/{branch}"), "--"]).map(|o| String::from_utf8_lossy(&o).trim().to_owned()).unwrap_or_default()
}

/// 打开着的拉取请求，最近更新的在前
pub fn list(repo: &Repo, remote: &Remote) -> Result<Vec<Pr>> {
    let out = gh(repo, remote, &["pr", "list", "--state", "open", "--limit", "100", "--json", "number,title,url,headRefName,baseRefName,author,isDraft,reviewDecision,statusCheckRollup"], &[])?;
    parse_list(&out)
}

/// 新建拉取请求，返回它的网址。`head` 要先推送到远程
pub fn create(repo: &Repo, remote: &Remote, base: &str, head: &str, title: &str, body: &str, draft: bool) -> Result<String> {
    let mut args = vec!["pr", "create", "--base", base, "--head", head, "--title", title, "--body-file", "-"];
    if draft {
        args.push("--draft");
    }
    let out = gh(repo, remote, &args, body.as_bytes())?;
    Ok(String::from_utf8_lossy(&out).lines().rfind(|l| l.starts_with("http")).unwrap_or_default().to_owned())
}

/// 把拉取请求的分支检出到本地
pub fn checkout(repo: &Repo, remote: &Remote, number: u64) -> Result<()> {
    gh(repo, remote, &["pr", "checkout", &number.to_string()], &[]).map(drop)
}

/// 在 GitHub 上合并。`method`：merge / squash / rebase
pub fn merge(repo: &Repo, remote: &Remote, number: u64, method: &str) -> Result<()> {
    if !matches!(method, "merge" | "squash" | "rebase") {
        return Err("PR_INVALID_NAME".into());
    }
    gh(repo, remote, &["pr", "merge", &number.to_string(), &format!("--{method}")], &[]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_urls() {
        let gh = |h: &str, r: &str| Some((h.to_owned(), r.to_owned()));
        assert_eq!(parse_url("https://github.com/ArthurLauCS/PushRight.git"), gh("github.com", "ArthurLauCS/PushRight"));
        assert_eq!(parse_url("https://user@github.com/a/b"), gh("github.com", "a/b"));
        assert_eq!(parse_url("git@github.com:a/b.git"), gh("github.com", "a/b"));
        assert_eq!(parse_url("ssh://git@github.com:22/a/b.git"), gh("github.com", "a/b"));
        assert_eq!(parse_url("https://github.example.com/a/b/"), gh("github.example.com", "a/b"));
        assert_eq!(parse_url("ssh://git@gitlab.wuxusgames.com/a/b.git"), None);
        assert_eq!(parse_url("C:/repos/b"), None);
    }

    #[test]
    fn summarizes_checks() {
        let json = br#"[{"number":3,"title":"T","url":"https://github.com/a/b/pull/3","headRefName":"feat","baseRefName":"main","author":{"login":"me"},"isDraft":true,"reviewDecision":"","statusCheckRollup":[{"__typename":"CheckRun","status":"COMPLETED","conclusion":"SUCCESS"},{"__typename":"StatusContext","state":"PENDING"}]},
            {"number":4,"title":"U","url":"u","headRefName":"x","baseRefName":"main","author":null,"isDraft":false,"reviewDecision":"APPROVED","statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"},{"status":"IN_PROGRESS","conclusion":""}]},
            {"number":5,"title":"V","url":"v","headRefName":"y","baseRefName":"main","author":{"login":"o"},"isDraft":false,"reviewDecision":null,"statusCheckRollup":[{"status":"COMPLETED","conclusion":"SKIPPED"}]},
            {"number":6,"title":"W","url":"w","headRefName":"z","baseRefName":"main","author":{"login":"o"},"isDraft":false,"reviewDecision":null,"statusCheckRollup":[]}]"#;
        let list = parse_list(json).unwrap();
        assert_eq!(list.iter().map(|p| p.checks.as_str()).collect::<Vec<_>>(), ["pending", "failure", "success", ""]);
        assert_eq!((list[0].author.as_str(), list[0].draft, list[0].head.as_str()), ("me", true, "feat"));
        assert_eq!((list[1].author.as_str(), list[1].review.as_str()), ("", "APPROVED"));
    }
}
