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
        cmd.raw_arg("/D /C").raw_arg(&command).creation_flags(0x0800_0000);
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

#[cfg(test)]
mod tests {
    #[test]
    fn strips_terminal_colours_only() {
        assert_eq!(super::strip_ansi("\u{1b}[31mFAIL\u{1b}[0m spec/a.luau [1/2]\u{1b}[2K"), "FAIL spec/a.luau [1/2]");
    }
}
