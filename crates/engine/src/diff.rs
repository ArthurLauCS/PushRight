use crate::{decode, err, Repo, Result};
use gix::ObjectId;
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Read};
use base64::Engine;

// Editable patches stay bounded; larger patches use the read-only streaming pages below.
const MAX_PATCH: usize = 4 << 20;

/// 界面上的 diff 工具：上下文行数和忽略空白。缺省即 git 的默认（3 行、不忽略）。
#[derive(Deserialize, Default, Clone, Copy)]
pub struct Options {
    pub context: Option<u32>,
    #[serde(default)]
    pub ignore_ws: bool,
}

impl Options {
    fn flags(&self) -> Vec<String> {
        let mut flags: Vec<String> = self.context.iter().map(|n| format!("-U{n}")).collect();
        if self.ignore_ws {
            flags.push("-w".into());
        }
        flags
    }
}

#[derive(Serialize, Default, Clone)]
pub struct Diff {
    pub binary: bool,
    pub too_large: bool,
    pub hunks: Vec<Hunk>,
    pub images: Option<[Option<String>; 2]>,
    pub next: Option<usize>,
    pub paged: bool,
    pub clipped: bool,
}

#[derive(Serialize, Clone)]
pub struct Hunk {
    /// `@@ -a,b +c,d @@ ...` 整行，按行暂存时用来确认文件没变过
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Serialize, Clone)]
pub struct Line {
    /// ' ' 上下文，'+' 新增，'-' 删除，'\\' 文件末尾无换行的标记
    pub kind: char,
    pub text: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
}

/// 补丁按原始字节切开：按行暂存要原样拼回去，不能经过解码。
struct Raw<'a> {
    file_header: &'a [u8],
    binary: bool,
    hunks: Vec<(&'a [u8], Vec<&'a [u8]>)>,
}

fn split(patch: &[u8]) -> Raw<'_> {
    let mut raw = Raw { file_header: patch, binary: false, hunks: Vec::new() };
    let mut pos = 0;
    for line in patch.split_inclusive(|&b| b == b'\n') {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        if body.starts_with(b"@@") {
            if raw.hunks.is_empty() {
                raw.file_header = &patch[..pos];
            }
            raw.hunks.push((body, Vec::new()));
        } else if let Some(h) = raw.hunks.last_mut() {
            h.1.push(body);
        } else if body.starts_with(b"Binary files") || body.starts_with(b"GIT binary patch") {
            raw.binary = true;
        }
        pos += line.len();
    }
    raw
}

fn parse(patch: &[u8]) -> Diff {
    if patch.len() > MAX_PATCH {
        return Diff { too_large: true, ..Default::default() };
    }
    let raw = split(patch);
    let hunks = raw
        .hunks
        .iter()
        .map(|(header, lines)| {
            let header = decode(header);
            // @@ -旧起始[,行数] +新起始[,行数] @@
            let start = |sign: char| -> u32 {
                let s = header.split(' ').find(|s| s.starts_with(sign)).unwrap_or_default();
                s.get(1..).and_then(|s| s.split(',').next()).and_then(|n| n.parse().ok()).unwrap_or(0)
            };
            let (mut old_no, mut new_no) = (start('-'), start('+'));
            let take = |n: &mut u32| {
                *n += 1;
                Some(*n - 1)
            };
            let lines = lines
                .iter()
                .map(|l| {
                    let kind = l.first().map_or(' ', |&b| b as char);
                    let text = decode(l.get(1..).unwrap_or_default()).trim_end_matches('\r').to_owned();
                    match kind {
                        '+' => Line { kind, text, old_no: None, new_no: take(&mut new_no) },
                        '-' => Line { kind, text, old_no: take(&mut old_no), new_no: None },
                        '\\' => Line { kind, text, old_no: None, new_no: None },
                        _ => Line { kind: ' ', text, old_no: take(&mut old_no), new_no: take(&mut new_no) },
                    }
                })
                .collect();
            Hunk { header, lines }
        })
        .collect();
    Diff { binary: raw.binary, hunks, ..Default::default() }
}

