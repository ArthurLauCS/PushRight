//! 本地更改：左边未暂存、分组、已暂存、新提交四栏（可折叠、可拖动分隔线，没有组时分组栏不显示），右边是选中文件的 diff 或冲突。

use crate::conflict_view::{ConflictEvent, ConflictView};
use crate::dialog::{Field, Spec};
use crate::diff_view::{self, Apply, DiffPrefs, DiffView, Mode, Pager};
use crate::file_tree::{file_icon, file_list_bar, filter_input, folder_row, indent, tree_rows, TreeRow};
use crate::git::{self, Session};
use crate::i18n::{error_text, lookup, t};
use crate::icons::{icon, Icon};
use crate::layout::{self, layout, Key};
use crate::menu::Item;
use crate::repo_view::RepoView;
use crate::theme::{palette, FS_MD, FS_SM, LH_BODY, MONO, PROSE, ROW, R_MD};
use crate::ui::{btn, label};
use engine::groups::Group;
use engine::status::Entry;
use gpui::prelude::*;
use gpui::{canvas, div, px, relative, uniform_list, App, Context, Entity, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, SharedString, Subscription, Task, UniformListScrollHandle, WeakEntity, Window};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState, TextareaState};
use gpui_component::scroll::ScrollableElement;
use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

/// 左边四栏的序号，是 folded / sizes 的下标；前三栏是文件列表，也是 closed / scrolls 的下标
const UNSTAGED: usize = 0;
const GROUPS: usize = 1;
const STAGED: usize = 2;
const COMMIT: usize = 3;
const DEFAULT_SIZES: [f32; 4] = [120., 120., 120., 170.];

/// 文件列表里的一行
enum Row {
    /// 分组栏里一个组的标题：组的序号、组、组里的文件数
    Group(usize, Group, usize),
    Tree(TreeRow<Entry>),
}

/// 拖动中的文件行或文件夹行：放到组标题上移进这个组，放到未暂存标题上移出分组
#[derive(Clone)]
struct DraggedFiles {
    paths: Vec<String>,
    label: SharedString,
}

impl Render for DraggedFiles {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div().px(px(12.)).py(px(4.)).rounded(R_MD).bg(p.raised).border_1().border_color(p.border).text_color(p.text).text_size(FS_SM).font_family(MONO).shadow(crate::theme::overlay_shadow(p)).child(self.label.clone())
    }
}

pub struct WorkingCopy {
    session: Arc<Session>,
    repo: WeakEntity<RepoView>,
    sel: Option<(String, bool)>,
    diff: Entity<DiffView>,
    conflict: Entity<ConflictView>,
    conflicted: bool,
    message: Entity<TextareaState>,
    amend: bool,
    error: String,
    /// 上一次自动填进提交框的内容。提交框里还是它（或还空着）才会换，用户写过的不动
    filled: String,
    filter: Entity<InputState>,
    /// 折起来的文件夹；分组栏里还有折起来的组（键是 `fold_key`），组里文件夹的键前面带上组的键
    closed: [HashSet<String>; 3],
    scrolls: [UniformListScrollHandle; 3],
    folded: [bool; 4],
    sizes: [f32; 4],
    /// 各栏上一次布局时的实际高度，拖动起点用
    heights: Rc<RefCell<[f32; 4]>>,
    request: usize,
    pub busy: bool,
    _subs: Vec<Subscription>,
}

impl WorkingCopy {
    pub fn new(session: Arc<Session>, repo: WeakEntity<RepoView>, window: &mut Window, cx: &mut Context<Self>) -> WorkingCopy {
        let diff = cx.new(DiffView::new);
        let conflict = cx.new(|_| ConflictView::new());
        let placeholder = t(cx).commit_placeholder;
        let message = cx.new(|cx| TextareaState::new(window, cx).placeholder(placeholder));
        let filter = filter_input(window, cx);
        let subs = vec![
            cx.observe_global::<DiffPrefs>(|this, cx| this.load(cx)),
            cx.subscribe(&filter, |_, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&message, |_, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&conflict, window, |this, _, e: &ConflictEvent, window, cx| {
                let Some((path, _)) = this.sel.clone() else { return };
                match e {
                    ConflictEvent::Resolve(choices) => {
                        let choices = choices.clone();
                        this.run(move |s| engine::conflict::resolve(&s.repo, &path, &choices), cx).detach();
                    }
                    ConflictEvent::Take(theirs) => {
                        let theirs = *theirs;
                        this.run(move |s| engine::conflict::take(&s.repo, &path, theirs), cx).detach();
                    }
                    ConflictEvent::External => {
                        let _ = this.repo.update(cx, |r, cx| r.external_open("merge", "", String::new(), path, cx));
                    }
                }
                let _ = window;
            }),
        ];
        WorkingCopy {
            session,
            repo,
            sel: None,
            diff,
            conflict,
            conflicted: false,
            message,
            amend: false,
            error: String::new(),
            filled: String::new(),
            filter,
            closed: Default::default(),
            scrolls: [UniformListScrollHandle::new(), UniformListScrollHandle::new(), UniformListScrollHandle::new()],
            folded: [false; 4],
            sizes: DEFAULT_SIZES,
            heights: Rc::new(RefCell::new(DEFAULT_SIZES)),
            request: 0,
            busy: false,
            _subs: subs,
        }
    }

    /// 合并、摘取、撤销进行中时 Git 准备好的提交信息，否则是仓库设置里的模板
    pub fn prefill(&mut self, next: String, window: &mut Window, cx: &mut Context<Self>) {
        if next == self.filled {
            return;
        }
        let current = self.message.read(cx).value().to_string();
        if current == self.filled {
            let v = next.clone();
            self.message.update(cx, |m, cx| m.set_value(v, window, cx));
        }
        self.filled = next;
    }

    fn entries(&self, cx: &App) -> Vec<Entry> {
        self.repo.upgrade().map(|r| r.read(cx).entries.clone()).unwrap_or_default()
    }

    fn groups(&self, cx: &App) -> Vec<Group> {
        self.repo.upgrade().map(|r| r.read(cx).groups.clone()).unwrap_or_default()
    }

