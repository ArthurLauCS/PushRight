# Changelog

## 1.1.12

- Fix Windows pre-commit checks failing with Python `UnicodeEncodeError` when printing emoji, and preserve Chinese output from nested PowerShell commands. The shared runner sets the child console code page and Python streams to UTF-8 while preserving command exit status.

修复 Windows 提交前检查中 Python 输出 emoji 时的 `UnicodeEncodeError`，以及嵌套 PowerShell 中文输出变成问号的问题。共用检查入口为子进程设置 UTF-8，保留命令原有的成功／失败状态。

## 1.1.11

- Add a global time-zone dropdown to the shared full graph. Persist the selection across restarts and apply it to all repository views, native history and blame, including daylight-saving transitions and monthly statistics.
- Expand graph filtering and branch management; add diff tools, a changed-file tree and folder staging, hand-edited conflict resolution, external diff/merge tools and a warning for overlapping binary changes before pull.

新增全局时区下拉框，覆盖各仓库视图、原生历史与 Blame、统计日期和月份，重启后保留，并自动处理夏令时。完善分支管理、提交图筛选、差异工具、目录暂存、冲突手动编辑和外部工具，并在拉取前提示二进制文件的双端改动。

- Add a **Worktrees** view to the PushRight page: list worktrees, open one in a new window, create and remove them. A submodule's own folder is now reported as its main worktree.
- Chinese labels for the commit identity prompts show the `git config` command they run.
- Sidebar graph: add **Tags** and **First parent only** toggles, shared with the full graph.
- Source Control: pre-fill the repository's commit message template; add **Recent Commit Messages…** and **Run Pre-Commit Check** commands.
- Full graph, Repository tools: find the commit that broke something (bisect), submodules, Git LFS, apply a pasted patch, and repository statistics. External diff / merge tools, hand-edited conflict resolution with previous / next navigation, commit template, recent messages and a pre-commit check.

PushRight 页面新增“工作树”视图：列出工作树、在新窗口打开、新建和删除。修改提交身份的输入框写出对应的 `git config` 指令。侧栏提交图新增“标签”“只看主线”开关。源代码管理面板预填提交信息模板，新增“最近的提交信息…”“运行提交前检查”命令。完整提交图的仓库工具新增二分查找、子模块、LFS、应用补丁和仓库统计，并带上外部对比／合并工具、冲突手动编辑与跳转、提交模板和提交前检查。

## 1.1.10

- Keep the full graph toolbar on one line at every width. Chinese labels show the action name first and the Git command in smaller monospace text; where space is tight the command moves to the tooltip.
- Fix the conflict view header, which squeezed its hint into a one-character column and pushed the resolve button off screen. Keep dialog buttons visible in short windows and mark invalid branch names.
- Show `--` options verbatim in the command log, add a close button, and mark failed commands with a border as well as color.
- Use the desktop icons for fetch, pull and push in the sidebar graph, and show the scope selector as a dropdown.

完整提交图的工具栏保持一行，中文名称在前、Git 命令以小号等宽字跟随，空间不足时移入悬停提示。修复冲突视图头部被挤压、矮窗口下弹窗按钮不可见、命令日志把 `--` 画成破折号的问题。侧栏提交图改用与桌面版一致的图标。

## 1.1.9

- Add Blue / Orange, Purple / Gold and Cyan / Rose diff presets, custom added/deleted HEX colors, and color pickers with a live preview in the full graph. Retain + / − markers and colored borders in shared diff views.
- Apply colors to native VS Code / Cursor diffs through user settings. Restore original colors without replacing unrelated settings or later manual edits.
- Resolve stale Windows path casing against the index or the displayed historical revision before retrying blame. Report Git failures in the PushRight output channel and avoid guessing between case-colliding paths.

新增差异配色预设及新增／删除独立选色、预览和恢复功能，覆盖完整提交图与编辑器原生 diff。修复 Windows 路径大小写不一致造成的 Blame 缺失，并记录 Git 错误。

## 1.1.8

- Replace comparison and interactive rebase revision dropdowns with editable, searchable pickers in the shared full graph UI.
- Group the current branch and upstream first, then common branches (main, master, season, dev, develop), other local branches, remote branches and tags.
- Support mouse selection, arrow keys, Enter and Escape while retaining direct commit IDs and revision expressions.

版本比较与交互式变基的选择框支持输入筛选，当前分支及上游置顶，season、dev 等常用分支优先，并保留直接输入提交号及版本表达式的能力。

## 1.1.7

- Strengthen force-push protection with `--force-with-lease --force-if-includes` so background fetch cannot silently weaken the lease check.
- Desktop automatic refresh/fetch timers are not enabled in the extension.