fn patch(repo: &Repo, path: &str, staged: bool, opts: Options) -> Result<Vec<u8>> {
    let flags = opts.flags();
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    args.extend(flags.iter().map(String::as_str));
    if staged {
        args.push("--cached");
    }
    args.extend(["--", path]);
    repo.git_limited(&args, MAX_PATCH)
}

/// 工作区（`staged` 为 false）或暂存区的单文件改动。未跟踪文件整个显示为新增。
pub fn worktree(repo: &Repo, path: &str, staged: bool, untracked: bool) -> Result<Diff> {
    worktree_with(repo, path, staged, untracked, Options::default())
}

pub fn worktree_with(repo: &Repo, path: &str, staged: bool, untracked: bool, opts: Options) -> Result<Diff> {
    if !untracked {
        let mut diff = parse(&patch(repo, path, staged, opts)?);
        if diff.binary { diff.images = images(repo, if staged { Some("HEAD") } else { Some("") }, if staged { Some("") } else { None }, path, path)?; }
        return Ok(diff);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(repo.path.join(path)).map_err(err)?
        .take(MAX_PATCH as u64 + 1).read_to_end(&mut bytes).map_err(err)?;
    if bytes.len() > MAX_PATCH {
        if let Some(images) = images(repo, Some("HEAD"), None, path, path)? {
            if images.iter().any(Option::is_some) { return Ok(Diff { binary: true, images: Some(images), ..Default::default() }); }
        }
        return Ok(Diff { too_large: true, ..Default::default() });
    }
    if bytes.contains(&0) {
        return Ok(Diff { binary: true, images: images(repo, Some("HEAD"), None, path, path)?, ..Default::default() });
    }
    let text = decode(&bytes);
    let lines: Vec<_> = text
        .lines()
        .enumerate()
        .map(|(i, l)| Line { kind: '+', text: l.to_owned(), old_no: None, new_no: Some(i as u32 + 1) })
        .collect();
    Ok(Diff { hunks: vec![Hunk { header: format!("@@ -0,0 +1,{} @@", lines.len()), lines }], ..Default::default() })
}

/// 某个提交里单个文件的改动；合并提交对比第一个父提交。
pub fn commit(repo: &Repo, id: &str, path: &str) -> Result<Diff> {
    commit_with(repo, id, path, Options::default())
}

pub fn commit_with(repo: &Repo, id: &str, path: &str, opts: Options) -> Result<Diff> {
    let oid = ObjectId::from_hex(id.as_bytes()).map_err(err)?.to_string();
    let flags = opts.flags();
    let mut args = vec!["show", "--format=", "--no-color", "--no-ext-diff", "--diff-merges=first-parent"];
    args.extend(flags.iter().map(String::as_str));
    args.extend([oid.as_str(), "--", path]);
    let mut diff = parse(&repo.git_limited(&args, MAX_PATCH)?);
    if diff.binary {
        let detail = crate::detail::detail(repo, &oid)?;
        let old = detail.files.iter().find(|f| f.path == path).and_then(|f| f.old_path.as_deref()).unwrap_or(path);
        diff.images = images(repo, Some(&format!("{oid}^")), Some(&oid), old, path)?;
    }
    Ok(diff)
}

fn stash_part(repo: &Repo, id: &str, path: &str) -> Result<String> {
    let oid = ObjectId::from_hex(id.as_bytes()).map_err(err)?;
    let gix = repo.gix();
    let commit = gix.find_commit(oid).map_err(err)?;
    if let Some(untracked) = commit.decode().map_err(err)?.parents().nth(2) {
        let untracked = untracked.to_string();
        if repo.git(&["cat-file", "-e", &format!("{untracked}:{path}")]).is_ok() { return Ok(untracked); }
    }
    Ok(oid.to_string())
}

/// 贮藏里的单文件改动；`-u` 保存的文件从贮藏的未跟踪父提交读取。
pub fn stash(repo: &Repo, id: &str, path: &str) -> Result<Diff> {
    stash_with(repo, id, path, Options::default())
}

pub fn stash_with(repo: &Repo, id: &str, path: &str, opts: Options) -> Result<Diff> {
    commit_with(repo, &stash_part(repo, id, path)?, path, opts)
}

pub fn between(repo: &Repo, left: &str, right: &str, path: &str, old_path: Option<&str>) -> Result<Diff> {
    between_with(repo, left, right, path, old_path, Options::default())
}

pub fn between_with(repo: &Repo, left: &str, right: &str, path: &str, old_path: Option<&str>, opts: Options) -> Result<Diff> {
    let left = crate::tools::revision(repo, left)?;
    let right = crate::tools::revision(repo, right)?;
    let old = old_path.unwrap_or(path);
    let flags = opts.flags();
    let mut args = vec!["--literal-pathspecs", "diff", "-M", "--no-color", "--no-ext-diff", "--no-textconv"];
    args.extend(flags.iter().map(String::as_str));
    args.extend([left.as_str(), right.as_str(), "--", old, path]);
    let mut diff = parse(&repo.git_limited(&args, MAX_PATCH)?);
    if diff.binary { diff.images = images(repo, Some(&left), Some(&right), old, path)?; }
    Ok(diff)
}

fn images(repo: &Repo, left: Option<&str>, right: Option<&str>, old: &str, path: &str) -> Result<Option<[Option<String>; 2]>> {
    if !["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"].contains(&path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str()) { return Ok(None); }
    let read = |rev: Option<&str>, path: &str| -> Result<Option<String>> {
        const LIMIT: usize = 32 << 20;
        let bytes = if let Some(rev) = rev {
            let spec = format!("{rev}:{path}");
            let info = repo.git_in(&["cat-file", "--batch-check=%(objecttype) %(objectsize)", "-z"], format!("{spec}\0").as_bytes())?;
            if decode(&info).ends_with(" missing\n") { return Ok(None); }
            if !decode(&info).strip_prefix("blob ").and_then(|s| s.trim().parse::<usize>().ok()).is_some_and(|n| n <= LIMIT) { return Err("PR_IMAGE_TOO_LARGE".into()); }
            repo.git_limited(&["show", &spec], LIMIT)?
        } else {
            let real = match repo.path.join(path).canonicalize() {
                Ok(real) => real,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(err(e)),
            };
            if !real.starts_with(repo.path.canonicalize().map_err(err)?) { return Err("PR_INVALID_PATH".into()); }
            let mut bytes = Vec::new();
            std::fs::File::open(real).map_err(err)?.take(LIMIT as u64 + 1).read_to_end(&mut bytes).map_err(err)?;
            bytes
        };
        if bytes.len() > LIMIT { return Err("PR_IMAGE_TOO_LARGE".into()); }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" }
            else if bytes.starts_with(b"\xff\xd8\xff") { "image/jpeg" }
            else if bytes.starts_with(b"GIF8") { "image/gif" }
            else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { "image/webp" }
            else if bytes.starts_with(b"BM") { "image/bmp" }
            else if bytes.starts_with(b"\0\0\x01\0") { "image/x-icon" }
            else { return Ok(None); };
        Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
    };
    Ok(Some([read(left, old)?, read(right, path)?]))
}

/// Stream a read-only page. Long lines are clipped without retaining their remainder.
/// ponytail: later pages rescan the Git stream; add a disk spool only if profiling warrants it.
pub fn page(repo: &Repo, mode: &str, left: &str, right: &str, path: &str, old_path: Option<&str>, skip: usize) -> Result<Diff> {
    page_with(repo, mode, left, right, path, old_path, skip, Options::default())
}

#[allow(clippy::too_many_arguments)]
pub fn page_with(repo: &Repo, mode: &str, left: &str, right: &str, path: &str, old_path: Option<&str>, skip: usize, opts: Options) -> Result<Diff> {
    if mode == "untracked" { return untracked_page(repo, path, skip); }
    let flags = opts.flags();
    let mut args = vec!["--literal-pathspecs", "diff", "-M", "--no-color", "--no-ext-diff", "--no-textconv"];
    let resolved_stash;
    match mode {
        "compare" => { crate::ops::safe(left)?; crate::ops::safe(right)?; args.extend([left, right]); }
        "commit" => { crate::ops::safe(right)?; args = vec!["--literal-pathspecs", "show", "--format=", "--no-color", "--no-ext-diff", "--no-textconv", "--diff-merges=first-parent", right]; }
        "stash" => {
            crate::ops::safe(right)?;
            resolved_stash = stash_part(repo, right, path)?;
            args = vec!["--literal-pathspecs", "show", "--format=", "--no-color", "--no-ext-diff", "--no-textconv", "--diff-merges=first-parent", &resolved_stash];
        }
        "staged" => args.push("--cached"),
        "unstaged" => {},
        _ => return Err("PR_INVALID_NAME".into()),
    }
    args.extend(flags.iter().map(String::as_str));
    args.extend(["--", old_path.unwrap_or(path), path]);
    let mut child = crate::git_command(&repo.path).args(&args).stdin(std::process::Stdio::null()).spawn().map_err(err)?;
    let mut stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || { let mut out = Vec::new(); let _ = stderr.by_ref().take(65536).read_to_end(&mut out); let _ = std::io::copy(&mut stderr, &mut std::io::sink()); out });
    let result: Result<Diff> = (|| {
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut result = Diff { paged: true, ..Default::default() };
        let (mut old, mut new, mut index) = (0, 0, 0);
        let mut header = String::new();
        loop {
            let (line, clipped) = crate::preview_line(&mut reader)?;
            if line.is_empty() { break; }
            if line.starts_with(b"diff --git ") { header.clear(); }
            if line.starts_with(b"@@ ") {
                header = decode(&line).trim_end().to_owned();
                let start = |sign| header.split(' ').find(|s| s.starts_with(sign)).and_then(|s| s[1..].split(',').next()?.parse::<u32>().ok()).unwrap_or(0);
                old = start('-'); new = start('+');
                continue;
            }
            if header.is_empty() { if line.starts_with(b"Binary files") { result.binary = true; } continue; }
            let kind = line[0] as char;
            if ![' ', '+', '-', '\\'].contains(&kind) { continue; }
            if index >= skip + 1000 { result.next = Some(index); break; }
            if index >= skip {
                if result.hunks.last().is_none_or(|h| h.header != header) { result.hunks.push(Hunk { header: header.clone(), lines: vec![] }); }
                result.hunks.last_mut().unwrap().lines.push(Line { kind, text: decode(&line[1..]).trim_end_matches(['\n', '\r']).to_owned(), old_no: matches!(kind, ' ' | '-').then_some(old), new_no: matches!(kind, ' ' | '+').then_some(new) });
                result.clipped |= clipped;
            }
            if matches!(kind, ' ' | '-') { old += 1; }
            if matches!(kind, ' ' | '+') { new += 1; }
            index += 1;
        }
        Ok(result)
    })();
    if result.as_ref().map_or(true, |d: &Diff| d.next.is_some()) { let _ = child.kill(); }
    let status = child.wait().map_err(err)?;
    let errors = errors.join().unwrap();
    let result = result?;
    if !status.success() && result.next.is_none() { return Err(decode(&errors)); }
    Ok(result)
}

