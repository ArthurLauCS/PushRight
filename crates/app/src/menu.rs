//! 右键菜单。给了 `items` 的是子菜单：点它换成子项，顶上多一行「返回」。整个窗口一个。

use crate::i18n::t;
use crate::icons::{icon, Icon};
use crate::theme::{palette, FS_SM, R_MD, R_SM};
use crate::ui::label;
use gpui::prelude::*;
use gpui::{anchored, deferred, div, px, Animation, AnimationExt, App, Context, FocusHandle, KeyDownEvent, Pixels, Point, Window};
use std::rc::Rc;
use std::time::Duration;

pub type Action = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub enum Item {
    /// 分隔线
    Sep,
    Entry { label: String, hint: Option<String>, action: Option<Action>, items: Option<Vec<Item>>, danger: bool, disabled: bool },
}

impl Item {
    pub fn new(label: impl Into<String>, action: impl Fn(&mut Window, &mut App) + 'static) -> Item {
        Item::Entry { label: label.into(), hint: None, action: Some(Rc::new(action)), items: None, danger: false, disabled: false }
    }
    pub fn group(label: impl Into<String>, items: Vec<Item>) -> Item {
        Item::Entry { label: label.into(), hint: None, action: None, items: Some(items), danger: false, disabled: false }
    }
    pub fn hint(mut self, h: impl Into<String>) -> Item {
        if let Item::Entry { hint, .. } = &mut self {
            *hint = Some(h.into());
        }
        self
    }
    pub fn danger(mut self) -> Item {
        if let Item::Entry { danger, .. } = &mut self {
            *danger = true;
        }
        self
    }
    pub fn disabled(mut self, d: bool) -> Item {
        if let Item::Entry { disabled, .. } = &mut self {
            *disabled = d;
        }
        self
    }
}

pub struct Menu {
    items: Vec<Item>,
    /// 进入子菜单前的各级菜单，「返回」时弹出
    parents: Vec<(String, Vec<Item>)>,
    at: Point<Pixels>,
    focus: FocusHandle,
    serial: usize,
}

impl Menu {
    pub fn new(cx: &mut Context<Self>) -> Menu {
        Menu { items: Vec::new(), parents: Vec::new(), at: Point::default(), focus: cx.focus_handle(), serial: 0 }
    }

    pub fn show(&mut self, at: Point<Pixels>, items: Vec<Item>, window: &mut Window, cx: &mut Context<Self>) {
        self.items = items;
        self.parents.clear();
        self.at = at;
        self.serial += 1;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.items.clear();
        self.parents.clear();
        cx.notify();
    }

    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Item::Entry { label, action, items: sub, .. }) = self.items.get(index).cloned() else { return };
        if let Some(sub) = sub {
            let parent = std::mem::replace(&mut self.items, sub);
            self.parents.push((label, parent));
            cx.notify();
        } else {
            self.close(cx);
            if let Some(action) = action {
                action(window, cx);
            }
        }
    }
}

impl Render for Menu {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.items.is_empty() {
            return div().into_any_element();
        }
        let p = palette(cx).clone();
        let t = t(cx);
        let mut list = div()
            .id(("menu", self.serial))
            .track_focus(&self.focus)
            .min_w(px(264.))
            .max_w(px(420.))
            .max_h(px(600.))
            .overflow_y_scroll()
            .p(px(4.))
            .border_1()
            .border_color(p.border)
            .rounded(R_MD)
            .bg(p.bg)
            .shadow(crate::theme::overlay_shadow(&p))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.close(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)));
        if let Some((title, _)) = self.parents.last() {
            list = list
                .child(
                    div()
                        .id("menu-back")
                        .group("menu-row")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(12.))
                        .py(px(4.))
                        .rounded(R_SM)
                        .text_size(FS_SM)
                        .text_color(p.muted)
                        .cursor_pointer()
                        .hover(|s| s.bg(p.raised).text_color(p.text))
                        // 图标不随文字变色，悬停时单独换成深色
                        .child(icon(Icon::Left, px(12.), p.muted).group_hover("menu-row", |s| s.text_color(p.text)))
                        .child(format!("{} · {}", t.menu_back, title))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some((_, parent)) = this.parents.pop() {
                                this.items = parent;
                            }
                            cx.notify();
                        })),
                )
                .child(div().my(px(4.)).h(px(1.)).bg(p.border));
        }
        for (i, item) in self.items.iter().enumerate() {
            list = match item {
                Item::Sep => list.child(div().my(px(4.)).h(px(1.)).bg(p.border)),
                Item::Entry { label: text, hint, items, danger, disabled, .. } => {
                    let (fg, hover_bg, hover_fg) = if *danger { (p.red, p.red, p.on_danger) } else { (p.text, p.raised, p.text) };
                    let mut row = div()
                        .id(("menu-item", i))
                        .w_full()
                        .px(px(12.))
                        .py(px(4.))
                        .rounded(R_SM)
                        .text_color(fg)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .child(div().flex_1().child(label(text, true)))
                                .when(items.is_some(), |s| s.child(div().opacity(0.7).child(icon(Icon::Right, px(12.), fg).group_hover("menu-row", |s| s.text_color(hover_fg))))),
                        )
                        .when_some(hint.clone(), |s, h| s.child(div().text_size(FS_SM).opacity(0.7).child(h)));
                    if *disabled {
                        row = row.opacity(0.45);
                    } else {
                        row = row.group("menu-row").cursor_pointer().hover(move |s| s.bg(hover_bg).text_color(hover_fg)).on_click(cx.listener(move |this, _, window, cx| this.pick(i, window, cx)));
                    }
                    list.child(row)
                }
            };
        }
        let list = list.with_animation(("menu-pop", self.serial), Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()), |el, d| el.opacity(d));
        deferred(anchored().position(self.at).snap_to_window_with_margin(px(4.)).child(list)).with_priority(2).into_any_element()
    }
}
