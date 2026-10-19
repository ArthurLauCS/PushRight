# PushRight 1.2.1 — Tab row and GitHub pull requests

仓库页签单独占一行、按数量平分宽度，选中的页签为陶土色；页签可拖动排序，双击或右键可自定义名字，名字后显示当前分支，同一仓库开多个工作树时更容易区分。全局设置移到页签上方单独一行。

Repository tabs get their own row and share its width; the selected tab is terracotta. Drag tabs to reorder them, double-click or right-click to rename them, and each tab shows its checked-out branch, so several worktrees of one repository are easy to tell apart. The global settings move to a row above the tabs.

## GitHub pull requests

侧栏新增“拉取请求”分组，显示打开着的拉取请求及审阅、检查状态；可从本地分支创建（需要时先推送）、检出到本地、在 GitHub 上合并（合并提交、压缩或变基），或在浏览器中打开。需要 GitHub CLI（`gh`）；`gh` 未登录时使用 Git 凭据管理器里保存的 GitHub 登录。GitLab 合并请求暂不支持。

A **Pull requests** group in the sidebar lists open pull requests with review and check status. Create one from a local branch (pushed first when needed), check one out, merge it on GitHub (merge commit, squash or rebase) or open it in the browser. Requires [GitHub CLI](https://cli.github.com/); when `gh` is not signed in, the GitHub sign-in saved by Git Credential Manager is used. GitLab merge requests are not supported yet.

## 1.2.0 known limits resolved

- 差异文本可以拖动选中，`Ctrl+C` 复制原文；长行的差异底部有可拖动的横向滚动条。
  Drag to select diff text and copy the original text with `Ctrl+C`; diffs with long lines get a draggable horizontal scrollbar.
- 差异配色弹窗改为新增、删除各一个取色器，带预设色板和实时预览。
  The Diff Colors dialog has a color picker for added and deleted lines, preset swatches and a live preview.

## Fixes

- 仓库工具：版本比较占满页面高度；单独的按钮不再被拉满整行；Reflog 表格不再超出右边界；命令输出框超长时可滚动。
  Repository tools: the comparison fills the page; standalone buttons keep their width; reflog rows stay inside the table; long command output scrolls.
- 工作树总览、提交详情、本地更改的标题文字对齐与截断。
  Alignment and truncation in the worktree overview, commit details and Local Changes headers.
- “全部暂存 / 全部取消 / 全部丢弃”作用于整组，不受文件筛选影响。
  Stage all, Unstage all and Discard all act on the whole group regardless of the file filter.
- ICO 图片预览恢复；版本比较中文件右键菜单取错版本的问题已去掉。
  ICO previews work again; the comparison no longer offers a file menu that used the wrong revisions.
- 分隔条可用方向键调整（Home 恢复默认）；作者处按 Shift+F10 打开作者样式。
  Splitters respond to arrow keys (Home restores the default); Shift+F10 on an author opens author styling.
- 外部合并工具名写错时立即报错。
  A mistyped external merge tool name is reported immediately.

## Validation

- `cargo test --workspace`: all tests passed (1 opt-in test ignored); the desktop app builds with no warnings.
- The dev build was driven on this repository and a large work repository: tab row with one and two tabs, tab menu and rename, Diff Colors, every Repository tools page, worktrees, Local Changes, drag-selecting diff text, and the Create pull request dialog. Creating, checking out and merging a real pull request were not exercised.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension is unchanged and stays at 1.2.0.

## Downloads

- `PushRight-Setup-1.2.1.exe` — Windows x64 desktop installer.
- `PushRight-1.2.1-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