fn untracked_page(repo: &Repo, path: &str, skip: usize) -> Result<Diff> {
    let real = repo.path.join(path).canonicalize().map_err(err)?;
    if !real.starts_with(repo.path.canonicalize().map_err(err)?) { return Err("PR_INVALID_PATH".into()); }
    let mut reader = BufReader::new(std::fs::File::open(real).map_err(err)?);
    let mut result = Diff { paged: true, hunks: vec![Hunk { header: "@@ new file @@".into(), lines: vec![] }], ..Default::default() };
    let mut index = 0;
    loop {
        let (line, clipped) = crate::preview_line(&mut reader)?;
        if line.is_empty() { break; }
        if line.contains(&0) { return Ok(Diff { binary: true, ..Default::default() }); }
        if index >= skip + 1000 { result.next = Some(index); break; }
        if index >= skip {
            result.hunks[0].lines.push(Line { kind: '+', text: decode(&line).trim_end_matches(['\n', '\r']).into(), old_no: None, new_no: Some(index as u32 + 1) });
            result.clipped |= clipped;
        }
        index += 1;
    }
    Ok(result)
}

/// 暂存（`staged` 为 false）或取消暂存第 `hunk` 个区块里选中的行。`lines` 是区块内的行下标。
pub fn apply_lines(repo: &Repo, path: &str, staged: bool, hunk: usize, header: &str, lines: &[usize]) -> Result<()> {
    apply_lines_with(repo, path, staged, hunk, header, lines, None)
}

