//! 各栏位的尺寸和拖动分隔条。尺寸存设置的 layout，所有页签共用一份。
//! 分隔条按下时登记一次拖动，最外层视图把鼠标移动交给它，松开时保存。

use crate::store;
use crate::theme::palette;
use gpui::prelude::*;
use gpui::{div, px, App, ElementId, Global, MouseButton, MouseMoveEvent, Pixels, Window};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// 侧栏宽度
    pub sidebar: f32,
    /// 提交详情面板高度
    pub detail: f32,
    /// 本地更改页左侧文件列表宽度
    pub changes: f32,
    /// 提交详情里文件列表宽度
    pub files: f32,
    /// 提交图的作者列、日期列宽度
    pub author: f32,
    pub date: f32,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout { sidebar: 264., detail: 320., changes: 400., files: 340., author: 150., date: 136. }
    }
}

impl Global for Layout {}

pub fn layout(cx: &App) -> Layout {
    *cx.global::<Layout>()
}

pub fn init(cx: &mut App) {
    let saved = store::get::<Layout>(cx, "layout").unwrap_or_default();
    cx.set_global(saved);
}

/// 正在拖的分隔条：起点和把位移换算成尺寸的方法
struct Drag {
    start: Pixels,
    horizontal: bool,
    apply: Rc<dyn Fn(f32, &mut App)>,
}

#[derive(Default)]
struct Dragging(Option<Drag>);

impl Global for Dragging {}

/// 拖到的位移交给 `apply`（像素，向右或向下为正）；松开时保存布局
pub fn start(position: gpui::Point<Pixels>, horizontal: bool, apply: impl Fn(f32, &mut App) + 'static, cx: &mut App) {
    let start = if horizontal { position.x } else { position.y };
    cx.set_global(Dragging(Some(Drag { start, horizontal, apply: Rc::new(apply) })));
}

/// 最外层视图的鼠标移动；拖动中返回 true
pub fn on_move(e: &MouseMoveEvent, window: &mut Window, cx: &mut App) {
    let Some(Dragging(Some(drag))) = cx.try_global::<Dragging>() else { return };
    if e.pressed_button != Some(MouseButton::Left) {
        return end(cx);
    }
    let delta = f32::from(if drag.horizontal { e.position.x } else { e.position.y } - drag.start);
    let apply = drag.apply.clone();
    apply(delta, cx);
    window.refresh();
}

pub fn end(cx: &mut App) {
    if cx.try_global::<Dragging>().is_some_and(|d| d.0.is_some()) {
        cx.set_global(Dragging(None));
        let l = layout(cx);
        store::set(cx, "layout", l);
    }
}

#[derive(Clone, Copy)]
pub enum Key {
    Sidebar,
    Detail,
    Changes,
    Files,
    Author,
    Date,
}

fn slot(l: &mut Layout, key: Key) -> &mut f32 {
    match key {
        Key::Sidebar => &mut l.sidebar,
        Key::Detail => &mut l.detail,
        Key::Changes => &mut l.changes,
        Key::Files => &mut l.files,
        Key::Author => &mut l.author,
        Key::Date => &mut l.date,
    }
}

/// 改布局里一项的分隔条。`invert`：被调整的栏位在分隔条的右侧或下方，拖动方向与尺寸变化相反。双击恢复默认
pub fn splitter(id: impl Into<ElementId>, key: Key, horizontal: bool, min: f32, max: f32, invert: bool, cx: &App) -> impl IntoElement {
    bar(
        id,
        horizontal,
        move |position, cx| {
            let from = *slot(&mut layout(cx), key);
            start(position, horizontal, move |delta, cx| {
                let v = (from + if invert { -delta } else { delta }).clamp(min, max).round();
                *slot(cx.global_mut::<Layout>(), key) = v;
            }, cx);
        },
        move |cx| {
            let mut l = layout(cx);
            *slot(&mut l, key) = *slot(&mut Layout::default(), key);
            cx.set_global(l);
            store::set(cx, "layout", l);
        },
        cx,
    )
}

/// 看得见的是 1px 的线，能抓住的是它两侧各 4px；悬停时线变成强调色
pub fn bar(
    id: impl Into<ElementId>,
    horizontal: bool,
    on_down: impl Fn(gpui::Point<Pixels>, &mut App) + 'static,
    on_reset: impl Fn(&mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let p = palette(cx);
    let accent = p.accent;
    let line = div()
        .absolute()
        .when(horizontal, |s| s.top_0().bottom_0().left(px(3.)).w(px(3.)))
        .when(!horizontal, |s| s.left_0().right_0().top(px(3.)).h(px(3.)))
        .hover(move |s| s.bg(accent));
    let grab = div()
        .id(id)
        .absolute()
        .when(horizontal, |s| s.top_0().bottom_0().left(px(-4.)).w(px(9.)).cursor_col_resize())
        .when(!horizontal, |s| s.left_0().right_0().top(px(-4.)).h(px(9.)).cursor_row_resize())
        .on_mouse_down(MouseButton::Left, move |e, _, cx| {
            cx.stop_propagation();
            if e.click_count == 2 {
                on_reset(cx);
            } else {
                on_down(e.position, cx);
            }
        })
        .child(line);
    div()
        .relative()
        .flex_none()
        .bg(p.border)
        .when(horizontal, |s| s.w(px(1.)))
        .when(!horizontal, |s| s.h(px(1.)))
        .child(grab)
}
