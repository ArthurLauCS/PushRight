//! 仓库工具：历史类（版本比较、交互式变基、Reflog 恢复、二分查找）和仓库类（远程、子模块、LFS、应用补丁、统计）。

use crate::detail::{DetailView, Source};
use crate::dialog::{Field, Spec};
use crate::git::{self, Session};
use crate::i18n::{error_text, lookup, t};
use crate::icons::Icon;
use crate::repo_view::RepoView;
use crate::revision_picker::RevisionPicker;
use crate::theme::{palette, DISPLAY, FS_LG, FS_MD, FS_SM, LH_BODY, MONO, PROSE, R_MD};
use crate::time::{calendar, Zone};
use crate::ui::{btn, Opt};
use engine::ops::{Log, Op, Verdict};
use engine::rebase::Plan;
use engine::tools::{Comparison, Lfs, Reflog, Remote, Stats, Submodule};
use gpui::prelude::*;
use gpui::{div, px, relative, AnyElement, App, Context, Entity, FontWeight, SharedString, Subscription, Task, WeakEntity, Window};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::{IndexPath, Sizable};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Compare,
    Rebase,
    Reflog,
    Bisect,
    Remotes,
    Submodules,
    Lfs,
    Patch,
    Stats,
}

const ACTIONS: [&str; 6] = ["pick", "reword", "edit", "squash", "fixup", "drop"];

struct PlanStep {
    action: Entity<SelectState<Vec<Opt>>>,
    message: Entity<TextareaState>,
}

pub struct Tools {
    session: Arc<Session>,
    repo: WeakEntity<RepoView>,
    section: Section,
    error: String,
    loading: bool,
    left: Entity<RevisionPicker>,
    right: Entity<RevisionPicker>,
    common_base: bool,
    comparison: Option<Comparison>,
    compare_detail: Entity<DetailView>,
    remotes: Vec<Remote>,
    logs: Vec<Reflog>,
    more_logs: bool,
    base: Entity<RevisionPicker>,
    plan: Option<Plan>,
    steps: Vec<PlanStep>,
    /// 指向被改写提交的其他本地分支要不要跟着挪
    update_refs: bool,
    /// 展开查看内容的那一步
    preview: Option<String>,
    preview_detail: Entity<DetailView>,
    /// 刚完成的变基留下的备份分支
    backup: String,
    // 下面是仓库类工具
    /// 最近一次操作的输出：二分查找每一步的结论、补丁失败的原因都在里面
    last: Option<Log>,
    bisecting: Option<String>,
    bad: Entity<RevisionPicker>,
    good: Entity<RevisionPicker>,
    modules: Vec<Submodule>,
    lfs: Option<Lfs>,
    stats: Option<Stats>,
    /// None 表示还没查过
    locks: Option<String>,
    pattern: Entity<InputState>,
    lock_path: Entity<InputState>,
    patch: Entity<TextareaState>,
    index: bool,
    _subs: Vec<Subscription>,
}