/// `context` 是界面当前显示用的上下文行数：区块要按同样的行数重新取，标题才对得上。
pub fn apply_lines_with(repo: &Repo, path: &str, staged: bool, hunk: usize, header: &str, lines: &[usize], context: Option<u32>) -> Result<()> {
    let out = partial(repo, path, staged, staged, hunk, header, lines, context)?;
    let mut args = vec!["apply", "--cached", "--recount", "--whitespace=nowarn"];
    if staged {
        args.push("--reverse");
    }
    repo.git_in(&args, &out).map(drop)
}

/// 丢弃工作区里第 `hunk` 个区块中选中的行。丢弃前把当前全部改动存进贮藏列表，后悔了可以取回。
pub fn discard_lines(repo: &Repo, path: &str, hunk: usize, header: &str, lines: &[usize]) -> Result<()> {
    discard_lines_with(repo, path, hunk, header, lines, None)
}

pub fn discard_lines_with(repo: &Repo, path: &str, hunk: usize, header: &str, lines: &[usize], context: Option<u32>) -> Result<()> {
    let out = partial(repo, path, false, true, hunk, header, lines, context)?;
    // stash create 只生成备份提交，不动工作区；store 把它挂进贮藏列表
    let backup = String::from_utf8_lossy(&repo.git(&["stash", "create"])?).trim().to_owned();
    if !backup.is_empty() {
        repo.git(&["stash", "store", "-m", &format!("PushRight: backup before discard: {path}"), &backup])?;
    }
    repo.git_in(&["apply", "--reverse", "--recount", "--whitespace=nowarn"], &out).map(drop)
}

