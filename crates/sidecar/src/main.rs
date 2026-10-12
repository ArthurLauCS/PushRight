//! 把引擎包成子进程供 VS Code 扩展使用：标准输入每行一个 `{id, cmd, args}`，
//! 标准输出每行一个 `{id, ok, value}`。命令在 engine::rpc 里定义，与桌面版共用。

use engine::rpc::{call, Tabs};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;

#[derive(Deserialize)]
struct Request {
    id: u64,
    cmd: String,
    #[serde(default)]
    args: Value,
}

fn main() {
    // 扩展靠监听 .git 的变化来刷新；不让只读命令（status）顺手改写索引，否则每次刷新又触发一次变化
    std::env::set_var("GIT_OPTIONAL_LOCKS", "0");
    let tabs = Arc::new(Tabs::default());
    // 标准输入关闭（扩展主机退出）时循环结束，进程随之退出
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let tabs = tabs.clone();
        // ponytail: 每个请求一个线程；实测往返成为瓶颈时换线程池
        std::thread::spawn(move || {
            let reply = match serde_json::from_str::<Request>(&line) {
                Ok(r) => match call(&tabs, &r.cmd, &r.args) {
                    Ok(value) => json!({ "id": r.id, "ok": true, "value": value }),
                    Err(e) => json!({ "id": r.id, "ok": false, "value": e }),
                },
                Err(e) => json!({ "id": null, "ok": false, "value": e.to_string() }),
            };
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        });
    }
}
