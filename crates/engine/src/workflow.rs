//! 提交相关的仓库级约定：提交信息模板、最近的提交信息、提交前检查命令。
//! 设置存在仓库的本地 git 配置里（`pushright.*`），桌面版、扩展和关联工作树共用。

use crate::{decode, err, ops::Log, Repo, Result};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Settings {
    /// 提交框为空时自动带出的内容
    pub template: String,
    /// 提交前检查用的命令，在仓库根目录下经系统 shell 执行，例如 `just test --error`
    pub check_command: String,
    /// 每次提交前自动跑一遍检查
    pub check_before_commit: bool,
}

const KEYS: [&str; 3] = ["pushright.commitTemplate", "pushright.checkCommand", "pushright.checkBeforeCommit"];

pub fn read(repo: &Repo) -> Result<Settings> {
    // 没有任何匹配时 git 以退出码 1 结束，按「全部未设置」处理
    let out = repo.git(&["config", "--local", "--null", "--get-regexp", "^pushright[.]"]).unwrap_or_default();
    let text = decode(&out);
    // --null 的格式是「键\n值\0」，键名是小写的
    let get = |key: &str| text.split('\0').find_map(|rec| rec.split_once('\n').filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.to_owned())).unwrap_or_default();
    let mut settings = Settings { template: get(KEYS[0]), check_command: get(KEYS[1]), check_before_commit: get(KEYS[2]) == "true" };
    if settings.template.is_empty() {
        // 没在这里设过模板时，沿用 Git 自己的 commit.template 文件
        if let Ok(path) = repo.git(&["config", "--get", "--type=path", "commit.template"]) {
            settings.template = std::fs::read(repo.path.join(decode(&path).trim())).map(|b| decode(&b).trim_end().to_owned()).unwrap_or_default();
        }
    }
    Ok(settings)
}

pub fn write(repo: &Repo, settings: &Settings) -> Result<()> {
    let check = if settings.check_before_commit { "true" } else { "" };
    for (key, value) in KEYS.into_iter().zip([settings.template.trim_end(), settings.check_command.trim(), check]) {
        if value.is_empty() {
            // 本来就没设时 --unset 以退出码 5 结束，不算错
            let _ = repo.git(&["config", "--local", "--unset-all", key]);
        } else {
            repo.git(&["config", "--local", "--replace-all", key, value])?;
        }
    }
    Ok(())
}

/// 当前分支上最近的提交信息（完整内容，去重），给提交框的「最近的提交信息」下拉用。
pub fn recent_messages(repo: &Repo, limit: usize) -> Result<Vec<String>> {
    // 还没有提交的仓库里 git log 会失败，当作没有
    let out = repo.git(&["log", "-n", &limit.to_string(), "--format=%B%x00"]).unwrap_or_default();
    let mut list: Vec<String> = Vec::new();
    for message in decode(&out).split('\0').map(str::trim).filter(|m| !m.is_empty()) {
        if !list.iter().any(|m| m == message) {
            list.push(message.to_owned());
        }
    }
    Ok(list)
}

/// 去掉终端颜色等 ANSI 转义序列（ESC [ … 字母），界面上按纯文本显示。
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
        } else if chars.next() == Some('[') {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    out
}

/// 只保留输出的最后这么多字节：失败信息通常在结尾
const TAIL: usize = 256 * 1024;

/// 在仓库根目录跑设置里的检查命令，等它结束，返回命令、输出和是否通过（退出码为 0）。
/// 命令取自仓库配置而不是调用参数：界面只能触发，不能临时换成别的命令。
pub fn run_check(repo: &Repo) -> Result<Log> {
    let command = read(repo)?.check_command;
    if command.is_empty() {
        return Err("PR_NO_CHECK_COMMAND".into());
    }
    let mut cmd;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd = Command::new("cmd");
        // 命令行原样交给 cmd，不经过 Rust 的参数转义；不弹出控制台窗口
        // 子 shell（如 just 的 PowerShell）继承 UTF-8 代码页；Python 的管道编码需单独设置。
        cmd.raw_arg("/D /C").raw_arg(format!("chcp 65001 >nul && {command}")).creation_flags(0x0800_0000);
        cmd.env("PYTHONIOENCODING", "utf-8");
    }
    #[cfg(not(windows))]
    {
        cmd = Command::new("sh");
        cmd.arg("-c").arg(&command);
    }
    let out = cmd.current_dir(&repo.path).env("NO_COLOR", "1").stdin(Stdio::null()).output().map_err(err)?;
    let mut bytes = [out.stdout, out.stderr].concat();
    if bytes.len() > TAIL {
        bytes.drain(..bytes.len() - TAIL);
    }
    Ok(Log { command, output: strip_ansi(&decode(&bytes)).trim().to_owned(), ok: out.status.success() })
}

