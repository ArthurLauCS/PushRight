//! 本地私用文件：忽略规则不等于停止跟踪，写操作分别处理。
use crate::{err, Repo, Result};
use std::io::Write;
use std::path::{Component, Path};

fn path_in_repo<'a>(repo: &Repo, path: &'a str) -> Result<&'a str> {
    if path.is_empty() || path.contains(['\0', '\n', '\r', '\\']) || Path::new(path).components().any(|c| !matches!(c, Component::Normal(_)))
        || path.split('/').any(|p| p.eq_ignore_ascii_case(".git")) {
        return Err("PR_INVALID_PATH".into());
    }
    // 文件本身可以是符号链接；父目录不能把写入或跟踪引到仓库外。
    let parent = repo.path.join(path).parent().unwrap().canonicalize().map_err(err)?;
    if !parent.starts_with(repo.path.canonicalize().map_err(err)?) { return Err("PR_INVALID_PATH".into()); }
    Ok(path)
}

fn paths(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes).split('\0').filter(|p| !p.is_empty()).map(str::to_owned).collect()
}

/// 根层由 Git 折叠完整忽略目录；只在用户展开某目录时读取它的直接子项。
pub fn ignored(repo: &Repo, path: &str) -> Result<Vec<String>> {
    if path.is_empty() {
        return Ok(paths(&repo.git(&["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "--no-empty-directory", "-z"])?));
    }
    let path = path_in_repo(repo, path.trim_end_matches('/'))?;
    if repo.path.join(path).symlink_metadata().map_err(err)?.file_type().is_symlink() { return Ok(vec![]); }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(repo.path.join(path)).map_err(err)? {
        let entry = entry.map_err(err)?;
        if entry.file_name() == ".git" { continue; }
        let suffix = if entry.file_type().map_err(err)?.is_dir() { "/" } else { "" };
        entries.push(format!("{path}/{}{suffix}", entry.file_name().to_string_lossy()));
    }
    if entries.is_empty() { return Ok(entries); }
    // check-ignore 用退出码 1 表示没有匹配；这里用 -v -n，让每个输入都有结果。
    let input = format!("{}\0", entries.join("\0"));
    let out = repo.git_in(&["check-ignore", "--stdin", "-z", "-v", "-n"], input.as_bytes());
    match out {
        Ok(out) => Ok(String::from_utf8_lossy(&out).split('\0').collect::<Vec<_>>().chunks_exact(4)
            .filter(|r| !r[2].is_empty() && !r[2].starts_with('!')).map(|r| r[3].to_owned()).collect()),
        Err(e) if e.is_empty() => Ok(vec![]),
        Err(e) => Err(e),
    }
}

pub fn tracked(repo: &Repo, path: &str) -> Result<bool> {
    let path = path_in_repo(repo, path)?;
    Ok(!repo.git(&["--literal-pathspecs", "ls-files", "-z", "--", path])?.is_empty())
}

pub fn ignore_file(repo: &Repo, path: &str, shared: bool) -> Result<()> {
    let path = path_in_repo(repo, path)?;
    if tracked(repo, path)? { return Err("PR_IGNORE_TRACKED".into()); }
    let target = if shared { repo.path.join(".gitignore") } else {
        let out = repo.git(&["rev-parse", "--git-path", "info/exclude"])?;
        repo.path.join(String::from_utf8_lossy(&out).trim())
    };
    if target.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) { return Err("PR_INVALID_PATH".into()); }
    let mut pattern = String::from("/");
    for ch in path.chars() {
        if "*?[]\\ ".contains(ch) { pattern.push('\\'); }
        pattern.push(ch);
    }
    if repo.path.join(path).is_dir() { pattern.push('/'); }
    let old = match std::fs::read(&target) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(err(e)),
    };
    if String::from_utf8_lossy(&old).lines().any(|l| l == pattern) { return Ok(()); }
    std::fs::create_dir_all(target.parent().unwrap()).map_err(err)?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(target).map_err(err)?;
    if !old.is_empty() && !old.ends_with(b"\n") { file.write_all(b"\n").map_err(err)?; }
    writeln!(file, "{pattern}").map_err(err)
}

pub fn track(repo: &Repo, path: &str, track: bool) -> Result<()> {
    let path = path_in_repo(repo, path)?;
    if repo.path.join(path).is_dir() { return Err("PR_EXPECTED_FILE".into()); }
    if track {
        repo.git(&["--literal-pathspecs", "add", "-f", "--", path]).map(drop)
    } else {
        // 不用 -f：暂存区已有独立修改时必须拒绝，不能丢掉用户的暂存内容。
        repo.git(&["--literal-pathspecs", "rm", "--cached", "--", path])?;
        ignore_file(repo, path, false)
    }
}

/// Gitlink（含嵌套子模块）和未跟踪的嵌套仓库；普通新文件不需要扫描目录树。
pub fn repositories(repo: &Repo) -> Result<Vec<String>> {
    // 两个 git 进程并行跑
    let (staged, others) = std::thread::scope(|s| {
        let others = s.spawn(|| repo.git(&["ls-files", "--others", "--exclude-standard", "-z"]));
        (repo.git(&["ls-files", "--stage", "-z"]), others.join().unwrap())
    });
    let mut children = paths(&staged?).into_iter().filter_map(|r| r.strip_prefix("160000 ").and_then(|r| r.split_once('\t')).map(|(_, p)| p.to_owned())).collect::<Vec<_>>();
    children.extend(paths(&others?).into_iter().filter(|p| p.ends_with('/')));
    children.retain(|p| repo.path.join(p).join(".git").exists());
    children.sort();
    children.dedup();
    Ok(children.into_iter().map(|p| repo.path.join(p.trim_end_matches('/')).to_string_lossy().into_owned()).collect())
}

/// 在资源管理器里定位这个文件（`open` 为 true 时改为用默认程序打开）。文件已删除时定位到它所在的文件夹。
pub fn reveal(repo: &Repo, path: &str, open: bool) -> Result<()> {
    let path = path_in_repo(repo, path.trim_end_matches('/'))?;
    let full = repo.path.join(path);
    let exists = full.exists();
    if open && !exists { return Err("PR_INVALID_PATH".into()); }
    let target = if exists { full.as_path() } else { full.parent().unwrap() };
    let mut cmd = std::process::Command::new(if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
    if cfg!(windows) && exists && !open {
        // explorer 只认反斜杠路径；「/select,」和路径必须是同一个参数
        cmd.arg(format!("/select,{}", target.display().to_string().replace('/', "\\")));
    } else if cfg!(target_os = "macos") && exists && !open {
        cmd.arg("-R").arg(target);
    } else {
        cmd.arg(target);
    }
    // explorer 成功时退出码也是 1，只看能不能启动
    cmd.spawn().map(drop).map_err(err)
}

/// 单个文件的补丁文本，「复制为补丁」用。`mode`：unstaged / staged / commit（此时 `id` 是提交）。
pub fn patch(repo: &Repo, mode: &str, id: &str, path: &str) -> Result<String> {
    const LIMIT: usize = 4 << 20;
    let mut args = vec!["--literal-pathspecs", "-c", "core.quotepath=false"];
    match mode {
        "unstaged" => args.extend(["diff", "--no-color", "--no-ext-diff"]),
        "staged" => args.extend(["diff", "--no-color", "--no-ext-diff", "--cached"]),
        "commit" => args.extend(["show", "--format=", "--no-color", "--no-ext-diff", "--diff-merges=first-parent", crate::ops::safe(id)?]),
        _ => return Err("PR_INVALID_NAME".into()),
    }
    args.extend(["--", path]);
    let out = repo.git_limited(&args, LIMIT)?;
    if out.len() > LIMIT { return Err("PR_DIFF_TOO_LARGE".into()); }
    Ok(crate::decode(&out))
}
