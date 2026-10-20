//! 仓库页左侧：四个页面的入口、引用筛选框、本地分支 / 远程分支 / 标签 / 贮藏 / 工作树。

use crate::i18n::t;
use crate::icons::{icon, Icon};
use crate::repo_view::{Kind, RepoView, View};
use crate::rules::{order_branches, short};
use crate::theme::{palette, Palette, FS_SM, MONO, R_LG, R_MD, R_SM};
use crate::ui::{label, name_only};
use gpui::prelude::*;
use gpui::{div, px, uniform_list, AnyElement, ClickEvent, Context, Entity, FontWeight, MouseButton, MouseDownEvent, SharedString, Subscription, UniformListScrollHandle, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::Sizable;
use std::ops::Range;
use std::rc::Rc;

pub struct SidebarState {
    filter: Entity<InputState>,
    /// 单击选中的引用或贮藏：只高亮，不改变列表顺序
    selected: String,
    /// 本地分支、远程分支、标签、贮藏、工作树各组是否展开
    open: [bool; 5],
    scroll: UniformListScrollHandle,
    _sub: Subscription,
}

impl SidebarState {
    pub fn new(window: &mut Window, cx: &mut Context<RepoView>) -> SidebarState {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(t(cx).filter));
        let sub = cx.subscribe(&filter, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        });
        SidebarState { filter, selected: String::new(), open: [true, false, false, false, true], scroll: UniformListScrollHandle::new(), _sub: sub }
    }
}

struct RefRow {
    name: String,
    id: String,
    label: String,
}

/// 列表里的一行：各组的标题和组里的条目，都是 26px 高。列表只渲染看得见的行，上千个分支也不拖慢每一帧
enum Row {
    Summary { i: usize, title: &'static str, count: usize },
    Ref(Kind, RefRow),
    Stash(usize),
    Worktree(usize),
}

const ROW_H: f32 = 26.;

fn nav_button(id: &'static str, icn: Icon, text: &str, on: bool, pill: Option<(usize, bool)>, p: &Palette) -> gpui::Stateful<gpui::Div> {
    let fg = if on { p.accent } else { p.text };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(32.))
        .px(px(8.))
        .rounded(R_MD)
        .cursor_pointer()
        .text_color(fg)
        .when(on, |s| s.bg(p.accent_soft).font_weight(FontWeight::SEMIBOLD))
        .when(!on, |s| s.hover(|s| s.bg(p.hover)))
        .child(icon(icn, px(16.), fg))
        .child(div().flex_1().min_w_0().truncate().child(label(text, true)))
        .when_some(pill, |s, (n, quiet)| {
            s.child(
                div()
                    .min_w(px(20.))
                    .px(px(4.))
                    .rounded(R_LG)
                    .bg(if quiet { p.raised } else { p.accent })
                    .text_color(if quiet { p.muted } else { p.on_accent })
                    .text_size(FS_SM)
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(20.))
                    .flex()
                    .justify_center()
                    .child(n.to_string()),
            )
        })
}

/// 行尾的小图标：固定、加入提交图筛选。悬停时出现，已生效的常亮
fn toggle_icon(id: SharedString, group: SharedString, icn: Icon, active: bool, tip: &'static str, p: &Palette, on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static) -> impl IntoElement {
    let color = if active { p.accent } else { p.muted };
    div()
        .id(id)
        .flex_none()
        .p(px(2.))
        .rounded(R_SM)
        .cursor_pointer()
        .when(!active, |s| s.invisible().group_hover(group, |s| s.visible()))
        .hover(|s| s.bg(p.hover))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip).build(window, cx))
        .on_click(move |e, window, cx| {
            cx.stop_propagation();
            on_click(e, window, cx)
        })
        .child(icon(icn, px(12.), color))
}

