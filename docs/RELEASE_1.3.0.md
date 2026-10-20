# PushRight 1.3.0 — Pull and merge requests, large-repository speed

新增「拉取 / 合并请求」页，GitHub 拉取请求和 GitLab 合并请求（含自建服务器）在同一页处理；长列表只渲染看得见的行，大仓库明显更流畅。

A new Pull / merge requests page handles GitHub pull requests and GitLab merge requests, including self-hosted servers, in one place. Long lists render only their visible rows, so large repositories feel much faster.

## Pull and merge requests

侧栏「拉取 / 合并请求」：打开的、我创建的、待我审阅、全部四个筛选；详情里看说明、CI 检查、审阅、可合并状态、提交列表和改动文件（用差异视图）。可检出到本地、合并（GitHub 只列出仓库允许的合并方式；GitLab 可选压缩和删除源分支）、批准、关闭、在浏览器打开、复制链接。从页面或分支右键菜单新建，分支没推送时先推送。

**Pull / merge requests** in the sidebar: open, mine, review requested and all filters; details show the description, checks, reviews, merge status, commits and changed files (in the diff view). Check out, merge (GitHub lists only the methods the repository allows; GitLab offers squash and delete source branch), approve, close, open in the browser or copy the link. Create one from the page or the branch menu; the branch is pushed first when needed.

- 平台按远程地址识别，认不出的自建主机会探测接口；「仓库设置」里可以手动指定平台，并填写托管平台网页地址（网页和远程不在同一个主机端口上时用，例如 SSH 走一个端口、网页在另一个端口）。
  The platform is detected from the remote URL and unknown self-hosted servers are probed. Repository settings can pick the platform and set the web address, for servers whose web host or port differs from the remote (for example SSH on one port and the web interface on another).
- 令牌先从环境变量、`gh` 和 Git 凭据里找；都不能用时可在浏览器里创建（权限已预填）后粘贴。粘贴的令牌验证通过后才存进 Windows 凭据管理器，名字是 `PushRight:<主机>`，只发给同一主机的 API。请求经系统自带的 curl 发出，令牌不出现在命令行里。
  Tokens are taken from environment variables, `gh` or Git credentials; otherwise create one in the browser (scopes prefilled) and paste it. A pasted token is stored in Windows Credential Manager as `PushRight:<host>` only after it is accepted, and is sent only to that host's API. Requests go through the system curl; the token never appears on a command line.
- 不再需要 GitHub CLI。
  GitHub CLI is no longer required.

## Large repositories

- 侧栏分支和标签、本地更改的文件列表、提交和分支对比的文件列表只渲染看得见的行。在约 8.5 万个提交、1012 个分支的仓库上，提交图页每帧从约 30 ms 降到约 5 ms；302 个改动文件的本地更改页从 20–24 ms 降到约 4 ms。
  The sidebar ref lists, Local Changes file lists and commit / comparison file lists render only their visible rows. On a repository with about 85,000 commits and 1,012 branches, a commit-graph frame drops from about 30 ms to about 5 ms; Local Changes with 302 changed files drops from 20–24 ms to about 4 ms.

## Tabs and scrollbars

- 页签上的仓库名和分支名居中、加粗，关闭按钮浮在右边。
  Tabs show the repository and branch names centered and bold, with the close button floating on the right.
- 滚动条常驻不淡出；差异的竖向滚动条从打开起就按整份差异的长度算。
  Scrollbars stay visible; the diff's vertical scrollbar reflects the whole diff from the start.
- 差异底部的横向滚动条占自己的一条高度，滚到底时不再挡住最后一行。
  The horizontal diff scrollbar takes its own strip, so it no longer covers the last line.

## Validation

- `cargo test --workspace`: all tests passed (1 opt-in test ignored); the desktop app builds with no warnings.
- The release build was driven on test repositories: the tab row, a long diff scrolled to its last line with both scrollbars, Repository settings with the platform and web address, the Pull / merge requests page, and the token screen. A self-hosted GitLab whose web interface is on a different port than its SSH remote was reached through the web address setting (checked with an invalid token, which the server rejected). Against GitHub only reading was exercised: list, details, files, and opening and cancelling the create dialog. Creating, merging, approving and closing real pull or merge requests were not exercised.
- The large-repository numbers come from a git/git clone with 1,012 branches.
- The installer was installed silently to a test folder, launched and uninstalled.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension is unchanged and stays at 1.2.0.

## Downloads

- `PushRight-Setup-1.3.0.exe` — Windows x64 desktop installer.
- `PushRight-1.3.0-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