impl Tools {
    pub fn new(session: Arc<Session>, repo: WeakEntity<RepoView>, left: String, right: String, auto: bool, window: &mut Window, cx: &mut Context<Self>) -> Tools {
        let t = t(cx);
        let picker = |label: &'static str, value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let r = repo.clone();
            let v = value.to_owned();
            cx.new(|cx| RevisionPicker::new(r, label, &v, window, cx))
        };
        let me = cx.entity().downgrade();
        let detail = |window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| DetailView::new(session.clone(), repo.clone(), window, cx));
        let compare_detail = detail(window, cx);
        compare_detail.update(cx, |d, _| d.comparison = true);
        let preview_detail = detail(window, cx);
        preview_detail.update(cx, |d, _| d.embedded = true);
        let input = |window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx));
        let pattern = input(window, cx);
        let lock_path = input(window, cx);
        let patch = cx.new(|cx| TextareaState::new(window, cx).rows(10));
        let subs = vec![
            cx.subscribe(&pattern, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&lock_path, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&patch, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let _ = me;
        let mut tools = Tools {
            left: picker(t.tools.left, &left, window, cx),
            right: picker(t.tools.right, &right, window, cx),
            base: picker(t.tools.base, "HEAD~1", window, cx),
            bad: picker(t.repo_tools.bisect_bad, "HEAD", window, cx),
            good: picker(t.repo_tools.bisect_good, "", window, cx),
            session,
            repo,
            section: Section::Compare,
            error: String::new(),
            loading: false,
            common_base: auto,
            comparison: None,
            compare_detail,
            remotes: Vec::new(),
            logs: Vec::new(),
            more_logs: false,
            plan: None,
            steps: Vec::new(),
            update_refs: false,
            preview: None,
            preview_detail,
            backup: String::new(),
            last: None,
            bisecting: None,
            modules: Vec::new(),
            lfs: None,
            stats: None,
            locks: None,
            pattern,
            lock_path,
            patch,
            index: false,
            _subs: subs,
        };
        // 从提交图的「相比某分支」跳过来时一打开就从共同祖先比较
        if auto {
            tools.run_compare(cx);
        }
        tools
    }

    fn busy(&self, cx: &App) -> bool {
        self.loading || self.repo.upgrade().is_some_and(|r| r.read(cx).busy)
    }

    /// 在后台跑一个只读查询，出错显示在页面上
    fn query<T: Send + 'static>(&mut self, work: impl FnOnce(&Session) -> engine::Result<T> + Send + 'static, done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static, cx: &mut Context<Self>) {
        self.loading = true;
        self.error.clear();
        cx.notify();
        let session = self.session.clone();
        git::bg(cx, move || work(&session), move |this, r, cx| {
            this.loading = false;
            match r {
                Ok(v) => done(this, v, cx),
                Err(e) => this.error = error_text(cx, &e),
            }
            cx.notify();
        })
        .detach();
    }

    /// 执行一条写操作（进命令日志和撤销列表、刷新仓库页），交回它的日志
    fn exec(&mut self, op: Op, cx: &mut Context<Self>) -> Task<Option<Log>> {
        match self.repo.update(cx, |r, cx| r.exec_log(op, cx)) {
            Ok(task) => task,
            Err(_) => Task::ready(None),
        }
    }

    fn run_compare(&mut self, cx: &mut Context<Self>) {
        let (left, right, common) = (self.left.read(cx).value(cx), self.right.read(cx).value(cx), self.common_base);
        self.query(move |s| engine::tools::compare(&s.repo, &left, &right, common), |this, c, cx| {
            let detail = engine::detail::Detail {
                id: c.right.clone(),
                parents: vec![c.left.clone()],
                author: String::new(),
                author_email: String::new(),
                author_time: 0,
                committer: String::new(),
                committer_time: 0,
                message: format!("{} → {}", &c.left[..12.min(c.left.len())], &c.right[..12.min(c.right.len())]),
                files: c.files.clone(),
            };
            let path = this.repo.upgrade().map(|r| r.read(cx).path.clone()).unwrap_or_default();
            let key = format!("review:{}", serde_json::to_string(&(&path, &c.left, &c.right)).unwrap());
            let source = Source::Compare(c.left.clone(), c.right.clone());
            this.compare_detail.update(cx, |d, cx| d.set(Some(detail), source, key, cx));
            this.comparison = Some(c);
        }, cx);
    }

    fn choose(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.last = None;
        self.error.clear();
        match section {
            Section::Remotes => self.query(|s| engine::tools::remotes(&s.repo), |this, v, _| this.remotes = v, cx),
            Section::Reflog => self.load_logs(false, cx),
            Section::Bisect => self.query(|s| Ok(engine::tools::bisect_log(&s.repo)), |this, v, _| this.bisecting = v, cx),
            Section::Submodules => self.query(|s| engine::tools::submodules(&s.repo), |this, v, _| this.modules = v, cx),
            Section::Lfs => self.query(|s| Ok(engine::tools::lfs(&s.repo)), |this, v, _| this.lfs = Some(v), cx),
            Section::Stats => self.query(|s| engine::tools::stats(&s.repo), |this, v, _| this.stats = Some(v), cx),
            _ => cx.notify(),
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let section = self.section;
        let last = self.last.take();
        self.choose(section, cx);
        self.last = last;
    }

    fn load_logs(&mut self, append: bool, cx: &mut Context<Self>) {
        let skip = if append { self.logs.len() } else { 0 };
        self.query(move |s| engine::tools::reflog(&s.repo, skip, 201), move |this, mut next, _| {
            this.more_logs = next.len() > 200;
            next.truncate(200);
            if !append {
                this.logs.clear();
            }
            this.logs.extend(next);
        }, cx);
    }

    fn remote(&mut self, action: &'static str, item: Option<Remote>, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let title = match action {
            "add" => t.tools.add_remote,
            "remove" => t.tools.delete_remote,
            "rename" => t.tools.rename_remote,
            "push" => t.tools.edit_push,
            _ => t.tools.edit_fetch,
        };
        let mut spec = Spec::new(title);
        if let Some(i) = &item {
            spec = spec.message(i.name.clone());
        }
        spec = match action {
            "remove" => spec.warning(t.tools.remove_remote).danger(),
            "rename" => spec.field(Field::text("name", t.tools.remote_name).value(item.as_ref().unwrap().name.clone())),
            _ => {
                let url = item.as_ref().map(|i| if action == "push" { i.push.clone() } else { i.fetch.clone() }).unwrap_or_default();
                let spec = if action == "add" { spec.field(Field::text("name", t.tools.remote_name).value("origin")) } else { spec };
                spec.field(Field::text("url", t.tools.url).value(url))
            }
        };
        let Some(rx) = self.repo.upgrade().map(|r| r.update(cx, |r, cx| r.ask(spec, window, cx))) else { return };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some(v)) = rx.await else { return };
            let name = item.as_ref().map(|i| i.name.clone()).unwrap_or_default();
            let op = match action {
                "add" => Op::RemoteAdd { name: v.text("name"), url: v.text("url") },
                "remove" => Op::RemoteRemove { name },
                "rename" => Op::RemoteRename { old: name, new: v.text("name") },
                _ => Op::RemoteUrl { name, url: v.text("url"), push: action == "push" },
            };
            let Ok(run) = this.update(cx, |this, cx| this.exec(op, cx)) else { return };
            run.await;
            let _ = this.update(cx, |this, cx| this.reload(cx));
        })
        .detach();
    }

    fn recover(&mut self, entry: Reflog, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let spec = Spec::new(t.tools.recover).message(format!("{}\n{}", entry.id, entry.subject)).field(Field::text("name", t.new_name).value(format!("recovered/{}", &entry.id[..8.min(entry.id.len())])));
        let Some(rx) = self.repo.upgrade().map(|r| r.update(cx, |r, cx| r.ask(spec, window, cx))) else { return };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some(v)) = rx.await else { return };
            if let Ok(run) = this.update(cx, |this, cx| this.exec(Op::CreateBranch { name: v.text("name"), start: entry.id, checkout: false }, cx)) {
                run.await;
            }
        })
        .detach();
    }

    fn load_plan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let base = self.base.read(cx).value(cx);
        let w = window.window_handle();
        self.query(move |s| engine::rebase::plan(&s.repo, &base), move |this, plan, cx| {
            this.steps = plan
                .steps
                .iter()
                .map(|s| {
                    let selected = ACTIONS.iter().position(|a| *a == s.action).unwrap_or(0);
                    let t = t(cx);
                    let opts: Vec<Opt> = ACTIONS.iter().map(|a| Opt::new(*a, lookup(t.tools.rebase_actions, a).unwrap_or(a))).collect();
                    let message = s.message.clone();
                    w.update(cx, |_, window, cx| {
                        let action = cx.new(|cx| SelectState::new(opts, Some(IndexPath::new(selected)), window, cx));
                        let msg = cx.new(|cx| {
                            let mut m = TextareaState::new(window, cx).rows(3);
                            m.set_value(message, window, cx);
                            m
                        });
                        PlanStep { action, message: msg }
                    })
                    .expect("window")
                })
                .collect();
            for step in &this.steps {
                let sub = cx.subscribe(&step.action, |_, _, _: &SelectEvent<Vec<Opt>>, cx| cx.notify());
                this._subs.push(sub);
            }
            this.plan = Some(plan);
            this.update_refs = false;
            this.preview = None;
            this.backup.clear();
        }, cx);
    }

    fn action(&self, i: usize, cx: &App) -> String {
        self.steps[i].action.read(cx).selected_value().map(|v| v.to_string()).unwrap_or_else(|| "pick".into())
    }

    fn summary(&self, cx: &App) -> String {
        let t = t(cx);
        let n = self.steps.len();
        let count = |acts: &[&str]| (0..n).filter(|&i| acts.contains(&self.action(i, cx).as_str())).count();
        let (fold, drop) = (count(&["squash", "fixup"]), count(&["drop"]));
        let mut parts = vec![(t.tools.plan_keep)(n - fold - drop)];
        if fold > 0 {
            parts.push((t.tools.plan_fold)(fold));
        }
        if drop > 0 {
            parts.push((t.tools.plan_drop)(drop));
        }
        parts.join(" · ")
    }

    fn move_step(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let other = if up { index - 1 } else { index + 1 };
        if let Some(plan) = &mut self.plan {
            plan.steps.swap(index, other);
            self.steps.swap(index, other);
        }
        cx.notify();
    }

    fn toggle_preview(&mut self, id: String, cx: &mut Context<Self>) {
        if self.preview.as_deref() == Some(&id) {
            self.preview = None;
            return cx.notify();
        }
        let i = id.clone();
        self.query(move |s| engine::detail::detail(&s.repo, &i), move |this, d, cx| {
            this.preview_detail.update(cx, |v, cx| v.set(Some(d), Source::Commit, String::new(), cx));
            this.preview = Some(id);
        }, cx);
    }

    fn apply_plan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(plan) = self.plan.clone() else { return };
        let t = t(cx);
        let mut plan = plan;
        for (i, step) in plan.steps.iter_mut().enumerate() {
            step.action = self.action(i, cx);
            step.message = self.steps[i].message.read(cx).value().to_string();
        }
        let update_refs = self.update_refs;
        let spec = Spec::new(t.tools.apply_plan).message(self.summary(cx)).warning(t.tools.rebase_warning).danger();
        let Some(rx) = self.repo.upgrade().map(|r| r.update(cx, |r, cx| r.ask(spec, window, cx))) else { return };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some(_)) = rx.await else { return };
            let Ok(repo) = this.update(cx, |this, _| this.repo.clone()) else { return };
            let Ok(run) = repo.update(cx, |r, cx| r.rebase(plan, update_refs, cx)) else { return };
            let log = run.await;
            let _ = this.update(cx, |this, cx| {
                let Some(log) = log.filter(|l| l.ok) else { return };
                // 引擎把备份分支的引用名放在输出第一行
                this.backup = log.output.lines().next().unwrap_or("").trim_start_matches("refs/heads/").to_owned();
                this.plan = None;
                this.steps.clear();
                this.preview = None;
                cx.notify();
            });
        })
        .detach();
    }

    fn extra(&mut self, op: Op, cx: &mut Context<Self>) -> Task<bool> {
        let run = self.exec(op, cx);
        cx.spawn(async move |this, cx| {
            let log = run.await;
            let ok = log.as_ref().is_some_and(|l| l.ok);
            let _ = this.update(cx, |this, cx| {
                this.last = log;
                this.reload(cx);
            });
            ok
        })
    }

    fn render_compare(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = t(cx);
        let busy = self.busy(cx);
        let form = div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(8.))
            .child(self.left.clone())
            .child(btn("swap").icon_size(Icon::Sync, px(14.)).tip(t.tools.swap).disabled(busy).on_click(cx.listener(|this, _, window, cx| {
                let (l, r) = (this.left.read(cx).value(cx), this.right.read(cx).value(cx));
                this.left.update(cx, |p, cx| p.set_value(r, window, cx));
                this.right.update(cx, |p, cx| p.set_value(l, window, cx));
                if this.comparison.is_some() {
                    this.run_compare(cx);
                }
            })))
            .child(self.right.clone())
            .child(div().h(px(28.)).flex().items_center().child(Checkbox::new("common-base").checked(self.common_base).label(t.tools.common_base).on_click(cx.listener(|this, v: &bool, _, cx| {
                this.common_base = *v;
                cx.notify();
            }))))
            .child(btn("compare").primary().text(t.tools.compare).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.run_compare(cx))));
        let mut out = vec![form.into_any_element()];
        if self.comparison.is_some() {
            out.push(div().flex().flex_col().flex_1().min_h(px(300.)).child(self.compare_detail.clone()).into_any_element());
        }
        out
    }

    fn render_rebase(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = palette(cx).clone();
        let t = t(cx);
        let busy = self.busy(cx);
        let in_progress = self.repo.upgrade().is_some_and(|r| r.read(cx).refs.in_progress.is_some());
        let head_id = self.repo.upgrade().and_then(|r| r.read(cx).refs.head_id.clone());
        let mut out = vec![prose(t.tools.rebase_warning, cx)];
        out.push(div().flex().flex_wrap().items_end().gap(px(8.)).child(self.base.clone()).child(btn("load-plan").text(t.tools.load_plan).disabled(busy || in_progress).on_click(cx.listener(|this, _, window, cx| this.load_plan(window, cx)))).into_any_element());
        if !self.backup.is_empty() {
            out.push(div().text_color(p.green).child((t.tools.rebase_done)(&self.backup)).into_any_element());
        }
        let Some(plan) = self.plan.clone() else { return out };
        out.push(div().child(format!("{} → {} · {}", &plan.base[..12.min(plan.base.len())], &plan.head[..12.min(plan.head.len())], self.summary(cx))).into_any_element());
        let n = plan.steps.len();
        for (i, step) in plan.steps.iter().enumerate() {
            let action = self.action(i, cx);
            let id = step.id.clone();
            let mut row = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .child(btn(("up", i)).small().icon_size(Icon::Up, px(14.)).tip(t.tools.up).disabled(busy || i == 0).on_click(cx.listener(move |this, _, _, cx| this.move_step(i, true, cx))))
                .child(btn(("down", i)).small().icon_size(Icon::Down, px(14.)).tip(t.tools.down).disabled(busy || i + 1 == n).on_click(cx.listener(move |this, _, _, cx| this.move_step(i, false, cx))))
                .child(div().w(px(120.)).child(Select::new(&self.steps[i].action).small()))
                .child(
                    btn(("preview", i))
                        .small()
                        .quiet()
                        .on(self.preview.as_deref() == Some(&step.id))
                        .text(format!("{}  {}", &step.id[..8.min(step.id.len())], step.subject))
                        .tip(t.tools.preview_step)
                        .disabled(self.loading)
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_preview(id.clone(), cx))),
                );
            for b in &step.branches {
                row = row.child(div().px(px(8.)).border_1().border_color(p.accent).rounded(crate::theme::R_MD).text_color(p.accent).text_size(FS_SM).child(b.trim_start_matches("refs/heads/").to_owned()));
            }
            let mut card = div().px(px(16.)).py(px(12.)).bg(p.surface).border_1().border_color(p.border).rounded(R_MD).child(row);
            if action == "reword" || action == "squash" {
                card = card.child(div().mt(px(8.)).text_color(p.muted).child(t.tools.rebase_message).child(Textarea::new(&self.steps[i].message)));
            }
            if self.preview.as_deref() == Some(&step.id) {
                card = card.child(div().mt(px(12.)).h(px(360.)).flex().border_t_1().border_color(p.border).child(self.preview_detail.clone()));
            }
            out.push(card.into_any_element());
        }
        if plan.steps.iter().any(|s| !s.branches.is_empty()) {
            out.push(Checkbox::new("update-refs").checked(self.update_refs).label(format!("{} · {}", t.tools.update_refs, t.tools.update_refs_hint)).on_click(cx.listener(|this, v: &bool, _, cx| {
                this.update_refs = *v;
                cx.notify();
            })).into_any_element());
        }
        let stale = head_id.as_deref() != Some(&plan.head);
        out.push(div().flex().child(btn("apply-plan").primary().text(t.tools.apply_plan).disabled(busy || n == 0 || in_progress || stale).on_click(cx.listener(|this, _, window, cx| this.apply_plan(window, cx)))).into_any_element());
        out
    }

    fn render_extra(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = palette(cx).clone();
        let t = t(cx);
        let r = &t.repo_tools;
        let busy = self.busy(cx);
        let mut out = Vec::new();
        let table = || div().bg(p.surface).border_1().border_color(p.border).rounded(R_MD).overflow_hidden();
        let row = |first: bool| div().flex().items_center().gap(px(12.)).min_h(px(36.)).pl(px(16.)).pr(px(8.)).py(px(4.)).when(!first, |s| s.border_t_1().border_color(p.border)).text_size(FS_SM);
        let none = |text: &str| div().p(px(24.)).text_color(p.muted).child(text.to_owned());
        match self.section {
            Section::Patch => {
                out.push(prose(r.patch_hint, cx));
                out.push(div().text_color(p.muted).child(r.patch_text).child(Textarea::new(&self.patch).font_family(MONO).text_size(FS_SM)).into_any_element());
                out.push(Checkbox::new("patch-index").checked(self.index).label(r.patch_index).on_click(cx.listener(|this, v: &bool, _, cx| {
                    this.index = *v;
                    cx.notify();
                })).into_any_element());
                let empty = self.patch.read(cx).value().trim().is_empty();
                out.push(div().flex().child(btn("apply-patch").primary().text(r.patch_apply).disabled(busy || empty).on_click(cx.listener(|this, _, window, cx| {
                    let text = this.patch.read(cx).value().to_string();
                    let run = this.extra(Op::ApplyPatch { text, index: this.index }, cx);
                    cx.spawn_in(window, async move |this, cx| {
                        if run.await {
                            let _ = this.update_in(cx, |this, window, cx| this.patch.update(cx, |s, cx| s.set_value("", window, cx)));
                        }
                    })
                    .detach();
                }))).into_any_element());
                if self.last.as_ref().is_some_and(|l| l.ok) {
                    out.push(div().text_color(p.green).child(r.patch_done).into_any_element());
                }
            }
            Section::Bisect => {
                out.push(prose(r.bisect_hint, cx));
                let found = self.bisecting.as_ref().and_then(|log| log.lines().find_map(|l| l.strip_prefix("# first bad commit: [").or_else(|| l.strip_prefix("# first 'bad' commit: ["))).and_then(|rest| rest.split_once("] ")).map(|(id, s)| (id.to_owned(), s.to_owned())));
                let in_progress = self.repo.upgrade().is_some_and(|r| r.read(cx).refs.in_progress.is_some());
                match (&self.bisecting, found) {
                    (None, _) => {
                        let ready = !self.bad.read(cx).value(cx).is_empty() && !self.good.read(cx).value(cx).is_empty();
                        out.push(div().flex().flex_wrap().items_end().gap(px(8.)).child(self.bad.clone()).child(self.good.clone()).child(btn("bisect-start").primary().text(r.bisect_start).disabled(busy || !ready || in_progress).on_click(cx.listener(|this, _, _, cx| {
                            let (bad, good) = (this.bad.read(cx).value(cx), this.good.read(cx).value(cx));
                            this.extra(Op::BisectStart { bad, good }, cx).detach();
                        }))).into_any_element());
                    }
                    (Some(_), Some((id, subject))) => {
                        out.push(div().text_color(p.green).child((r.bisect_found)(&id[..12.min(id.len())], &subject)).into_any_element());
                        out.push(div().flex().child(btn("bisect-reset").primary().text(r.bisect_reset).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.extra(Op::BisectReset, cx).detach()))).into_any_element());
                    }
                    (Some(_), None) => {
                        let head = self.repo.upgrade().and_then(|r| r.read(cx).refs.head_id.clone()).unwrap_or_default();
                        out.push(div().child((r.bisect_now)(&head[..12.min(head.len())])).into_any_element());
                        let mark = |id: &'static str, text: &'static str, v: Verdict, cx: &mut Context<Self>| btn(id).text(text).disabled(busy).on_click(cx.listener(move |this, _, _, cx| this.extra(Op::BisectMark { verdict: v }, cx).detach()));
                        out.push(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.))
                                .child(mark("good", r.bisect_mark_good, Verdict::Good, cx))
                                .child(mark("bad", r.bisect_mark_bad, Verdict::Bad, cx))
                                .child(mark("skip", r.bisect_skip, Verdict::Skip, cx).quiet())
                                .child(btn("reset").danger().text(r.bisect_reset).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.extra(Op::BisectReset, cx).detach())))
                                .into_any_element(),
                        );
                    }
                }
                if let Some(log) = &self.bisecting {
                    out.push(div().child(div().text_color(p.muted).mb(px(8.)).child(r.bisect_log)).child(pre("bisect-log", log, &p)).into_any_element());
                }
            }
            Section::Submodules => {
                out.push(prose(r.submodule_hint, cx));
                let empty = self.modules.is_empty();
                out.push(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(btn("sub-update").text(r.submodule_update).disabled(busy || empty).on_click(cx.listener(|this, _, _, cx| this.extra(Op::SubmoduleUpdate, cx).detach())))
                        .child(btn("sub-sync").text(r.submodule_sync).disabled(busy || empty).on_click(cx.listener(|this, _, _, cx| this.extra(Op::SubmoduleSync, cx).detach())))
                        .into_any_element(),
                );
                let mut list = table();
                if empty {
                    list = list.child(none(r.submodule_none));
                }
                let root = self.repo.upgrade().map(|r| r.read(cx).path.clone()).unwrap_or_default();
                for (i, m) in self.modules.iter().enumerate() {
                    let path = format!("{root}/{}", m.path);
                    list = list.child(
                        row(i == 0)
                            .child(div().font_family(MONO).child(m.path.clone()))
                            .child(div().font_family(MONO).text_color(p.muted).child(m.id[..12.min(m.id.len())].to_owned()))
                            .child(div().flex_1().text_color(if m.state != "ok" { p.orange } else { p.muted }).child(lookup(r.submodule_states, m.state).unwrap_or(m.state)))
                            .when(m.state != "missing", |s| s.child(btn(SharedString::from(format!("open-{}", m.path))).small().text(r.submodule_open).on_click(cx.listener(move |this, _, window, cx| {
                                let path = path.clone();
                                let _ = this.repo.update(cx, |r, cx| r.open_path(path, window, cx));
                            })))),
                    );
                }
                out.push(list.into_any_element());
            }
            Section::Stats => {
                out.push(prose(r.stats_hint, cx));
                match &self.stats {
                    Some(stats) if stats.commits > 0 => {
                        let dates = calendar(&stats.times, cx.global::<Zone>());
                        let peak = dates.months.iter().max_by_key(|m| m.count).map(|m| (m.name.clone(), m.count)).unwrap_or_default();
                        let tile = |value: String, label: &str| div().flex().flex_col().gap(px(4.)).min_w(px(140.)).px(px(16.)).py(px(12.)).bg(p.surface).border_1().border_color(p.border).rounded(R_MD).text_size(FS_SM).text_color(p.muted).child(div().text_size(FS_LG).font_weight(FontWeight::BOLD).text_color(p.text).child(value)).child(label.to_owned());
                        out.push(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.))
                                .child(tile(crate::i18n::group(stats.commits as usize), r.stats_commits))
                                .child(tile(stats.authors.len().to_string(), r.stats_authors))
                                .child(tile(dates.first.clone(), r.stats_first))
                                .child(tile(dates.last.clone(), r.stats_last))
                                .into_any_element(),
                        );
                        out.push(div().font_weight(FontWeight::BOLD).child(r.stats_months).into_any_element());
                        // 每月一根柱子：贴着基线、顶端圆角、柱间留 2px
                        let peak_n = peak.1.max(1);
                        out.push(
                            div()
                                .flex()
                                .flex_none()
                                .items_end()
                                .gap(px(2.))
                                .h(px(120.))
                                .border_b_1()
                                .border_color(p.border)
                                .children(dates.months.iter().map(|m| {
                                    let tip = (r.stats_month)(&m.name, m.count);
                                    div()
                                        .id(SharedString::from(format!("month-{}", m.name)))
                                        .flex_1()
                                        .min_w(px(1.))
                                        .max_w(px(48.))
                                        .min_h(px(1.))
                                        .h(relative(m.count as f32 / peak_n as f32))
                                        .bg(p.accent)
                                        .rounded_t(px(4.))
                                        .hover(|s| s.opacity(0.8))
                                        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                                }))
                                .into_any_element(),
                        );
                        out.push(
                            div()
                                .flex()
                                .justify_between()
                                .mt(px(-8.))
                                .text_size(FS_SM)
                                .text_color(p.muted)
                                .child(dates.months.first().map(|m| m.name.clone()).unwrap_or_default())
                                .child((r.stats_peak)(&peak.0, peak.1))
                                .child(dates.months.last().map(|m| m.name.clone()).unwrap_or_default())
                                .into_any_element(),
                        );
                        for (title, list) in [(r.stats_by_author, &stats.authors), (r.stats_by_file, &stats.files)] {
                            out.push(div().font_weight(FontWeight::BOLD).child(title).into_any_element());
                            let top = list.first().map_or(1, |c| c.count.max(1)) as f32;
                            let mut t = table().id(SharedString::from(title)).flex_none().max_h(px(420.)).overflow_y_scroll();
                            for (i, c) in list.iter().enumerate() {
                                let w = c.count as f32 / top;
                                t = t.child(
                                    row(i == 0)
                                        .relative()
                                        .min_h(px(28.))
                                        .child(div().absolute().left_0().top_0().bottom_0().w(relative(w)).bg(p.selection))
                                        .child(div().relative().flex_1().min_w_0().truncate().font_family(MONO).child(c.name.clone()))
                                        .child(div().relative().font_family(MONO).child(crate::i18n::group(c.count as usize))),
                                );
                            }
                            out.push(t.into_any_element());
                        }
                    }
                    Some(_) => out.push(none(r.stats_none).into_any_element()),
                    None => {}
                }
            }
            Section::Lfs => {
                out.push(prose(r.lfs_hint, cx));
                let Some(lfs) = self.lfs.clone() else { return out };
                if lfs.version.is_none() {
                    out.push(div().text_color(p.red).child(r.lfs_missing).into_any_element());
                    return out;
                }
                out.push(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .child(div().text_color(if lfs.installed { p.text } else { p.orange }).child(if lfs.installed { r.lfs_installed } else { r.lfs_not_installed }))
                        .child(div().text_size(FS_SM).text_color(p.muted).child(lfs.version.clone().unwrap_or_default()))
                        .when(!lfs.installed, |s| s.child(btn("lfs-install").primary().text(r.lfs_install).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.extra(Op::LfsInstall, cx).detach()))))
                        .child(btn("lfs-pull").text(r.lfs_pull).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.extra(Op::LfsPull, cx).detach())))
                        .into_any_element(),
                );
                out.push(div().font_weight(FontWeight::BOLD).child(r.lfs_patterns).into_any_element());
                let mut list = table();
                if lfs.patterns.is_empty() {
                    list = list.child(none(r.lfs_no_patterns));
                }
                for (i, pat) in lfs.patterns.iter().enumerate() {
                    let pattern = pat.clone();
                    list = list.child(row(i == 0).child(div().flex_1().font_family(MONO).child(pat.clone())).child(btn(SharedString::from(format!("untrack-{pat}"))).small().text(r.lfs_untrack).disabled(busy).on_click(cx.listener(move |this, _, _, cx| this.extra(Op::LfsUntrack { pattern: pattern.clone() }, cx).detach()))));
                }
                out.push(list.into_any_element());
                let pattern_empty = self.pattern.read(cx).value().trim().is_empty();
                out.push(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_end()
                        .gap(px(8.))
                        .child(div().flex_1().min_w(px(200.)).text_color(p.muted).child(r.lfs_pattern).child(Input::new(&self.pattern)))
                        .child(btn("lfs-track").text(r.lfs_track).disabled(busy || pattern_empty).on_click(cx.listener(|this, _, window, cx| {
                            let pattern = this.pattern.read(cx).value().to_string();
                            let run = this.extra(Op::LfsTrack { pattern }, cx);
                            cx.spawn_in(window, async move |this, cx| {
                                if run.await {
                                    let _ = this.update_in(cx, |this, window, cx| this.pattern.update(cx, |s, cx| s.set_value("", window, cx)));
                                }
                            })
                            .detach();
                        })))
                        .into_any_element(),
                );
                out.push(prose(r.lfs_track_note, cx));
                let path_empty = self.lock_path.read(cx).value().trim().is_empty();
                out.push(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_end()
                        .gap(px(8.))
                        .child(div().flex_1().min_w(px(200.)).text_color(p.muted).child(r.lfs_lock_path).child(Input::new(&self.lock_path)))
                        .child(btn("lfs-lock").text(r.lfs_lock).disabled(busy || path_empty).on_click(cx.listener(|this, _, _, cx| {
                            let path = this.lock_path.read(cx).value().to_string();
                            this.extra(Op::LfsLock { path }, cx).detach();
                        })))
                        .child(btn("lfs-unlock").text(r.lfs_unlock).disabled(busy || path_empty).on_click(cx.listener(|this, _, _, cx| {
                            let path = this.lock_path.read(cx).value().to_string();
                            this.extra(Op::LfsUnlock { path }, cx).detach();
                        })))
                        .child(btn("lfs-locks").text(r.lfs_locks).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.query(|s| engine::tools::lfs_locks(&s.repo), |this, v, _| this.locks = Some(v), cx))))
                        .into_any_element(),
                );
                if let Some(locks) = &self.locks {
                    out.push(pre("lfs-locks", if locks.is_empty() { r.lfs_no_locks } else { locks }, &p).into_any_element());
                }
            }
            _ => {}
        }
        if let Some(log) = self.last.as_ref().filter(|l| !l.output.is_empty() && (self.section != Section::Patch || !l.ok)) {
            out.push(pre("last-output", &log.output, &p).when(!log.ok, |s| s.text_color(p.red)).into_any_element());
        }
        out
    }
}

