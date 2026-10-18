//! 只开一个桌面版：再次启动（包括 VS Code 扩展的「打开提交图」）时，把要打开的仓库交给已经开着的那个，自己退出。
//! 两边通过一个按用户区分的命名管道传参数，每行一个。

use std::io::Write;
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED};
use windows::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT};

fn pipe_name() -> String {
    format!(r"\\.\pipe\pushright-{}", std::env::var("USERNAME").unwrap_or_default())
}

/// 已经有一个在运行：把参数交给它，返回 true，调用方直接退出
pub fn forward(args: &[String]) -> bool {
    // 客户端这一侧，命名管道就是一个文件
    match std::fs::OpenOptions::new().write(true).open(pipe_name()) {
        Ok(mut pipe) => pipe.write_all(args.join("\n").as_bytes()).is_ok(),
        Err(_) => false,
    }
}

/// 在后台线程等别的进程送来的参数，每收到一次调一次 `deliver`
pub fn serve(deliver: impl Fn(Vec<String>) + Send + 'static) {
    std::thread::spawn(move || {
        let name = HSTRING::from(pipe_name());
        loop {
            let pipe = unsafe { CreateNamedPipeW(&name, PIPE_ACCESS_INBOUND, PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, 0, 4096, 0, None) };
            if pipe.is_invalid() {
                return;
            }
            let connected = unsafe { ConnectNamedPipe(pipe, None) }.is_ok() || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            let mut bytes = Vec::new();
            if connected {
                let mut buf = [0u8; 4096];
                loop {
                    let mut n = 0;
                    if unsafe { ReadFile(pipe, Some(&mut buf), Some(&mut n), None) }.is_err() || n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n as usize]);
                }
            }
            let _ = unsafe { CloseHandle(pipe) };
            if connected {
                let text = String::from_utf8_lossy(&bytes);
                deliver(text.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect());
            }
        }
    });
}

/// 命令行：仓库路径若干，`--reveal <提交>` 打开后定位到这个提交（VS Code 扩展的「在提交图中显示」）
pub fn parse(args: &[String]) -> (Vec<String>, Option<String>) {
    let mut paths = Vec::new();
    let mut reveal = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--reveal" {
            reveal = it.next().cloned();
        } else {
            paths.push(a.clone());
        }
    }
    (paths, reveal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paths_and_reveal() {
        let args: Vec<String> = ["C:/a", "--reveal", "abc123", "C:/b"].map(String::from).to_vec();
        assert_eq!(parse(&args), (vec!["C:/a".to_owned(), "C:/b".to_owned()], Some("abc123".to_owned())));
        assert_eq!(parse(&[]), (vec![], None));
    }
}
