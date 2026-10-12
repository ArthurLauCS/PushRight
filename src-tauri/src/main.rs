// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use engine::rpc::Tabs;
use engine::Result;
use serde_json::Value;

/// 命令行传入的仓库路径：`pushright <路径>...`
#[tauri::command]
fn initial_repos() -> Vec<String> {
    std::env::args().skip(1).collect()
}

/// 引擎命令统一走这里；命令在 engine::rpc 里定义，与扩展的 sidecar 共用。
#[tauri::command(async)]
fn call(state: tauri::State<Tabs>, cmd: String, args: Value) -> Result<Value> {
    engine::rpc::call(&state, &cmd, &args)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Tabs::default())
        .invoke_handler(tauri::generate_handler![initial_repos, call])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