fn prose(text: &str, cx: &App) -> AnyElement {
    let p = palette(cx);
    div().max_w(px(72. * 8.)).text_color(p.muted).font_family(PROSE).line_height(relative(LH_BODY)).child(text.to_owned()).into_any_element()
}

/// 命令输出：超过 320px 高时在框里滚动
fn pre(id: &'static str, text: &str, p: &crate::theme::Palette) -> gpui::Stateful<gpui::Div> {
    div().id(id).overflow_y_scroll().px(px(16.)).py(px(12.)).max_h(px(320.)).bg(p.surface).border_1().border_color(p.border).rounded(R_MD).font_family(MONO).text_size(FS_SM).child(text.to_owned())
}

impl Render for Tools {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let busy = self.busy(cx);
        let groups = [
            (t.tools.group_history, [(Section::Compare, t.tools.compare), (Section::Rebase, t.tools.rebase), (Section::Reflog, t.tools.reflog), (Section::Bisect, t.tools.bisect)].to_vec()),
            (t.tools.group_repo, [(Section::Remotes, t.tools.remotes), (Section::Submodules, t.tools.submodules), (Section::Lfs, t.tools.lfs), (Section::Patch, t.tools.patch), (Section::Stats, t.tools.stats)].to_vec()),
        ];
        let mut nav = div().flex().flex_col().gap(px(8.));
        for (i, (group, items)) in groups.into_iter().enumerate() {
            let mut row = div().flex().flex_wrap().items_center().gap(px(8.)).child(div().min_w(gpui::rems(4.)).text_size(FS_SM).text_color(p.muted).child(group));
            for (j, (section, text)) in items.into_iter().enumerate() {
                row = row.child(btn(("tool", i * 10 + j)).text(text).on(self.section == section).disabled(busy).on_click(cx.listener(move |this, _, _, cx| this.choose(section, cx))));
            }
            nav = nav.child(row);
        }
        let mut page = div().id("toolbox").flex().flex_col().flex_1().min_w_0().min_h_0().overflow_y_scroll().px(px(32.)).py(px(24.)).gap(px(16.)).text_size(FS_MD).child(nav);
        if !self.error.is_empty() {
            page = page.child(div().text_color(p.red).child(self.error.clone()));
        }
        let content = match self.section {
            Section::Compare => self.render_compare(cx),
            Section::Rebase => self.render_rebase(cx),
            Section::Remotes => {
                let mut out = vec![div().flex().child(btn("add-remote").text(t.tools.add_remote).disabled(busy).on_click(cx.listener(|this, _, window, cx| this.remote("add", None, window, cx)))).into_any_element()];
                for item in self.remotes.clone() {
                    let dl = |k: &str, v: &str| div().flex().gap(px(16.)).child(div().w(px(80.)).flex_none().text_color(p.muted).child(k.to_owned())).child(div().flex_1().min_w_0().font_family(MONO).child(v.to_owned()));
                    let mut actions = div().flex().flex_wrap().gap(px(8.)).mt(px(12.));
                    for (action, text) in [("fetch", t.tools.edit_fetch), ("push", t.tools.edit_push), ("rename", t.tools.rename_remote), ("remove", t.tools.delete_remote)] {
                        let it = item.clone();
                        actions = actions.child(btn(SharedString::from(format!("{action}-{}", item.name))).small().text(text).danger_if(action == "remove").disabled(busy).on_click(cx.listener(move |this, _, window, cx| this.remote(action, Some(it.clone()), window, cx))));
                    }
                    out.push(
                        div()
                            .px(px(16.))
                            .py(px(12.))
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border)
                            .rounded(R_MD)
                            .child(div().mb(px(8.)).font_family(DISPLAY).text_size(FS_LG).font_weight(FontWeight::SEMIBOLD).child(item.name.clone()))
                            .child(div().flex().flex_col().gap(px(4.)).text_size(FS_SM).child(dl(t.tools.fetch_url, &item.fetch)).child(dl(t.tools.push_url, &item.push)))
                            .child(actions)
                            .into_any_element(),
                    );
                }
                out
            }
            Section::Reflog => {
                let mut out = vec![div().flex().child(btn("reflog-refresh").text(t.tools.refresh).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.load_logs(false, cx)))).into_any_element()];
                // Reflog 是一张表：行高和提交图一致，不再一条记录一张卡片
                let mut table = div().bg(p.surface).border_1().border_color(p.border).rounded(R_MD).overflow_hidden();
                if self.logs.is_empty() {
                    table = table.child(div().p(px(24.)).text_color(p.muted).font_family(PROSE).child(t.tools.no_reflog));
                }
                for (i, entry) in self.logs.clone().into_iter().enumerate() {
                    let e = entry.clone();
                    table = table.child(
                        div()
                            .id(("reflog", i))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .min_h(px(36.))
                            .pl(px(16.))
                            .pr(px(8.))
                            .py(px(4.))
                            .when(i > 0, |s| s.border_t_1().border_color(p.border))
                            .text_size(FS_SM)
                            .hover(|s| s.bg(p.hover))
                            .child(div().flex_none().font_family(MONO).text_color(p.muted).child(entry.id[..12.min(entry.id.len())].to_owned()))
                            .child(div().flex().flex_1().min_w_0().child(div().flex_none().mr(px(12.)).font_family(MONO).text_color(p.accent).child(entry.selector.clone())).child(div().min_w_0().truncate().font_family(PROSE).child(entry.subject.clone())))
                            .child(btn(("recover", i)).small().text(t.tools.recover).disabled(busy).on_click(cx.listener(move |this, _, window, cx| this.recover(e.clone(), window, cx)))),
                    );
                }
                out.push(table.into_any_element());
                if self.more_logs {
                    out.push(div().flex().child(btn("reflog-more").text(t.tools.load_more).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.load_logs(true, cx)))).into_any_element());
                }
                out
            }
            _ => self.render_extra(cx),
        };
        page.children(content)
    }
}
