//! 「操作说明」：每个操作的白话说明和示意图，以及让勾过「不再显示」的说明重新出现。

use crate::explain::explainer;
use crate::i18n::{explain, t};
use crate::store;
use crate::theme::{palette, DISPLAY, FS_2XL, FS_MD, FS_XL, LH_BODY, PROSE, R_MD};
use crate::ui::{btn, label};
use gpui::prelude::*;
use gpui::{div, px, relative, Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton, Window};

pub struct Help {
    open: bool,
    reset: bool,
    focus: FocusHandle,
}

impl Help {
    pub fn new(cx: &mut Context<Self>) -> Help {
        Help { open: false, reset: false, focus: cx.focus_handle() }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.reset = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }
}

impl Render for Help {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let p = palette(cx).clone();
        let t = t(cx);
        let mut list = div().id("help-list").flex_1().min_h_0().overflow_y_scroll().px(px(24.)).pb(px(24.));
        let all = explain(cx).all();
        let n = all.len();
        for (i, e) in all.into_iter().enumerate() {
            list = list.child(
                div()
                    .py(px(24.))
                    .when(i + 1 < n, |s| s.border_b_1().border_color(p.border))
                    .child(div().mb(px(12.)).text_color(p.accent).font_family(DISPLAY).text_size(FS_XL).font_weight(FontWeight::SEMIBOLD).child(label(e.name, true)))
                    .child(explainer(e, cx)),
            );
        }
        let card = div()
            .id("help")
            .w(px(680.))
            .max_h(relative(0.86))
            .flex()
            .flex_col()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .rounded(R_MD)
            .shadow(crate::theme::overlay_shadow(&p))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .px(px(24.))
                    .pt(px(24.))
                    .pb(px(8.))
                    .child(div().flex_1().font_family(DISPLAY).text_size(FS_2XL).font_weight(FontWeight::SEMIBOLD).child(t.help))
                    .child(btn("help-reset").small().text(if self.reset { t.help_reset_done } else { t.help_reset }).disabled(self.reset).on_click(cx.listener(|this, _, _, cx| {
                        // 让所有勾过「下次不再显示」的说明重新出现
                        cx.global_mut::<store::Store>().remove_prefix("skip:");
                        this.reset = true;
                        cx.notify();
                    })))
                    .child(btn("help-close").small().text(t.close).on_click(cx.listener(|this, _, _, cx| this.close(cx)))),
            )
            .child(div().flex_none().px(px(24.)).pb(px(16.)).border_b_1().border_color(p.border).text_color(p.muted).font_family(PROSE).text_size(FS_MD).line_height(relative(LH_BODY)).child(t.help_legend))
            .child(list);
        div()
            .id("help-layer")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.backdrop)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.close(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
            .child(card)
            .into_any_element()
    }
}
