//! 一个仓库页签：顶栏（撤销、获取、拉取、推送，当前分支，贮藏、新建分支、日志、设置、帮助）、
//! 侧栏、提交图 / 本地更改 / 工作树 / 仓库工具，以及所有写操作的确认、执行、撤销和刷新。

use crate::app::PushRight;
use crate::commit_search::CommitSearch;
use crate::detail::{DetailView, Source};
use crate::dialog::{skipped, Dialog, Field, Spec, Values};
use crate::git::{self, Session};
use crate::graph::{Graph, GraphEvent};
use crate::help::Help;
use crate::i18n::{self, explain, lookup, t};
use crate::icons::{icon, Icon};
use crate::layout::{self, layout, Key};
use crate::menu::{Item, Menu};
use crate::rules::{branch_name_ok, graph_key, short, split_upstream};
use crate::store;
use crate::theme::{alpha, palette, DISPLAY, FS_LG, FS_MD, FS_SM, MONO, PROSE, R_LG};
use crate::tools::Tools;
use crate::ui::{btn, name_only, sep};
use crate::working_copy::WorkingCopy;
use crate::{diff_view, worktrees};
use engine::conflict::Operation;
use engine::detail::Detail;
use engine::graph::Graph as EngineGraph;
use engine::identity::Identity;
use engine::ops::{InProgress, Log, Op, ResetMode};
use engine::refs::{Refs, Stash, Track, Worktree};
use engine::status::Entry;
use engine::workflow::{Settings, Tools as ExternalTools};
use futures::channel::oneshot;
use futures::future::{FutureExt, Shared};
use gpui::prelude::*;
use gpui::{div, px, relative, AnyElement, App, Context, Entity, FontWeight, Pixels, Point, SharedString, Subscription, Task, WeakEntity, Window};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    History,
    Changes,
    Worktrees,
    Tools,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Branch,
    Remote,
    Tag,
    Stash,
}

/// 提交图的显示开关，所有仓库共用：隐藏标签只影响标记；只看主线要重新加载图
#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphPrefs {
    pub hide_tags: bool,
    pub first_parent: bool,
}

/// 可撤销的操作：标题和把仓库退回去要执行的操作
struct Undo {
    title: &'static str,
    ops: Vec<Op>,
}

/// 自动获取、自动刷新的开关，存设置的 autoSync
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct AutoSync {
    pub refresh: bool,
    pub fetch: bool,
}

impl Default for AutoSync {
    fn default() -> AutoSync {
        AutoSync { refresh: true, fetch: true }
    }
}

pub struct RepoView {
    pub session: Arc<Session>,
    pub path: String,
    opener: WeakEntity<PushRight>,
    pub dialog: Entity<Dialog>,
    pub menu: Entity<Menu>,
    help: Entity<Help>,
    pub graph: Entity<Graph>,
    pub detail: Entity<DetailView>,
    pub working: Entity<WorkingCopy>,
    search: Entity<CommitSearch>,
    tools: Option<Entity<Tools>>,

    /// 提交图只显示这些引用可达的提交，空表示全部；以 ^ 开头的是「相比」的基准。按仓库路径记住
    pub scope: Vec<String>,
    /// 自动获取之后要快进的本地分支（引用全名）；按仓库路径记住
    pub auto_ff: Vec<String>,
    /// 上一轮自动快进里没能快进的分支和原因
    auto_ff_skipped: Vec<String>,
    /// 固定在侧栏前列的分支（引用全名）；按仓库路径记住
    pub pinned: Vec<String>,
    pub count: usize,
    pub loading_all: bool,
    pub busy: bool,
    /// 本地更改页自己在跑（暂存、提交），这时不能再执行别的写操作
    pub working_busy: bool,
    initializing: bool,
    started: bool,
    auto_fetch_error: bool,
    pub error: String,
    pub refs: Refs,
    pub stashes: Vec<Stash>,
    pub worktrees: Vec<Worktree>,
    pub entries: Vec<Entry>,
    /// 拉取请求所在的 GitHub 远程；不是 GitHub 仓库时为 None，侧栏不显示拉取请求
    pub github: Option<engine::github::Remote>,
    /// 打开着的拉取请求，打开页签时和点刷新时从 GitHub 读
    pub prs: Vec<engine::github::Pr>,
    pub pr_error: String,
    prs_loading: bool,
    pub identity: Option<Identity>,
    /// 进行中的操作在 Git 里记下的信息；没有进行中的操作时为 None
    pub operation: Option<Operation>,
    /// 提交信息模板和提交前检查，存在仓库的本地 git 配置里
    pub workflow: Settings,
    pub external_tools: ExternalTools,
    /// 上游改了、本地也改着的二进制文件；拉取前提示
    overlap: Vec<String>,
    logs: Vec<Log>,
    show_log: bool,
    pub view: View,
    pub compare_left: String,
    pub compare_right: String,
    /// 打开比较页时直接从共同祖先开始比较
    compare_auto: bool,
    pub tracks: HashMap<String, Track>,
    pub selected_row: Option<usize>,
    selected_id: Option<String>,
    detail_request: usize,
    pending_jump: Option<String>,
    /// 页签还没加载完时要定位的提交（VS Code 扩展的「在提交图中显示」）
    reveal_on_start: Option<String>,
    undos: Vec<Undo>,
    pub graph_prefs: GraphPrefs,
    /// 最近一次检查的结果；None 表示还没跑过或设置变了
    pub check_result: Option<Log>,
    pub checking: bool,
    refreshing: Option<Shared<Task<()>>>,
    refresh_again: bool,
    foreground_refresh: bool,
    /// 侧栏：筛选词、单击选中的引用、各分组是否展开
    pub sidebar: crate::sidebar::SidebarState,
    _subscriptions: Vec<Subscription>,
    _auto_sync: Option<Task<()>>,
}

/// 按仓库路径记住的引用名单
fn saved_list(cx: &App, key: &str, path: &str) -> Vec<String> {
    store::get::<HashMap<String, Vec<String>>>(cx, key).and_then(|mut all| all.remove(path)).unwrap_or_default()
}

fn save_list(cx: &mut App, key: &str, path: &str, list: &[String]) {
    let mut all = store::get::<HashMap<String, Vec<String>>>(cx, key).unwrap_or_default();
    if list.is_empty() {
        all.remove(path);
    } else {
        all.insert(path.to_owned(), list.to_vec());
    }
    store::set(cx, key, all);
}

/// 撤销列表和日志里用的操作名，和文案 opNames 的键一致
fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Checkout { .. } => "checkout",
        Op::Track { .. } => "track",
        Op::CreateBranch { .. } => "create_branch",
        Op::DeleteBranch { .. } => "delete_branch",
        Op::RenameBranch { .. } => "rename_branch",
        Op::Merge { .. } => "merge",
        Op::Rebase { .. } => "rebase",
        Op::CherryPick { .. } => "cherry_pick",
        Op::Revert { .. } => "revert",
        Op::SetRevertDisabled { .. } => "set_revert_disabled",
        Op::Reset { .. } => "reset",
        Op::StashPush { .. } => "stash_push",
        Op::Discard { .. } => "discard",
        Op::StashApply { .. } => "stash_apply",
        Op::StashDrop { .. } => "stash_drop",
        Op::Fetch { .. } => "fetch",
        Op::Pull { .. } => "pull",
        Op::FastForward { .. } => "fast_forward",
        Op::CreateTag { .. } => "create_tag",
        Op::DeleteTag { .. } => "delete_tag",
        _ => "op",
    }
}

pub fn in_progress(what: &str) -> InProgress {
    match what {
        "rebase" => InProgress::Rebase,
        "cherry-pick" => InProgress::CherryPick,
        "revert" => InProgress::Revert,
        _ => InProgress::Merge,
    }
}

/// 根据操作之前的状态，算出把它退回去要执行什么；不能撤销的返回 None
fn undo_for(op: &Op, before: &Refs) -> Option<Vec<Op>> {
    let id_of = |name: &str| before.refs.iter().find(|r| r.name == name).map(|r| r.id.clone());
    let back = before.head.as_deref().map(|h| Op::Checkout { target: short(h).to_owned() }).or_else(|| before.head_id.clone().map(|id| Op::Checkout { target: id }));
    let keep = |id: &String| Op::Reset { target: id.clone(), mode: ResetMode::Keep };
    match op {
        Op::Checkout { .. } | Op::Track { .. } => back.map(|b| vec![b]),
        Op::CreateBranch { name, checkout, .. } => {
            let mut ops: Vec<Op> = if *checkout { back.into_iter().collect() } else { Vec::new() };
            ops.push(Op::DeleteBranch { name: name.clone(), force: true });
            Some(ops)
        }
        Op::DeleteBranch { name, .. } => id_of(&format!("refs/heads/{name}")).map(|id| vec![Op::CreateBranch { name: name.clone(), start: id, checkout: false }]),
        Op::CreateTag { name, .. } => Some(vec![Op::DeleteTag { name: name.clone() }]),
        Op::DeleteTag { name } => id_of(&format!("refs/tags/{name}")).map(|id| vec![Op::CreateTag { name: name.clone(), target: id, message: String::new() }]),
        Op::Merge { .. } | Op::Rebase { .. } | Op::CherryPick { .. } | Op::Revert { .. } | Op::Pull { .. } => before.head_id.as_ref().map(|id| vec![keep(id)]),
        // 只有当前分支能用重置退回；其他分支的旧位置在 reflog 里
        Op::FastForward { branch } => (before.head.as_deref() == Some(&format!("refs/heads/{branch}"))).then_some(()).and(before.head_id.as_ref()).map(|id| vec![keep(id)]),
        // 软/混合重置后改动还在工作区，用同样的方式移回去正好还原；硬重置后工作区是干净的，用 keep
        Op::Reset { mode, .. } => before.head_id.as_ref().map(|id| vec![Op::Reset { target: id.clone(), mode: if matches!(mode, ResetMode::Hard) { ResetMode::Keep } else { *mode } }]),
        Op::StashPush { .. } | Op::Discard { .. } => Some(vec![Op::StashApply { name: "stash@{0}".into(), pop: true }]),
        _ => None,
    }
}

type Ask = oneshot::Receiver<Option<Values>>;

impl RepoView {
    pub fn new(session: Arc<Session>, opener: WeakEntity<PushRight>, handles: (Entity<Dialog>, Entity<Menu>, Entity<Help>), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = session.path();
        let scope = saved_list(cx, "graphScopes", &path);
        let count = session.graph().len();
        // 有筛选时首屏不先画打开仓库时加载的全量图
        let count = if scope.is_empty() { count } else { 0 };
        let graph = cx.new(|cx| Graph::new(session.clone(), count, cx));
        let me = cx.entity().downgrade();
        let detail = cx.new(|cx| DetailView::new(session.clone(), me.clone(), window, cx));
        let working = cx.new(|cx| WorkingCopy::new(session.clone(), me.clone(), window, cx));
        let search = cx.new(|cx| CommitSearch::new(session.clone(), me.clone(), window, cx));
        let subs = vec![cx.subscribe_in(&graph, window, |this, _, e: &GraphEvent, window, cx| match e {
            GraphEvent::Select(row, id) => this.select(*row, id.clone(), cx),
            GraphEvent::Menu(at, id) => this.commit_menu(*at, id.clone(), window, cx),
            GraphEvent::StyleAuthor { key, name, email } => this.style_author(key.clone(), name.clone(), email.clone(), window, cx),
        })];
        RepoView {
            auto_ff: saved_list(cx, "autoFastForward", &path),
            pinned: saved_list(cx, "pinnedBranches", &path),
            graph_prefs: store::get(cx, "graphView").unwrap_or_default(),
            sidebar: crate::sidebar::SidebarState::new(window, cx),
            session,
            path,
            opener,
            dialog: handles.0,
            menu: handles.1,
            help: handles.2,
            graph,
            detail,
            working,
            search,
            tools: None,
            scope,
            auto_ff_skipped: Vec::new(),
            count,
            loading_all: false,
            busy: false,
            working_busy: false,
            initializing: false,
            started: false,
            auto_fetch_error: false,
            error: String::new(),
            refs: Refs { head: None, head_id: None, ahead_behind: None, in_progress: None, revert_disabled: false, refs: Vec::new() },
            stashes: Vec::new(),
            worktrees: Vec::new(),
            entries: Vec::new(),
            github: None,
            prs: Vec::new(),
            pr_error: String::new(),
            prs_loading: false,
            identity: None,
            operation: None,
            workflow: Settings::default(),
            external_tools: ExternalTools::default(),
            overlap: Vec::new(),
            logs: Vec::new(),
            show_log: false,
            view: View::History,
            compare_left: "HEAD~1".into(),
            compare_right: "HEAD".into(),
            compare_auto: false,
            tracks: HashMap::new(),
            selected_row: None,
            selected_id: None,
            detail_request: 0,
            pending_jump: None,
            reveal_on_start: None,
            undos: Vec::new(),
            check_result: None,
            checking: false,
            refreshing: None,
            refresh_again: false,
            foreground_refresh: false,
            _subscriptions: subs,
            _auto_sync: None,
        }
    }

