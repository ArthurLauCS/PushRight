# PushRight 1.5.0 — Assignee and reviewer for GitLab merge requests

新建 GitLab 合并请求时可以选指派人和审核者，之后还能修改审核者；对话框里的长下拉框可以输入筛选。

GitLab merge requests can now be created with an assignee and a reviewer, and the reviewer can be changed later. Long dropdowns in dialogs can be filtered by typing.

## Assignee and reviewer (GitLab)

- 新建合并请求的对话框多了「指派人」和「审核者」，从能访问这个项目的成员里选（含从群组继承的，已停用的账号不列出）。
  The create dialog has Assignee and Reviewer fields, chosen from the members who can access the project (including members inherited from groups; blocked accounts are not listed).
- 指派人默认是自己；审核者默认是这个项目上次指派的人。指派过的审核者按项目记在本机，排在选项最前面，最近的在前。
  The assignee defaults to you; the reviewer defaults to the one you picked last for that project. Reviewers you have picked are remembered per project on this computer and listed first, most recent first.
- GitLab 免费版每个合并请求只能有一个指派人和一个审核者，所以两项都是单选，也可以选「无」。
  GitLab Free allows one assignee and one reviewer per merge request, so both are single choices, and either can be None.
- 打开的合并请求有「修改审核者」按钮，默认选中当前审核者，可以换人或去掉。没有编辑权限时 GitLab 不报错也不修改，PushRight 会核对结果并提示。
  Open merge requests have a Change reviewer button that starts on the current reviewer and can switch or clear it. Without edit rights GitLab silently keeps the old reviewer; PushRight checks the result and says so.
- 概览的「审阅」一栏显示当前的审核者。
  The overview's Reviews section shows the current reviewers.
- 不是审核者时批准、评论、合并和关闭前的提醒不变，仍然可以继续。
  The reminder before approving, commenting on, merging or closing a merge request you are not reviewing is unchanged, and you can still go ahead.

## Searchable dropdowns

- 对话框里超过 12 项的下拉框（目标分支、项目成员等）显示的搜索框原先输入后不筛选，现在按显示的文字筛选，不区分大小写。
  Dialog dropdowns with more than 12 options (target branches, project members and others) showed a search box that did not filter; typing now filters by the shown text, ignoring case.

## Validation

- `cargo test --workspace`: all tests passed; the desktop app builds with no warnings.
- A debug build was driven against a local mock of the GitLab API with 105 members over two pages and one blocked account: the create dialog defaulted to the user as assignee, members on the second page could be found by search, the request carried `assignee_ids` and `reviewer_ids`, the reviewer was changed, cleared and preselected, previously picked reviewers were listed first and the last one was the default, and a server that ignored the change produced the error.
- The release build was launched on a test repository; the installer was built from it and was not test-installed.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension is unchanged and stays at 1.2.0.

## Downloads

- `PushRight-Setup-1.5.0.exe` — Windows x64 desktop installer.
- `PushRight-1.5.0-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
