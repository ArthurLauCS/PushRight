//! 提交图：左边是泳道和节点，右边是说明、作者、日期、提交号。只渲染看得见的行，行内容按块向引擎要。

use crate::author::{self, author_key};
use crate::git::{self, Session};
use crate::i18n::t;
use crate::layout::{self, layout, Key};
use crate::theme::{alpha, mix, palette, Palette, FS_SM, MONO, R_LG, R_SM, ROW};
use crate::time::fmt_time;
use crate::ui::chip;
use gpui_component::scroll::ScrollableElement;
use engine::graph::Row;
use engine::refs::Ref;
use gpui::prelude::*;
use gpui::{
    canvas, div, point, px, uniform_list, BoxShadow, Context, EventEmitter, FocusHandle, FontWeight, KeyDownEvent, MouseButton, PathBuilder, Pixels, Point,
    ScrollStrategy, UniformListScrollHandle, Window,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

const CHUNK: usize = 128;
/// 超过这么多条泳道的部分不画（内核图宽 572）；需要时给图列加横向滚动
const LANE_CAP: usize = 24;
const LANE_W: f32 = 14.;
const PAD: f32 = 12.;

pub enum GraphEvent {
    Select(usize, String),
    Menu(Point<Pixels>, String),
    /// 右键作者名：设置这个作者的边框和底色
    StyleAuthor { key: String, name: String, email: String },
}

pub struct Graph {
    session: Arc<Session>,
    pub count: usize,
    version: usize,
    chunks: HashMap<usize, Arc<Vec<Row>>>,
    /// 重新加载后、新行到达前先显示的旧行，免得整屏空白闪一下
    stale: HashMap<usize, Arc<Vec<Row>>>,
    pending: HashSet<usize>,
    pub badges: HashMap<String, Vec<Ref>>,
    pub head_id: Option<String>,
    pub selected: Option<usize>,
    /// 只高亮这个作者的提交，其余变淡
    focus_author: Option<(String, String)>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    /// 上一帧看得见的最大泳道号，算图列宽度
    lanes: Rc<Cell<usize>>,
    visible: Rc<Cell<usize>>,
}

impl EventEmitter<GraphEvent> for Graph {}

impl Graph {
    pub fn new(session: Arc<Session>, count: usize, cx: &mut Context<Self>) -> Graph {
        Graph {
            session,
            count,
            version: 0,
            chunks: HashMap::new(),
            stale: HashMap::new(),
            pending: HashSet::new(),
            badges: HashMap::new(),
            head_id: None,
            selected: None,
            focus_author: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            lanes: Rc::new(Cell::new(1)),
            visible: Rc::new(Cell::new(20)),
        }
    }

    /// 提交图重新加载过：作废已缓存的行，旧行留着顶到新行到达
    pub fn reload(&mut self, count: usize, cx: &mut Context<Self>) {
        self.count = count;
        self.version += 1;
        self.stale.extend(self.chunks.drain());
        self.pending.clear();
        cx.notify();
    }

    pub fn row(&self, r: usize) -> Option<&Row> {
        let c = r / CHUNK;
        self.chunks.get(&c).or_else(|| self.stale.get(&c)).and_then(|rows| rows.get(r % CHUNK))
    }

    pub fn scroll_to_row(&self, row: usize, center: bool) {
        self.scroll.scroll_to_item(row, if center { ScrollStrategy::Center } else { ScrollStrategy::Nearest });
    }

    fn fetch(&mut self, chunk: usize, cx: &mut Context<Self>) {
        if self.chunks.contains_key(&chunk) || !self.pending.insert(chunk) {
            return;
        }
        let session = self.session.clone();
        let version = self.version;
        git::bg(cx, move || session.graph().rows(&session.repo, chunk * CHUNK, CHUNK), move |this, rows, cx| {
            if this.version != version {
                return;
            }
            this.pending.remove(&chunk);
            if let Ok(rows) = rows {
                if this.chunks.len() > 256 {
                    this.chunks.clear();
                }
                this.chunks.insert(chunk, Arc::new(rows));
            }
            if this.pending.is_empty() {
                this.stale.clear();
            }
            cx.notify();
        })
        .detach();
    }

    fn select(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.row(row).map(|r| r.id.clone()) else { return };
        window.focus(&self.focus, cx);
        cx.emit(GraphEvent::Select(row, id));
    }

    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.selected else { return };
        let page = self.visible.get().saturating_sub(2).max(1) as isize;
        let step = match e.keystroke.key.as_str() {
            "down" => 1,
            "up" => -1,
            "pagedown" => page,
            "pageup" => -page,
            _ => return,
        };
        let next = (current as isize + step).clamp(0, self.count as isize - 1) as usize;
        self.select(next, window, cx);
        self.scroll_to_row(next, false);
    }
}

