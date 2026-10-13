# PushRight 1.1.11 — Global time zones and repository tools

桌面版和 VS Code／Cursor 扩展新增全局时区设置，并同步发布自 1.1.10 以来的仓库工具与工作流更新。

## Changes / 更新

- **全局时区**：完整提交图右上角新增下拉框，可跟随系统、使用 UTC 或选择地区。覆盖全部仓库的提交图、详情、历史、搜索、Blame、工作树和仓库统计；立即生效、重启保留，按提交日期自动处理夏令时。桌面版和各编辑器分别保存偏好。
- **仓库统计**：显示提交数、作者排行、按月提交数、活跃文件和首末提交日期；日期和月份遵循所选时区。
- **提交与分支**：提交模板、最近提交信息、提交前检查；分支固定、清理、分组菜单、多分支图筛选、隐藏标签、只看主线和分支比较。
- **差异与冲突**：差异显示工具、文件树和目录暂存、文件图标与语法高亮；冲突手动编辑及逐处跳转、外部对比／合并工具、拉取前的二进制双端改动提示。
- **仓库工具**：二分查找、子模块、Git LFS、应用补丁及扩展工作树视图；桌面和扩展共用命令分发，桌面只读 Git 命令不再占用索引锁。

The time-zone dropdown applies to all repositories in the current app, including native editor history and blame. It handles date and month boundaries and daylight-saving transitions. This release also includes repository statistics, commit assistance, branch and graph controls, expanded diff and conflict tools, external tools, bisect, submodules, Git LFS, patch application and the extension Worktrees view.

## Downloads

- `PushRight_1.1.11_x64-setup.exe` — Windows x64 desktop installer.
- `pushright-win32-x64-1.1.11.vsix` — VS Code / Cursor extension.
- `PushRight-1.1.11-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for all three artifacts.

## Validation and scope

- 57 Node tests and Svelte/TypeScript checks passed; the time-zone tests cover persistence, live updates, date boundaries, fractional offsets, daylight saving and monthly statistics.
- Rust workspace tests passed, including the engine's timestamp export for statistics.
- Desktop installer and VSIX builds passed. The packaged extension passed all 26 smoke checks in VS Code and all 26 in Cursor.
- The shared dropdown was checked in a browser, including Chinese labels and persistence after reload. Existing README screenshots remain labeled as 1.1.10 previews.
- Full native installer acceptance was not performed. Windows 10/11 x64 local workspaces only; system Git and WebView2 are required. The installer is unsigned.