    // ---------- 数据加载与刷新 ----------

    /// 这个页签第一次显示时才加载完整历史，恢复多个页签时不会同时全量加载
    pub fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.started {
            let _ = self.refresh(false, cx);
            return;
        }
        self.started = true;
        self.initializing = true;
        self.load_prs(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(load) = this.update(cx, |this, cx| this.load_sidebar(cx)) else { return };
            load.await;
            let _ = this.update(cx, |this, cx| {
                this.prune_scope(cx);
                // 有筛选时行号要等筛选后的图加载完再找，打开仓库时那份全量图里的行号对不上
                if let (Some(head), true) = (this.refs.head_id.clone(), this.scope.is_empty()) {
                    this.jump(&head, cx);
                }
            });
            let Ok(all) = this.update(cx, |this, cx| this.load_all(cx)) else { return };
            all.await;
            let _ = this.update(cx, |this, cx| {
                if let (Some(head), false) = (this.refs.head_id.clone(), this.scope.is_empty()) {
                    this.jump(&head, cx);
                }
                this.initializing = false;
                if let Some(id) = this.reveal_on_start.take() {
                    this.reveal(id, cx);
                }
                cx.notify();
            });
            let _ = this.update_in(cx, |this, window, cx| this.start_auto_sync(window, cx));
        })
        .detach();
    }

    /// 侧栏和本地更改需要的数据一次取齐；各分支的同步状态在分支多时要算一两秒，不等它
    fn load_sidebar(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let session = self.session.clone();
        let tracking = self.session.clone();
        git::bg(cx, move || engine::refs::tracking(&tracking.repo), |this, tracks, cx| {
            if let Ok(list) = tracks {
                this.tracks = list.into_iter().map(|t| (t.name.clone(), t)).collect();
                cx.notify();
            }
        })
        .detach();
        git::bg(
            cx,
            move || {
                let repo = &session.repo;
                let all = (|| {
                    Ok::<_, String>((
                        engine::refs::list(repo)?,
                        engine::refs::stashes(repo)?,
                        engine::refs::worktrees(repo)?,
                        engine::status::status(repo)?,
                        engine::identity::read(repo)?,
                        engine::workflow::read(repo)?,
                        engine::workflow::tools(repo),
                    ))
                })();
                let all = all.map(|(refs, stashes, worktrees, entries, identity, workflow, tools)| {
                    let operation = if refs.in_progress.is_some() { engine::conflict::operation(repo).ok().flatten() } else { None };
                    // 只有落后于上游时才可能撞上；多数刷新不用多跑这两条命令
                    let overlap = if refs.ahead_behind.is_some_and(|(_, behind)| behind > 0) { engine::refs::binary_overlap(repo).unwrap_or_default() } else { Vec::new() };
                    (refs, stashes, worktrees, entries, identity, workflow, tools, operation, overlap)
                });
                all
            },
            |this, result, cx| {
                match result {
                    Ok((refs, stashes, worktrees, entries, identity, workflow, tools, operation, overlap)) => {
                        this.refs = refs;
                        this.stashes = stashes;
                        this.worktrees = worktrees;
                        this.entries = entries;
                        this.identity = Some(identity);
                        this.workflow = workflow;
                        this.external_tools = tools;
                        this.operation = operation;
                        this.overlap = overlap;
                        this.update_badges(cx);
                    }
                    Err(e) => this.error = e,
                }
                cx.notify();
            },
        )
    }

    fn update_badges(&mut self, cx: &mut Context<Self>) {
        let mut badges: HashMap<String, Vec<engine::refs::Ref>> = HashMap::new();
        for r in &self.refs.refs {
            if !self.graph_prefs.hide_tags || !r.name.starts_with("refs/tags/") {
                badges.entry(r.id.clone()).or_default().push(r.clone());
            }
        }
        let head = self.refs.head_id.clone();
        self.graph.update(cx, |g, cx| {
            g.badges = badges;
            g.head_id = head;
            cx.notify();
        });
    }

    fn load_all(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.loading_all = true;
        cx.notify();
        let session = self.session.clone();
        let scope = if self.scope.is_empty() { "all".to_owned() } else { self.scope.join("\n") };
        let first_parent = self.graph_prefs.first_parent;
        git::bg(
            cx,
            move || {
                let graph = EngineGraph::load_view(&session.repo, usize::MAX, &scope, first_parent)?;
                let n = graph.len();
                session.set_graph(graph);
                Ok::<_, String>(n)
            },
            |this, n, cx| {
                this.loading_all = false;
                match n {
                    Ok(n) => {
                        this.count = n;
                        this.graph.update(cx, |g, cx| g.reload(n, cx));
                        if let Some(id) = this.pending_jump.take() {
                            this.jump(&id, cx);
                        }
                    }
                    Err(e) => this.error = e,
                }
                cx.notify();
            },
        )
    }

    /// 刷新合并：正在刷的时候再要求刷新，等这一轮完了再来一轮。后台刷新保留正在阅读的提交与页面
    pub fn refresh(&mut self, background: bool, cx: &mut Context<Self>) -> Shared<Task<()>> {
        self.foreground_refresh |= !background;
        if let Some(running) = &self.refreshing {
            self.refresh_again = true;
            return running.clone();
        }
        let task = cx
            .spawn(async move |this, cx| loop {
                let Ok(step) = this.update(cx, |this, cx| {
                    this.refresh_again = false;
                    let background = !this.foreground_refresh;
                    this.foreground_refresh = false;
                    this.refresh_now(background, cx)
                }) else { return };
                step.await;
                let again = this.update(cx, |this, _| {
                    if !this.refresh_again {
                        this.refreshing = None;
                    }
                    this.refresh_again
                });
                if !again.unwrap_or(false) {
                    return;
                }
            })
            .shared();
        self.refreshing = Some(task.clone());
        task
    }

    fn refresh_now(&mut self, background: bool, cx: &mut Context<Self>) -> Task<()> {
        // 首次加载还没完：它自己会加载完整历史并定位，这时刷新会把定位冲掉
        if self.loading_all || self.initializing {
            return Task::ready(());
        }
        let before = graph_key(&self.refs);
        let before_head = self.refs.head_id.clone();
        let load = self.load_sidebar(cx);
        cx.spawn(async move |this, cx| {
            load.await;
            let Ok(next) = this.update(cx, |this, cx| {
                if !this.started {
                    return None;
                }
                let pruned = this.prune_scope(cx);
                if graph_key(&this.refs) == before && !pruned {
                    if !background && this.refs.head_id.is_some() && this.refs.head_id != before_head && this.view == View::History {
                        let head = this.refs.head_id.clone().unwrap();
                        this.jump(&head, cx);
                    }
                    return None;
                }
                if !background {
                    this.selected_row = None;
                    this.selected_id = None;
                    this.set_detail(None, Source::Commit, cx);
                }
                Some(this.load_all(cx))
            }) else { return };
            let Some(all) = next else { return };
            all.await;
            let _ = this.update(cx, |this, cx| {
                if background {
                    if let Some(id) = this.selected_id.clone() {
                        this.selected_row = this.session.graph().row_of(&id).map(|r| r as usize);
                        this.sync_selection(cx);
                    }
                } else if let (Some(head), View::History) = (this.refs.head_id.clone(), this.view) {
                    // 正在看本地更改或工作树时不把人拽回提交图
                    this.jump(&head, cx);
                }
            });
        })
    }

    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        let row = self.selected_row;
        self.graph.update(cx, |g, cx| {
            g.selected = row;
            cx.notify();
        });
        cx.notify();
    }

    // ---------- 提交图：筛选、选中、跳转 ----------

    fn save_scope(&mut self, next: Vec<String>, cx: &mut Context<Self>) {
        save_list(cx, "graphScopes", &self.path, &next);
        self.scope = next;
    }

    /// 被删掉的引用从筛选里拿掉；返回筛选有没有变
    fn prune_scope(&mut self, cx: &mut Context<Self>) -> bool {
        let mut alive: Vec<String> = self.scope.iter().filter(|n| self.refs.refs.iter().any(|r| r.name == n.trim_start_matches('^'))).cloned().collect();
        // 只剩对比基准、没有要看的分支时，整个筛选作废
        if alive.iter().all(|n| n.starts_with('^')) {
            alive.clear();
        }
        let changed = alive.len() != self.scope.len();
        if changed {
            self.save_scope(alive, cx);
        }
        changed
    }

    /// 换一组引用重画提交图；正在看的提交留着，只重新找它在第几行
    pub fn set_scope(&mut self, mut next: Vec<String>, cx: &mut Context<Self>) -> Task<()> {
        if self.loading_all {
            return Task::ready(());
        }
        if next.iter().all(|n| n.starts_with('^')) {
            next.clear();
        }
        self.save_scope(next, cx);
        let all = self.load_all(cx);
        cx.spawn(async move |this, cx| {
            all.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(id) = this.selected_id.clone() {
                    this.selected_row = this.session.graph().row_of(&id).map(|r| r as usize);
                    this.sync_selection(cx);
                }
            });
        })
    }

    pub fn toggle_scope(&mut self, name: &str, cx: &mut Context<Self>) {
        let next = if self.scope.iter().any(|n| n == name) { self.scope.iter().filter(|n| *n != name).cloned().collect() } else { [self.scope.clone(), vec![name.to_owned()]].concat() };
        self.set_scope(next, cx).detach();
    }

    pub fn scope_refs(&self) -> Vec<String> {
        self.scope.iter().filter(|n| !n.starts_with('^')).cloned().collect()
    }

    pub fn scope_base(&self) -> Option<String> {
        self.scope.iter().find_map(|n| n.strip_prefix('^').map(str::to_owned))
    }

    /// 只看 `name` 上有、`base` 上没有的提交
    fn compare_branch(&mut self, name: &str, base: &str, cx: &mut Context<Self>) {
        self.view = View::History;
        self.set_scope(vec![name.to_owned(), format!("^{base}")], cx).detach();
    }

    /// 右键菜单里直接列出来的对比基准：常用主干分支（本地优先）和当前分支
    fn compare_bases(&self, name: &str) -> Vec<String> {
        let has = |r: &str| self.refs.refs.iter().any(|x| x.name == r);
        let mut out: Vec<String> = Vec::new();
        for b in ["develop", "dev", "main", "master"] {
            if let Some(r) = [format!("refs/heads/{b}"), format!("refs/remotes/origin/{b}")].into_iter().find(|r| has(r)) {
                out.push(r);
            }
        }
        if let Some(h) = &self.refs.head {
            out.push(h.clone());
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|b| b != name && seen.insert(b.clone()));
        out
    }

    fn pick_compare_base(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let options: Vec<(String, String)> = self
            .refs
            .refs
            .iter()
            .filter(|r| (r.name.starts_with("refs/heads/") || r.name.starts_with("refs/remotes/")) && !r.name.ends_with("/HEAD") && r.name != name)
            .map(|r| (r.name.clone(), short(&r.name).to_owned()))
            .collect();
        let Some(first) = options.first().map(|o| o.0.clone()) else { return };
        let rx = self.ask(Spec::new(format!("{} · {}", short(&name), t.compare_branch.other)).message(t.compare_branch.hint).field(Field::select("base", t.compare_branch.base, options).value(first)), window, cx);
        self.then(rx, window, cx, move |this, v, _, cx| this.compare_branch(&name, &v.text("base"), cx));
    }

    /// 从「相比某分支」的提交图跳到文件改动：基准和分支从共同祖先开始比较
    pub fn compare_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(base), Some(name)) = (self.scope_base(), self.scope_refs().first().cloned()) else { return };
        self.compare_left = short(&base).to_owned();
        self.compare_right = short(&name).to_owned();
        self.compare_auto = true;
        self.open_tools(window, cx);
    }

    pub fn compare_with(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.compare_right = id;
        self.compare_auto = false;
        self.open_tools(window, cx);
    }

    /// 仓库工具每次打开都按当前的比较起点重建
    fn open_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let (left, right, auto) = (self.compare_left.clone(), self.compare_right.clone(), self.compare_auto);
        let session = self.session.clone();
        self.tools = Some(cx.new(|cx| Tools::new(session, me, left, right, auto, window, cx)));
        self.view = View::Tools;
        cx.notify();
    }

    pub fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if view == View::Tools && self.tools.is_none() {
            return self.open_tools(window, cx);
        }
        self.view = view;
        cx.notify();
    }

    pub fn show_log_panel(&mut self, cx: &mut Context<Self>) {
        self.show_log = true;
        cx.notify();
    }

    /// 交互式变基：引擎先建备份分支再按计划重放；停在冲突上时切到本地更改页
    pub fn rebase(&mut self, plan: engine::rebase::Plan, update_refs: bool, cx: &mut Context<Self>) -> Task<Option<Log>> {
        if self.busy || self.working_busy {
            return Task::ready(None);
        }
        self.busy = true;
        self.error.clear();
        cx.notify();
        let session = self.session.clone();
        let run = cx.background_spawn(async move { engine::rebase::run(&session.repo, &plan.base, &plan.head, &plan.steps, update_refs) });
        cx.spawn(async move |this, cx| {
            let r = run.await;
            let Ok((log, refresh)) = this.update(cx, |this, cx| {
                this.busy = false;
                let log = match r {
                    Ok(log) => {
                        this.show_log = true;
                        this.push_log(log.clone());
                        Some(log)
                    }
                    Err(e) => {
                        this.error = e;
                        None
                    }
                };
                (log, this.refresh(false, cx))
            }) else { return None };
            refresh.await;
            let _ = this.update(cx, |this, cx| {
                if this.refs.in_progress.is_some() {
                    this.view = View::Changes;
                    cx.notify();
                }
            });
            log
        })
    }

    /// 打开后定位到这个提交；页签还在首次加载时等加载完
    pub fn reveal_later(&mut self, id: String, cx: &mut Context<Self>) {
        if self.started && !self.initializing {
            self.reveal(id, cx);
        } else {
            self.reveal_on_start = Some(id);
        }
    }

    /// 搜索结果里的提交可能不在当前筛选范围内：找不到就先清掉筛选
    pub fn reveal(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.scope.is_empty() && self.session.graph().row_of(&id).is_none() {
            let clear = self.set_scope(Vec::new(), cx);
            cx.spawn(async move |this, cx| {
                clear.await;
                let _ = this.update(cx, |this, cx| this.jump(&id, cx));
            })
            .detach();
        } else {
            self.jump(&id, cx);
        }
    }

    pub fn select(&mut self, row: usize, id: String, cx: &mut Context<Self>) {
        self.selected_row = Some(row);
        self.selected_id = Some(id.clone());
        self.sync_selection(cx);
        self.detail_request += 1;
        let request = self.detail_request;
        let session = self.session.clone();
        git::bg(cx, move || engine::detail::detail(&session.repo, &id), move |this, d, cx| match d {
            Ok(d) if request == this.detail_request => this.set_detail(Some(d), Source::Commit, cx),
            Ok(_) => {}
            Err(e) => {
                this.error = e;
                cx.notify();
            }
        })
        .detach();
    }

    fn set_detail(&mut self, d: Option<Detail>, source: Source, cx: &mut Context<Self>) {
        let key = format!("review:{}", serde_json::to_string(&(&self.path, source.mode(), d.as_ref().map(|d| d.id.clone()))).unwrap());
        self.detail.update(cx, |v, cx| v.set(d, source, key, cx));
    }

    pub fn view_stash(&mut self, name: String, cx: &mut Context<Self>) {
        self.view = View::History;
        self.selected_row = None;
        self.selected_id = None;
        self.sync_selection(cx);
        self.detail_request += 1;
        let request = self.detail_request;
        let session = self.session.clone();
        git::bg(cx, move || engine::detail::stash(&session.repo, &name), move |this, d, cx| match d {
            Ok(d) if request == this.detail_request => this.set_detail(Some(d), Source::Stash, cx),
            Ok(_) => {}
            Err(e) => {
                this.error = e;
                cx.notify();
            }
        })
        .detach();
    }

    pub fn jump(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(row) = self.session.graph().row_of(id).map(|r| r as usize) else {
            // 完整历史还没加载完，目标提交不在首批里：等加载完再跳
            if self.loading_all {
                self.pending_jump = Some(id.to_owned());
            }
            return;
        };
        self.view = View::History;
        self.graph.update(cx, |g, _| g.scroll_to_row(row, true));
        self.select(row, id.to_owned(), cx);
    }

    pub fn set_graph_pref(&mut self, first_parent: bool, cx: &mut Context<Self>) {
        if self.loading_all {
            return;
        }
        if first_parent {
            self.graph_prefs.first_parent = !self.graph_prefs.first_parent;
        } else {
            self.graph_prefs.hide_tags = !self.graph_prefs.hide_tags;
        }
        store::set(cx, "graphView", self.graph_prefs);
        if first_parent {
            let scope = self.scope.clone();
            self.set_scope(scope, cx).detach();
        } else {
            self.update_badges(cx);
        }
        cx.notify();
    }

    // ---------- 执行写操作 ----------

    pub fn ask(&self, spec: Spec, window: &mut Window, cx: &mut Context<Self>) -> Ask {
        self.dialog.update(cx, |d, cx| d.ask(spec, window, cx))
    }

    /// 等对话框确认后再做 `f`；取消就什么都不做
    pub fn then(&self, rx: Ask, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Self, Values, &mut Window, &mut Context<Self>) + 'static) {
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Some(v)) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| f(this, v, window, cx));
            }
        })
        .detach();
    }

    fn push_log(&mut self, log: Log) {
        self.logs.push(log);
        if self.logs.len() > 100 {
            self.logs.remove(0);
        }
    }

    /// 执行一个写操作，记入命令日志；失败时展开日志。`record` 为 false 时不进撤销列表。结果是是否成功
    pub fn exec(&mut self, op: Op, record: bool, cx: &mut Context<Self>) -> Task<bool> {
        if self.busy || self.working_busy {
            return Task::ready(false);
        }
        self.busy = true;
        self.error.clear();
        self.auto_ff_skipped.clear();
        cx.notify();
        let before = self.refs.clone();
        let session = self.session.clone();
        let is_fetch = matches!(op, Op::Fetch { .. });
        let undo = if record { undo_for(&op, &before).map(|ops| Undo { title: op_name(&op), ops }) } else { None };
        let run = cx.background_spawn(async move { engine::ops::run(&session.repo, op) });
        cx.spawn(async move |this, cx| {
            let result = run.await;
            let Ok(refresh) = this.update(cx, |this, cx| {
                this.busy = false;
                let ok = match result {
                    Ok(log) => {
                        let ok = log.ok;
                        if !ok {
                            this.show_log = true;
                        }
                        this.push_log(log);
                        ok
                    }
                    Err(e) => {
                        this.error = e;
                        false
                    }
                };
                if is_fetch && ok {
                    this.auto_fetch_error = false;
                }
                if let (true, Some(undo)) = (ok, undo) {
                    this.undos.push(undo);
                }
                (ok, this.refresh(false, cx))
            }) else { return false };
            refresh.1.await;
            refresh.0
        })
    }

    /// 同 exec，但交回这次的命令日志：仓库工具要把输出显示在页面上
    pub fn exec_log(&mut self, op: Op, cx: &mut Context<Self>) -> Task<Option<Log>> {
        let before = self.logs.len();
        let run = self.exec(op, true, cx);
        cx.spawn(async move |this, cx| {
            run.await;
            this.update(cx, |this, _| (this.logs.len() != before).then(|| this.logs.last().cloned()).flatten()).ok().flatten()
        })
    }

    fn exec_detached(&mut self, op: Op, cx: &mut Context<Self>) {
        self.exec(op, true, cx).detach();
    }

    fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.undos.last() else { return };
        let t = t(cx);
        let title = (t.undo_title)(lookup(t.op_names, last.title).unwrap_or(last.title));
        let rx = self.ask(Spec::new(title).explain(&explain(cx).undo).confirm(t.undo), window, cx);
        self.then(rx, window, cx, |this, _, window, cx| {
            let Some(last) = this.undos.pop() else { return };
            cx.spawn_in(window, async move |this, cx| {
                for op in last.ops {
                    let Ok(run) = this.update(cx, |this, cx| this.exec(op, false, cx)) else { return };
                    if !run.await {
                        break;
                    }
                }
            })
            .detach();
        });
    }

    /// 提交不走 exec，单独记一条撤销：软重置回提交之前的位置，改动回到已暂存。
    /// `before` 要在提交之前取：提交完本地更改页会先刷新，那时 HEAD 已经是新提交了
    pub fn committed(&mut self, before: Option<String>, cx: &mut Context<Self>) {
        if let Some(id) = before {
            self.undos.push(Undo { title: "commit", ops: vec![Op::Reset { target: id, mode: ResetMode::Soft }] });
            cx.notify();
        }
    }

    pub fn discard(&mut self, paths: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let rx = self.ask(Spec::new((t.discard_title)(paths.len())).explain(&explain(cx).discard).danger().confirm(t.discard_confirm), window, cx);
        self.then(rx, window, cx, move |this, _, _, cx| this.exec_detached(Op::Discard { paths }, cx));
    }

    pub fn discard_lines(&mut self, file: String, hunk: usize, header: String, lines: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.working_busy {
            return;
        }
        let t = t(cx);
        let rx = self.ask(Spec::new(t.discard_lines_title).explain(&explain(cx).discard).danger().confirm(t.discard_lines_confirm), window, cx);
        self.then(rx, window, cx, move |this, _, _, cx| {
            this.busy = true;
            let session = this.session.clone();
            let context = diff_view::options(cx).context;
            git::bg(cx, move || engine::diff::discard_lines_with(&session.repo, &file, hunk, &header, &lines, context), |this, r, cx| {
                if let Err(e) = r {
                    this.error = e;
                }
                let refresh = this.refresh(false, cx);
                cx.spawn(async move |this, cx| {
                    refresh.await;
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        cx.notify();
                    });
                })
                .detach();
            })
            .detach();
        });
    }

    fn repository_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let session = self.session.clone();
        cx.spawn_in(window, async move |this, cx| {
            let fresh = cx.background_spawn(async move { (engine::refs::list(&session.repo), engine::workflow::read(&session.repo)) }).await;
            let Ok(rx) = this.update_in(cx, |this, window, cx| {
                match fresh {
                    (Ok(refs), workflow) => {
                        this.refs = refs;
                        if let Ok(w) = workflow {
                            this.workflow = w;
                        }
                    }
                    (Err(e), _) => {
                        this.error = e;
                        return None;
                    }
                }
                let t = t(cx);
                Some(this.ask(
                    Spec::new(format!("{} · {}", t.repository_settings, this.path)).message(t.revert_scope).fields([
                        Field::checkbox("disabled", t.disable_revert, this.refs.revert_disabled),
                        Field::text("check", t.check.command).optional().value(this.workflow.check_command.clone()),
                        Field::checkbox("auto", t.check.auto, this.workflow.check_before_commit),
                        Field::textarea("template", t.commit_template).optional().value(this.workflow.template.clone()),
                        Field::text("diffTool", t.external_tools.diff_setting).optional().value(this.external_tools.diff.clone()),
                        Field::text("mergeTool", t.external_tools.merge_setting).optional().value(this.external_tools.merge.clone()),
                    ]),
                    window,
                    cx,
                ))
            }) else { return };
            let Some(rx) = rx else { return };
            let Ok(Some(v)) = rx.await else { return };
            let Ok(session) = this.update(cx, |this, _| this.session.clone()) else { return };
            let tools = ExternalTools { diff: v.text("diffTool"), merge: v.text("mergeTool") };
            let settings = Settings { template: v.text("template"), check_command: v.text("check"), check_before_commit: v.bool("auto") };
            let saved = cx.background_spawn(async move { engine::workflow::set_tools(&session.repo, &tools).and_then(|_| engine::workflow::write(&session.repo, &settings)) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(e) = saved {
                    this.error = e;
                }
                // 只有这一项会进命令日志和撤销列表，没改就不执行
                if v.bool("disabled") != this.refs.revert_disabled {
                    this.exec_detached(Op::SetRevertDisabled { disabled: v.bool("disabled") }, cx);
                } else {
                    let _ = this.refresh(false, cx);
                }
            });
        })
        .detach();
    }

    /// 跑提交前检查，输出进命令日志；没通过时把日志展开。结果是是否通过
    pub fn run_check(&mut self, cx: &mut Context<Self>) -> Task<bool> {
        if self.checking {
            return Task::ready(false);
        }
        self.checking = true;
        self.check_result = None;
        cx.notify();
        let session = self.session.clone();
        let run = cx.background_spawn(async move { engine::workflow::run_check(&session.repo) });
        cx.spawn(async move |this, cx| {
            let r = run.await;
            this.update(cx, |this, cx| {
                this.checking = false;
                cx.notify();
                match r {
                    Ok(log) => {
                        let ok = log.ok;
                        this.check_result = Some(log.clone());
                        this.push_log(log);
                        if !ok {
                            this.show_log = true;
                        }
                        ok
                    }
                    Err(e) => {
                        this.error = e;
                        false
                    }
                }
            })
            .unwrap_or(false)
        })
    }

    /// 提交之前：设置了自动检查就先跑，没通过时由用户决定要不要照样提交
    pub fn before_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        if !self.workflow.check_before_commit || self.workflow.check_command.is_empty() {
            return Task::ready(true);
        }
        let check = self.run_check(cx);
        let command = self.workflow.check_command.clone();
        cx.spawn_in(window, async move |this, cx| {
            if check.await {
                return true;
            }
            let Ok(rx) = this.update_in(cx, |this, window, cx| {
                let t = t(cx);
                this.ask(Spec::new(t.check.failed_title).message(command).warning(t.check.failed_warning).danger().confirm(t.check.commit_anyway), window, cx)
            }) else { return false };
            matches!(rx.await, Ok(Some(_)))
        })
    }

    pub fn edit_identity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let (name, email) = self.identity.as_ref().map(|i| (i.name.clone(), i.email.clone())).unwrap_or_default();
        let rx = self.ask(Spec::new(t.edit_identity).message(t.identity_scope).fields([Field::text("name", t.identity_name).value(name), Field::text("email", t.identity_email).value(email)]), window, cx);
        self.then(rx, window, cx, move |this, v, _, cx| {
            this.busy = true;
            this.error.clear();
            let session = this.session.clone();
            let (name, email) = (v.text("name"), v.text("email"));
            let expected = format!("{} <{}>", name.trim(), email.trim());
            git::bg(cx, move || engine::identity::set(&session.repo, &name, &email).or_else(|e| engine::identity::read(&session.repo).map(|_| Err(e))?), move |this, r, cx| {
                this.busy = false;
                match r {
                    Ok(identity) => {
                        if identity.author.as_deref() != Some(&expected) || identity.committer.as_deref() != Some(&expected) {
                            this.error = t.identity_overridden.into();
                        }
                        this.identity = Some(identity);
                    }
                    Err(e) => this.error = e,
                }
                cx.notify();
            })
            .detach();
        });
    }

    /// 先用白话和示意图说明这个操作会发生什么，确认后再执行；勾过「不再显示」的直接执行
    pub fn explained(&mut self, key: &'static str, title: String, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        if skipped(cx, key) {
            return self.exec_detached(op, cx);
        }
        let rx = self.ask(Spec::new(title).explain(explain(cx).get(key)).skip_key(key), window, cx);
        self.then(rx, window, cx, move |this, _, _, cx| this.exec_detached(op, cx));
    }

    fn head(&self) -> Option<String> {
        self.refs.head.as_deref().and_then(|h| h.strip_prefix("refs/heads/")).map(str::to_owned)
    }

    pub fn new_branch(&mut self, start: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let current = if start == "HEAD" { format!("{} ({})", t.current_branch, self.head().unwrap_or_else(|| t.detached.into())) } else { start[..7.min(start.len())].to_owned() };
        let mut options = vec![(start.clone(), current)];
        options.extend(
            self.refs
                .refs
                .iter()
                .filter(|r| (r.name.starts_with("refs/heads/") || r.name.starts_with("refs/remotes/")) && !r.name.ends_with("/HEAD") && (start != "HEAD" || Some(&r.name) != self.refs.head.as_ref()))
                .map(|r| (short(&r.name).to_owned(), short(&r.name).to_owned())),
        );
        let rx = self.ask(
            Spec::new(t.new_branch).explain(&explain(cx).create_branch).message(t.branch_rule).fields([
                Field::text("name", t.branch_name).pattern(branch_name_ok),
                Field::select("start", t.branch_from, options).value(start),
                Field::checkbox("checkout", t.checkout_after_create, true),
            ]),
            window,
            cx,
        );
        self.then(rx, window, cx, |this, v, _, cx| this.exec_detached(Op::CreateBranch { name: v.text("name"), start: v.text("start"), checkout: v.bool("checkout") }, cx));
    }

    fn stash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let rx = self.ask(Spec::new(t.stash_title).explain(&explain(cx).stash).fields([Field::text("message", t.stash_message).optional(), Field::checkbox("untracked", t.include_untracked, true)]), window, cx);
        self.then(rx, window, cx, |this, v, _, cx| this.exec_detached(Op::StashPush { message: v.text("message"), include_untracked: v.bool("untracked"), paths: Vec::new() }, cx));
    }

    pub fn push(&mut self, branch: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let Some(branch) = branch else {
            self.error = t.no_branch.into();
            return cx.notify();
        };
        let session = self.session.clone();
        cx.spawn_in(window, async move |this, cx| {
            let remotes = cx.background_spawn(async move { engine::refs::remotes(&session.repo) }).await.unwrap_or_default();
            let Ok(rx) = this.update_in(cx, |this, window, cx| {
                if remotes.is_empty() {
                    this.error = t.no_remote.into();
                    cx.notify();
                    return None;
                }
                let upstream = this.refs.refs.iter().find(|r| r.name == format!("refs/heads/{branch}")).and_then(|r| r.upstream.clone());
                let target = split_upstream(upstream.as_deref(), &remotes);
                let force = Field::checkbox("force", t.force_push, false);
                let tags = Field::checkbox("tags", t.push_tags, false);
                let spec = match &target {
                    Some((remote, up)) if *up == branch => Spec::new((t.push_title)(&branch)).explain(&explain(cx).push).message((t.push_to)(&format!("{remote}/{branch}"))).confirm(t.push).fields([force, tags]),
                    // 上游与本地分支不同名：先推荐同名分支，仍需明确选一个目标
                    Some((remote, up)) => Spec::new((t.push_title)(&branch)).warning((t.push_mismatch)(&branch, &format!("{remote}/{up}"))).confirm(t.push).fields([
                        Field::radio("target", t.push_target, vec![("same".into(), (t.push_to_same_name)(&format!("{remote}/{branch}"))), ("upstream".into(), (t.push_to_upstream)(&format!("{remote}/{up}")))]),
                        force,
                        tags,
                    ]),
                    None => {
                        let dest = if remotes.len() == 1 { remotes[0].clone() } else { format!("<{}>", t.remote) };
                        let mut spec = Spec::new((t.push_title)(&branch)).explain(&explain(cx).push).message((t.push_new)(&format!("{dest}/{branch}"))).confirm(t.push);
                        if remotes.len() > 1 {
                            spec = spec.field(Field::radio("remote", t.remote, remotes.iter().map(|r| (r.clone(), r.clone())).collect()).value(remotes[0].clone()));
                        }
                        spec.fields([force, tags])
                    }
                };
                Some((this.ask(spec, window, cx), target, remotes[0].clone()))
            }) else { return };
            let Some((rx, target, first)) = rx else { return };
            let Ok(Some(v)) = rx.await else { return };
            let op = match target {
                Some((remote, up)) if up == branch => Op::Push { remote, branch: branch.clone(), remote_branch: branch, force: v.bool("force"), set_upstream: false, tags: v.bool("tags") },
                Some((remote, up)) => {
                    let same = v.text("target") == "same";
                    Op::Push { remote, remote_branch: if same { branch.clone() } else { up }, branch, force: v.bool("force"), set_upstream: same, tags: v.bool("tags") }
                }
                None => {
                    let remote = Some(v.text("remote")).filter(|r| !r.is_empty()).unwrap_or(first);
                    Op::Push { remote, branch: branch.clone(), remote_branch: branch, force: v.bool("force"), set_upstream: true, tags: v.bool("tags") }
                }
            };
            let _ = this.update(cx, |this, cx| this.exec_detached(op, cx));
        })
        .detach();
    }

    // ---------- GitHub 拉取请求 ----------

    /// 重新读打开着的拉取请求；要联网，不跟着仓库刷新走
    pub fn load_prs(&mut self, cx: &mut Context<Self>) {
        if self.prs_loading {
            return;
        }
        self.prs_loading = true;
        let session = self.session.clone();
        git::bg(
            cx,
            move || {
                let remote = engine::github::remote(&session.repo).ok()?;
                let list = engine::github::list(&session.repo, &remote);
                Some((remote, list))
            },
            |this, r, cx| {
                this.prs_loading = false;
                this.github = r.as_ref().map(|(remote, _)| remote.clone());
                match r.map(|(_, list)| list) {
                    Some(Ok(list)) => {
                        this.prs = list;
                        this.pr_error.clear();
                    }
                    Some(Err(e)) => this.pr_error = e,
                    None => this.prs.clear(),
                }
                cx.notify();
            },
        )
        .detach();
    }

    /// 在后台跑一个 gh 操作；完成后重读拉取请求，`fetch` 时再获取一次让本地看到 GitHub 上的变化
    fn run_pr(&mut self, work: impl FnOnce(&engine::Repo, &engine::github::Remote) -> engine::Result<()> + Send + 'static, fetch: bool, cx: &mut Context<Self>) {
        let Some(remote) = self.github.clone() else { return };
        if self.busy || self.working_busy {
            return;
        }
        self.busy = true;
        self.error.clear();
        cx.notify();
        let session = self.session.clone();
        git::bg(cx, move || work(&session.repo, &remote), move |this, r, cx| {
            this.busy = false;
            this.load_prs(cx);
            match r {
                Err(e) => this.error = e,
                Ok(()) if fetch => this.exec_detached(Op::Fetch { background: false }, cx),
                Ok(()) => drop(this.refresh(false, cx)),
            }
            cx.notify();
        })
        .detach();
    }

    /// 从本地分支创建拉取请求；分支还没推送或有没推送的提交时先推送
    pub fn create_pr(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let Some(remote) = self.github.clone() else { return };
        let prefix = format!("refs/remotes/{}/", remote.name);
        let bases: Vec<String> = self.refs.refs.iter().filter_map(|r| r.name.strip_prefix(&prefix)).filter(|b| *b != "HEAD" && *b != branch).map(str::to_owned).collect();
        if bases.is_empty() {
            self.error = t.pr.no_base.into();
            return cx.notify();
        }
        let base = ["main", "master", "develop"].into_iter().find(|b| bases.iter().any(|x| x == b)).map_or_else(|| bases[0].clone(), str::to_owned);
        let full = format!("refs/heads/{branch}");
        let upstream = self.refs.refs.iter().find(|r| r.name == full).and_then(|r| r.upstream.clone());
        let ahead = self.tracks.get(&full).map_or(0, |t| t.ahead);
        let push = upstream.as_deref() != Some(&format!("{prefix}{branch}")) || ahead > 0;
        let session = self.session.clone();
        let b = branch.clone();
        cx.spawn_in(window, async move |this, cx| {
            let title = cx.background_spawn(async move { engine::github::default_title(&session.repo, &b) }).await;
            let Ok(rx) = this.update_in(cx, |this, window, cx| {
                let options = bases.iter().map(|b| (b.clone(), format!("{}/{b}", remote.name))).collect();
                let spec = Spec::new((t.pr.create_title)(&branch)).confirm(t.pr.create).fields([
                    Field::select("base", t.pr.base, options).value(base),
                    Field::text("title", t.pr.title).value(title),
                    Field::textarea("body", t.pr.body).optional(),
                    Field::checkbox("draft", t.pr.draft, false),
                ]);
                let spec = if push { spec.message((t.pr.push_first)(&format!("{}/{branch}", remote.name))) } else { spec };
                this.ask(spec, window, cx)
            }) else { return };
            let Ok(Some(v)) = rx.await else { return };
            if push {
                let op = Op::Push { remote: remote.name.clone(), branch: branch.clone(), remote_branch: branch.clone(), force: false, set_upstream: upstream.is_none(), tags: false };
                let Ok(run) = this.update(cx, |this, cx| this.exec(op, false, cx)) else { return };
                if !run.await {
                    return;
                }
            }
            let (base, title, body, draft) = (v.text("base"), v.text("title"), v.text("body"), v.bool("draft"));
            let _ = this.update(cx, |this, cx| this.run_pr(move |repo, remote| engine::github::create(repo, remote, &base, &branch, &title, &body, draft).map(drop), false, cx));
        })
        .detach();
    }

    fn merge_pr(&mut self, pr: &engine::github::Pr, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let methods = vec![("merge".into(), t.pr.merge_commit.into()), ("squash".into(), t.pr.squash.into()), ("rebase".into(), t.pr.rebase.into())];
        let rx = self.ask(
            Spec::new((t.pr.merge_title)(pr.number))
                .message(format!("{}\n{} → {}", pr.title, pr.head, pr.base))
                .maybe_warning((pr.checks == "failure").then(|| t.pr.checks_failed.to_owned()))
                .confirm(t.pr.merge)
                .field(Field::radio("method", t.pr.method, methods).value("merge")),
            window,
            cx,
        );
        let n = pr.number;
        self.then(rx, window, cx, move |this, v, _, cx| {
            let method = v.text("method");
            this.run_pr(move |repo, remote| engine::github::merge(repo, remote, n, &method), true, cx)
        });
    }

    pub fn checkout_pr(&mut self, number: u64, cx: &mut Context<Self>) {
        self.run_pr(move |repo, remote| engine::github::checkout(repo, remote, number), false, cx);
    }

    /// 在提交图里定位到拉取请求的分支：本地有就用本地的，否则用远程的
    pub fn jump_pr(&mut self, head: &str, cx: &mut Context<Self>) {
        let remote = self.github.as_ref().map(|r| format!("refs/remotes/{}/{head}", r.name)).unwrap_or_default();
        let local = format!("refs/heads/{head}");
        let id = self.refs.refs.iter().find(|r| r.name == local).or_else(|| self.refs.refs.iter().find(|r| r.name == remote)).map(|r| r.id.clone());
        if let Some(id) = id {
            self.jump(&id, cx);
        }
    }

    pub fn pr_menu(&mut self, at: Point<Pixels>, pr: &engine::github::Pr, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let (url, link, n, p) = (pr.url.clone(), pr.url.clone(), pr.number, pr.clone());
        let items = vec![
            Item::new(t.pr.open, move |_, cx| cx.open_url(&url)),
            Item::new(t.pr.checkout, self.act(cx, move |this, _, cx| this.checkout_pr(n, cx))),
            Item::new(t.pr.merge_menu, self.act(cx, move |this, window, cx| this.merge_pr(&p, window, cx))).disabled(pr.draft),
            Item::Sep,
            Item::new(t.pr.copy_link, move |_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(link.clone()))),
        ];
        self.show_menu(at, items, window, cx);
    }

    fn pull(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let upstream = self.refs.refs.iter().find(|r| Some(&r.name) == self.refs.head.as_ref()).and_then(|r| r.upstream.clone());
        let rx = self.ask(
            Spec::new(t.pull)
                .explain(&explain(cx).pull)
                .message((t.pull_from)(upstream.as_deref().map(short).unwrap_or(t.no_upstream)))
                .maybe_warning((!self.overlap.is_empty()).then(|| (t.binary_overlap)(&self.overlap.join("、"))))
                .confirm(t.pull)
                .fields([Field::checkbox("rebase", t.pull_rebase, true), Field::checkbox("autostash", t.pull_autostash, false)]),
            window,
            cx,
        );
        self.then(rx, window, cx, |this, v, _, cx| this.exec_detached(Op::Pull { rebase: v.bool("rebase"), autostash: v.bool("autostash") }, cx));
    }

    fn reset(&mut self, target: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let rx = self.ask(
            Spec::new((t.reset_title)(&self.head().unwrap_or_else(|| "HEAD".into()))).explain(&explain(cx).reset).danger().field(
                Field::radio("mode", t.reset_mode, vec![("soft".into(), t.reset_soft.into()), ("mixed".into(), t.reset_mixed.into()), ("hard".into(), t.reset_hard.into())]).value("mixed"),
            ),
            window,
            cx,
        );
        self.then(rx, window, cx, move |this, v, _, cx| {
            let mode = match v.text("mode").as_str() {
                "soft" => ResetMode::Soft,
                "hard" => ResetMode::Hard,
                _ => ResetMode::Mixed,
            };
            this.exec_detached(Op::Reset { target, mode }, cx)
        });
    }

    /// 切换到本地分支；有未提交的改动或分支已在别处打开时先问清楚
    fn switch_to(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        if let Some(other) = self.worktrees.iter().find(|w| !w.current && w.branch.as_deref() == Some(&format!("refs/heads/{branch}"))) {
            let path = other.path.clone();
            let rx = self.ask(Spec::new((t.switch_dirty_title)(&branch)).message((t.in_other_worktree)(&branch, &path)).confirm(t.open_in_tab), window, cx);
            return self.then(rx, window, cx, move |this, _, window, cx| this.open_path(path, window, cx));
        }
        // 未跟踪的文件不受切换影响
        if !self.entries.iter().any(|e| e.unstaged != Some('?')) {
            return self.exec_detached(Op::Checkout { target: branch }, cx);
        }
        let rx = self.ask(
            Spec::new((t.switch_dirty_title)(&branch)).explain(&explain(cx).switch_dirty).field(
                Field::radio("how", t.switch_how, vec![("stash".into(), t.switch_stash.into()), ("carry".into(), t.switch_carry.into()), ("worktree".into(), t.switch_worktree.into())]).value("stash"),
            ),
            window,
            cx,
        );
        self.then(rx, window, cx, move |this, v, window, cx| match v.text("how").as_str() {
            "carry" => this.exec_detached(Op::Checkout { target: branch }, cx),
            "worktree" => this.add_worktree(branch, true, window, cx),
            _ => {
                let stash = this.exec(Op::StashPush { message: (t.auto_stash)(&branch), include_untracked: false, paths: Vec::new() }, false, cx);
                cx.spawn(async move |this, cx| {
                    if !stash.await {
                        return;
                    }
                    // 切换失败时改动也要放回来，所以不看 checkout 的结果
                    if let Ok(checkout) = this.update(cx, |this, cx| this.exec(Op::Checkout { target: branch }, true, cx)) {
                        checkout.await;
                    }
                    if let Ok(apply) = this.update(cx, |this, cx| this.exec(Op::StashApply { name: "stash@{0}".into(), pop: true }, false, cx)) {
                        apply.await;
                    }
                })
                .detach();
            }
        });
    }

    /// 新建工作树并在页签中打开。`existing` 为 true 时直接检出 `start` 这个分支，否则从 `start` 新建分支
    pub fn add_worktree(&mut self, start: String, existing: bool, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let mut spec = Spec::new(t.new_worktree).explain(&explain(cx).worktree);
        if !existing {
            spec = spec.message(t.branch_rule).field(Field::text("branch", t.branch_name).pattern(branch_name_ok));
        }
        let suffix = if existing { start.replace(['\\', '/'], "-") } else { "worktree".into() };
        let rx = self.ask(spec.field(Field::text("path", t.worktree_path).value(format!("{}-{suffix}", self.path))), window, cx);
        self.then(rx, window, cx, move |this, v, window, cx| {
            let dir = v.text("path");
            let run = this.exec(Op::WorktreeAdd { path: dir.clone(), start, new_branch: (!existing).then(|| v.text("branch")) }, true, cx);
            cx.spawn_in(window, async move |this, cx| {
                if run.await {
                    let _ = this.update_in(cx, |this, window, cx| this.open_path(dir, window, cx));
                }
            })
            .detach();
        });
    }

    pub fn remove_worktree(&mut self, w: &Worktree, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let name = w.path.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(&w.path).to_owned();
        let rx = self.ask(
            Spec::new((t.wt_remove_title)(&name))
                .message(explain(cx).worktree.undo.unwrap_or_default())
                .maybe_warning((w.changes > 0).then(|| (t.wt_remove_warning)(w.changes)))
                .danger(),
            window,
            cx,
        );
        let (path, force) = (w.path.clone(), w.changes > 0);
        self.then(rx, window, cx, move |this, _, _, cx| this.exec_detached(Op::WorktreeRemove { path, force }, cx));
    }

    /// 在（新）页签中打开另一个仓库或工作树
    pub fn open_path(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(opener) = self.opener.upgrade() {
            let w = window.window_handle();
            opener.update(cx, |app, cx| app.open_repo(path, w, cx));
        }
    }

    /// 侧栏双击：本地分支切换、标签检出、远程分支建本地分支、贮藏应用
    pub fn activate_ref(&mut self, kind: Kind, name: String, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            Kind::Branch => self.switch_to(short(&name).to_owned(), window, cx),
            Kind::Tag => self.explained("checkout_commit", short(&name).to_owned(), Op::Checkout { target: short(&name).to_owned() }, window, cx),
            Kind::Remote => self.explained("track", short(&name).to_owned(), Op::Track { remote_branch: short(&name).to_owned() }, window, cx),
            Kind::Stash => self.exec_detached(Op::StashApply { name, pop: false }, cx),
        }
    }

    pub fn toggle_pin(&mut self, name: &str, cx: &mut Context<Self>) {
        if !self.pinned.iter().any(|n| n == name) {
            self.pinned.push(name.to_owned());
        } else {
            self.pinned.retain(|n| n != name);
        }
        save_list(cx, "pinnedBranches", &self.path, &self.pinned);
        cx.notify();
    }

    fn toggle_auto_ff(&mut self, name: &str, cx: &mut Context<Self>) {
        if !self.auto_ff.iter().any(|n| n == name) {
            self.auto_ff.push(name.to_owned());
        } else {
            self.auto_ff.retain(|n| n != name);
        }
        save_list(cx, "autoFastForward", &self.path, &self.auto_ff);
        cx.notify();
    }

    /// 列出可以清理的本地分支让用户勾选：已经合入主干的默认勾上，上游已删除但没合并的默认不勾
    pub fn clean_branches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let Some(base) = self.compare_bases("").into_iter().next() else {
            self.error = t.branch_list.no_base.into();
            return cx.notify();
        };
        let session = self.session.clone();
        let base_short = short(&base).to_owned();
        let b = base_short.clone();
        cx.spawn_in(window, async move |this, cx| {
            let merged = cx.background_spawn(async move { engine::refs::merged_branches(&session.repo, &b) }).await;
            let Ok(rx) = this.update_in(cx, |this, window, cx| {
                let merged: Vec<String> = match merged {
                    Ok(m) => m,
                    Err(e) => {
                        this.error = e;
                        cx.notify();
                        return None;
                    }
                };
                let gone: Vec<String> = this.tracks.values().filter(|x| x.gone && Some(&x.name) != this.refs.head.as_ref() && !merged.contains(&x.name)).map(|x| x.name.clone()).collect();
                if merged.is_empty() && gone.is_empty() {
                    let _ = this.ask(Spec::new(t.branch_list.clean).message((t.branch_list.nothing)(&base_short)), window, cx);
                    return None;
                }
                let fields = merged
                    .iter()
                    .map(|n| Field::checkbox(n.clone(), (t.branch_list.merged)(short(n), &base_short), true))
                    .chain(gone.iter().map(|n| Field::checkbox(n.clone(), (t.branch_list.gone)(short(n)), false)));
                let rx = this.ask(Spec::new(t.branch_list.clean).message((t.branch_list.clean_hint)(&base_short)).danger().confirm(t.branch_list.clean_confirm).fields(fields), window, cx);
                Some((rx, merged, gone))
            }) else { return };
            let Some((rx, merged, gone)) = rx else { return };
            let Ok(Some(v)) = rx.await else { return };
            // 没合并的要强制删除才删得掉；每个删除都进撤销列表
            for name in merged.iter().chain(&gone) {
                if !v.bool(name) {
                    continue;
                }
                let op = Op::DeleteBranch { name: short(name).to_owned(), force: !merged.contains(name) };
                let Ok(run) = this.update(cx, |this, cx| this.exec(op, true, cx)) else { return };
                if !run.await {
                    break;
                }
            }
        })
        .detach();
    }

    // ---------- 自动刷新与获取 ----------

    /// 每个已打开的桌面仓库一个调度器：每分钟检查一次，不依赖焦点，不补跑休眠期间错过的任务
    fn start_auto_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.spawn_in(window, async move |this, cx| {
            let mut last_fetch: Option<std::time::Instant> = None;
            let mut fetch_enabled = false;
            loop {
                let Ok(step) = this.update(cx, |this, cx| {
                    let settings = store::get::<AutoSync>(cx, "autoSync").unwrap_or_default();
                    let ready = !this.busy && !this.working_busy && !this.initializing && !this.loading_all && this.refreshing.is_none() && !this.dialog.read(cx).is_open();
                    (settings, ready)
                }) else { return };
                let (settings, ready) = step;
                if settings.fetch && !fetch_enabled {
                    last_fetch = None;
                }
                fetch_enabled = settings.fetch;
                if ready {
                    let fetch = settings.fetch && last_fetch.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(600));
                    if fetch {
                        last_fetch = Some(std::time::Instant::now());
                        if let Ok(run) = this.update(cx, |this, cx| this.auto_fetch(cx)) {
                            run.await;
                        }
                    }
                    if settings.refresh || fetch {
                        if let Ok(r) = this.update(cx, |this, cx| this.refresh(true, cx)) {
                            r.await;
                        }
                    }
                }
                cx.background_executor().timer(std::time::Duration::from_secs(60)).await;
            }
        });
        self._auto_sync = Some(task);
    }

    fn auto_fetch(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.busy = true;
        let session = self.session.clone();
        let run = cx.background_spawn(async move {
            if engine::refs::remotes(&session.repo).map(|r| r.is_empty()).unwrap_or(true) {
                return None;
            }
            Some(engine::ops::run(&session.repo, Op::Fetch { background: true }))
        });
        cx.spawn(async move |this, cx| {
            let r = run.await;
            let ff = this.update(cx, |this, cx| {
                this.busy = false;
                cx.notify();
                match r {
                    None => {
                        this.auto_fetch_error = false;
                        None
                    }
                    Some(Ok(log)) => {
                        this.auto_fetch_error = !log.ok;
                        let ok = log.ok;
                        this.push_log(log);
                        ok.then(|| this.auto_fast_forward(cx))
                    }
                    Some(Err(e)) => {
                        this.auto_fetch_error = true;
                        this.push_log(Log { command: "git fetch --all --prune".into(), output: e, ok: false });
                        None
                    }
                }
            });
            if let Ok(Some(ff)) = ff {
                ff.await;
            }
        })
    }

    /// 自动获取之后，把勾选过的分支快进到上游。只做不丢提交、不碰未提交改动的那一种，其余的说明原因
    fn auto_fast_forward(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let alive: Vec<String> = self.auto_ff.iter().filter(|n| self.refs.refs.iter().any(|r| r.name == **n)).cloned().collect();
        if alive.len() != self.auto_ff.len() {
            self.auto_ff = alive.clone();
            save_list(cx, "autoFastForward", &self.path, &alive);
        }
        if alive.is_empty() {
            self.auto_ff_skipped.clear();
            return Task::ready(());
        }
        let t = t(cx);
        let session = self.session.clone();
        let head = self.refs.head.clone();
        let elsewhere: Vec<String> = self.worktrees.iter().filter(|w| !w.current).filter_map(|w| w.branch.clone()).collect();
        let run = cx.background_spawn(async move {
            let tracking = engine::refs::tracking(&session.repo).unwrap_or_default();
            let mut skipped = Vec::new();
            let mut logs = Vec::new();
            for name in alive {
                let track = tracking.iter().find(|x| x.name == name);
                let branch = short(&name).to_owned();
                match track {
                    None => skipped.push((t.auto_ff.no_upstream)(&branch)),
                    Some(x) if x.gone => skipped.push((t.auto_ff.no_upstream)(&branch)),
                    Some(x) if x.behind == 0 => {}
                    Some(x) if x.ahead > 0 => skipped.push((t.auto_ff.diverged)(&branch)),
                    _ if elsewhere.contains(&name) => skipped.push((t.auto_ff.elsewhere)(&branch)),
                    _ if Some(&name) == head.as_ref() && engine::status::status(&session.repo).is_ok_and(|s| !s.is_empty()) => skipped.push((t.auto_ff.dirty)(&branch)),
                    _ => match engine::ops::run(&session.repo, Op::FastForward { branch: branch.clone() }) {
                        Ok(log) => {
                            if !log.ok {
                                skipped.push((t.auto_ff.failed)(&branch));
                            }
                            logs.push(log);
                        }
                        Err(e) => skipped.push(e),
                    },
                }
            }
            (skipped, logs)
        });
        cx.spawn(async move |this, cx| {
            let (skipped, logs) = run.await;
            let _ = this.update(cx, |this, cx| {
                for log in logs {
                    this.push_log(log);
                }
                this.auto_ff_skipped = skipped;
                cx.notify();
            });
        })
    }

    // ---------- 右键菜单 ----------

    fn show_menu(&self, at: Point<Pixels>, items: Vec<Item>, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.update(cx, |m, cx| m.show(at, items, window, cx));
    }

    /// 包一个在本视图上执行的菜单动作
    fn act(&self, cx: &Context<Self>, f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> impl Fn(&mut Window, &mut App) + 'static {
        let me = cx.entity().downgrade();
        move |window, cx| {
            let _ = me.update(cx, |this, cx| f(this, window, cx));
        }
    }

    pub fn ref_menu(&mut self, at: Point<Pixels>, kind: Kind, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let ex = explain(cx);
        let s = short(&name).to_owned();
        if kind == Kind::Stash {
            let (n1, n2, n3) = (name.clone(), name.clone(), name.clone());
            let items = vec![
                Item::new(t.stash_apply, self.act(cx, move |this, _, cx| this.exec_detached(Op::StashApply { name: n1.clone(), pop: false }, cx))).hint(t.stash_apply_hint),
                Item::new(t.stash_pop, self.act(cx, move |this, _, cx| this.exec_detached(Op::StashApply { name: n2.clone(), pop: true }, cx))).hint(t.stash_pop_hint),
                Item::Sep,
                Item::new(
                    t.stash_drop,
                    self.act(cx, move |this, window, cx| {
                        let rx = this.ask(Spec::new(t.drop_stash_title).warning(t.drop_stash_warning).danger(), window, cx);
                        let n = n3.clone();
                        this.then(rx, window, cx, move |this, _, _, cx| this.exec_detached(Op::StashDrop { name: n }, cx));
                    }),
                )
                .hint(ex.stash_drop.short)
                .danger(),
            ];
            return self.show_menu(at, items, window, cx);
        }
        let current = Some(&name) == self.refs.head.as_ref();
        let onto = self.head().unwrap_or_else(|| "HEAD".into());
        // 一级只放天天用的操作，其余按「对比」「提交图」「管理」收进子菜单
        let mut items = Vec::new();
        if !current {
            let (n, k) = (name.clone(), kind);
            let key = match kind {
                Kind::Remote => "track",
                Kind::Tag => "checkout_commit",
                _ => "checkout",
            };
            items.push(Item::new(match kind { Kind::Remote => t.checkout_remote, Kind::Tag => t.checkout_commit, _ => t.checkout }, self.act(cx, move |this, window, cx| this.activate_ref(k, n.clone(), window, cx))).hint(ex.get(key).short));
            let (s1, s2, o1, o2) = (s.clone(), s.clone(), onto.clone(), onto.clone());
            items.push(Item::new((t.merge_into)(&onto), self.act(cx, move |this, window, cx| this.explained("merge", (t.merge_into)(&o1), Op::Merge { target: s1.clone() }, window, cx))).hint(ex.merge.short));
            items.push(Item::new((t.rebase_onto)(&onto), self.act(cx, move |this, window, cx| this.explained("rebase", (t.rebase_onto)(&o2), Op::Rebase { onto: s2.clone() }, window, cx))).hint(ex.rebase.short));
        }
        let mut manage = Vec::new();
        if kind == Kind::Branch {
            if let Some(upstream) = self.refs.refs.iter().find(|r| r.name == name).and_then(|r| r.upstream.clone()) {
                let title = (t.fast_forward_to)(short(&upstream));
                let s1 = s.clone();
                items.push(Item::new(title.clone(), self.act(cx, move |this, window, cx| this.explained("fast_forward", title.clone(), Op::FastForward { branch: s1.clone() }, window, cx))).hint(ex.fast_forward.short));
                let n = name.clone();
                manage.push(Item::new(if self.auto_ff.contains(&name) { t.auto_ff.off } else { t.auto_ff.on }, self.act(cx, move |this, _, cx| this.toggle_auto_ff(&n, cx))).hint(t.auto_ff.hint));
            }
            let s1 = s.clone();
            items.push(Item::new(t.push_branch, self.act(cx, move |this, window, cx| this.push(Some(s1.clone()), window, cx))).hint(ex.push.short));
            if self.github.is_some() {
                match self.prs.iter().find(|p| p.head == s) {
                    Some(pr) => {
                        let url = pr.url.clone();
                        items.push(Item::new((t.pr.open_number)(pr.number), move |_, cx| cx.open_url(&url)));
                    }
                    None => {
                        let s1 = s.clone();
                        items.push(Item::new(t.pr.create_menu, self.act(cx, move |this, window, cx| this.create_pr(s1.clone(), window, cx))));
                    }
                }
            }
            if !current {
                let s1 = s.clone();
                manage.push(Item::new(t.open_in_worktree, self.act(cx, move |this, window, cx| this.add_worktree(s1.clone(), true, window, cx))).hint(ex.worktree.short));
            }
            let s1 = s.clone();
            manage.push(
                Item::new(
                    t.rename,
                    self.act(cx, move |this, window, cx| {
                        let rx = this.ask(Spec::new(t.rename).field(Field::text("name", t.new_name).value(s1.clone())), window, cx);
                        let old = s1.clone();
                        this.then(rx, window, cx, move |this, v, _, cx| this.exec_detached(Op::RenameBranch { old, new: v.text("name") }, cx));
                    }),
                )
                .hint(t.rename_hint),
            );
        }
        if kind != Kind::Tag {
            let n = name.clone();
            manage.push(Item::new(if self.pinned.contains(&name) { t.branch_list.unpin } else { t.branch_list.pin }, self.act(cx, move |this, _, cx| this.toggle_pin(&n, cx))));
        }
        let s1 = s.clone();
        manage.push(Item::new(t.copy_name, move |_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(s1.clone()))));
        if kind == Kind::Branch && !current {
            let s1 = s.clone();
            manage.push(Item::Sep);
            manage.push(
                Item::new(
                    t.delete,
                    self.act(cx, move |this, window, cx| {
                        let rx = this.ask(Spec::new((t.delete_branch_title)(&s1)).explain(&explain(cx).delete_branch).danger().field(Field::checkbox("force", t.force_delete, false)), window, cx);
                        let name = s1.clone();
                        this.then(rx, window, cx, move |this, v, _, cx| this.exec_detached(Op::DeleteBranch { name, force: v.bool("force") }, cx));
                    }),
                )
                .hint(ex.delete_branch.short)
                .danger(),
            );
        } else if kind == Kind::Tag {
            let s1 = s.clone();
            manage.push(Item::Sep);
            manage.push(
                Item::new(
                    t.delete_tag,
                    self.act(cx, move |this, window, cx| {
                        let rx = this.ask(Spec::new((t.delete_tag_title)(&s1)).danger(), window, cx);
                        let name = s1.clone();
                        this.then(rx, window, cx, move |this, _, _, cx| this.exec_detached(Op::DeleteTag { name }, cx));
                    }),
                )
                .danger(),
            );
        }
        let mut compare: Vec<Item> = self
            .compare_bases(&name)
            .into_iter()
            .map(|base| {
                let n = name.clone();
                let label = (t.compare_branch.with)(short(&base));
                Item::new(label, self.act(cx, move |this, _, cx| this.compare_branch(&n, &base, cx)))
            })
            .collect();
        let (n1, n2, n3) = (name.clone(), name.clone(), name.clone());
        compare.push(Item::new(t.compare_branch.other, self.act(cx, move |this, window, cx| this.pick_compare_base(n1.clone(), window, cx))));
        compare.push(Item::Sep);
        compare.push(Item::new(t.tools.compare_from, self.act(cx, move |this, _, _| this.compare_left = n2.clone())));
        compare.push(Item::new(t.tools.compare_to, self.act(cx, move |this, window, cx| this.compare_with(n3.clone(), window, cx))));
        let (n1, n2) = (name.clone(), name.clone());
        let in_scope = self.scope.contains(&name);
        let graph = vec![
            Item::new(t.graph_filter.only, self.act(cx, move |this, _, cx| this.set_scope(vec![n1.clone()], cx).detach())),
            Item::new(if in_scope { t.graph_filter.remove } else { t.graph_filter.add }, self.act(cx, move |this, _, cx| this.toggle_scope(&n2, cx))),
        ];
        items.push(Item::Sep);
        items.push(Item::group(t.menu_groups.compare, compare).hint(t.compare_branch.hint));
        items.push(Item::group(t.menu_groups.graph, graph));
        items.push(Item::group(t.menu_groups.manage, manage));
        self.show_menu(at, items, window, cx);
    }

    /// 文件右键菜单里工作区和提交详情共用的部分。`mode` 决定补丁从哪里取，commit 时 `id` 是提交
    fn file_items(&self, file: &str, mode: &'static str, id: &str, gone: bool, untracked: bool, cx: &Context<Self>) -> Vec<Item> {
        let t = t(cx);
        let sep = if self.path.contains('\\') { "\\" } else { "/" };
        let full = format!("{}/{}", self.path, file).replace(['\\', '/'], sep);
        let mut items = Vec::new();
        let reveal = |open: bool| {
            let f = file.to_owned();
            self.act(cx, move |this, _, cx| {
                let session = this.session.clone();
                let f = f.clone();
                git::bg(cx, move || engine::local_files::reveal(&session.repo, &f, open), |this, r, cx| {
                    if let Err(e) = r {
                        this.error = e;
                        cx.notify();
                    }
                })
                .detach();
            })
        };
        if !gone {
            items.push(Item::new(t.file_menu.open, reveal(true)));
        }
        items.push(Item::new(t.file_menu.reveal, reveal(false)));
        items.push(Item::Sep);
        let f = file.to_owned();
        items.push(Item::new(t.file_menu.copy_path, move |_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(f.clone()))));
        items.push(Item::new(t.file_menu.copy_full_path, move |_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(full.clone()))));
        if !untracked {
            let (f, pid) = (file.to_owned(), id.to_owned());
            items.push(Item::new(
                t.file_menu.copy_patch,
                self.act(cx, move |this, _, cx| {
                    let session = this.session.clone();
                    let (f, id) = (f.clone(), pid.clone());
                    git::bg(cx, move || engine::local_files::patch(&session.repo, mode, &id, &f), |this, r, cx| match r {
                        Ok(text) if !text.is_empty() => cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)),
                        Ok(_) => {}
                        Err(e) => {
                            this.error = e;
                            cx.notify();
                        }
                    })
                    .detach();
                }),
            ));
            if !self.external_tools.diff.is_empty() {
                let (f, id) = (file.to_owned(), id.to_owned());
                items.push(Item::new(t.external_tools.diff, self.act(cx, move |this, _, cx| this.external_open("diff", mode, id.clone(), f.clone(), cx))));
            }
            let f = file.to_owned();
            items.push(Item::Sep);
            items.push(Item::new(
                t.file_menu.history,
                self.act(cx, move |this, window, cx| {
                    this.view = View::History;
                    let f = f.clone();
                    this.search.update(cx, |s, cx| s.file_history(f, window, cx));
                }),
            ));
        }
        items
    }

    pub fn external_open(&mut self, kind: &'static str, mode: &'static str, id: String, file: String, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let tools = self.external_tools.clone();
        git::bg(
            cx,
            move || {
                // 工具取自配置而不是调用参数：界面只能触发，不能临时换成别的命令
                let tool = if kind == "merge" { tools.merge } else { tools.diff };
                engine::workflow::launch(&session.repo, kind, &tool, mode, &id, &file)
            },
            |this, r, cx| {
                if let Err(e) = r {
                    this.error = e;
                    cx.notify();
                }
            },
        )
        .detach();
    }

    pub fn work_file_menu(&mut self, at: Point<Pixels>, entry: &Entry, staged: bool, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let untracked = entry.unstaged == Some('?');
        let file = entry.path.clone();
        let gone = (if staged { entry.staged } else { entry.unstaged }) == Some('D');
        let mut items = self.file_items(&file, if staged { "staged" } else { "unstaged" }, "", gone, untracked, cx);
        if !staged && !entry.conflicted {
            let f = file.clone();
            items.push(Item::Sep);
            items.push(Item::new(t.file_menu.stash, self.act(cx, move |this, _, cx| this.exec_detached(Op::StashPush { message: String::new(), include_untracked: untracked, paths: vec![f.clone()] }, cx))).hint(explain(cx).stash.short));
            if untracked {
                let ignore = |shared: bool| {
                    let f = file.clone();
                    self.act(cx, move |this, _, cx| {
                        let session = this.session.clone();
                        let f = f.clone();
                        git::bg(cx, move || engine::local_files::ignore_file(&session.repo, &f, shared), |this, r, cx| {
                            if let Err(e) = r {
                                this.error = e;
                            }
                            let _ = this.refresh(false, cx);
                        })
                        .detach();
                    })
                };
                items.push(Item::new(t.vscode.ignore_local, ignore(false)).hint(t.vscode.ignore_local_hint));
                items.push(Item::new(t.vscode.ignore_shared, ignore(true)).hint(t.vscode.ignore_shared_hint));
            }
        }
        self.show_menu(at, items, window, cx);
    }

    /// 贮藏详情和提交详情共用这个入口；补丁统一按提交取
    pub fn commit_file_menu(&mut self, at: Point<Pixels>, id: String, file: String, gone: bool, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.file_items(&file, "commit", &id, gone, false, cx);
        self.show_menu(at, items, window, cx);
    }

    fn commit_menu(&mut self, at: Point<Pixels>, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let ex = explain(cx);
        let onto = self.head().unwrap_or_else(|| "HEAD".into());
        let i = || id.clone();
        let (a, b, c, d, e, f, g, h, k) = (i(), i(), i(), i(), i(), i(), i(), i(), i());
        let items = vec![
            Item::new(t.tools.compare_from, self.act(cx, move |this, _, _| this.compare_left = a.clone())),
            Item::new(t.tools.compare_to, self.act(cx, move |this, window, cx| this.compare_with(b.clone(), window, cx))),
            Item::Sep,
            Item::new(t.checkout_commit, self.act(cx, move |this, window, cx| this.explained("checkout_commit", t.checkout_commit.into(), Op::Checkout { target: c.clone() }, window, cx))).hint(ex.checkout_commit.short),
            Item::new(t.new_branch_here, self.act(cx, move |this, window, cx| this.new_branch(d.clone(), window, cx))).hint(ex.create_branch.short),
            Item::new(
                t.new_tag_here,
                self.act(cx, move |this, window, cx| {
                    let rx = this.ask(Spec::new(t.new_tag_here).explain(&explain(cx).create_tag).fields([Field::text("name", t.tag_name), Field::text("message", t.tag_message).optional()]), window, cx);
                    let target = e.clone();
                    this.then(rx, window, cx, move |this, v, _, cx| this.exec_detached(Op::CreateTag { name: v.text("name"), target, message: v.text("message") }, cx));
                }),
            )
            .hint(ex.create_tag.short),
            Item::Sep,
            Item::new(t.cherry_pick, self.act(cx, move |this, window, cx| this.explained("cherry_pick", t.cherry_pick.into(), Op::CherryPick { id: f.clone() }, window, cx))).hint(ex.cherry_pick.short),
            Item::new(t.revert_commit, self.act(cx, move |this, window, cx| this.explained("revert", t.revert_commit.into(), Op::Revert { id: g.clone() }, window, cx)))
                .disabled(self.refs.revert_disabled)
                .hint(if self.refs.revert_disabled { i18n::lookup(t.errors, "PR_REVERT_DISABLED").unwrap_or_default() } else { ex.revert.short }),
            Item::new((t.reset_to)(&onto), self.act(cx, move |this, window, cx| this.reset(h.clone(), window, cx))).hint(ex.reset.short).danger(),
            Item::Sep,
            Item::new(t.copy_id, move |_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(k.clone()))),
        ];
        self.show_menu(at, items, window, cx);
    }

    fn style_author(&mut self, key: String, name: String, email: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let style = crate::author::style(cx, &key);
        let rx = self.ask(Spec::new((t.author_style)(&name)).message(email).fields([Field::checkbox("border", t.author_border, style.border), Field::checkbox("fill", t.author_fill, style.fill)]), window, cx);
        self.then(rx, window, cx, move |_, v, _, cx| {
            crate::author::set_style(cx, key, crate::author::AuthorStyle { border: v.bool("border"), fill: v.bool("fill") });
            cx.refresh_windows();
        });
    }

    // ---------- 渲染 ----------

    fn render_header(&mut self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let ex = explain(cx);
        let busy = self.busy;
        let head = self.head();
        let undo_tip = match self.undos.last() {
            Some(u) => (t.undo_title)(lookup(t.op_names, u.title).unwrap_or(u.title)),
            None => t.undo_nothing.to_owned(),
        };
        let state: String = if busy {
            t.working.into()
        } else if self.loading_all {
            t.loading_history.into()
        } else if let Some((ahead, behind)) = self.refs.ahead_behind {
            if ahead + behind > 0 { format!("{} · {}", (t.to_push)(ahead as usize), (t.to_pull)(behind as usize)) } else { t.wt_synced.into() }
        } else {
            (t.commits)(self.count)
        };
        let detached = head.is_none();
        let tone = if detached { p.yellow } else { p.accent };
        let in_progress = self.refs.in_progress.is_some();
        let no_undo = self.undos.is_empty();
        let show_log = self.show_log;
        let head_for_push = head.clone();
        // 左右两组工具等宽，当前分支才能落在正中；窗口变窄时先收起右侧三个辅助按钮的文字，再窄就全部只留图标
        let (texts, right_texts) = (width > 1100., width > 1360.);
        let named = |b: crate::ui::Btn, text: &str, show: bool| if show { b.px(px(8.)).text(name_only(text).to_owned()).no_cmd() } else { b };
        let tools_left = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .min_w_0()
            .child(named(btn("undo").quiet().icon(Icon::Undo), t.undo, texts).disabled(busy || no_undo).tip(undo_tip).on_click(cx.listener(|this, _, window, cx| this.undo(window, cx))))
            .child(sep(cx))
            .child(named(btn("fetch").quiet().icon(Icon::Fetch), t.fetch, texts).disabled(busy).tip(format!("{} · {}", t.fetch, ex.fetch.short)).on_click(cx.listener(|this, _, _, cx| this.exec_detached(Op::Fetch { background: false }, cx))))
            .child(named(btn("pull").quiet().icon(Icon::Pull), t.pull, texts).disabled(busy || in_progress).tip(format!("{} · {}", t.pull, ex.pull.short)).on_click(cx.listener(|this, _, window, cx| this.pull(window, cx))))
            .child(named(btn("push").quiet().icon(Icon::Push), t.push, texts).disabled(busy).tip(format!("{} · {}", t.push, ex.push.short)).on_click(cx.listener(move |this, _, window, cx| this.push(head_for_push.clone(), window, cx))));
        let current = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(40.))
            .max_w(relative(0.44))
            .px(px(16.))
            .border_1()
            .border_color(alpha(tone, 0.45))
            .rounded(R_LG)
            .bg(if detached { alpha(p.yellow, 0.14) } else { p.accent_soft })
            .text_color(tone)
            .child(icon(Icon::Branch, px(18.), tone))
            .child(div().min_w_0().truncate().font_family(DISPLAY).text_size(FS_LG).font_weight(FontWeight::BOLD).child(head.clone().unwrap_or_else(|| t.detached.into())))
            .child(
                div()
                    .id("ahead-behind")
                    .flex_none()
                    .pl(px(8.))
                    .border_l_1()
                    .border_color(alpha(tone, 0.35))
                    .text_color(p.muted)
                    .text_size(FS_SM)
                    .whitespace_nowrap()
                    .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.ahead_behind_hint).build(window, cx))
                    .child(state),
            );
        let tools_right = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.))
            .min_w_0()
            .child(named(btn("stash").quiet().icon(Icon::Stash), t.stash, texts).disabled(busy).tip(format!("{} · {}", t.stash, ex.stash.short)).on_click(cx.listener(|this, _, window, cx| this.stash(window, cx))))
            .child(named(btn("new-branch").quiet().icon(Icon::Branch), t.new_branch, texts).disabled(busy).tip(format!("{} · {}", t.new_branch, ex.create_branch.short)).on_click(cx.listener(|this, _, window, cx| this.new_branch("HEAD".into(), window, cx))))
            .child(sep(cx))
            .child(named(btn("log").quiet().icon(Icon::Log), t.log, right_texts).on(show_log).tip(format!("{} · {}", t.log, t.log_hint)).on_click(cx.listener(|this, _, _, cx| {
                this.show_log = !this.show_log;
                cx.notify();
            })))
            .child(named(btn("settings").quiet().icon(Icon::Settings), t.repository_settings, right_texts).disabled(busy).tip(t.repository_settings).on_click(cx.listener(|this, _, window, cx| this.repository_settings(window, cx))))
            .child(named(btn("help").quiet().icon(Icon::Help), t.help, right_texts).tip(t.help).on_click(cx.listener(|this, _, window, cx| this.help.update(cx, |h, cx| h.open(window, cx)))));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(16.))
            .min_h(px(56.))
            .p(px(8.))
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .child(div().flex_1().child(tools_left))
            .child(current)
            .child(div().flex_1().flex().justify_end().child(tools_right))
    }

    fn render_banners(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = palette(cx).clone();
        let t = t(cx);
        let bar = |color, text: String| div().px(px(16.)).py(px(8.)).text_size(FS_SM).text_color(color).bg(alpha(color, 0.12)).child(text).into_any_element();
        let mut out = Vec::new();
        if !self.error.is_empty() {
            out.push(bar(p.red, i18n::error_text(cx, &self.error)));
        }
        if self.auto_fetch_error {
            out.push(bar(p.red, t.auto_fetch_failed.into()));
        }
        if !self.auto_ff_skipped.is_empty() {
            out.push(bar(p.red, format!("{}{}", t.auto_ff.skipped, self.auto_ff_skipped.join(" · "))));
        }
        if !self.overlap.is_empty() {
            out.push(bar(p.yellow, (t.binary_overlap)(&self.overlap.join("、"))));
        }
        if let Some(what) = self.refs.in_progress {
            let resolved = self.conflicts().iter().filter(|c| c.1).count();
            let total = self.conflicts().len();
            let mut info = Vec::new();
            if let Some(op) = self.operation.as_ref() {
                if let Some(id) = &op.id {
                    info.push((t.operation_commit)(&id[..8.min(id.len())], op.message.lines().next().unwrap_or("")));
                    if let Some((n, total)) = op.step {
                        info.push((t.operation_step)(n as usize, total as usize));
                    }
                    if total > 0 {
                        info.push((t.conflict_progress)(resolved, total));
                    }
                }
            }
            let busy = self.busy;
            let blocked = what == "revert" && self.refs.revert_disabled;
            out.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(16.))
                    .py(px(8.))
                    .bg(alpha(p.yellow, 0.12))
                    .text_size(FS_SM)
                    .child(div().flex_none().font_weight(FontWeight::BOLD).text_color(p.yellow).child(lookup(t.in_progress, what).unwrap_or(what)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(PROSE)
                            .text_size(FS_MD)
                            .when(!info.is_empty(), |s| s.child(div().truncate().font_weight(FontWeight::BOLD).child(info.join(" · "))))
                            .child(t.in_progress_hint),
                    )
                    .child(btn("continue").small().primary().text((t.continue_operation)(what)).disabled(busy || blocked).on_click(cx.listener(move |this, _, _, cx| this.exec_detached(Op::Continue { what: in_progress(what) }, cx))))
                    .child(btn("abort").small().text((t.abort_operation)(what)).disabled(busy).on_click(cx.listener(move |this, _, _, cx| this.exec_detached(Op::Abort { what: in_progress(what) }, cx))))
                    .into_any_element(),
            );
        }
        out
    }

    /// 这次操作冲突过的全部文件：Git 记下的清单加上现在还没解决的；（路径, 是否已解决）
    pub fn conflicts(&self) -> Vec<(String, bool)> {
        let open: Vec<&str> = self.entries.iter().filter(|e| e.conflicted).map(|e| e.path.as_str()).collect();
        let mut out: Vec<(String, bool)> = Vec::new();
        for path in self.operation.iter().flat_map(|o| o.conflicts.iter().map(String::as_str)).chain(open.iter().copied()) {
            if !out.iter().any(|(p, _)| p == path) {
                out.push((path.to_owned(), !open.contains(&path)));
            }
        }
        out
    }

    fn render_history(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let mut pane = div().flex().flex_col().flex_1().min_w_0().child(self.search.clone());
        if !self.scope.is_empty() {
            let loading = self.loading_all;
            let refs = self.scope_refs();
            let base = self.scope_base();
            let mut bar = div().flex().flex_none().flex_wrap().items_center().gap(px(4.)).px(px(12.)).py(px(4.)).border_b_1().border_color(p.border).text_size(FS_SM).text_color(p.muted).child(t.graph_filter.showing);
            for name in &refs {
                let n = name.clone();
                bar = bar.child(crate::ui::chip(SharedString::from(format!("scope-{name}")), format!("{} ×", short(name)), p.accent, cx.listener(move |this, _, _, cx| this.toggle_scope(&n, cx))));
            }
            if let Some(base) = &base {
                let keep = refs.clone();
                bar = bar.child(div().id("scope-base-tip").tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.compare_branch.remove).build(window, cx)).child(crate::ui::chip("scope-base", format!("{} ×", (t.compare_branch.against)(short(base))), p.accent, cx.listener(move |this, _, _, cx| this.set_scope(keep.clone(), cx).detach()))));
                if refs.len() == 1 {
                    bar = bar.child(btn("compare-files").small().text(t.compare_branch.files).on_click(cx.listener(|this, _, window, cx| this.compare_files(window, cx))));
                }
            } else if refs.len() == 1 {
                let n = refs[0].clone();
                bar = bar.child(btn("compare-other").small().quiet().text(t.compare_branch.other).disabled(loading).on_click(cx.listener(move |this, _, window, cx| this.pick_compare_base(n.clone(), window, cx))));
            }
            bar = bar.child(btn("scope-clear").small().quiet().text(t.graph_filter.clear).disabled(loading).on_click(cx.listener(|this, _, _, cx| this.set_scope(Vec::new(), cx).detach())));
            pane = pane.child(bar);
        }
        pane = if self.count > 0 {
            pane.child(self.graph.clone())
        } else {
            let text = if self.loading_all {
                t.loading_history.to_owned()
            } else if let Some(base) = self.scope_base() {
                (t.compare_branch.none)(short(&base))
            } else {
                t.no_commits.to_owned()
            };
            pane.child(div().flex_1().flex().items_center().justify_center().font_family(PROSE).text_size(FS_LG).text_color(p.muted).child(text))
        };
        let l = layout(cx);
        pane.child(layout::splitter("split-detail", Key::Detail, false, 120., 640., true, cx)).child(div().h(px(l.detail)).max_h(relative(0.62)).flex_none().flex().child(self.detail.clone()))
    }

    fn render_log(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let mut log = div()
            .id("command-log")
            .relative()
            .flex_none()
            .h(px(180.))
            .overflow_y_scroll()
            .px(px(16.))
            .py(px(8.))
            .border_t_1()
            .border_color(p.border)
            .bg(p.bg)
            .font_family(MONO)
            .text_size(FS_SM)
            .line_height(relative(1.6));
        if self.logs.is_empty() {
            log = log.child(div().text_color(p.muted).child(t.log_empty));
        }
        for entry in &self.logs {
            // 失败的命令除了变红还多一条竖线，不只靠颜色区分
            let color = if entry.ok { p.text } else { p.red };
            log = log.child(
                div()
                    .mb(px(8.))
                    .pl(px(8.))
                    .border_l_2()
                    .border_color(if entry.ok { alpha(p.red, 0.) } else { p.red })
                    .child(div().text_color(color).child(format!("$ {}", entry.command)))
                    .when(!entry.output.is_empty(), |s| s.child(div().text_color(if entry.ok { p.muted } else { p.red }).child(entry.output.clone()))),
            );
        }
        div().relative().child(log).child(div().absolute().top(px(8.)).right(px(16.)).child(btn("close-log").small().quiet().icon_size(Icon::Close, px(14.)).tip(t.close).on_click(cx.listener(|this, _, _, cx| {
            this.show_log = false;
            cx.notify();
        }))))
    }
}

impl Render for RepoView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.started {
            self.activate(window, cx);
        }
        let l = layout(cx);
        let vw = f32::from(window.viewport_size().width);
        let content = match self.view {
            View::History => self.render_history(cx).into_any_element(),
            View::Changes => self.working.clone().into_any_element(),
            View::Worktrees => worktrees::render(self, cx).into_any_element(),
            View::Tools => self.tools.clone().map_or_else(|| div().into_any_element(), |t| t.into_any_element()),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.render_header(vw, cx))
            .children(self.render_banners(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().w(px(l.sidebar.min(vw * 0.36))).flex_none().flex().flex_col().child(crate::sidebar::render(self, window, cx)))
                    .child(layout::splitter("split-sidebar", Key::Sidebar, true, 180., 520., false, cx))
                    .child(div().flex().flex_1().min_w_0().child(content)),
            )
            .when(self.show_log, |s| s.child(self.render_log(cx)))
    }
}
