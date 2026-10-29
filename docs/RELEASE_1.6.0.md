# PushRight 1.6.0 — New look

桌面版换了一套更扁平、更克制的外观，仓库页签和标题栏也一并调整；扩展里的提交图用同一套配色。功能没有变化。

The desktop app has a new, flatter and calmer look, with reworked repository tabs and title bar. The extension's commit graph uses the same palette. No functional changes.

## Look

- 配色、圆角、边线和阴影重新定了一套，浅色和深色主题各有一组，列表选中、按钮、对话框、差异和冲突视图都按同一套取色。
  A new set of colors, radii, borders and shadows for both light and dark themes, applied to lists, buttons, dialogs, the diff and the conflict view.
- 每种用途一种字体：界面用 Segoe UI Variable Text，标题用 Noto Serif SC，正文用 LXGW WenKai，代码和哈希用 Cascadia Code。
  One font per role: Segoe UI Variable Text for the interface, Noto Serif SC for titles, LXGW WenKai for prose, Cascadia Code for code and hashes.
- 仓库页签带底线，页签栏右侧是新建页签按钮。
  Repository tabs sit on a bottom rule, with a new-tab button at the end of the strip.
- 标题栏跟随主题：深色主题下 Windows 的标题栏也是深色。
  The title bar follows the theme: with the dark theme the Windows title bar is dark too.
- 扩展的提交图（侧栏的图和网页视图）改用同一套配色。
  The extension's commit graph uses the same palette. The extension itself is not re-released.

## Runtime

- 程序静态链接 VC 运行库，不再依赖 Visual C++ 可再发行组件包。
  The app links the VC runtime statically and runs without the Visual C++ Redistributable.

## Unchanged

- 1.5.0 的 GitLab 指派人、审核者和可筛选的下拉框保持不变，对话框里的下拉框使用新配色。
  The 1.5.0 GitLab assignee and reviewer fields and searchable dropdowns are unchanged and use the new colors.

## Validation

- `cargo test -p pushright`: all tests passed; `npm run check` (svelte-check and the extension type check): no errors.
- A debug build was driven on a test repository in light and dark themes (graph, diff, dialogs, tabs). The pull / merge request page and the GitLab create dialog could not be driven without an account, so only their code was checked against the new theme.
- The release build was launched on a test repository; the installer was built from it and was not test-installed. The app now links the VC runtime statically (`.cargo/config.toml`), so it no longer needs the Visual C++ Redistributable; the exe imports no VCRUNTIME / MSVCP DLLs.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension stays at 1.2.0.

## Downloads

- `PushRight-Setup-1.6.0.exe` — Windows x64 desktop installer.
- `PushRight-1.6.0-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
