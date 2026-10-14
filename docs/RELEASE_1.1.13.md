# PushRight 1.1.13 — Resizable Local Changes sections

本地更改中的「未暂存」「已暂存」「新提交作者」三个区域现在可以点击标题折叠、展开，并通过分隔线拖动调整高度。折叠中间栏后，仍可调整上下两个展开区域；折叠保留提交草稿和文件树状态。分隔线支持方向键调整、双击或 Home 恢复默认尺寸。文件列表与提交区缩小时各自滚动，矮窗口下仍可访问操作按钮。

Local Changes now has resizable, collapsible Unstaged, Staged and Commit Author sections. Drag a divider to resize neighboring expanded sections, including when the middle section is folded. Folding preserves the commit draft and file-tree state. Arrow keys resize; double-click or Home restores default sizes. Lists and the commit form scroll independently when space is limited.

This applies to the desktop app and the extension's shared full panel. Native editor Source Control keeps its existing layout. Section sizes and folding are retained while the panel is mounted; they are not saved across application restarts.

## Downloads

- `PushRight_1.1.13_x64-setup.exe` — Windows x64 desktop installer.
- `pushright-win32-x64-1.1.13.vsix` — VS Code / Cursor extension.
- `PushRight-1.1.13-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for all three artifacts.

## Validation and scope

- Browser checks with 150 file entries verified pointer and keyboard resizing, all-folded and middle-folded layouts, double-click reset, draft retention, independent identity actions, and scrolling in short/narrow layouts.
- Svelte/TypeScript checks passed with no errors or warnings; all 57 Node tests passed.
- The packaged VSIX passed all 26 smoke checks in VS Code and all 26 in Cursor. Cursor's first run failed while constructing a file-change argument; a fresh run in the normal temporary directory passed. Successful host logs showed no PushRight activation or duplicate-command registration errors.
- NSIS and VSIX builds passed. The desktop executable reported version 1.1.13 and passed an isolated startup check; this is not full installer acceptance.
- Rust workspace validation: 46 passed, 1 failed, 1 opt-in Python test ignored, and 1 merge-pull test excluded to respect the rebase-only policy. The existing test `external_tools_get_the_right_files_and_resolve_conflicts` failed its invalid-tool assertion in both parallel and serial runs. Its unchanged implementation waits 1.5 seconds for startup failure; a direct invalid-tool invocation took about 1.56 seconds on this machine. This release does not change the external-tool runner.
- Windows x64 only. Native installation/upgrade acceptance, signing and Marketplace publication are outside this release validation.