/// 一行的泳道：竖直方向出入、中间平滑过渡
fn paint_lanes(row: &Row, head: bool, max_lanes: usize, p: &Palette, origin: Point<Pixels>, window: &mut Window) {
    let h = f32::from(ROW);
    let x = |lane: u32| PAD + lane as f32 * LANE_W + LANE_W / 2.;
    let at = |x: f32, y: f32| origin + point(px(x), px(y));
    let color = |lane: u32| p.lanes[lane as usize % p.lanes.len()];
    let link = |window: &mut Window, lane: u32, x0: f32, y0: f32, x1: f32, y1: f32| {
        let mut b = PathBuilder::stroke(px(1.6));
        b.move_to(at(x0, y0));
        b.cubic_bezier_to(at(x1, y1), at(x0, (y0 + y1) / 2.), at(x1, (y0 + y1) / 2.));
        if let Ok(path) = b.build() {
            window.paint_path(path, color(lane));
        }
    };
    let cap = max_lanes as u32;
    let (cx, cy) = (x(row.lane), h / 2.);
    for &l in row.through.iter().filter(|&&l| l < cap) {
        link(window, l, x(l), 0., x(l), h);
    }
    for &l in row.incoming.iter().filter(|&&l| l < cap) {
        link(window, l, x(l), 0., cx, cy);
    }
    for &l in row.out.iter().filter(|&&l| l < cap) {
        link(window, l, cx, cy, x(l), h);
    }
    if row.lane >= cap {
        return;
    }
    // 当前检出的提交画成空心
    let r = 4.;
    let ring = |b: &mut PathBuilder| {
        b.move_to(at(cx + r, cy));
        b.arc_to(point(px(r), px(r)), px(0.), false, false, at(cx - r, cy));
        b.arc_to(point(px(r), px(r)), px(0.), false, false, at(cx + r, cy));
        b.close();
    };
    let mut fill = PathBuilder::fill();
    ring(&mut fill);
    if let Ok(path) = fill.build() {
        window.paint_path(path, if head { p.bg } else { color(row.lane) });
    }
    let mut stroke = PathBuilder::stroke(px(2.));
    ring(&mut stroke);
    if let Ok(path) = stroke.build() {
        window.paint_path(path, color(row.lane));
    }
}

