# PushRight 1.4.1 — New app icon

桌面版换了应用图标，其他功能不变。

The desktop app gets a new icon; nothing else changes.

## New app icon

- 新图标是黄色向右的箭头路牌，上面画一条深色分支：主干带两个提交，分支从主干弯出去指向箭头尖。作者的几个桌面应用（ClaudeTile、MDWisp、Nookery、PushRight、Ramulet）原来都是同款圆角方块，PushRight 和 MDWisp 都是深色底配陶土色，在任务栏里容易点错；现在每个应用的颜色和外形都不一样。
  The new icon is a yellow signpost pointing right with a dark branch on it: a trunk with two commits and a branch curving out toward the tip. The author's desktop apps (ClaudeTile, MDWisp, Nookery, PushRight, Ramulet) all used the same rounded square, and PushRight and MDWisp were both dark with a terracotta accent, so they were easy to mix up in the taskbar; each now has its own color and shape.
- 16～32px 用单独画的小图标，只留路牌和两条粗线，任务栏和标题栏里也看得清。
  16–32 px sizes use a separately drawn small icon with only the signpost and two thick lines, so it stays clear in the taskbar and title bar.
- 界面里的主题色不变。
  The colors inside the app are unchanged.

## Validation

- `scripts/icon.py` regenerated `app.ico`, `app.res` and `icon-128.png`; every size packed in `app.ico` (16–256 px) was checked on light and dark backgrounds.
- The release build was launched on a test repository and its window and taskbar icon checked; the installer was built from it and was not test-installed.
- Windows x64 only; the installer is unsigned. The VS Code / Cursor extension is unchanged and stays at 1.2.0.

## Downloads

- `PushRight-Setup-1.4.1.exe` — Windows x64 desktop installer.
- `PushRight-1.4.1-source.zip` — Source code from the release tag.
- `SHA256SUMS.txt` — SHA-256 checksums for both artifacts.
- The extension VSIX remains on the [1.2.0 release](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0).
