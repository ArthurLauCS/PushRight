# PushRight

**See your branches. Push to the right place.**

[English](https://github.com/ArthurLauCS/PushRight/blob/main/extension/README.md) · [简体中文](https://github.com/ArthurLauCS/PushRight/blob/main/extension/README.zh-CN.md)

PushRight brings Git source control, blame, history and a commit graph into VS Code. It makes push destinations explicit and defaults every Pull to **Rebase instead of merge**. English and Simplified Chinese are included; the extension follows VS Code's display language.

**1.2.0**: **PushRight: Open Commit Graph** and **Show in Commit Graph** open the repository in the [PushRight desktop app](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0) instead of an editor tab; a running instance switches to that repository. Install the desktop app first, or set `pushright.desktopPath` to its `pushright.exe`. The VSIX shrinks from 13 MB to 1.7 MB. Choose the date time zone with **PushRight: Time Zone…**.

PushRight adds native Git commands to Chinese action labels across menus, dialogs and the command palette. Open **Repository settings** in Source Control to disable revert for the local repository. The setting is shared with the desktop app and linked worktrees; it blocks starting and continuing revert while allowing abort. Revert is allowed by default. Terminal Git and other tools are unaffected.

**[Download VSIX · Windows x64 · 1.2.0](https://github.com/ArthurLauCS/PushRight/releases/download/v1.2.0/pushright-win32-x64-1.2.0.vsix)** · [Release notes and checksums](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0)

## Preview

![PushRight 1.2.0: commit graph and file differences in the desktop app](https://raw.githubusercontent.com/ArthurLauCS/PushRight/v1.2.0/docs/images/desktop-1.2.0.png)

Open Commit Graph shows the repository in the desktop app, here 1.2.0 with a local demo repository and fictional identities.

![PushRight 1.2.0: Pull defaults to Rebase instead of merge](https://raw.githubusercontent.com/ArthurLauCS/PushRight/v1.2.0/docs/images/pull-rebase-1.2.0.png)

## Time zone

Run **PushRight: Time Zone…** from the command palette or the Source Control menu. Choose **System time zone**, **UTC** or a region such as **Asia/Shanghai**. The preference is saved globally in the editor and applies immediately to all repositories: the sidebar graph, commit details, file and line history, search, blame and worktrees. Daylight saving follows the commit's date. The desktop app and each editor keep separate preferences.

## Diff colors

Run **PushRight: Diff Colors…** from the command palette or Source Control menu. Choose Blue / Orange, Purple / Gold, Cyan / Rose, or custom added/deleted HEX colors. Applies to PushRight and native VS Code / Cursor diff editors, including Review Branch. This updates user-level `workbench.colorCustomizations`; workspace overrides may take precedence. **Restore original colors** restores the previous values while preserving unrelated settings and later manual edits.

## Diff tools and file tree

Diffs in the desktop app have seven tools above them: ignore whitespace, show hidden symbols, word wrap, fewer / more context lines, show entire file, and side by side. Changed files can be shown as a folder tree, filtered, and staged a folder at a time. Native diff editors do not repeat these buttons; use the settings VS Code / Cursor already provide:

| Tool | Native diff setting |
| - | - |
| Ignore whitespace | `diffEditor.ignoreTrimWhitespace` (the whitespace icon at the top right of the editor) |
| Show hidden symbols | `editor.renderWhitespace` |
| Word wrap | `diffEditor.wordWrap` |
| Context lines, entire file | `diffEditor.hideUnchangedRegions.enabled` and `contextLineCount` |
| Side by side | `diffEditor.renderSideBySide` |

The Source Control panel switches between tree and list with its own **View as Tree**. Files under an expanded commit in the sidebar graph are indented by folder.

## Requirements

- Windows 10 / 11 x64, VS Code 1.90 or newer, and Git on PATH.
- A trusted local Git workspace. Linux, macOS, WSL, SSH and browser workspaces are not supported by this build.
- Use your existing Git credential helper or SSH configuration for authentication.

## Push to the intended branch

If local `feature/login` accidentally tracks `origin/main`, PushRight warns about the mismatch and recommends `origin/feature/login`. You choose the destination before it pushes with an explicit remote and refspec. Force push uses `--force-with-lease --force-if-includes` and requires an additional confirmation.

Pull asks every time, with **Rebase instead of merge** checked. Keep it checked when your team requires linear pull history. Automatic stashing is disabled; commit or stash local changes first. Resolve conflicts and use **Continue / Abort** when Git pauses.


New branch names in the creation dialogs must match `(fix|feat)_(S|C|SC)_lowerCamelCase`, for example `feat_S_userLogin` or `fix_SC_branchPicker`. The suffix starts with a lowercase letter and contains only ASCII letters and digits. The desktop new-branch dialog and editor picker let you choose a starting branch; the desktop new-worktree dialog also validates new branch names. Existing branch names are unaffected.

The branch picker lists local branches before remote branches in a fixed order and ticks the checked-out branch. Each branch shows its latest commit author, short ID, subject and age; typing can search commit details. Use the graph's top branch button to select Auto, All or a branch without checking it out.

## Source control and history

- Review staged, unstaged and conflicted files; stage, unstage or discard files or selected lines; commit and amend.
- Inspect current-line blame, author hover cards, CodeLens and whole-file blame (`Alt+B`). Apply backgrounds and borders to individual authors.
- Keep **PushRight > Line History** expanded to follow the clicked line or selected range automatically, without a context-menu command or moving focus away from the editor. Rapid cursor moves are coalesced and hidden views do not query history.
- Browse file history; compare revisions (`Alt+,` / `Alt+.`); search by message, author, file, changed text or commit ID.
- **PushRight Graph** appears below changes in **Source Control**, including when built-in Git is disabled. It opens automatically once per workspace. Click a commit to expand its changed files in place; click a file for a native diff preview, or double-click to keep the diff open. Arrow keys navigate commits and files; Left/Right collapse/expand.
- Choose **Auto**, **All branches**, or a local/remote branch. Auto shows HEAD, its upstream and a remote base (the existing VS Code base setting, the branch creation source, then the remote default branch). Selection is remembered per repository and does not check out a branch. Fetch, Pull and Push act on the displayed repository's **checked-out branch**; the graph's Push always targets a remote branch with the same name and confirms the destination.
- Initialized submodules and nested repositories have separate repository entries, graphs and source controls. Child names include their parent path. Use **Refresh** after adding a repository; workspace-folder additions are detected automatically. An ignored nested repository is also detected when you open one of its files.
- New folders list their untracked files individually, so you can inspect and stage each file before committing. Ignored files stay excluded.
- Run **PushRight: Open Commit Graph**, or use the sidebar graph's title button, to browse branches, commits and file differences in the desktop app. If the sidebar graph was hidden, enable **PushRight Graph** in Source Control's **Views** menu.
- Change the commit name and email for the current repository.
- The commit box is pre-filled with the repository's commit message template. **PushRight: Recent Commit Messages…** reuses an earlier message; **PushRight: Run Pre-Commit Check** runs the repository's check command. Set the template and the check command under **Repository settings** in the desktop app.
- The sidebar graph has **Tags** and **First parent only** toggles.
- The **Worktrees** view on the PushRight page lists the repository's worktrees. Click one to open it in a new window (the current window stays), use **+** to create one from an existing or a new branch, and the trash button to remove one. Removing deletes only the folder; the branch and its commits remain.
- In the desktop app, choose **Repository tools** to compare revisions, mark reviewed files, manage remote URLs, edit rebase actions/order, and recover reflog entries into new branches. Clone and initialize are available on the welcome page.
- File history, line history and search load 200 entries per page with **Load more**. Unchanged selections in modified or unsaved buffers are mapped back to HEAD for line history.
- Preview image changes side by side and large patches in read-only pages of 1,000 lines.
- Right-click a repository and choose **Close Repository** to keep its files open without rediscovering it on file switches, refreshes or reloads. Use **PushRight: Reopen Closed Repository…** to restore it.

On first use, PushRight offers to turn off built-in Git, so push and pull go through its checks. This is optional. Extensions that depend on built-in Git may stop working when it is disabled. If you keep GitLens enabled, turn off duplicate blame annotations in one extension.

## Private local files

Expand **Source Control > PushRight Local Files** to browse **Untracked files** and **Ignored files** for each repository. Ignored directories load their direct children only when expanded.

Right-click a file in Explorer to choose:

| Action | Effect |
| --- | --- |
| Ignore Locally | Add its exact path to `.git/info/exclude`; suitable for private skills, never committed or shared. |
| Add to .gitignore | Add its exact path to a rule file that can be committed and shared. |
| Track File (Stage) | Stage the file, even if ignored. It will be included in the next commit. |
| Stop Tracking and Keep Locally | Keep the working file, remove it from the index and ignore it locally. For committed files this **stages a deletion for the team**; the confirmation explains this before applying it. |

Ignore rules do not hide changes to tracked files. Stop-tracking refuses to discard independently staged contents. To undo an ignore rule, edit the corresponding `.gitignore` or `.git/info/exclude`; force-tracking a file does not remove its rule.

## Current limits

- PR / Issue and hosting-provider collaboration are not included yet. PushRight does not replace those parts of GitLens or GitHub Pull Requests.
- Submodules must already be initialized. Ignored directory trees are not scanned for repositories in the background; open a file inside one or add it as a workspace folder.
- Line history reports selections containing new or modified lines that cannot map to HEAD; use file history or commit first.
- Native text revisions default to 4 MiB; `pushright.history.maxFileSizeMiB` raises this up to 64 MiB. Large patches are read-only pages; lines over 64 KiB are clipped. Image previews support PNG/JPEG/GIF/WebP/BMP/ICO, up to 32 MiB per side. Other binary files support whole-file operations.
- Interactive rebase edits linear commits after an ancestor of HEAD, excluding merge ranges and the root commit, and retains a `pushright-backup/*` branch. Reflog recovery creates a new branch without resetting the current one.
- Full commit history loads in the background. See the [performance report](https://github.com/ArthurLauCS/PushRight/blob/main/docs/PERFORMANCE.md) for measured limits.

## Install from VSIX

Download the Windows x64 VSIX from [GitHub Releases](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0), then use **Extensions: Install from VSIX** in VS Code or Cursor, or:

```sh
code --install-extension pushright-win32-x64-1.2.0.vsix
cursor --install-extension pushright-win32-x64-1.2.0.vsix
```

Report problems in [GitHub Issues](https://github.com/ArthurLauCS/PushRight/issues), including the version and reproduction steps. Remove credentials and private repository information from logs before sharing.

Third-party notices ship in `THIRD_PARTY_NOTICES.txt`. PushRight is licensed under the [MIT License](https://github.com/ArthurLauCS/PushRight/blob/main/LICENSE).