/// 外部对比 / 合并工具。存在全局 git 配置里：工具装在这台机器上，所有仓库共用。
/// 值是 git 认识的工具名（`git difftool --tool-help` 列出的，如 vscode、bc、meld、winmerge），
/// 或一整条命令行，里面用 $LOCAL $REMOTE（合并时还有 $BASE $MERGED）指代文件。
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Tools {
    pub diff: String,
    pub merge: String,
}

const TOOL_KEYS: [&str; 2] = ["pushright.diffTool", "pushright.mergeTool"];

pub fn tools(repo: &Repo) -> Tools {
    let text = decode(&repo.git(&["config", "--global", "--null", "--get-regexp", "^pushright[.](diff|merge)tool$"]).unwrap_or_default());
    let get = |key: &str| text.split('\0').find_map(|rec| rec.split_once('\n').filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.to_owned())).unwrap_or_default();
    Tools { diff: get(TOOL_KEYS[0]), merge: get(TOOL_KEYS[1]) }
}

pub fn set_tools(repo: &Repo, tools: &Tools) -> Result<()> {
    for (key, value) in TOOL_KEYS.into_iter().zip([tools.diff.trim(), tools.merge.trim()]) {
        if value.is_empty() {
            let _ = repo.git(&["config", "--global", "--unset-all", key]);
        } else {
            repo.git(&["config", "--global", "--replace-all", key, value])?;
        }
    }
    Ok(())
}

/// 用外部工具打开一个文件的对比（`kind` 为 diff）或冲突（`kind` 为 merge），不等工具关闭。
/// `mode`：unstaged / staged / commit（此时 `id` 是提交），只对 diff 有意义。
/// 工具改完冲突文件并正常退出后，git 会把它标记为已解决；界面回到前台时自己刷新。
// ponytail: commit 模式和第一个父提交比，仓库的第一个提交没有父提交，会报错
pub fn launch(repo: &Repo, kind: &str, tool: &str, mode: &str, id: &str, path: &str) -> Result<()> {
    if tool.is_empty() || !matches!(kind, "diff" | "merge") {
        return Err("PR_NO_EXTERNAL_TOOL".into());
    }
    let command = format!("{kind}tool");
    let mut args = vec!["--literal-pathspecs".to_owned(), "-c".into(), "mergetool.keepBackup=false".into()];
    let name = if tool.contains([' ', '$']) {
        args.extend(["-c".into(), format!("{command}.pushright.cmd={tool}"), "-c".into(), format!("{command}.pushright.trustExitCode=true")]);
        "pushright"
    } else {
        tool
    };
    args.extend([command, "-y".into(), "-t".into(), name.to_owned()]);
    match (kind, mode) {
        ("merge", _) | (_, "unstaged") => {}
        (_, "staged") => args.push("--cached".into()),
        (_, "commit") => args.extend([format!("{}~1", crate::ops::safe(id)?), id.to_owned()]),
        _ => return Err("PR_INVALID_NAME".into()),
    }
    args.extend(["--".into(), path.to_owned()]);
    let mut child = crate::git_command(&repo.path).args(&args).stdout(Stdio::null()).spawn().map_err(err)?;
    // 需要在终端里回答的情况（比如一边删除一边修改的冲突）读到 EOF 后放弃
    drop(child.stdin.take());
    // 工具名写错、命令找不到这类问题会马上失败，等一小会儿把报错带回界面
    for _ in 0..15 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(status) = child.try_wait().map_err(err)? {
            if status.success() {
                return Ok(());
            }
            let mut message = Vec::new();
            let _ = std::io::Read::read_to_end(&mut child.stderr.take().unwrap(), &mut message);
            return Err(decode(&message).trim().to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_terminal_colours_only() {
        assert_eq!(super::strip_ansi("\u{1b}[31mFAIL\u{1b}[0m spec/a.luau [1/2]\u{1b}[2K"), "FAIL spec/a.luau [1/2]");
    }
}
