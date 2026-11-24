# PushRight 1.6.2 — Clean merge messages, extension 1.3.0

合并、摘取、撤销进行中时提交，不再把 Git 预填说明里的 `# Conflicts:` 注释行写进提交。同时发布 VS Code / Cursor 扩展 1.3.0：随 VS Code 一起启动，跟得上工作树和子模块里在终端做的提交。

Committing during a merge, cherry-pick or revert no longer writes the `# Conflicts:` comment lines of Git's prepared message into the commit. The VS Code / Cursor extension 1.3.0 ships alongside: it starts with VS Code and follows terminal commits in worktrees and submodules.

## Commits

- 合并、摘取、撤销进行中时，提交框预填的是 Git 准备的说明；提交时和 Git 用编辑器提交一样去掉以 `#` 开头的注释行。桌面版和扩展都适用。
  During a merge, cherry-pick or revert the commit box is pre-filled with Git's prepared message; committing strips the lines starting with `#`, as Git does when committing from its editor. This applies to the desktop app and the extension.

## Extension 1.3.0

- 打开含 Git 仓库的工作区时立即启动，不再等其他扩展全部启动完：装了几十个扩展时，源代码管理里的改动从 12–24 秒缩短到 3–4 秒出现；上次留着的对比标签页也能恢复。
  Activates as soon as a workspace with a Git repository opens instead of waiting for every other extension: with a few dozen extensions installed, Source Control shows changes after 3–4 s instead of 12–24 s, and diff tabs from the last session reopen.
- 关联工作树和子模块的 Git 目录在工作区外，以前在终端里提交、切分支后源代码管理不刷新；现在约 1 秒内刷新。
  Linked worktrees and submodules keep their Git directory outside the workspace, so commits and branch switches made in a terminal never refreshed Source Control; now it refreshes within about a second.
- 分支选择框标出已在别的工作树签出的分支，选中时改为在新窗口打开那个工作树，不再执行会被 Git 拒绝的切换。
  The branch picker marks branches checked out in another worktree; picking one opens that worktree in a new window instead of running a checkout Git would refuse.
- 搜索提交可以直接输入内容，回车按提交说明搜，或再选别的范围。
  Search Commits takes the text right away: press Enter to search messages, or pick another field.
- 暂存、取消暂存、忽略和跟踪只重读状态；新文件和还没有提交的仓库在行内作者里显示为未提交，不再报错；没有改动的文件第一次「上一版本」直接看最近一次提交改了什么。
  Staging, unstaging, ignoring and tracking reread only the status; new files and repositories without commits show as uncommitted in inline blame instead of an error; Previous Revision on a file without changes opens the latest commit's diff first.
- 第一条启动改动、一边删除一边修改的冲突选被删的一边直接删除文件、侧栏提交图使用桌面版 1.6 配色，都来自只在 GitHub 发布过的扩展 1.2.1，这次一起上 VS Code Marketplace。
  The faster start above, deleting the file when taking the deleted side of a modify/delete conflict, and the desktop 1.6 palette in the sidebar graph come from extension 1.2.1, which was released on GitHub only; they reach VS Code Marketplace with this version.

## Validation

- `cargo test -p engine`: all tests passed, including a new test that commits a prepared merge message.
- `node --test tests/*.test.mjs`: 45 tests passed; `node tests/vscode-smoke.mjs` passed in a real VS Code.
- In a real VS Code, a commit made from a terminal inside a linked worktree refreshed the extension's Source Control list after about 1 s; extension 1.2.1 never refreshed it.
- An isolated VS Code with 62 installed extensions on a large repository showed the extension's changes after 3.2–4.4 s instead of 11.8–24.4 s.
- The release build of the desktop app was launched before packaging; the installer was not test-installed. The exe imports no VCRUNTIME / MSVCP DLLs.
- Windows x64 only; the installer is unsigned.

## Downloads

- `PushRight-Setup-1.6.2.exe` — Windows x64 desktop installer.
- `pushright-win32-x64-1.3.0.vsix` — VS Code / Cursor extension, Windows x64. The extension also updates from VS Code Marketplace.
- `PushRight-1.6.2-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for all three artifacts.