/// 从 `path` 的补丁里取出第 `hunk` 个区块，只保留选中的行，拼成一个可以单独应用的补丁。
/// `staged` 决定用暂存区还是工作区的补丁，`reverse` 说明这个补丁之后会不会被反向应用。
#[allow(clippy::too_many_arguments)]
fn partial(repo: &Repo, path: &str, staged: bool, reverse: bool, hunk: usize, header: &str, lines: &[usize], context: Option<u32>) -> Result<Vec<u8>> {
    // 没有上下文行的部分补丁，git apply 无法可靠定位
    if context == Some(0) {
        return Err("PR_DIFF_FILTERED".into());
    }
    let patch = patch(repo, path, staged, Options { context, ignore_ws: false })?;
    if patch.len() > MAX_PATCH {
        return Err("PR_DIFF_TOO_LARGE".into());
    }
    let raw = split(&patch);
    let (raw_header, raw_lines) =
        raw.hunks.get(hunk).filter(|h| decode(h.0) == header).ok_or("PR_FILE_CHANGED")?;

    // 正向应用时目标是补丁的旧侧：没选中的删除行在目标里还在，当上下文；没选中的新增行不存在，丢掉。
    // 反向应用时目标是新侧，两者对调。
    let (keep, discard) = if reverse { (b'+', b'-') } else { (b'-', b'+') };
    let mut out = [raw.file_header, raw_header, b"\n"].concat();
    let mut dropped = false;
    for (i, l) in raw_lines.iter().enumerate() {
        let kind = l.first().copied().unwrap_or(b' ');
        if kind == b'\\' {
            // 「文件末尾无换行」的标记跟随它前一行的去留
            if !dropped {
                out.extend_from_slice(l);
                out.push(b'\n');
            }
            continue;
        }
        dropped = kind == discard && !lines.contains(&i);
        if dropped {
            continue;
        }
        if kind == keep && !lines.contains(&i) {
            out.push(b' ');
            out.extend_from_slice(&l[1..]);
        } else {
            out.extend_from_slice(l);
        }
        out.push(b'\n');
    }
    Ok(out)
}