    /// 三个文件列表（未筛选）：未分组的未暂存文件、各组和组里的文件、已暂存文件
    fn lists(&self, cx: &App) -> (Vec<Entry>, Vec<(Group, Vec<Entry>)>, Vec<Entry>) {
        let Some(repo) = self.repo.upgrade() else { return Default::default() };
        let r = repo.read(cx);
        let grouped: HashSet<&str> = r.groups.iter().flat_map(|g| g.paths.iter().map(String::as_str)).collect();
        let unstaged = r.entries.iter().filter(|e| e.unstaged.is_some() && !grouped.contains(e.path.as_str())).cloned().collect();
        let groups = r.groups.iter().map(|g| (g.clone(), r.entries.iter().filter(|e| g.paths.contains(&e.path)).cloned().collect())).collect();
        let staged = r.entries.iter().filter(|e| e.staged.is_some()).cloned().collect();
        (unstaged, groups, staged)
    }

    /// 一个列表的行：按文件夹显示时是树，否则平铺
    fn rows(&self, items: Vec<Entry>, closed: &HashSet<String>, cx: &App) -> Vec<TreeRow<Entry>> {
        if diff_view::prefs(cx).tree {
            tree_rows(&items, |e| &e.path, closed)
        } else {
            items.into_iter().map(|e| TreeRow { key: e.path.clone(), depth: 0, name: String::new(), item: Some(e), items: Vec::new(), open: false }).collect()
        }
    }

