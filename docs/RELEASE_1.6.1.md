# PushRight 1.6.1 — Conflict view fixes

修复冲突视图：有长行的文件、一边删除一边修改的文件、子模块冲突以前没法查看或处理，现在都能看到两边并解决。本地改动页「工作区是干净的」提示不再压住筛选框。

The conflict view is fixed for files with long lines, modify/delete conflicts and submodule conflicts, which could not be viewed or resolved before. The “working tree is clean” hint no longer covers the file filter.

## Conflicts

- 有长行的文件（压缩脚本、长数据表）不再把冲突面板撑出窗口：冲突内容折行显示，顶部和每处冲突的按钮都留在窗口里。
  Files with long lines no longer push the conflict pane past the window: conflict text wraps, and the header and per-conflict buttons stay visible.
- 一边删除（或改名）、另一边修改的文件并排显示留下的版本和被删的一边；选删掉的那边就删除文件（git rm），以前这一步会被 Git 拒绝。
  Modify/delete and rename conflicts show the kept version against the deleted side; choosing the deleted side deletes the file (git rm) instead of failing.
- 子模块冲突显示两边各自指向的提交，可以整个取一边。
  Submodule conflicts show the commit each side points to, and either side can be taken.
- 两边都删除的文件不再报“找不到文件”。
  Files deleted on both sides no longer fail with “file not found”.
- 和 Git 一样只看文件前 8000 字节判断是否二进制：NUL 字节靠后的文本文件可以逐处选择。
  Binary detection looks at the first 8000 bytes like Git, so text files with a late NUL byte keep their conflict blocks.
- 文件里原本就有以 `<<<<<<<` 开头的行时，冲突块不再错位，保存时也不会丢掉这一行。
  A line that merely starts with `<<<<<<<` stays part of the text: blocks no longer shift and the line is no longer dropped on save.
- 冲突标记已在别处改掉的文件可以直接标记为已解决。
  A file whose markers were already removed elsewhere can be marked resolved.
- 读取冲突出错时在冲突面板里显示原因，不再停留在上一个文件的冲突上。
  A read error is shown in the conflict pane instead of leaving the previous file's conflict on screen.

## Local changes

- 「工作区是干净的」显示在空的未暂存列表里，不再压住文件筛选框。
  The “working tree is clean” hint sits in the empty unstaged list instead of covering the file filter.

## Validation

- `cargo test -p engine -p pushright`: all tests passed, including new tests for modify/delete conflicts and literal marker lines.
- A debug build was driven on a test repository holding 28 kinds of conflicts (large blocks, long lines, CRLF, GB18030, BOM, diff3, binary, UTF-16, non-ASCII paths, modify/delete, both deleted, renames, a submodule). Deleting a file, taking a submodule side, marking a marker-free file resolved and resolving a long-line file block by block were each confirmed with `git status`.
- The release build was launched on a conflict test repository; the installer was built from it and was not test-installed. The exe imports no VCRUNTIME / MSVCP DLLs.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension stays at 1.2.0; its take-ours / take-theirs on a modify/delete conflict gets the engine fix with its next release.

## Downloads

- `PushRight-Setup-1.6.1.exe` — Windows x64 desktop installer.
- `PushRight-1.6.1-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
