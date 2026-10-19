# PushRight

**See your branches. Push to the right place.**

[English](README.md) · [简体中文](README.zh-CN.md)

A Git desktop client written in Rust, with a native GPU-rendered interface (gpui) instead of a web view. PushRight makes push destinations explicit, recommends a remote branch with the same name, and defaults Pull to **Rebase instead of merge**.

## Why PushRight

1. **Pushes go to the right place.** Every push states its remote and branch. When the local branch and its upstream have different names, PushRight asks you to choose and recommends the same-name remote branch. Force push uses `--force-with-lease --force-if-includes`.
2. **The Git command sits next to the action.** In the Chinese interface, menus, dialogs and buttons append the command they run; toolbar buttons show it in the tooltip. The command log lists every command executed and its output, so you learn what to type in a terminal.
3. **Consequences are explained before you confirm.** Merge, rebase, reset, discard and similar operations describe the outcome in plain language, with before/after diagrams and recovery advice. Some operations offer one-click undo.
4. **Color-vision-friendly diffs.** Blue / Orange, Purple / Gold and Cyan / Rose presets, or independent custom added/deleted colors. Diff lines keep + / − signs and borders, so color is never the only cue. The extension applies the same palette to native VS Code / Cursor diffs.
5. **One extension instead of several.** The VS Code / Cursor extension combines source control, end-of-line authorship, whole-file blame, CodeLens, file history, line history, previous / next revision, comparison with any revision, commit search and the commit graph.
6. **Automatic refresh and fetch.** The desktop app refreshes open repositories every minute and fetches when they open and every ten minutes, including background tabs. It updates remote refs only: no automatic pull, merge or working-file changes. Each can be turned off.
7. **Pull rebases by default.** Every Pull asks, with **Rebase instead of merge** checked.
8. **Conflicts are resolved block by block.** Ours and theirs appear side by side for each conflict; in-progress operations can be continued or aborted at any time.
9. **Worktrees and repository tools.** Worktree overview, arbitrary revision comparison, reviewed-file marks, remote URLs, interactive rebase and reflog recovery are available in the interface.
10. **Works on large repositories.** Measured on a Linux repository with 1.48 million commits and a 128 MiB file; see the [performance report (Chinese)](docs/PERFORMANCE.md).
11. **English and Simplified Chinese, light and dark themes**, with individual author styles.
12. **The desktop app and extension share one engine.** The extension includes its own engine; its Open Commit Graph command opens the desktop app. MIT licensed.

The extension updates automatically when installed from VS Code Marketplace. The desktop app has no automatic updates yet; download the installer for each new version.

## This release

Version **1.2.0** rewrites the desktop app in Rust with a native GPU-rendered interface, replacing the Tauri web view. The look, layout, features and both languages stay the same. On the same machine, a repository opens in about 0.5–0.7 s using one process and about 124 MB of memory, down from 7 processes and about 415 MB. WebView2 is no longer required. See the [release notes](docs/RELEASE_1.2.0.md).

In VS Code / Cursor, **PushRight: Open Commit Graph** and **Show in Commit Graph** now open the repository in the desktop app; if it is already running, it switches to that repository. The VSIX shrinks from 13 MB to 1.7 MB. Choose the date time zone with **PushRight: Time Zone…**.

Choose **System time zone**, **UTC** or a region such as **Asia/Shanghai** in the top-right time-zone dropdown of the desktop app. The setting applies to every repository in that app, including commit details, history, blame, worktrees and the dates and months in repository statistics. Changes apply immediately and survive restarts; daylight saving follows each commit's date. The desktop app and each editor store their preferences separately.

Chinese action labels append native Git commands across menus, dialogs, buttons and extension commands; toolbar buttons show the name only, with the command in the tooltip. **Repository settings**, available in the desktop toolbar and extension Source Control menu, can disable revert for the local repository. Both apps and linked worktrees share `pushright.disableRevert` in the local Git config; other repositories are unaffected. Revert remains allowed by default. When disabled, PushRight blocks starting or continuing revert while allowing abort. Terminal Git and other tools are outside this setting's scope.

