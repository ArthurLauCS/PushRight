# PushRight 1.1.10 — Unified interface

统一桌面版与扩展的界面，修复多处由长文案引起的布局问题。

## Changes / 更新

- 中文操作名称拆成两部分显示：名称在前，Git 命令以小号等宽字跟随。空间不足的位置（顶部工具栏、分组标题、窄面板）只显示名称，完整文案在悬停提示里。
- 顶部工具栏在任何宽度和语言下保持一行；窗口变窄时先收起右侧辅助按钮的文字，再全部只留图标。
- 页签栏的“差异配色”改为图标按钮，“自动刷新与获取”使用独立图标，不再与“仓库设置”相同。
- 侧栏导航按使用频率排列：本地更改、全部提交、工作树总览、仓库工具。
- 修复冲突视图头部：说明文字曾被挤成一字一行，“保存并标记为已解决”被推出可见区域。
- 弹窗的确认和取消按钮固定在底部，窗口较矮时无须滚动；分支名不符合规则时输入框标红。
- 命令日志关闭等宽字体连字，`--all` 不再显示为破折号；面板内新增关闭按钮；失败的命令除颜色外增加竖线标记。
- 仓库工具页与其他页面统一：按钮按内容定宽，Reflog 改为表格行，远程地址标签本地化，上移／下移改用图标。
- 提交图表头与各列对齐；侧栏的待推送／待拉取标记改用等宽字体，避免 `↑2` 被读成 `12`。
- 扩展侧栏提交图的获取、拉取、推送改用与桌面版一致的图标，范围选择显示为下拉框；菜单项 `Repository Settings…` 统一大小写。

Chinese action labels now render the name first and the Git command in smaller monospace text; tight spots show the name only and keep the full label in the tooltip. The toolbar stays on one line at every width and language. The conflict view header no longer collapses, dialog buttons stay visible in short windows, invalid branch names are marked, and the command log shows `--` options verbatim with a close button and a non-color failure marker. Repository tools follow the same layout as other pages. The sidebar graph in the extension uses the desktop icons.

## Downloads

- `PushRight_1.1.10_x64-setup.exe` — Windows x64 desktop installer.
- `pushright-win32-x64-1.1.10.vsix` — VS Code / Cursor extension.
- `PushRight-1.1.10-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for all three artifacts.

## Validation and scope

- All 42 Node tests, Svelte/TypeScript checks, the desktop build, VSIX packaging and packaged extension smoke runs in VS Code and Cursor pass. The smoke test waits for the coalesced repository refresh before checking the revert setting. Rust sources are unchanged; the Rust test suite was not rerun.
- README previews were captured from the 1.1.10 desktop build with a local demo repository and fictional identities; the extension guides use the same shared-interface previews.
- The desktop app and the extension were driven with real mouse and keyboard input against a temporary repository: stage, commit, push, merge into a conflict, resolve block by block and continue; context menus, dialogs, repository tools, worktrees and the extension Source Control view.
- A scripted sweep over English and Chinese at 1024, 1280, 1440 and 1920 px across the four main views found no wrapped toolbars, page overflow or clipped buttons.
- Not verified: the extension in a Chinese VS Code display language (the language pack could not be installed in the isolated profile), the native folder picker, and full native installer acceptance. Light theme was spot-checked only.
- Windows 10/11 x64 local workspaces only; system Git and WebView2 are required. The installer is unsigned.
