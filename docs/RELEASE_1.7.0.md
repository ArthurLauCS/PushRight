# PushRight 1.7.0 — Change groups, review marks, worktree overview

为审查、落地分散在多个工作树里的改动而做：本地更改可以分组逐个提交，审阅过的文件之后只看新改动，工作树页能跨仓库总览并清理已合并的工作树。diff 里的代码可以缩放，代码字体换成随应用打包的 Maple Mono CN。

Built for reviewing and landing changes spread across several worktrees: Local Changes can be split into groups committed one at a time, reviewed files later show only their new changes, and the worktree page gives an overview across repositories with cleanup of merged worktrees. Code in diffs can be zoomed, and code uses the bundled Maple Mono CN font.

## Local changes

- **改动分组**：新建有名字的组，用右键菜单或拖放把文件移进组；组名就是提交说明，第一行是组的标题。每组单独提交（`git commit --only`），组外已暂存的文件不受影响，提交后组自动删除。删除组时文件回到未暂存。合并、变基、摘取进行中时不能按组提交。
  **Change groups**: create named groups and move files into them from the context menu or by dragging; a group's name is its commit message, and the first line is its title. Each group is committed on its own (`git commit --only`) without touching files staged outside it, and the group goes away once committed. Deleting a group returns its files to Unstaged. Group commits are unavailable while a merge, rebase or cherry-pick is in progress.
- **审阅标记**：文件行前的勾选框记下文件现在的内容。之后文件又被改了，勾选框变成橙色方块，diff 默认只显示审阅后的改动（只读），可以切回全部改动；改回审阅时的内容会自动回到已审阅。可以隐藏已审阅且之后没再改的文件。标记存在仓库的 Git 目录里。
  **Review marks**: the checkbox in front of a file remembers its current content. When the file changes again, the box turns into an orange square and its diff shows only the changes since the review (read-only), with a switch back to all changes; changing the file back to the reviewed content marks it reviewed again. Files that are reviewed and unchanged since can be hidden. Marks are kept in the repository's Git directory.

## Worktrees

- 工作树页可以在「本仓库」和「所有仓库」（打开的页签加最近打开的仓库）之间切换，同一仓库的工作树归在一起。
  The worktree page switches between this repository and all repositories (open tabs plus recent ones), grouping the worktrees of each repository.
- 每个工作树显示是否已合并（最新提交已在远程默认分支或 develop / dev / main / master 里）、没有上游时的未推送提交，以及最后活动时间（最后一次提交或最新的未提交修改），超过 7 天显示为橙色。
  Each worktree shows a Merged badge (its tip is contained in the remote default branch or develop / dev / main / master), unpushed commits when it has no upstream, and its last activity (last commit or newest uncommitted edit), orange after 7 days.
- **清理已合并**删除已合并、没有改动且全部推送的工作树，不动主工作树和已打开的页签：删掉文件夹和本地分支，远程分支保留。列出之后又有新提交的工作树会保留。
  **Clean up merged** removes worktrees that are merged, clean and fully pushed, leaving the main worktree and open tabs alone: the folder and local branch go, remote branches stay. A worktree with new commits since it was listed is kept.

## Code view

- `Ctrl+=` / `Ctrl+-` 每次把 diff 和冲突视图里的代码放大或缩小 1px（10–24px），`Ctrl+0` 恢复 12px；行号和增删记号一起缩放。字号所有页签共用，重启后保留。
  `Ctrl+=` / `Ctrl+-` make the code in diffs and the conflict view 1 px larger or smaller (10–24 px), and `Ctrl+0` restores 12 px; line numbers and +/- signs scale with it. The size is shared by all tabs and kept across restarts.
- 等宽文字（代码、哈希、路径）改用随应用打包的 Maple Mono CN，中文正好占两个英文字符宽，中文行的选字高亮和横向滚动能对齐。
  Monospace text (code, hashes, paths) uses the bundled Maple Mono CN, whose Chinese characters are exactly two columns wide, so selection highlights and horizontal scrolling line up on Chinese lines.

## Fixes

- 窗口较窄（最小 1024 px）时，提交详情和它的 diff 不再被挤出窗口；过长的作者邮箱显示省略号；diff 不超过 640 px 宽时区块按钮只显示名字，git 命令放在悬停提示里；合并请求列表最多占页面一半。
  In narrow windows (down to 1024 px) the commit detail and its diff stay inside the window, a long author email is truncated, hunk buttons show only their names when the diff is 640 px or narrower with the git command in the tooltip, and the pull / merge request list takes at most half the page.
- 点击右键菜单的菜单项不会再点到菜单下面的行。
  Clicking a context menu item no longer also clicks the row beneath it.
- 对话框的第一个字段是多行输入框时，打开对话框就能直接输入。
  A dialog whose first field is multi-line focuses that field when it opens.

## Validation

- `cargo test --workspace`: all tests passed.
- Change groups, review marks and the worktree overview were exercised in the desktop app through simulated input, including the minimum window size 1024×640 for review marks and the worktree overview; cleanup removed merged worktrees of two repositories and kept their remote branches.
- Code zoom was checked by screenshot at 20 px in a 1024×640 window, and its shortcuts by hand.
- The release build of the desktop app was launched before packaging; the installer was not test-installed. The exe imports no VCRUNTIME / MSVCP DLLs.
- Windows x64 only; the installer is unsigned.

## Downloads

- `PushRight-Setup-1.7.0.exe` — Windows x64 desktop installer.
- `PushRight-1.7.0-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.

The VS Code / Cursor extension is unchanged; extension 1.3.0 stays available from VS Code Marketplace and the [1.6.2 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.6.2).