pub fn render(this: &mut RepoView, _window: &mut Window, cx: &mut Context<RepoView>) -> impl IntoElement {
    let p = palette(cx).clone();
    let t = t(cx);
    let query = this.sidebar.filter.read(cx).value().to_lowercase();
    let view = this.view;
    let changes = this.entries.len();
    let worktrees = this.worktrees.len();

    let nav = div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .p(px(8.))
        .child(nav_button("nav-changes", Icon::Changes, t.changes, view == View::Changes, (changes > 0).then_some((changes, false)), &p).on_click(cx.listener(|this, _, window, cx| this.set_view(View::Changes, window, cx))))
        .child(nav_button("nav-history", Icon::History, t.history, view == View::History, None, &p).on_click(cx.listener(|this, _, window, cx| this.set_view(View::History, window, cx))))
        .child(nav_button("nav-worktrees", Icon::Folders, t.worktrees_nav, view == View::Worktrees, (worktrees > 1).then_some((worktrees, true)), &p).on_click(cx.listener(|this, _, window, cx| this.set_view(View::Worktrees, window, cx))))
        .child(nav_button("nav-requests", Icon::Request, t.requests.nav, view == View::Requests, None, &p).on_click(cx.listener(|this, _, window, cx| this.set_view(View::Requests, window, cx))))
        .child(nav_button("nav-tools", Icon::Tools, t.tools.nav, view == View::Tools, None, &p).on_click(cx.listener(|this, _, window, cx| this.set_view(View::Tools, window, cx))));

    let search = div().mx(px(8.)).mb(px(8.)).child(Input::new(&this.sidebar.filter).small().prefix(icon(Icon::Search, px(14.), p.muted)).text_size(FS_SM).cleanable(true));

    let group = |prefix: &str| -> Vec<RefRow> {
        this.refs
            .refs
            .iter()
            .filter(|r| r.name.starts_with(prefix) && r.name.to_lowercase().contains(&query))
            .map(|r| RefRow { name: r.name.clone(), id: r.id.clone(), label: r.name[prefix.len()..].to_owned() })
            .collect()
    };
    let mut branches = group("refs/heads/");
    order_branches(&mut branches, |r| &r.name, this.refs.head.as_deref(), &this.pinned);
    let mut remotes = group("refs/remotes/");
    order_branches(&mut remotes, |r| &r.name, None, &this.pinned);
    let tags = group("refs/tags/");
    let filtering = !query.is_empty();
    let mut rows = Vec::new();
    for (i, kind, title, refs) in [(0, Kind::Branch, t.branches, branches), (1, Kind::Remote, t.remotes, remotes), (2, Kind::Tag, t.tags, tags)] {
        rows.push(Row::Summary { i, title, count: refs.len() });
        if this.sidebar.open[i] || (filtering && i > 0) {
            rows.extend(refs.into_iter().map(|r| Row::Ref(kind, r)));
        }
    }
    rows.push(Row::Summary { i: 3, title: t.stashes, count: this.stashes.len() });
    if this.sidebar.open[3] {
        rows.extend((0..this.stashes.len()).map(Row::Stash));
    }
    rows.push(Row::Summary { i: 4, title: t.worktrees, count: this.worktrees.len() });
    if this.sidebar.open[4] {
        rows.extend((0..this.worktrees.len()).map(Row::Worktree));
    }
    let rows = Rc::new(rows);
    let list = uniform_list(
        "sidebar-list",
        rows.len(),
        cx.processor(move |this: &mut RepoView, range: Range<usize>, _window: &mut Window, cx: &mut Context<RepoView>| {
            // 当前分支的上游短名显示在它那一行里；在别的工作树里打开着的分支带个标记
            let upstream = this.refs.refs.iter().find(|r| Some(&r.name) == this.refs.head.as_ref()).and_then(|r| r.upstream.as_deref()).map(|u| u.trim_start_matches("refs/remotes/").to_owned());
            let elsewhere: Vec<String> = this.worktrees.iter().filter(|w| !w.current).filter_map(|w| w.branch.clone()).collect();
            range.map(|ix| render_row(this, &rows[ix], upstream.as_deref(), &elsewhere, cx)).collect::<Vec<_>>()
        }),
    )
    .track_scroll(&this.sidebar.scroll)
    .size_full();
    let scroll = div().relative().flex_1().min_h_0().child(list).vertical_scrollbar(&this.sidebar.scroll);

    let _ = name_only;
    div().flex().flex_col().flex_1().min_h_0().bg(p.panel).child(nav).child(search).child(scroll).into_any_element() as AnyElement
}

