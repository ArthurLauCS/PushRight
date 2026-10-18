# PushRight 1.2.0 — Native desktop app

桌面版用 Rust 重写，界面由 GPU 直接绘制（gpui），不再使用 Tauri 和 WebView2。外观、配色、字体、布局、功能和中英双语保持与 1.1.13 一致。同一台机器、同一仓库下实测：

| | 1.1.13（Tauri） | 1.2.0 |
| --- | --- | --- |
| 打开仓库到显示提交图 | 约 0.69 秒 | 约 0.45–0.71 秒 |
| 进程数 | 7 | 1 |
| 内存 | 约 415 MB | 约 124 MB |
| VSIX | 13 MB | 1.7 MB |

The desktop app is rewritten in Rust with a native GPU-rendered interface (gpui), replacing Tauri and WebView2. Look, colors, fonts, layout, features and both languages match 1.1.13. On the same machine and repository, the app shows the graph in about 0.45–0.71 s using one process and about 124 MB, down from 7 processes and about 415 MB.

扩展的“打开提交图”和“在提交图中定位”改为用桌面版打开仓库（已运行时切换过去，可用 `pushright.desktopPath` 指定位置），编辑器内的完整面板移除，VSIX 从 13 MB 减到 1.7 MB；新增“PushRight: 时区…”命令。

In the extension, **Open Commit Graph** and **Show in Commit Graph** open the desktop app (a running instance switches to the repository; `pushright.desktopPath` overrides its location). The in-editor full panel is removed and the VSIX shrinks to 1.7 MB. **PushRight: Time Zone…** is new.

## Upgrade notes

- The installer is now per-user (`%LOCALAPPDATA%\Programs\PushRight`, no administrator rights) and removes a 1.1.x installation automatically.
- Preferences live in `%APPDATA%\PushRight\settings.json`. Language, theme, time zone, diff colors, author styles and recent repositories from 1.1.x are not migrated.
- New app icon.

升级说明：安装包改为按用户安装、无需管理员权限，并自动卸载 1.1.x；偏好保存在 `%APPDATA%\PushRight\settings.json`，1.1.x 的语言、主题、时区、配色、作者样式和最近仓库不迁移。应用图标更新。

## Known limits

- Diff text cannot be selected or copied with the mouse yet.
- The Diff Colors dialog offers presets and HEX input; the color pickers and live preview of 1.1.x are not included.

已知限制：差异文本暂不能用鼠标选中复制；差异配色弹窗提供预设和 HEX 输入，暂无颜色选择器与实时预览。

## Validation

- `cargo test --workspace`: all tests passed (1 opt-in test ignored).
- `pnpm check`: 0 errors, 0 warnings; `pnpm test`: 43 / 43 passed.
- The release app was driven on a demo repository: graph, commit details, diffs, staging and committing, undo, conflicts, Pull / Push dialogs, repository tools, light / dark themes and both languages. Opening a repository from the extension with `--reveal` selects the commit in a running instance.
- Windows x64 only; the installer is unsigned.

## Downloads

- `PushRight-Setup-1.2.0.exe` — Windows x64 desktop installer.
- `pushright-win32-x64-1.2.0.vsix` — VS Code / Cursor extension.
- `PushRight-1.2.0-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for all three artifacts.