    /// 选中的文件变了或它的状态变了：重新取 diff 或冲突内容
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let Some((path, staged)) = self.sel.clone() else {
            self.diff.update(cx, |d, cx| d.clear(cx));
            return;
        };
        let entries = self.entries(cx);
        let Some(entry) = entries.iter().find(|e| e.path == path && if staged { e.staged.is_some() } else { e.unstaged.is_some() }) else {
            self.sel = None;
            self.diff.update(cx, |d, cx| d.clear(cx));
            return cx.notify();
        };
        self.request += 1;
        let request = self.request;
        self.conflicted = entry.conflicted;
        let session = self.session.clone();
        if entry.conflicted {
            let rebasing = self.repo.upgrade().is_some_and(|r| r.read(cx).refs.in_progress == Some("rebase"));
            let external = self.repo.upgrade().is_some_and(|r| !r.read(cx).external_tools.merge.is_empty());
            git::bg(cx, move || engine::conflict::read(&session.repo, &path), move |this, r, cx| {
                if request != this.request {
                    return;
                }
                this.conflict.update(cx, |v, cx| {
                    v.rebasing = rebasing;
                    v.external = external;
                    v.set(r, cx)
                });
                cx.notify();
            })
            .detach();
            return;
        }
        let untracked = !staged && entry.unstaged == Some('?');
        let opts = diff_view::options(cx);
        let (s, p) = (session.clone(), path.clone());
        let mode_name = if staged { "staged" } else if untracked { "untracked" } else { "unstaged" };
        let pager: Pager = Rc::new(move |skip| {
            let (s, p) = (s.clone(), p.clone());
            Box::new(move || engine::diff::page_with(&s.repo, mode_name, "", "", &p, None, skip, opts))
        });
        let repo = self.repo.clone();
        let apply: Apply = {
            let (repo, path) = (repo.clone(), path.clone());
            Rc::new(move |hunk, header, lines, _, cx| {
                let path = path.clone();
                let Some(wc) = repo.upgrade().map(|r| r.read(cx).working.clone()) else { return };
                let context = diff_view::options(cx).context;
                wc.update(cx, |this, cx| this.run(move |s| engine::diff::apply_lines_with(&s.repo, &path, staged, hunk, &header, &lines, context), cx).detach());
            })
        };
        let discard: Apply = {
            let path = path.clone();
            Rc::new(move |hunk, header, lines, window, cx| {
                let _ = repo.update(cx, |r, cx| r.discard_lines(path.clone(), hunk, header, lines, window, cx));
            })
        };
        // 组里的文件不能按块暂存：暂存了就会离开组
        let grouped = self.groups(cx).iter().any(|g| g.paths.contains(&path));
        let p = path.clone();
        git::bg(cx, move || engine::diff::worktree_with(&session.repo, &p, staged, untracked, opts), move |this, r, cx| {
            if request != this.request {
                return;
            }
            match r {
                Ok(d) => this.diff.update(cx, |v, cx| v.set(Some(d), &path, if staged { Mode::Staged } else { Mode::Unstaged }, Some(pager), (!grouped).then_some(apply), (!staged).then_some(discard), cx)),
                Err(e) => this.error = e,
            }
            cx.notify();
        })
        .detach();
    }

    fn disabled(&self, cx: &App) -> bool {
        self.busy || self.repo.upgrade().is_some_and(|r| r.read(cx).busy)
    }

    /// 跑一个改动本地文件的操作，完了刷新整个仓库页
    pub fn run(&mut self, op: impl FnOnce(&Session) -> engine::Result<()> + Send + 'static, cx: &mut Context<Self>) -> Task<()> {
        if self.disabled(cx) {
            return Task::ready(());
        }
        self.set_busy(true, cx);
        self.error.clear();
        let session = self.session.clone();
        let work = cx.background_spawn(async move { op(&session) });
        cx.spawn(async move |this, cx| {
            let r = work.await;
            let Ok(refresh) = this.update(cx, |this, cx| {
                if let Err(e) = r {
                    this.error = e;
                }
                this.repo.update(cx, |r, cx| r.refresh(false, cx)).ok()
            }) else { return };
            if let Some(refresh) = refresh {
                refresh.await;
            }
            let _ = this.update(cx, |this, cx| {
                this.set_busy(false, cx);
                this.load(cx);
            });
        })
    }

    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        let _ = self.repo.update(cx, |r, cx| {
            r.working_busy = busy;
            cx.notify();
        });
        cx.notify();
    }

    fn stage(&mut self, paths: Vec<String>, staged: bool, cx: &mut Context<Self>) {
        self.run(move |s| if staged { engine::status::unstage(&s.repo, &paths) } else { engine::status::stage(&s.repo, &paths) }, cx).detach();
    }

    fn can_commit(&self, cx: &App) -> bool {
        let Some(repo) = self.repo.upgrade() else { return false };
        let r = repo.read(cx);
        !self.disabled(cx) && !r.checking && !self.message.read(cx).value().trim().is_empty() && (r.entries.iter().any(|e| e.staged.is_some()) || self.amend)
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_commit(cx) {
            return;
        }
        let message = self.message.read(cx).value().to_string();
        let amend = self.amend;
        let done = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            // 回到自动填入的内容（提交信息模板），没有就是空的
            let filled = this.filled.clone();
            this.message.update(cx, |m, cx| m.set_value(filled, window, cx));
            this.amend = false;
        };
        self.commit_with(move |s| engine::status::commit(&s.repo, &message, amend), done, window, cx);
    }

    /// 合并、变基、摘取进行中时 Git 不允许只提交一部分文件
    fn can_commit_group(&self, group: &Group, cx: &App) -> bool {
        let Some(repo) = self.repo.upgrade() else { return false };
        let r = repo.read(cx);
        !self.disabled(cx) && !r.checking && r.refs.in_progress.is_none() && !group.paths.is_empty()
    }

    fn commit_group(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.groups(cx).get(index).cloned().filter(|g| self.can_commit_group(g, cx)) else { return };
        self.commit_with(move |s| engine::groups::commit(&s.repo, &group), |_, _, _| {}, window, cx);
    }

    /// 提交前检查通过后跑提交；成功后调用 `done`，再记一条撤销
    fn commit_with(&mut self, op: impl FnOnce(&Session) -> engine::Result<()> + Send + 'static, done: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(check) = self.repo.update(cx, |r, cx| r.before_commit(window, cx)) else { return };
        let before = self.repo.upgrade().and_then(|r| r.read(cx).refs.head_id.clone());
        cx.spawn_in(window, async move |this, cx| {
            if !check.await {
                return;
            }
            let Ok(run) = this.update(cx, |this, cx| this.run(op, cx)) else { return };
            run.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.error.is_empty() {
                    return;
                }
                done(this, window, cx);
                let _ = this.repo.update(cx, |r, cx| r.committed(before, cx));
            });
        })
        .detach();
    }

    /// 把文件移到第 `target` 个组，None 是移出分组
    fn move_to(&mut self, paths: Vec<String>, target: Option<usize>, cx: &mut Context<Self>) {
        let mut groups = self.groups(cx);
        engine::groups::assign(&mut groups, &paths, target);
        self.save_groups(groups, paths, cx);
    }

    /// 新建组，把 `paths` 移进去（可以没有）
    fn new_group(&mut self, paths: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_name(t(cx).groups.new_group, String::new(), window, cx, move |this, name, cx| {
            let mut groups = this.groups(cx);
            groups.push(Group { name, paths: Vec::new() });
            let last = groups.len() - 1;
            engine::groups::assign(&mut groups, &paths, Some(last));
            this.save_groups(groups, paths, cx);
        });
    }

    fn rename_group(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(old) = self.groups(cx).get(index).map(|g| g.name.clone()) else { return };
        self.ask_name(t(cx).groups.rename, old, window, cx, move |this, name, cx| {
            let mut groups = this.groups(cx);
            if let Some(g) = groups.get_mut(index) {
                g.name = name;
                this.save_groups(groups, Vec::new(), cx);
            }
        });
    }

    /// 删掉组，组里的文件回到未暂存
    fn delete_group(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut groups = self.groups(cx);
        if index < groups.len() {
            groups.remove(index);
            self.save_groups(groups, Vec::new(), cx);
        }
    }

    /// 组名对话框：组名就是提交说明，可以多行；确认后用去掉首尾空白的组名调用 `f`
    fn ask_name(&mut self, title: &str, value: String, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static) {
        let t = t(cx);
        let spec = Spec::new(title).message(t.groups.name_hint).fields([Field::textarea("name", t.groups.name).value(value)]);
        let Ok(rx) = self.repo.update(cx, |r, cx| r.ask(spec, window, cx)) else { return };
        cx.spawn(async move |this, cx| {
            let Ok(Some(v)) = rx.await else { return };
            let name = v.text("name").trim().to_owned();
            if !name.is_empty() {
                let _ = this.update(cx, |this, cx| f(this, name, cx));
            }
        })
        .detach();
    }

    /// 存分组；`moved` 里已暂存的文件先取消暂存：组里和未暂存栏里的文件都只有未暂存的改动
    fn save_groups(&mut self, groups: Vec<Group>, moved: Vec<String>, cx: &mut Context<Self>) {
        let entries = self.entries(cx);
        let staged: Vec<String> = moved.into_iter().filter(|p| entries.iter().any(|e| e.path == *p && e.staged.is_some())).collect();
        self.run(
            move |s| {
                if !staged.is_empty() {
                    engine::status::unstage(&s.repo, &staged)?;
                }
                engine::groups::save(&s.repo, &groups)
            },
            cx,
        )
        .detach();
    }

    /// 「移到组」子菜单：已有的组（文件都已在里面的那个不可选）、新建组、移出分组
    fn move_menu(&self, paths: Vec<String>, cx: &mut Context<Self>) -> Item {
        let t = t(cx);
        let groups = self.groups(cx);
        let me = cx.entity().downgrade();
        let act = |target: Option<usize>| {
            let (me, paths) = (me.clone(), paths.clone());
            move |_: &mut Window, cx: &mut App| {
                let _ = me.update(cx, |this, cx| this.move_to(paths.clone(), target, cx));
            }
        };
        let mut items: Vec<Item> = groups.iter().enumerate().map(|(i, g)| Item::new(title(&g.name), act(Some(i))).disabled(paths.iter().all(|p| g.paths.contains(p)))).collect();
        if !items.is_empty() {
            items.push(Item::Sep);
        }
        let (me2, p2) = (me.clone(), paths.clone());
        items.push(Item::new(t.groups.new_group, move |window, cx| {
            let _ = me2.update(cx, |this, cx| this.new_group(p2.clone(), window, cx));
        }));
        let grouped = paths.iter().any(|p| groups.iter().any(|g| g.paths.contains(p)));
        items.push(Item::new(t.groups.ungroup, act(None)).disabled(!grouped));
        Item::group(t.groups.move_to, items)
    }

    /// 组的更多操作：重命名、删除
    fn group_menu(&mut self, index: usize, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let (me, me2) = (cx.entity().downgrade(), cx.entity().downgrade());
        let items = vec![
            Item::new(t.groups.rename, move |window, cx| {
                let _ = me.update(cx, |this, cx| this.rename_group(index, window, cx));
            }),
            Item::new(t.groups.delete, move |_, cx| {
                let _ = me2.update(cx, |this, cx| this.delete_group(index, cx));
            })
            .hint(t.groups.delete_hint),
        ];
        self.show_menu(at, items, window, cx);
    }

    fn show_menu(&self, at: Point<Pixels>, items: Vec<Item>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.repo.upgrade().map(|r| r.read(cx).menu.clone()) else { return };
        menu.update(cx, |m, cx| m.show(at, items, window, cx));
    }

    fn toggle_amend(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.amend = on;
        cx.notify();
        if !on || !self.message.read(cx).value().trim().is_empty() {
            return;
        }
        let session = self.session.clone();
        let task = cx.background_spawn(async move { engine::status::last_message(&session.repo).unwrap_or_default() });
        cx.spawn_in(window, async move |this, cx| {
            let last = task.await;
            let _ = this.update_in(cx, |this, window, cx| this.message.update(cx, |m, cx| m.set_value(last, window, cx)));
        })
        .detach();
    }

    /// 最近的提交信息，点开时才取，选一条填进提交框
    fn recent(&mut self, at: gpui::Point<gpui::Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let task = cx.background_spawn(async move { engine::workflow::recent_messages(&session.repo, 20).unwrap_or_default() });
        cx.spawn_in(window, async move |this, cx| {
            let list = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let me = cx.entity().downgrade();
                let items = list
                    .into_iter()
                    .map(|m| {
                        let first: String = m.lines().next().unwrap_or("").chars().take(80).collect();
                        let me = me.clone();
                        Item::new(first, move |window, cx| {
                            let m = m.clone();
                            let _ = me.update(cx, |this, cx| this.message.update(cx, |s, cx| s.set_value(m, window, cx)));
                        })
                    })
                    .collect();
                this.show_menu(at, items, window, cx);
            });
        })
        .detach();
    }

    /// 这一栏展开着：没折起来；分组栏还要有组才显示
    fn open(&self, index: usize, cx: &App) -> bool {
        !self.folded[index] && (index != GROUPS || self.repo.upgrade().is_some_and(|r| !r.read(cx).groups.is_empty()))
    }

    fn next_open(&self, index: usize, cx: &App) -> Option<usize> {
        (index + 1..4).find(|&i| self.open(i, cx))
    }

    /// 拖动两栏之间的分隔线：只在相邻的两个展开栏之间分配
    /// 从当前实际高度出发，把第 index 栏加高 delta、下一个展开的栏相应变矮；拖动和方向键共用
    fn resizer(&self, index: usize, cx: &mut Context<Self>) -> Option<impl Fn(f32, &mut App) + 'static> {
        let next = self.next_open(index, cx)?;
        // 起点按实际高度算（扣掉各栏的 80px 底），窗口缩放后也不跳动
        let heights = *self.heights.borrow();
        let drag: Vec<f32> = (0..4).map(|i| if self.open(i, cx) { (heights[i] - 80.).max(1.) } else { self.sizes[i] }).collect();
        let me = cx.entity().downgrade();
        Some(move |delta: f32, cx: &mut App| {
            let total = drag[index] + drag[next];
            let first = (drag[index] + delta).clamp(1., total - 1.);
            let _ = me.update(cx, |this, cx| {
                for i in 0..4 {
                    this.sizes[i] = if i == index { first } else if i == next { total - first } else { drag[i] };
                }
                cx.notify();
            });
        })
    }

    fn start_resize(&mut self, index: usize, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if let Some(apply) = self.resizer(index, cx) {
            layout::start(at, false, apply, cx);
        }
    }

    fn render_item(&mut self, e: &Entry, depth: Option<usize>, staged: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let s = if staged { e.staged } else { e.unstaged }.unwrap_or(' ');
        let color = match s {
            'A' | '?' => p.diff_added,
            'D' => p.diff_deleted,
            'U' => p.red,
            'R' | 'C' => p.accent,
            _ => p.orange,
        };
        let selected = self.sel.as_ref().is_some_and(|(path, st)| *path == e.path && *st == staged);
        let disabled = self.disabled(cx);
        let path = e.path.clone();
        let group: SharedString = format!("wc-{}-{}", staged as u8, e.path).into();
        let base = e.path.trim_end_matches('/').rsplit('/').next().unwrap_or(&e.path).to_owned();
        let dir = e.path.trim_end_matches('/').rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
        // 行内操作按钮只在悬停或选中时出现
        let reveal = move |b: crate::ui::Btn, group: SharedString| div().when(!selected, |s| s.invisible().group_hover(group, |s| s.visible())).child(b);
        let entry = e.clone();
        let (p1, p2, p3, p4) = (path.clone(), path.clone(), path.clone(), path.clone());
        let status_tip = lookup(t.status, &s.to_string()).unwrap_or_default();
        let mut row = div()
            .id(group.clone())
            .group(group.clone())
            .flex()
            .items_center()
            .gap(px(8.))
            .w_full()
            .h(ROW)
            .pl(depth.map_or(px(12.), indent))
            .pr(px(8.))
            .when(selected, |s| s.bg(p.selection).shadow(crate::theme::select_bar(&p)))
            .when(!selected, |s| s.hover(|s| s.bg(p.hover)))
            .tooltip({
                let tip: SharedString = path.clone().into();
                move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _, cx| {
                if ev.click_count() >= 2 {
                    return this.stage(vec![p1.clone()], staged, cx);
                }
                this.sel = Some((p1.clone(), staged));
                this.load(cx);
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                this.sel = Some((p2.clone(), staged));
                this.load(cx);
                let (at, entry) = (ev.position, entry.clone());
                let move_to = (!entry.conflicted).then(|| this.move_menu(vec![entry.path.clone()], cx));
                let _ = this.repo.update(cx, |r, cx| r.work_file_menu(at, &entry, staged, move_to, window, cx));
            }))
            .when(!e.conflicted, |s| s.on_drag(DraggedFiles { paths: vec![path.clone()], label: base.clone().into() }, |d, _, _, cx| cx.new(|_| d.clone())))
            .child(div().id("status").flex_none().w(px(16.)).flex().justify_center().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_size(FS_SM).text_color(color).tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(status_tip).build(window, cx)).child(s.to_string()))
            .child(file_icon(&e.path, px(14.), cx))
            .child(div().flex().flex_1().min_w_0().items_baseline().truncate().font_family(MONO).child(if base.is_empty() { e.path.clone() } else { base }).when(depth.is_none() && !dir.is_empty(), |s| s.child(div().ml(px(8.)).text_size(FS_SM).text_color(p.muted).child(dir))));
        if !staged && !e.conflicted {
            row = row.child(reveal(btn(SharedString::from(format!("discard-{path}"))).small().quiet().danger().icon_size(Icon::Undo, px(14.)).disabled(disabled).tip(t.discard_file).on_click(cx.listener(move |this, _, window, cx| {
                let _ = this.repo.update(cx, |r, cx| r.discard(vec![p3.clone()], window, cx));
            })), group.clone()));
        }
        row.child(reveal(
            btn(SharedString::from(format!("stage-{path}")))
                .small()
                .quiet()
                .icon_size(if staged { Icon::Minus } else { Icon::Plus }, px(14.))
                .disabled(disabled)
                .tip(if staged { t.unstage_file } else { t.stage_file })
                .on_click(cx.listener(move |this, _, _, cx| this.stage(vec![p4.clone()], staged, cx))),
            group,
        ))
    }

    /// 一个文件列表栏。`all`：整栏文件的路径，「全部暂存 / 取消」不受筛选影响；`rows`：筛选后的行，栏折起来时为空
    fn render_group(&mut self, index: usize, title: &str, hint: &str, count: String, all: Vec<String>, rows: Vec<Row>, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let staged = index == STAGED;
        let folded = self.folded[index];
        let disabled = self.disabled(cx);
        let total = all.len();
        let ungroup = cx.listener(|this, d: &DraggedFiles, _, cx| this.move_to(d.paths.clone(), None, cx));
        let mut title_row = div()
            .when(index == UNSTAGED, |s| s.drag_over::<DraggedFiles>(move |s, _, _, _| s.bg(p.selection)).on_drop(ungroup))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .min_h(px(36.))
            .pl(px(12.))
            .pr(px(8.))
            .text_size(FS_SM)
            .text_color(p.muted)
            .child(
                div()
                    .id(("section", index))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .self_stretch()
                    .cursor_pointer()
                    .text_color(p.text)
                    .font_weight(FontWeight::BOLD)
                    .tooltip({
                        let tip: SharedString = format!("{title} · {hint}").into();
                        move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.folded[index] = !this.folded[index];
                        cx.notify();
                    }))
                    .child(icon(if folded { Icon::Right } else { Icon::Down }, px(12.), p.text))
                    .child(div().min_w_0().flex().child(div().flex_none().child(label(title, false))).child(div().flex_none().ml(px(8.)).font_weight(FontWeight::NORMAL).text_color(p.muted).child(count)).child(div().min_w_0().truncate().ml(px(8.)).font_weight(FontWeight::NORMAL).text_color(p.muted).child(hint.to_owned()))),
            );
        if index == UNSTAGED {
            title_row = title_row.child(btn("discard-all").small().quiet().danger().text(t.discard_all).no_cmd().disabled(disabled || total == 0).tip(t.discard_all).on_click(cx.listener(|this, _, window, cx| {
                let _ = this.repo.update(cx, |r, cx| r.discard(Vec::new(), window, cx));
            })));
        }
        title_row = if index == GROUPS {
            title_row.child(btn("new-group").small().text(t.groups.new_group).no_cmd().disabled(disabled).tip(t.groups.new_group).on_click(cx.listener(|this, _, window, cx| this.new_group(Vec::new(), window, cx))))
        } else {
            let all2 = all.clone();
            title_row.child(
                btn(("stage-all", index))
                    .small()
                    .text(if staged { t.unstage_all } else { t.stage_all })
                    .no_cmd()
                    .disabled(disabled || total == 0)
                    .tip(if staged { t.unstage_all } else { t.stage_all })
                    .on_click(cx.listener(move |this, _, _, cx| this.stage(all2.clone(), staged, cx))),
            )
        };
        // 只渲染看得见的行：几百个改动文件时每一帧不再把整张列表排一遍
        let tree = diff_view::prefs(cx).tree;
        let rows = Rc::new(rows);
        let list = uniform_list(
            ("items", index),
            rows.len(),
            cx.processor(move |this: &mut WorkingCopy, range: Range<usize>, _window: &mut Window, cx: &mut Context<WorkingCopy>| {
                range
                    .map(|ix| match &rows[ix] {
                        Row::Group(i, g, count) => this.render_group_row(*i, g, *count, cx),
                        Row::Tree(r) => match &r.item {
                            Some(e) => this.render_item(e, tree.then_some(r.depth), staged, cx).into_any_element(),
                            None => this.render_folder(index, r, cx),
                        },
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scrolls[index])
        .size_full();
        // 整个工作区都干净时在未暂存的列表里说明
        let clean = index == UNSTAGED && self.repo.upgrade().is_some_and(|r| r.read(cx).entries.is_empty());
        let list = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(list)
            .vertical_scrollbar(&self.scrolls[index])
            .when(clean, |s| s.child(div().absolute().top_0().left_0().right_0().p(px(16.)).text_color(p.muted).font_family(PROSE).child(t.clean)));
        self.section(index, title_row, list.into_any_element(), cx)
    }

    /// 分组栏里一个组的标题行：点击折叠，「提交」只提交这个组；把文件拖到这里就移进这个组
    fn render_group_row(&mut self, index: usize, group: &Group, count: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let key = fold_key(&group.name);
        let open = !self.closed[GROUPS].contains(&key);
        let blocked = self.repo.upgrade().is_some_and(|r| r.read(cx).refs.in_progress.is_some());
        let can = self.can_commit_group(group, cx);
        let tip: SharedString = group.name.clone().into();
        div()
            .id(("group", index))
            .flex()
            .items_center()
            .gap(px(6.))
            .w_full()
            .h(ROW)
            .pl(px(12.))
            .pr(px(8.))
            .cursor_pointer()
            .hover(|s| s.bg(p.hover))
            .drag_over::<DraggedFiles>(move |s, _, _, _| s.bg(p.selection))
            .on_drop(cx.listener(move |this, d: &DraggedFiles, _, cx| this.move_to(d.paths.clone(), Some(index), cx)))
            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.closed[GROUPS].remove(&key) {
                    this.closed[GROUPS].insert(key.clone());
                }
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, window, cx| this.group_menu(index, ev.position, window, cx)))
            .child(icon(if open { Icon::Down } else { Icon::Right }, px(12.), p.text))
            .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).child(title(&group.name).to_owned()))
            .child(div().flex_none().text_size(FS_SM).text_color(p.muted).child(if count == 0 { t.groups.empty.to_owned() } else { count.to_string() }))
            .child(btn(("commit-group", index)).small().text(t.groups.commit).no_cmd().disabled(!can).tip(if blocked { t.groups.commit_blocked } else { t.groups.commit_tip }).on_click(cx.listener(move |this, _, window, cx| this.commit_group(index, window, cx))))
            .child(btn(("group-menu", index)).small().quiet().icon_size(Icon::More, px(14.)).tip(t.groups.more).on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| this.group_menu(index, ev.position(), window, cx))))
            .into_any_element()
    }

    fn render_folder(&mut self, index: usize, row: &TreeRow<Entry>, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = t(cx);
        let staged = index == STAGED;
        let disabled = self.disabled(cx);
        let key = row.key.clone();
        let paths: Vec<String> = row.items.iter().map(|e| e.path.clone()).collect();
        // 空列表在丢弃里表示「全部」，文件夹里没有可丢弃的文件时必须禁用
        let discardable: Vec<String> = row.items.iter().filter(|e| !e.conflicted).map(|e| e.path.clone()).collect();
        // 冲突文件不能分组
        let movable = discardable.clone();
        let group: SharedString = format!("folder-{index}-{}", row.key).into();
        let show = move |b: crate::ui::Btn, g: SharedString| div().invisible().group_hover(g, |s| s.visible()).child(b);
        let dragged = DraggedFiles { paths: movable.clone(), label: row.name.clone().into() };
        let menu = cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
            let item = this.move_menu(movable.clone(), cx);
            this.show_menu(ev.position, vec![item], window, cx);
        });
        let mut f = folder_row(group.clone(), &row.key, &row.name, row.items.len(), row.open, row.depth, false, cx)
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.closed[index].remove(&key) {
                    this.closed[index].insert(key.clone());
                }
                cx.notify();
            }))
            .when(!dragged.paths.is_empty(), |s| s.on_mouse_down(MouseButton::Right, menu).on_drag(dragged, |d, _, _, cx| cx.new(|_| d.clone())));
        if !staged {
            f = f.child(show(btn(SharedString::from(format!("discard-dir-{}", row.key))).small().quiet().danger().icon_size(Icon::Undo, px(14.)).disabled(disabled || discardable.is_empty()).tip(t.file_list.discard_folder).on_click(cx.listener(move |this, _, window, cx| {
                let d = discardable.clone();
                let _ = this.repo.update(cx, |r, cx| r.discard(d, window, cx));
            })), group.clone()));
        }
        f.child(show(btn(SharedString::from(format!("stage-dir-{}", row.key))).small().quiet().icon_size(if staged { Icon::Minus } else { Icon::Plus }, px(14.)).disabled(disabled).tip(if staged { t.file_list.unstage_folder } else { t.file_list.stage_folder }).on_click(cx.listener(move |this, _, _, cx| this.stage(paths.clone(), staged, cx))), group))
            .into_any_element()
    }

    /// 一栏：标题加内容。展开时按比例分高度（至少 80px），折起来只剩标题
    fn section(&mut self, index: usize, title: impl IntoElement, body: gpui::AnyElement, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = palette(cx);
        let folded = self.folded[index];
        let heights = self.heights.clone();
        let measure = canvas(move |b, _, _| heights.borrow_mut()[index] = f32::from(b.size.height), |_, _, _, _| {}).absolute().size_full();
        div()
            .relative()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(p.border)
            .when(folded, |s| s.flex_none().h(px(37.)))
            .when(!folded, |s| s.flex_grow(self.sizes[index]).flex_basis(px(80.)).min_h(px(80.)))
            .child(measure)
            .child(title)
            .when(!folded, |s| s.child(body))
            .into_any_element()
    }

    fn divider(&mut self, index: usize, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.open(index, cx) || self.next_open(index, cx).is_none() {
            return None;
        }
        let me = cx.entity().downgrade();
        let (me2, me3) = (me.clone(), me.clone());
        Some(
            layout::bar(
                ("wc-divider", index),
                false,
                move |at, cx| {
                    let _ = me.update(cx, |this, cx| this.start_resize(index, at, cx));
                },
                move |cx| {
                    let _ = me2.update(cx, |this, cx| {
                        this.sizes = DEFAULT_SIZES;
                        cx.notify();
                    });
                },
                move |delta, cx| {
                    // 先取出调整函数再调用：它自己会更新这个视图，不能嵌在 update 里
                    if let Some(apply) = me3.update(cx, |this, cx| this.resizer(index, cx)).ok().flatten() {
                        apply(delta, cx);
                    }
                },
                cx,
            )
            .into_any_element(),
        )
    }
}