fn render_row(this: &mut RepoView, row: &Row, upstream: Option<&str>, elsewhere: &[String], cx: &mut Context<RepoView>) -> AnyElement {
    let p = palette(cx).clone();
    let t = t(cx);
    let item = |id: SharedString, on: bool, head: bool| {
        div()
            .id(id.clone())
            .group(id)
            .flex()
            .items_center()
            .gap(px(8.))
            .w_full()
            .h(px(ROW_H))
            .pl(px(24.))
            .pr(px(12.))
            .when(on, |s| s.bg(p.accent_soft))
            .when(!on, |s| s.hover(|s| s.bg(p.hover)))
            .when(head, |s| s.text_color(p.accent).font_weight(FontWeight::SEMIBOLD))
    };
    match row {
        Row::Summary { i, title, count } => {
            let i = *i;
            div()
                .id(("summary", i))
                .flex()
                .items_center()
                .w_full()
                .h(px(ROW_H))
                .px(px(12.))
                .pt(px(4.))
                .text_size(FS_SM)
                .text_color(p.muted)
                .hover(|s| s.text_color(p.text))
                .child(title.to_string())
                .child(div().ml(px(8.)).opacity(0.7).child(count.to_string()))
                .when(i == 0, |s| {
                    s.child(div().flex_1()).child(
                        div()
                            .id("clean-branches")
                            .px(px(4.))
                            .py(px(2.))
                            .cursor_pointer()
                            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.branch_list.clean).build(window, cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.clean_branches(window, cx);
                            }))
                            .child(icon(Icon::Broom, px(12.), p.muted)),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sidebar.open[i] = !this.sidebar.open[i];
                    cx.notify();
                }))
                .into_any_element()
        }
        Row::Ref(kind, r) => {
            let kind = *kind;
            let head = Some(&r.name) == this.refs.head.as_ref();
            let on = this.sidebar.selected == r.name;
            let row_id: SharedString = format!("ref-{}", r.name).into();
            let mut el = item(row_id.clone(), on, head)
                .when(head, |s| s.child(div().w(px(16.)).ml(px(-16.)).mr(px(-8.)).child("✓")))
                .child(div().min_w_0().truncate().child(r.label.clone()));
            if head {
                if let Some(up) = upstream {
                    let tip = (t.branch_list.upstream)(up);
                    el = el.child(div().id("upstream").flex_shrink(1.).min_w_0().truncate().text_size(FS_SM).text_color(p.muted).font_weight(FontWeight::NORMAL).tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)).child(format!("→ {up}")));
                }
            }
            if elsewhere.contains(&r.name) {
                el = el.child(div().id("elsewhere").flex_none().tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.in_other_worktree_mark).build(window, cx)).child(icon(Icon::Windows, px(12.), p.muted)));
            }
            if this.auto_ff.contains(&r.name) {
                el = el.child(div().id("auto-ff").flex_none().tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.auto_ff.mark).build(window, cx)).child(icon(Icon::Pull, px(12.), p.muted)));
            }
            if let Some(tr) = this.tracks.get(&r.name) {
                // 界面字体里 ↑ 和数字 1 太像，「↑2」会被读成「12」，用等宽字体
                let text = if tr.gone {
                    t.gone.to_owned()
                } else {
                    let a = if tr.ahead > 0 { format!("↑{}", tr.ahead) } else { String::new() };
                    let b = if tr.behind > 0 { format!("↓{}", tr.behind) } else { String::new() };
                    format!("{a} {b}").trim().to_owned()
                };
                let tip = if tr.gone { t.gone.to_owned() } else { format!("{} · {}", (t.to_push)(tr.ahead as usize), (t.to_pull)(tr.behind as usize)) };
                el = el.child(div().id("track").flex_none().font_family(MONO).text_size(FS_SM).text_color(p.muted).font_weight(FontWeight::NORMAL).tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)).child(text));
            }
            el = el.child(div().flex_1());
            if kind != Kind::Tag {
                let pinned = this.pinned.contains(&r.name);
                let n = r.name.clone();
                let me = cx.entity().downgrade();
                el = el.child(toggle_icon(format!("pin-{}", r.name).into(), row_id.clone(), Icon::Pin, pinned, if pinned { t.branch_list.unpin } else { t.branch_list.pin }, &p, move |_, _, cx| {
                    let _ = me.update(cx, |this, cx| this.toggle_pin(&n, cx));
                }));
            }
            let scoped = this.scope.contains(&r.name);
            let n = r.name.clone();
            let me = cx.entity().downgrade();
            el = el.child(toggle_icon(format!("scope-{}", r.name).into(), row_id, Icon::Filter, scoped, if scoped { t.graph_filter.remove } else { t.graph_filter.add }, &p, move |_, _, cx| {
                let _ = me.update(cx, |this, cx| this.toggle_scope(&n, cx));
            }));
            let (n1, n2, id) = (r.name.clone(), r.name.clone(), r.id.clone());
            el.tooltip({
                let l: SharedString = r.label.clone().into();
                move |window, cx| gpui_component::tooltip::Tooltip::new(l.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                if e.click_count() >= 2 {
                    return this.activate_ref(kind, n1.clone(), window, cx);
                }
                this.sidebar.selected = n1.clone();
                this.jump(&id, cx);
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, window, cx| this.ref_menu(e.position, kind, n2.clone(), window, cx)))
            .into_any_element()
        }
        Row::Stash(ix) => {
            let Some(s) = this.stashes.get(*ix) else { return div().into_any_element() };
            let on = this.sidebar.selected == s.name;
            let (n1, n2) = (s.name.clone(), s.name.clone());
            item(format!("stash-{}", s.name).into(), on, false)
                .child(div().min_w_0().truncate().child(s.subject.clone()))
                .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                    if e.click_count() >= 2 {
                        return this.activate_ref(Kind::Stash, n1.clone(), window, cx);
                    }
                    this.sidebar.selected = n1.clone();
                    this.view_stash(n1.clone(), cx);
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, window, cx| this.ref_menu(e.position, Kind::Stash, n2.clone(), window, cx)))
                .into_any_element()
        }
        Row::Worktree(ix) => {
            let Some(w) = this.worktrees.get(*ix) else { return div().into_any_element() };
            let base = w.path.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(&w.path).to_owned();
            let branch = w.branch.as_deref().map(|b| short(b).to_owned()).unwrap_or_else(|| t.detached.into());
            let sub = if w.changes > 0 { format!("{branch} · {}", (t.wt_dirty)(w.changes)) } else { branch };
            let (head, path, current) = (w.head.clone(), w.path.clone(), w.current);
            item(format!("wt-{}", w.path).into(), false, w.current)
                .child(div().min_w_0().truncate().child(base))
                .child(div().flex_none().text_size(FS_SM).text_color(p.muted).font_weight(FontWeight::NORMAL).child(sub))
                .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                    if e.click_count() >= 2 {
                        if !current {
                            this.open_path(path.clone(), window, cx);
                        }
                        return;
                    }
                    this.jump(&head, cx);
                }))
                .into_any_element()
        }
    }
}
