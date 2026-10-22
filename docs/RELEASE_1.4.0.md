# PushRight 1.4.0 — Line comments and zen mode

拉取 / 合并请求的差异里可以选行发评论、查看已有的行评论；GitLab 上不是审核者时操作前会提醒；新增禅模式，差异铺满窗口。

Pull / merge request diffs can now be commented on line by line, with existing line comments shown in place. GitLab reminds you before acting on a merge request you are not reviewing, and a new zen mode lets a diff fill the window.

## Line comments

在拉取 / 合并请求改动文件的差异里，按住左键在行号上拖选几行，松开就弹出评论框，评论立即发出（GitHub 是审阅评论，GitLab 是差异讨论，多行时带行范围）。

In a pull / merge request's changed files, hold the left button on the line numbers and drag over the lines; releasing opens the comment dialog and the comment is posted at once (a GitHub review comment, or a GitLab diff discussion with a line range when several lines are selected).

- 鼠标停在行号上会出现「+」；在代码上拖动仍然是选中文字复制。
  A "+" appears when the pointer is over a line number; dragging over code still selects text to copy.
- 已有的行评论显示在对应行下面（作者、时间、正文），统一和分栏视图都有；针对旧提交、已过时的评论不显示。
  Existing line comments show under their lines with author, time and text, in both unified and split views; outdated comments on earlier commits are not shown.
- 评论没发出去时，评论框带着原文和错误原因重新弹出。
  If a comment fails to post, the dialog reopens with your text and the error.

## Reviewer reminder (GitLab)

- 合并请求指定了审核者、你既不是审核者也不是作者时，批准、评论、合并和关闭前会先提醒，仍然可以继续。没有指定审核者或你是作者时不提醒。
  When a merge request has reviewers and you are neither one of them nor the author, approve, comment, merge and close show a reminder first and can still go ahead. There is no reminder when no reviewers are set or you are the author.

## Zen mode

- 差异工具栏最右边的按钮让文件列表和差异铺满应用窗口，顶栏、侧栏和页签都隐藏；提交详情、贮藏、比较、拉取 / 合并请求的文件和变基预览里都能用。再点按钮或按 Esc 退出。
  The rightmost diff toolbar button fills the app window with the file list and the diff, hiding the toolbar, sidebar and tabs. It works in commit details, stashes, comparisons, pull / merge request files and rebase previews. Click it again or press Esc to leave.
- 对话框、菜单和帮助浮层按 Esc 只关闭它们自己，不会连带退出禅模式。
  Esc in a dialog, menu or help popover closes only that, without leaving zen mode.

## Pull / merge request list

- 列表和详情之间的分隔条可以拖动调宽（280–720 px），宽度会记住。
  Drag the divider between the list and the details to resize the list (280–720 px); the width is remembered.

## Validation

- `cargo test --workspace`: all tests passed (1 opt-in test ignored); the desktop app builds with no warnings.
- A debug build was driven on a self-hosted GitLab merge request: dragging over line numbers opened the comment dialog for the selected range and Esc cancelled it; zen mode was entered and left with the button and with Esc; Esc in the comment dialog inside zen mode closed only the dialog; the list divider resized the list. Reviewer reminders were checked on merge requests with and without reviewers. Existing comments were rendered from injected sample data in unified and split views, because the test merge requests had none.
- The automated checks did not post comments, approve, merge or close; the features were then tried by hand.
- The release build was launched on a test repository; the installer was built from it and was not test-installed.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension is unchanged and stays at 1.2.0.

## Downloads

- `PushRight-Setup-1.4.0.exe` — Windows x64 desktop installer.
- `PushRight-1.4.0-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