The desktop app refreshes open repositories every minute and fetches when they open and every ten minutes, including background tabs. Both defaults can be disabled independently in **Automatic refresh and fetch** beside the language selector. Busy repositories defer work; failures appear in the command log and retry on schedule. Background fetch preserves `FETCH_HEAD`, the index and working files. Force push also uses `--force-if-includes` to protect remote commits fetched in the background. These timers run while the desktop app is open, not after it exits; the extension does not enable them.

**v1.2.0 (MIT licensed)** is available for Windows x64. It starts in English and includes complete English and Simplified Chinese UI packs. Switch languages in the top-right corner; your choice is remembered.

![PushRight 1.2.0: commit graph, branches and file differences](docs/images/desktop-1.2.0.png)

Screenshots show the actual 1.2.0 desktop app with a local demo repository and fictional identities.

Source code for this release is available as [ZIP](https://github.com/ArthurLauCS/PushRight/archive/refs/tags/v1.2.0.zip) or [tar.gz](https://github.com/ArthurLauCS/PushRight/archive/refs/tags/v1.2.0.tar.gz).

## Download and install

**[Download desktop installer](https://github.com/ArthurLauCS/PushRight/releases/download/v1.2.0/PushRight-Setup-1.2.0.exe)** · **[Download VSIX (VS Code / Cursor)](https://github.com/ArthurLauCS/PushRight/releases/download/v1.2.0/pushright-win32-x64-1.2.0.vsix)** · [SHA256 checksums](https://github.com/ArthurLauCS/PushRight/releases/download/v1.2.0/SHA256SUMS.txt)

Download `PushRight-Setup-1.2.0.exe` from [GitHub Releases](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0) and run it. It installs for the current user under `%LOCALAPPDATA%\Programs\PushRight` and needs no administrator rights.

- Windows 10 / 11 x64; built and checked on Windows x64.
- Install [Git for Windows](https://git-scm.com/downloads/win) and make `git` available on PATH.
- The installer offers English and Simplified Chinese. App language is selected independently.
- The installer is unsigned; Windows may show an unknown-publisher prompt. Verify against the release's `SHA256SUMS.txt` using `Get-FileHash .\PushRight-Setup-1.2.0.exe -Algorithm SHA256`.

Upgrading from 1.1.x removes the previous version automatically. Preferences are now stored in `%APPDATA%\PushRight\settings.json`; language, theme, time zone, diff colors, author styles and recent repositories from 1.1.x are not carried over and need to be set once again.

Configure your identity in Git or edit it per repository in the app:

```sh
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

PushRight uses system Git credentials or SSH configuration. Complete authentication in a terminal first if needed; the app does not provide account sign-in or interactive terminal prompts.

New branch names in the creation dialogs must match `(fix|feat)_(S|C|SC)_lowerCamelCase`, for example `feat_S_userLogin` or `fix_SC_branchPicker`. The suffix starts with a lowercase letter and contains only ASCII letters and digits. The desktop new-branch dialog and editor picker let you choose a starting branch; the desktop new-worktree dialog also validates new branch names. Existing branch names are unaffected.

## Usage

Open a local repository, or pass its path when launching:

```powershell
& "C:\path\to\pushright.exe" "C:\path\to\repository"
```

### Diff colors

The toolbar’s **Diff Colors** dialog has a color picker each for added and deleted lines, with the theme colors and Blue / Orange, Purple / Gold and Cyan / Rose swatches, and previews the result as you pick. Preferences persist locally; **Restore original colors** follows the app theme again. Diff lines retain + / − markers and colored borders.

### Commit and push

1. Review changes, stage files, hunks or selected lines, and enter a commit message.
2. Check the remote and branch before pushing. When a branch name differs from its upstream, the app requires an explicit choice and recommends the same-name remote branch.
3. Push uses an explicit remote and branch refspec. Force push uses `--force-with-lease --force-if-includes`; check the destination and expected effect first.

For example, if local `feature/login` accidentally tracks `origin/main`, the dialog recommends `origin/feature/login` and makes the mismatch visible.

### Pull with Rebase instead of merge

Click **Pull**. The dialog shows the upstream and checks **Rebase instead of merge** every time, even if the previous pull used merge. The dialog cannot be skipped with “Don't show again.”

- Checked: explicitly runs `git pull --rebase`, overriding `pull.rebase=false` and branch-level merge preferences.
- Unchecked: explicitly runs `git pull --no-rebase`, which may create a merge commit. Keep the box checked when your team requires it.
- Commit or stash unfinished work first. Automatic stashing and updates to other local branch refs are disabled for this operation.
- On conflicts, resolve and **Continue**, or **Abort** to restore the previous local state. During rebase, “ours” is upstream plus commits already replayed; “theirs” is the local commit being replayed.

![PushRight 1.2.0: Pull with Rebase instead of merge checked by default](docs/images/pull-rebase-1.2.0.png)

This is a **pull strategy** for any current branch. It does not determine how a feature branch merges into `dev`, `main` or `master`, or approve a hosted PR/MR. Follow the repository's separate merge policy. Rebase rewrites replayed commit IDs; coordinate before rewriting shared history. See the [Git pull reference](https://git-scm.com/docs/git-pull).

### Individual author styles

Right-click an author to set that person's background and border. Styles use the raw email, falling back to the name only when email is absent. Identical names with different emails remain separate. Settings are saved locally across repositories.

Left-click an author to highlight their commits temporarily. Keyboard users can focus an author and press `Shift+F10` to open styling.

### Commit identity

The **Local changes** commit panel shows the effective Git name and email. **Edit identity** saves repository-level `user.name` and `user.email`, without changing global configuration or existing commits. Linked worktrees share repository configuration.

The app reports effective identities and overrides from environment variables, `author.*`, `committer.*` or worktree configuration. Amending preserves the original author; the panel shows the current committer.

### Large repositories and files

The initial view loads 2,000 commits; full history follows in the background. The graph uses a compact skeleton, checkpoints every 1,024 rows and visible-window metadata. Existing branch switches and ref renames reuse the graph when the set of starting commit IDs stays unchanged.

Editable diff reads retain at most 4 MiB + 1 byte. Larger patches or previews beyond 5,000 lines offer read-only pages of 1,000 lines; lines beyond 64 KiB are clipped with a notice. Whole-file staging and commits remain available.

Measurements on a local Linux repository with 1.48 million commits and a 128 MiB file, including scope and reproduction commands, are in the [performance report (Chinese)](docs/PERFORMANCE.md).

### Included features

- Repository tabs on their own row, sharing its width; drag to reorder, double-click or right-click to rename (handy for several worktrees of one repository). Each tab shows the checked-out branch. Recent repositories and session restoration.
- Virtualized graph, branches, tags, commit details and file diffs.
- Stage, unstage, commit and amend; file, hunk and selected-line operations.
- Create, switch, rename and delete branches; merge, rebase, cherry-pick, revert, reset and tags.
- Fetch, explicit Pull mode, destination-aware Push and stashes.
- Worktrees, conflict resolution, Continue and Abort.
- Repository tools: arbitrary revision/branch and merge-base comparisons, reviewed files, remote URLs, interactive rebase and reflog recovery.
- Before/after PNG/JPEG/GIF/WebP/BMP/ICO previews; clone and initialize from the welcome page.
- English / Simplified Chinese, light / dark themes, resizable panels, individual author styles, operation explanations and command logs.

App text, tooltips, explanations and app-generated errors are translated. Repository content, commit messages, branch names and raw Git/OS output stay verbatim. Native OS dialogs use the system language.

## VS Code / Cursor extension

Extension **1.2.0** opens the commit graph in the desktop app and adds **PushRight: Time Zone…**. The sidebar graph keeps inline commit file lists and native diffs, Auto / All / branch filters, same-name push, submodule discovery, a fixed-order branch list with the checked-out branch ticked, and batch local ignore / tracking controls.

The same engine runs inside VS Code. Install the Windows x64 extension from [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=AthurLau.pushright) or download the VSIX from [GitHub Releases](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0). See the [extension guide](extension/README.md) for requirements and limits. To build from source:

```sh
pnpm install --frozen-lockfile
pnpm ext:package
code --install-extension extension/pushright-win32-x64-1.2.0.vsix
```

- **Source Control panel**: staged, unstaged and conflicted files; stage, unstage or discard files or selected lines; commit and amend; gutter change markers and Explorer badges. On first use PushRight explains what turning off VS Code's built-in Git means and lets you decide.
- **Push and pull checks**: Pull asks every time with **Rebase instead of merge** checked. Push recommends the same-name remote branch when the upstream has a different name, and the status bar warns about the mismatch. Force push uses `--force-with-lease --force-if-includes` and asks twice.
- **Authors in the editor**: author, date and subject at the end of the current line, with a hover card; whole-file blame (`Alt+B`) colored by author or by age; CodeLens above files, classes and functions; a status bar item. Author styles set in the commit graph also apply here, and "Highlight Only This Line's Author" marks that author's lines in the file and scrollbar.
- **History**: file history that follows the active editor, line history for a selection, previous / next revision (`Alt+,` / `Alt+.`), compare with any branch, tag or commit, and commit search by message, author, file, changed text or ID.
- **Commit graph**: "PushRight: Open Commit Graph" and "Show in Commit Graph" open the repository in the PushRight desktop app (an already running instance switches to it). Set `pushright.desktopPath` if the app is not installed in the default location.

Not included yet: macOS, Linux and remote (WSL / SSH) builds, PR / Issue integration. Submodules must already be initialized.

## Current limits

- PR / Issue and hosting-provider collaboration are not delivered yet.
- Visible rows are virtualized, but complete history is still read in the background.
- Large patches offer read-only pages that rescan the Git stream; Git may still consume time and memory before producing output. Partial staging/discard remains limited to 4 MiB patches.
- The graph has at most 24 lanes, reduced by available width. Highly parallel histories can omit connections.
- No automatic updates or macOS / Linux installers yet.
- Diff text cannot be selected or copied with the mouse yet.
- Undo covers some local operations, not every Git action. Check prompts before destructive operations.

## Development

Requires Rust stable, MSVC C++ tools and Windows SDK for the desktop app; Node.js 24 and pnpm for the extension; [Inno Setup 6](https://jrsoftware.org/isinfo.php) for the installer.

```powershell
.\scripts\fonts.ps1                       # downloads the two bundled CJK fonts (about 50 MB, not in git)
cargo run -p pushright -- C:\path\to\repo
cargo test --workspace
pnpm install --frozen-lockfile
pnpm check
pnpm test
pnpm desktop:build   # installer: target\installer\PushRight-Setup-<version>.exe
pnpm ext:build       # VS Code extension: engine sidecar, sidebar graph and host
pnpm ext:smoke       # runs the extension inside a throwaway VS Code profile
```

The app icon comes from `crates/app/assets/icon.svg` and `icon-small.svg`; after editing them run `python scripts/icon.py`.

| Path | Responsibility |
| --- | --- |
| `crates/app/` | Desktop app: native interface (gpui), English and Simplified Chinese text, icon and fonts |
| `crates/engine/` | Rust Git engine: gix reads and Git CLI operations |
| `crates/sidecar/` | The engine as a child process for the VS Code extension |
| `extension/` | VS Code extension: source control, blame, history and the sidebar commit graph |
| `src/lib/` | Svelte components and text packs used by the extension's sidebar graph and host |
| `installer/` | Inno Setup script for the desktop installer |
| `tests/` | Extension and shared-module regression checks |

## Feedback and licenses

Report problems in [Issues](https://github.com/ArthurLauCS/PushRight/issues) with the version, OS, reproduction steps and relevant logs. Remove credentials and private repository information before sharing logs.

Third-party notices are in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) and included in the installation. PushRight is licensed under the [MIT License](LICENSE).