强制推送增加远端提交已在本地整合的检查，避免后台 fetch 削弱保护。扩展版不启用桌面版定时器。

## 1.1.6

- Append native Git command names to Chinese action labels across menus, buttons, dialogs and the command palette.
- Add repository settings shared by the desktop app and extension. Disabling revert blocks starting and continuing revert at execution time; abort remains available. Linked worktrees share the setting, other repositories are unaffected.

中文操作名称增加对应 Git 命令；新增两端共用的仓库级“禁用 revert”设置，执行入口拦截新建和继续 revert，仍允许中止。

## 1.1.5

- Show current-line blame on both sides of diffs, including added, deleted and unchanged lines. Use each side's revision or index snapshot; uncommitted additions are labeled as uncommitted.

修复 diff 两侧的行末提交信息：新增、删除和未修改行均按各自版本显示归属，尚未提交的新增行显示“尚未提交”。

## 1.1.4

- Fix duplicate quick-diff visibility commands when multiple repositories are open.
- Read status alone for working-file changes and avoid redraws when status is unchanged; parallelize independent Git reads.
- Keep existing graph rows visible while refreshing and show authors in the compact graph.
- Inspect the first 8000 bytes for NUL when detecting binary revision contents.

修复多仓库 quick-diff 重复注册，减少重复读取与重绘，提交图刷新时保留旧行并显示作者，调整历史文件的二进制识别范围。

## 1.1.3

- Keep branch lists in a fixed order so clicking a branch does not move it away from the pointer.
- Highlight the selected branch, tag or stash in the graph sidebar and tick the checked-out branch.
- Keep local branches before remote branches in editor pickers, without recent-use reordering.

分支列表不再按最近使用重排；点击后的行保持原位并高亮，当前分支显示对钩，扩展选择框仍保持本地分支优先。

## 1.1.2

- Show each branch's latest commit (author, short ID, subject and age) in the branch picker.
- Choose the commit graph's branch from the same top picker instead of a dropdown.
- Create branches from the branch picker and the graph dialog: names follow `(fix|feat)_(S|C|SC)_lowerCamelCase` and the starting branch can be chosen. Existing branch names are unaffected.

分支选择框在每个分支下显示最新提交；提交图显示的分支改在顶部选择框中选择；新建分支时提示并校验命名规则 `(fix|feat)_(S|C|SC)_小驼峰`，可选择从哪个分支签出。已有分支名称不受影响。

## 1.1.1

- Sort branches by recent use, with local branches before remote branches in the editor picker.
- Allow locally ignoring multiple selected untracked files in one operation.
- Open stash contents without applying them, including files saved with `git stash -u`.
- Replace unreliable revision datalist choices with clickable selectors in compare and interactive rebase tools.

分支按最近使用排序，扩展的本地分支优先于远程分支；支持批量将选中的未跟踪文件设为仅本机忽略；贮藏无需应用即可查看；修复比较版本与交互式变基的版本下拉选项无法点击。

## 1.1.0

- Add repository comparisons, reviewed-file markers, remote management, interactive rebase and reflog recovery to the full graph interface.
- Add clone/init, image previews and read-only streaming pages for large patches.
- Page file/line history and search beyond 200 entries; map unchanged lines in dirty buffers back to HEAD.
- Allow native text revision limits of 4–64 MiB. Windows local workspaces only; PR/Issue collaboration and cloud patches are excluded.

新增整仓比较、审阅标记、远程管理、交互式变基、Reflog 恢复、克隆/初始化、图片预览及大补丁分页；历史可加载更多，本地修改后的未变选区仍可查询行历史。仅完善 Windows 本地功能。

## 1.0.1

- Adopt the MIT license and include it in the VSIX and desktop installer.
- Update direct downloads to the licensed release. Runtime behavior is unchanged from 1.0.0.

采用 MIT 开源许可证，随 VSIX 与桌面安装包分发，并更新直接下载入口。功能与首个正式版 1.0.0 保持一致。

## 1.0.0

First stable release for Windows x64, with English and Simplified Chinese UI.

- Fix duplicate text-model errors when opening unstaged diffs and inline changes.
- Share revision reads between gutter change markers and diff editors.
- Close individual repositories from Source Control and reopen them explicitly. Closed repositories stay closed across file switches, refreshes and window reloads.
- Include native commit-file diffs, branch filters, nested repositories and local ignore/tracking controls from the preview releases.
- Provide a direct VSIX download for VS Code and Cursor, alongside the desktop installer.

首次正式版，提供 Windows x64 桌面安装包和扩展 VSIX，内置英文与简体中文。修复未暂存差异及行内改动详情的重复模型错误，复用行边标记与差异编辑器的版本读取；支持关闭、重新打开仓库，并在刷新和重载后保留关闭状态。