impl Render for WorkingCopy {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let Some(repo) = self.repo.upgrade() else { return div().into_any_element() };
        let (identity, conflicts, prefill, check_cmd, check_auto, checking, check_result, repo_busy) = {
            let r = repo.read(cx);
            let prefill = match (r.refs.in_progress, r.operation.as_ref()) {
                (Some(what), Some(op)) if what != "rebase" && !op.message.is_empty() => op.message.clone(),
                _ => r.workflow.template.clone(),
            };
            (r.identity.clone(), r.conflicts(), prefill, r.workflow.check_command.clone(), r.workflow.check_before_commit, r.checking, r.check_result.clone(), r.busy)
        };
        self.prefill(prefill, window, cx);
        let query = self.filter.read(cx).value().trim().to_lowercase();
        let shown = |list: Vec<Entry>| -> Vec<Entry> { list.into_iter().filter(|e| query.is_empty() || e.path.to_lowercase().contains(&query)).collect() };
        let count = |shown: usize, total: usize| if query.is_empty() { total.to_string() } else { (t.file_list.shown)(shown, total) };
        let (unstaged_all, groups, staged_all) = self.lists(cx);
        let l = layout(cx);

        let mut left = div().id("working-left").relative().flex().flex_col().flex_none().w(px(l.changes)).max_w(relative(0.5)).min_h_0().bg(p.surface);
        if !conflicts.is_empty() {
            let resolved = conflicts.iter().filter(|c| c.1).count();
            let mut box_ = div()
                .id("conflict-list")
                .flex_none()
                .max_h(relative(0.3))
                .overflow_y_scroll()
                .border_b_1()
                .border_color(p.border)
                .child(div().flex().items_center().min_h(px(36.)).pl(px(12.)).pr(px(8.)).text_size(FS_SM).child(div().font_weight(FontWeight::BOLD).child(t.conflict_files)).child(div().ml(px(8.)).text_color(p.muted).child((t.conflict_progress)(resolved, conflicts.len()))));
            for (path, ok) in conflicts {
                let selected = self.sel.as_ref().is_some_and(|(p0, _)| *p0 == path);
                let base = path.rsplit('/').next().unwrap_or(&path).to_owned();
                let dir = path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
                let p1 = path.clone();
                box_ = box_.child(
                    div()
                        .id(SharedString::from(format!("conflict-{path}")))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(ROW)
                        .pl(px(12.))
                        .pr(px(8.))
                        .when(selected, |s| s.bg(p.selection))
                        .when(!selected, |s| s.hover(|s| s.bg(p.hover)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.sel = Some((p1.clone(), ok));
                            this.load(cx);
                            cx.notify();
                        }))
                        .child(div().id("mark").flex_none().w(px(16.)).flex().justify_center().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_size(FS_SM).text_color(if ok { p.diff_added } else { p.red }).tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(if ok { t.conflict_resolved } else { t.conflict_open }).build(window, cx)).child(if ok { "✓" } else { "!" }))
                        .child(file_icon(&path, px(14.), cx))
                        .child(div().flex().flex_1().min_w_0().truncate().child(base).child(div().ml(px(8.)).text_size(FS_SM).text_color(p.muted).child(dir))),
                );
            }
            left = left.child(box_);
        }
        let me = cx.entity().downgrade();
        let fold = move |open: bool, cx: &mut App| {
            let _ = me.update(cx, |this, cx| {
                let (unstaged, groups, staged) = this.lists(cx);
                // 全部折叠时分组栏里的组也折起来
                this.closed = if open {
                    Default::default()
                } else {
                    let grouped = groups.iter().flat_map(|(g, items)| {
                        let key = fold_key(&g.name);
                        folder_keys(items).into_iter().map(|k| format!("{key}{k}")).chain([key.clone()]).collect::<Vec<_>>()
                    });
                    [folder_keys(&unstaged).into_iter().collect(), grouped.collect(), folder_keys(&staged).into_iter().collect()]
                };
                cx.notify();
            });
        };
        left = left.child(file_list_bar(&self.filter, fold, cx));
        let paths = |list: &[Entry]| list.iter().map(|e| e.path.clone()).collect::<Vec<_>>();
        let (all0, all2) = (paths(&unstaged_all), paths(&staged_all));
        let (shown0, shown2) = (shown(unstaged_all), shown(staged_all));
        let (count0, count2) = (count(shown0.len(), all0.len()), count(shown2.len(), all2.len()));
        let rows0 = if self.folded[UNSTAGED] { Vec::new() } else { self.rows(shown0, &self.closed[UNSTAGED], cx).into_iter().map(Row::Tree).collect() };
        let rows2 = if self.folded[STAGED] { Vec::new() } else { self.rows(shown2, &self.closed[STAGED], cx).into_iter().map(Row::Tree).collect() };
        let g0 = self.render_group(UNSTAGED, t.unstaged, t.unstaged_hint, count0, all0, rows0, cx);
        let g1 = (!groups.is_empty()).then(|| {
            let files = groups.iter().map(|(_, items)| items.len()).sum();
            let mut rows = Vec::new();
            let shown_groups = if self.folded[GROUPS] { &groups[..0] } else { &groups[..] };
            for (i, (g, items)) in shown_groups.iter().enumerate() {
                rows.push(Row::Group(i, g.clone(), items.len()));
                let key = fold_key(&g.name);
                if self.closed[GROUPS].contains(&key) {
                    continue;
                }
                let closed: HashSet<String> = self.closed[GROUPS].iter().filter_map(|k| k.strip_prefix(&key)).map(str::to_owned).collect();
                for mut r in self.rows(shown(items.clone()), &closed, cx) {
                    // 树缩进到组标题下一层；文件夹的折叠键带上组的键，各组分开记
                    r.depth += 1;
                    if r.item.is_none() {
                        r.key = format!("{key}{}", r.key);
                    }
                    rows.push(Row::Tree(r));
                }
            }
            self.render_group(GROUPS, t.groups.title, t.groups.hint, (t.groups.count)(groups.len(), files), Vec::new(), rows, cx)
        });
        let g2 = self.render_group(STAGED, t.staged, t.staged_hint, count2, all2, rows2, cx);
        left = left.child(g0).children(self.divider(UNSTAGED, cx)).children(g1).children(self.divider(GROUPS, cx)).child(g2).children(self.divider(STAGED, cx));

        // 新提交：作者、最近的提交信息、提交框、提交前检查、修改上一个提交
        let amend = self.amend;
        let ident = identity.as_ref().and_then(|i| if amend { i.committer.clone() } else { i.author.clone() }).unwrap_or_else(|| t.identity_missing.into());
        let disabled = self.disabled(cx) || repo_busy;
        // 左右边距和上面的筛选框一样，输入框、按钮的边对齐
        let mut commit = div().id("commit-box").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().gap(px(8.)).px(px(8.)).py(px(12.));
        if !self.error.is_empty() {
            commit = commit.child(div().id("wc-error").flex_none().max_h(px(96.)).overflow_y_scroll().text_size(FS_SM).text_color(p.red).child(error_text(cx, &self.error)));
        }
        commit = commit.child(
            div()
                .flex()
                .flex_none()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().min_w_0().truncate().text_size(FS_SM).font_weight(FontWeight::NORMAL).child(ident))
                .child(btn("edit-identity").small().text(t.edit_identity).disabled(disabled).tip(t.edit_identity).on_click(cx.listener(|this, _, window, cx| {
                    let _ = this.repo.update(cx, |r, cx| r.edit_identity(window, cx));
                }))),
        );
        if amend {
            commit = commit.child(div().flex_none().text_size(FS_SM).text_color(p.muted).child(t.amend_identity_hint));
        } else if let Some(i) = identity.as_ref().filter(|i| i.committer.is_some() && i.committer != i.author) {
            commit = commit.child(div().flex_none().text_size(FS_SM).text_color(p.muted).child(format!("{}：{}", t.committer, i.committer.clone().unwrap_or_default())));
        }
        commit = commit
            .child(
                div()
                    .id("recent-messages")
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(24.))
                    .px(px(8.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(crate::theme::R_MD)
                    .bg(p.bg)
                    .text_size(FS_SM)
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(|s| s.border_color(p.border))
                    .child(div().flex_1().child(t.recent_messages))
                    .child(icon(Icon::Down, px(12.), p.muted))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.recent(e.position, window, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(96.))
                    // 捕获阶段：输入框自己会把回车当换行吃掉
                    .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                        if e.keystroke.modifiers.control && e.keystroke.key == "enter" {
                            cx.stop_propagation();
                            this.commit(window, cx);
                        }
                    }))
                    .child(gpui_component::input::Textarea::new(&self.message).h_full().font_family(PROSE).text_size(FS_MD).line_height(relative(LH_BODY))),
            );
        if !check_cmd.is_empty() {
            let mut row = div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .text_size(FS_SM)
                .child(btn("run-check").small().text(if checking { t.working } else { t.check.run }).disabled(checking || disabled).tip(format!("{} · {}", t.check.run, check_cmd)).on_click(cx.listener(|this, _, _, cx| {
                    let _ = this.repo.update(cx, |r, cx| r.run_check(cx).detach());
                })))
                .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_color(p.muted).child(check_cmd.clone()));
            row = match check_result {
                Some(result) => row.child(
                    btn("check-result")
                        .small()
                        .quiet()
                        .text(if result.ok { t.check.passed } else { t.check.failed })
                        .tip(t.check.output)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = this.repo.update(cx, |r, cx| r.show_log_panel(cx));
                        })),
                ),
                None if check_auto => row.child(div().text_color(p.muted).whitespace_nowrap().child(t.check.auto_on)),
                None => row,
            };
            commit = commit.child(row);
        }
        let can = self.can_commit(cx);
        commit = commit.child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(div().min_w_0().text_size(FS_SM).text_color(p.muted).child(Checkbox::new("amend").checked(amend).label(t.amend).on_click(cx.listener(|this, v: &bool, window, cx| this.toggle_amend(*v, window, cx)))))
                .child(btn("commit").primary().text(t.commit).disabled(!can).tip(t.commit).on_click(cx.listener(|this, _, window, cx| this.commit(window, cx)))),
        );
        let title = div()
            .flex()
            .flex_none()
            .items_center()
            .min_h(px(36.))
            .pl(px(12.))
            .pr(px(8.))
            .child(
                div()
                    .id("section-commit")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(FS_SM)
                    .font_weight(FontWeight::BOLD)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.folded[COMMIT] = !this.folded[COMMIT];
                        cx.notify();
                    }))
                    .child(icon(if self.folded[COMMIT] { Icon::Right } else { Icon::Down }, px(12.), p.text))
                    .child(if amend { t.committer } else { t.commit_identity }),
            );
        let g3 = self.section(COMMIT, title, commit.into_any_element(), cx);
        left = left.child(g3);

        let right = if self.sel.is_some() && self.conflicted { self.conflict.clone().into_any_element() } else { self.diff.clone().into_any_element() };
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(left)
            .child(layout::splitter("split-changes", Key::Changes, true, 260., 720., false, cx))
            .child(div().flex().flex_1().min_w_0().child(right))
            .into_any_element()
    }
}

/// 组在 `closed` 里的键；组里文件夹的键是它加上文件夹路径
fn fold_key(name: &str) -> String {
    format!("{name}\0")
}

/// 组标题：组名的第一行
fn title(name: &str) -> &str {
    name.lines().next().unwrap_or_default()
}

/// 列表按文件夹显示时所有文件夹的折叠键
fn folder_keys(list: &[Entry]) -> Vec<String> {
    tree_rows(list, |e| &e.path, &HashSet::new()).into_iter().filter(|r| r.item.is_none()).map(|r| r.key).collect()
}

#[allow(dead_code)]
fn _unused(_: &InputState, _: &Input) {}