fn badge(r: &Ref, p: &Palette) -> impl IntoElement {
    let short = r.name.trim_start_matches("refs/heads/").trim_start_matches("refs/tags/").trim_start_matches("refs/remotes/").to_owned();
    let el = div().flex_none().mr(px(8.)).px(px(8.)).py(px(1.)).rounded(R_SM).text_size(FS_SM).border_1().text_color(p.text).max_w(px(240.)).truncate();
    if r.name.starts_with("refs/heads/") {
        // 本地分支：左边一条 3px 的强调色
        el.relative()
            .pl(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .bg(p.ref_bg)
            .border_color(mix(p.accent, p.border_strong, 0.55))
            .child(div().absolute().left(px(-1.)).top(px(-1.)).bottom(px(-1.)).w(px(3.)).rounded_l(R_SM).bg(p.accent))
            .child(short)
    } else if r.name.starts_with("refs/tags/") {
        el.font_weight(FontWeight::SEMIBOLD).border_color(alpha(p.yellow, 0.45)).child(short)
    } else {
        el.border_color(p.border_strong).child(short)
    }
}

impl Render for Graph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let l = layout(cx);
        let vw = f32::from(window.viewport_size().width);
        let author_w = px(l.author.min(vw * 0.18));
        let date_w = px(l.date.min(vw * 0.13));
        let graph_w = px(PAD * 2. + self.lanes.get() as f32 * LANE_W);
        let focus_author = self.focus_author.clone();

        let head = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .h(ROW)
            .pl(graph_w)
            .pr(px(24.))
            .text_size(FS_SM)
            .text_color(p.muted)
            .border_b_1()
            .border_color(p.border)
            .child(div().flex().flex_1().min_w_0().items_center().child(t.col_subject).when_some(focus_author.clone(), |s, (_, name)| {
                let color = author::author_color(&name, cx);
                s.child(div().ml(px(8.)).child(chip("focus-author", (t.only_author)(&name), color, cx.listener(|this, _, _, cx| {
                    this.focus_author = None;
                    cx.notify();
                }))))
            }))
            .child(div().relative().w(author_w).flex_none().child(div().absolute().left(px(-6.)).top_0().bottom_0().child(layout::splitter("split-author", Key::Author, true, 80., 360., true, cx))).child(t.col_author))
            .child(div().relative().w(date_w).flex_none().child(div().absolute().left(px(-6.)).top_0().bottom_0().child(layout::splitter("split-date", Key::Date, true, 96., 220., true, cx))).child(t.col_date))
            .child(div().w(px(72.)).flex_none().child(t.col_commit));

        let lanes = self.lanes.clone();
        let visible = self.visible.clone();
        let rows = cx.processor(move |this: &mut Graph, range: std::ops::Range<usize>, window: &mut Window, cx: &mut Context<Graph>| {
            for c in (range.start / CHUNK)..=(range.end.saturating_sub(1) / CHUNK) {
                this.fetch(c, cx);
            }
            visible.set(range.len());
            let p = palette(cx).clone();
            // 泳道列最多占提交图宽度的 28%，窄窗口下给说明列留地方
            let width = f32::from(window.viewport_size().width);
            let max_lanes = ((width * 0.28 - PAD * 2.) / LANE_W).floor().clamp(4., LANE_CAP as f32) as usize;
            let widest = range.clone().filter_map(|r| this.row(r)).map(|row| row.through.iter().chain(&row.out).chain(&row.incoming).copied().chain([row.lane]).max().unwrap_or(0)).max().unwrap_or(0);
            let want = (widest as usize + 1).min(max_lanes);
            if want != lanes.get() {
                lanes.set(want);
                cx.notify();
            }
            let graph_w = px(PAD * 2. + want as f32 * LANE_W);
            range
                .map(|r| {
                    let row = this.row(r).cloned();
                    let selected = this.selected == Some(r);
                    let dim = match (&this.focus_author, &row) {
                        (Some((key, _)), Some(row)) => author_key(&row.author, &row.author_email) != *key,
                        (Some(_), None) => true,
                        _ => false,
                    };
                    let mut el = div()
                        .id(("row", r))
                        .relative()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .h(ROW)
                        .pl(graph_w)
                        .pr(px(24.))
                        .hover(|s| s.bg(p.hover))
                        .when(selected, |s| s.bg(p.accent_soft).shadow(vec![BoxShadow { color: p.accent, offset: point(px(2.), px(0.)), blur_radius: px(0.), spread_radius: px(0.), inset: true }]))
                        .on_click(cx.listener(move |this, _, window, cx| this.select(r, window, cx)));
                    let Some(row) = row else { return el };
                    let id = row.id.clone();
                    el = el.on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                        this.select(r, window, cx);
                        cx.emit(GraphEvent::Menu(e.position, id.clone()));
                    }));
                    let head = this.head_id.as_deref() == Some(row.id.as_str());
                    let lanes_row = row.clone();
                    let pc = p.clone();
                    el = el.child(canvas(|_, _, _| {}, move |b, _, window, _| paint_lanes(&lanes_row, head, max_lanes, &pc, b.origin, window)).absolute().left_0().top_0().w(graph_w).h(ROW));
                    let opacity = if dim { 0.3 } else { 1. };
                    let badges = this.badges.get(&row.id);
                    let key = author_key(&row.author, &row.author_email);
                    let style = author::style(cx, &key);
                    let color = author::author_color(&row.author, cx);
                    let (name, email) = (row.author.clone(), row.author_email.clone());
                    let (k1, n1) = (key.clone(), name.clone());
                    el.child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .opacity(opacity)
                            .when_some(badges, |s, refs| s.child(div().flex().flex_shrink(1.).max_w(gpui::relative(0.5)).overflow_hidden().children(refs.iter().map(|r| badge(r, &p)))))
                            .child(div().flex_1().min_w_0().truncate().child(row.subject.clone())),
                    )
                    .child(
                        div().w(author_w).flex_none().min_w_0().flex().opacity(opacity).child(
                            div()
                                .id(("author", r))
                                .max_w_full()
                                .truncate()
                                .text_size(FS_SM)
                                .text_color(color)
                                .cursor_pointer()
                                .hover(|s| s.underline())
                                .when(style.border || style.fill, |s| s.px(px(8.)).rounded(R_LG).line_height(px(20.)).border_1().border_color(if style.border { color } else { alpha(color, 0.) }))
                                .when(style.fill, |s| s.bg(alpha(color, 0.18)))
                                .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.author_hint).build(window, cx))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.focus_author = if this.focus_author.as_ref().is_some_and(|(k, _)| *k == k1) { None } else { Some((k1.clone(), n1.clone())) };
                                    cx.notify();
                                }))
                                .on_mouse_down(MouseButton::Right, cx.listener({
                                    let (key, name, email) = (key.clone(), name.clone(), email.clone());
                                    move |_, _, _, cx| {
                                        cx.stop_propagation();
                                        cx.emit(GraphEvent::StyleAuthor { key: key.clone(), name: name.clone(), email: email.clone() });
                                    }
                                }))
                                // 键盘：Tab 聚焦到作者后按 Shift+F10 或菜单键，和右键一样打开作者样式
                                .focusable()
                                .on_key_down(cx.listener(move |_, e: &gpui::KeyDownEvent, _, cx| {
                                    let k = &e.keystroke;
                                    if (k.key == "f10" && k.modifiers.shift) || k.key == "menu" {
                                        cx.stop_propagation();
                                        cx.emit(GraphEvent::StyleAuthor { key: key.clone(), name: name.clone(), email: email.clone() });
                                    }
                                }))
                                .child(row.author.clone()),
                        ),
                    )
                    .child(div().w(date_w).flex_none().truncate().text_size(FS_SM).text_color(p.muted).opacity(opacity).child(fmt_time(cx, row.time)))
                    .child(div().w(px(72.)).flex_none().text_size(FS_SM).text_color(p.muted).font_family(MONO).opacity(opacity).child(row.id[..7.min(row.id.len())].to_owned()))
                })
                .collect::<Vec<_>>()
        });
        let list = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(uniform_list("graph", self.count, rows).track_scroll(&self.scroll).size_full())
            .vertical_scrollbar(&self.scroll);
        div()
            .id("graph-view")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| this.key(e, window, cx)))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(head)
            .child(list)
    }
}
