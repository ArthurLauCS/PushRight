# PushRight 1.1.12 — Windows check output encoding

Windows 提交前检查运行 `just test --error` 等命令时，Python 输出 emoji 可能因 `cp1252` 编码抛出 `UnicodeEncodeError`，嵌套 PowerShell 的中文也可能显示为问号。桌面版和 VS Code／Cursor 扩展共用的检查入口现为子 shell 设置 UTF-8 代码页，并为 Python 标准输入输出设置 `PYTHONIOENCODING=utf-8`；命令的退出状态保持不变。设置仅作用于检查子进程，不修改系统环境变量或被检查仓库的源码。

Windows pre-commit checks now preserve Chinese and emoji output. The shared desktop and extension runner sets the child console code page and Python streams to UTF-8, preventing Python `UnicodeEncodeError` failures and question marks from nested PowerShell output. Command exit status is preserved; the settings apply only to check subprocesses.

## Downloads

- `PushRight_1.1.12_x64-setup.exe` — Windows x64 desktop installer.
- `pushright-win32-x64-1.1.12.vsix` — VS Code / Cursor extension.
- `PushRight-1.1.12-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for all three artifacts.

## Validation and scope

- Windows regression tests reproduced both failures before the fix and passed afterward, including Python started with inherited `cp1252` output encoding and a failing nested PowerShell command.
- The actual shared RPC check runner completed `just test --error` against the FPS project: 3,067 passed, 0 failed, 32 skipped. Chinese and emoji output was preserved.
- Svelte/TypeScript checks passed. Rust workspace tests passed with the merge-pull test excluded to respect the rebase-only policy; the opt-in Python regression was also run explicitly and passed.
- All 57 Node tests passed. The packaged VSIX passed all 26 smoke checks in VS Code and all 26 in Cursor; host logs showed no PushRight activation or duplicate-command registration failures.
- NSIS and VSIX builds passed. The desktop release executable reported version 1.1.12 and passed an isolated startup check. This does not constitute full installer or UI acceptance.
- Windows x64 only. The installer remains unsigned. Native installation/upgrade acceptance and Marketplace publication are outside this release validation.
