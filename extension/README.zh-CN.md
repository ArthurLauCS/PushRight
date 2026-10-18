# PushRight

**看清分支，推送到正确的位置。**

[English](https://github.com/ArthurLauCS/PushRight/blob/main/extension/README.md) · 简体中文

PushRight 在 VS Code 中提供 Git 源代码管理、Blame、历史和提交图。推送前明确目标，每次 Pull 默认勾选 **Rebase instead of merge**。包含中英文界面，扩展语言跟随 VS Code。

**1.2.0 正式版**：**PushRight: 打开提交图** 和 **在提交图中定位** 改为用 [PushRight 桌面版](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0)打开仓库，不再在编辑器页签里显示；桌面版已在运行时直接切换到该仓库。需要先安装桌面版，或把 `pushright.desktopPath` 设为它的 `pushright.exe`。VSIX 从 13 MB 减到 1.7 MB。日期时区改用 **PushRight: 时区…** 选择。

中文菜单、弹窗选项和命令面板附加对应 Git 命令。源代码管理中的“仓库设置”可禁用当前仓库的 revert，与桌面版及关联工作树共享；禁用后阻止发起和继续 revert，仍允许中止。默认允许 revert，此设置不限制终端或其他 Git 工具。

**[下载 VSIX · Windows x64 · 1.2.0](https://github.com/ArthurLauCS/PushRight/releases/download/v1.2.0/pushright-win32-x64-1.2.0.vsix)** · [版本说明与校验值](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0)

## 界面预览

![PushRight 1.2.0：桌面版中的提交图与文件差异](https://raw.githubusercontent.com/ArthurLauCS/PushRight/v1.2.0/docs/images/desktop-1.2.0.png)

“打开提交图”在桌面版中显示仓库；上图来自 1.2.0 桌面程序，使用本地示例仓库和虚构作者。

![PushRight 1.2.0：拉取默认使用 Rebase instead of merge](https://raw.githubusercontent.com/ArthurLauCS/PushRight/v1.2.0/docs/images/pull-rebase-1.2.0.png)

## 时区

在命令面板或源代码管理菜单运行 **PushRight: 时区…**，选择“跟随系统时区”、`UTC` 或 `Asia/Shanghai` 等地区。偏好全局保存在当前编辑器中，对所有仓库立即生效：侧栏提交图、提交详情、文件与行历史、搜索、Blame 和工作树均使用所选时区，按提交日期自动处理夏令时。桌面版和各编辑器分别保存偏好。

## 差异配色

在命令面板或源代码管理菜单运行 **PushRight: 差异配色…**。提供蓝／橙、紫／金、青／玫红三组预设，也可分别输入新增、删除的 HEX 色值。配色同时用于 PushRight 和 VS Code／Cursor 原生 diff（包括 Review Branch），会写入用户级 `workbench.colorCustomizations`，工作区设置可能优先覆盖。“恢复原配色”恢复此前值，同时保留无关设置和之后的手动改色。

## Diff 工具与文件树

桌面版的 diff 上方有七个工具：忽略空白差异、显示隐藏字符、自动换行、减少／增加上下文行、显示整个文件、并排对比；变更文件可按文件夹显示、筛选、整夹暂存。原生 diff 编辑器不重复这些按钮，用 VS Code／Cursor 自带的设置：

| 工具 | 原生 diff 的对应设置 |
| - | - |
| 忽略空白差异 | `diffEditor.ignoreTrimWhitespace`（编辑器右上角的空白图标） |
| 显示隐藏字符 | `editor.renderWhitespace` |
| 自动换行 | `diffEditor.wordWrap` |
| 上下文行、整个文件 | `diffEditor.hideUnchangedRegions.enabled` 与 `contextLineCount` |
| 并排对比 | `diffEditor.renderSideBySide` |

源代码管理面板的树／列表切换用面板自带的“以树查看”。侧栏提交图展开提交后，文件按文件夹缩进显示。

## 使用条件

- Windows 10 / 11 x64、VS Code 1.90 及以上、PATH 中可运行 Git。
- 已信任的本地 Git 工作区。本版不支持 Linux、macOS、WSL、SSH 或浏览器工作区。
- 远程认证沿用已有 Git 凭据助手或 SSH 配置。

## 推送与拉取

例如本地 `feature/login` 错误跟踪 `origin/main`，PushRight 会提示名称不一致，推荐 `origin/feature/login`，由你明确选择目标。推送使用指定的远程和分支 refspec；强制推送使用 `--force-with-lease --force-if-includes` 并再次确认。

每次 Pull 都会显示选择框，默认勾选 **Rebase instead of merge**。团队要求线性拉取历史时保持勾选。不会自动贮藏本地改动，请先提交或贮藏；遇到冲突后可解决并继续，或中止。

新建分支名称须符合 `(fix|feat)_(S|C|SC)_小驼峰`，例如 `feat_S_userLogin`；最后一段以小写英文字母开头，且仅包含英文字母和数字。在分支选择框中选择“新建分支…”后输入名称，并选择起始分支。已有分支名称不受影响。

分支选择框中本地分支优先，顺序固定，当前分支带对钩；每项显示最新提交的作者、短 ID、标题和时间，可输入提交内容搜索。点击提交图顶部分支按钮，选择 Auto、全部或指定分支，仅改变图的范围，不切换检出分支。

## 已有能力

- 查看暂存、未暂存和冲突文件；按文件或选中行暂存、取消暂存、丢弃；提交和 Amend。
- 当前行 Blame、作者悬浮信息、CodeLens、整文件 Blame（`Alt+B`）；给单个作者设置底色和边框。
- 展开 **PushRight > 行历史** 后，点击一行或选中几行就自动更新历史，无需右键命令，也不会切走编辑器焦点。连续移动光标时合并查询，面板隐藏时不查询。
- 文件历史、前后版本对比（`Alt+,` / `Alt+.`），按消息、作者、文件、改动内容或提交 ID 搜索。
- **源代码管理**下方显示 **PushRight 提交图**，关闭原生 Git 后仍可使用，每个工作区首次自动展开。点击提交就在原处展开本次文件列表；单击文件打开原生差异预览，双击固定差异页签。上下键导航，左右键收起/展开。
- 可选择 **自动（Auto）**、**全部分支**或某个本地/远程分支。Auto 显示 HEAD、上游及远程基准分支（依次尝试已有 VS Code 基准设置、分支创建来源、远程默认分支）。按仓库记住选择，筛选不切换分支。获取、拉取、推送作用于所选仓库的**当前检出分支**；图中的推送固定使用同名远程分支，并确认目标。
- 已初始化的子模块和嵌套仓库分别显示仓库入口、提交图和源代码管理，子仓库名称包含父路径。新增仓库后可点“刷新”；新增工作区文件夹会自动识别。打开被忽略的嵌套仓库内的文件，也会识别其仓库归属。
- 新增目录逐个列出未跟踪文件，可在暂存前查看每个文件，并分别暂存；仍遵守忽略规则。
- 运行 **PushRight: 打开提交图** 或点击侧栏提交图标题栏按钮，在桌面版中浏览分支、提交和差异。如果曾隐藏侧栏图，可在源代码管理的“视图”菜单中重新勾选 **PushRight 提交图**。
- 快速修改当前仓库的提交名和邮箱。
- 提交输入框自动带出仓库的提交信息模板；**PushRight: 最近的提交信息…** 复用之前写过的信息，**PushRight: 运行提交前检查** 执行仓库设置的检查命令。模板和检查命令在桌面版的“仓库设置”里填写。
- 侧栏提交图顶部有“标签”“只看主线”两个开关。
- PushRight 页面的“工作树”视图列出这个仓库的全部工作树。点一行在新窗口中打开（原窗口保留），加号从已有分支或新分支新建，垃圾桶按钮删除。删除的只是文件夹，分支和提交都还在。
- 在桌面版中选择 **仓库工具**：比较两个提交/分支、标记已审阅文件、管理远程地址、编辑变基顺序/动作、从 Reflog 创建恢复分支。欢迎页提供克隆与初始化。
- 文件、行历史和搜索每页 200 条，可“加载更多”；有本地或未保存修改时，将未改变的选区映射回 HEAD 后查询。
- 图片差异前后预览；大补丁可按每页 1000 行只读浏览。
- 在仓库列表中右键选择 **关闭仓库**，保留已打开的文件，并阻止切换文件、刷新或重载时自动重新发现。使用 **PushRight: 重新打开已关闭仓库…** 恢复。

首次启用会说明关闭 VS Code 内置 Git 的影响，由你决定是否让推送和拉取统一经过 PushRight。依赖内置 Git 的扩展可能因此停止工作。与 GitLens 同时启用时，可关闭其中一个的 Blame 注释以免重复。

## 私用本地文件

展开 **源代码管理 > PushRight 本地文件**，按仓库查看**未跟踪文件**和**已忽略文件**。忽略目录仅在展开时加载直接子项。

在资源管理器中右键文件：

| 操作 | 影响 |
| --- | --- |
| 仅本机忽略 | 精确路径加入 `.git/info/exclude`，不提交、不共享，适合私用 skill。 |
| 加入 .gitignore | 精确路径加入可随提交共享的忽略规则。 |
| 开始跟踪（暂存） | 即使文件已忽略，也会暂存并进入下一次提交。 |
| 停止跟踪并保留本地文件 | 保留工作区文件，从索引移除并仅本机忽略。对于已提交文件，这会**暂存团队仓库中的删除记录**，操作前会明确提示。 |

忽略规则不能隐藏已跟踪文件的改动。停止跟踪不会强行丢弃独立的暂存修改。撤销忽略可编辑相应的 `.gitignore` 或 `.git/info/exclude`；强制跟踪文件不会删除已有规则。

## 当前限制

- 尚无 PR／Issue 和托管平台协作，不能替代 GitLens 或 GitHub Pull Requests 的这些功能。
- 子模块需要先初始化。不会后台扫描被忽略的目录树来查找仓库；打开其中的文件或将其添加为工作区文件夹即可识别。
- 选区包含新增或被修改的行时，行历史会提示无法对应到 HEAD；改查文件历史或提交后再查。
- 原生历史文本默认 4 MiB，可通过 `pushright.history.maxFileSizeMiB` 调整至 64 MiB。大补丁按页只读；单行超过 64 KiB 会截断。图片预览限 32 MiB/侧，支持 PNG/JPEG/GIF/WebP/BMP/ICO；其他二进制仍可整文件操作。
- 交互式变基处理当前 HEAD 祖先之后的线性提交区间，不处理包含合并提交的区间或根提交；自动保留 `pushright-backup/*` 分支。Reflog 恢复创建新分支，不重置当前分支。
- 完整提交历史在后台加载。实测范围见[性能报告](https://github.com/ArthurLauCS/PushRight/blob/main/docs/PERFORMANCE.md)。

## VSIX 安装

在 [GitHub Releases](https://github.com/ArthurLauCS/PushRight/releases/tag/v1.2.0) 下载 Windows x64 VSIX，然后执行 VS Code 或 Cursor 的“扩展：从 VSIX 安装”，或：

```sh
code --install-extension pushright-win32-x64-1.2.0.vsix
cursor --install-extension pushright-win32-x64-1.2.0.vsix
```

反馈问题请提交到 [GitHub Issues](https://github.com/ArthurLauCS/PushRight/issues)，附版本和复现步骤；分享日志前移除凭据和私有仓库信息。

第三方声明随包提供，位于 `THIRD_PARTY_NOTICES.txt`。PushRight 使用 [MIT 开源许可证](https://github.com/ArthurLauCS/PushRight/blob/main/LICENSE)。
