//! 版本输入框：可以直接输入提交号、版本表达式，也可以从分组的分支和标签里选。

use crate::i18n::t;
use crate::icons::Icon;
use crate::repo_view::RepoView;
use crate::rules::revision_options;
use crate::theme::{palette, FS_SM, R_MD, R_SM};
use crate::ui::btn;
use gpui::prelude::*;
use gpui::{anchored, canvas, deferred, div, px, Context, Entity, KeyDownEvent, SharedString, Subscription, WeakEntity, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use std::cell::Cell;
use std::rc::Rc;

pub struct RevisionPicker {
    input: Entity<InputState>,
    repo: WeakEntity<RepoView>,
    label: &'static str,
    open: bool,
    query: String,
    active: Option<usize>,
    width: Rc<Cell<f32>>,
    _subs: Vec<Subscription>,
}

impl RevisionPicker {
    pub fn new(repo: WeakEntity<RepoView>, label: &'static str, value: &str, window: &mut Window, cx: &mut Context<Self>) -> RevisionPicker {
        let placeholder = t(cx).tools.select_revision;
        let value = value.to_owned();
        let input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(placeholder);
            s.set_value(value, window, cx);
            s
        });
        let subs = vec![cx.subscribe(&input, |this, input, e: &InputEvent, cx| {
            match e {
                InputEvent::Change => {
                    this.query = input.read(cx).value().to_string();
                    this.active = None;
                    this.open = true;
                }
                InputEvent::Focus => this.show(),
                InputEvent::Blur => this.open = false,
                InputEvent::PressEnter { .. } => {}
            }
            cx.notify();
        })];
        RevisionPicker { input, repo, label, open: false, query: String::new(), active: None, width: Rc::new(Cell::new(240.)), _subs: subs }
    }

    pub fn value(&self, cx: &gpui::App) -> String {
        self.input.read(cx).value().trim().to_owned()
    }

    pub fn set_value(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| s.set_value(value, window, cx));
    }

    fn show(&mut self) {
        self.query.clear();
        self.active = None;
        self.open = true;
    }

    fn names(&self, cx: &gpui::App) -> Vec<String> {
        let Some(repo) = self.repo.upgrade() else { return Vec::new() };
        revision_options(&repo.read(cx).refs, &self.query).into_iter().flat_map(|(_, l)| l.into_iter().map(|o| o.name)).collect()
    }

    fn pick(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| {
            s.set_value(name, window, cx);
            s.focus(window, cx);
        });
        self.open = false;
        self.active = None;
        cx.notify();
    }

    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.names(cx).len();
        match e.keystroke.key.as_str() {
            "escape" if self.open => {
                self.open = false;
                cx.stop_propagation();
            }
            "down" | "up" => {
                cx.stop_propagation();
                if !self.open {
                    self.show();
                }
                if n > 0 {
                    self.active = Some(match (e.keystroke.key.as_str(), self.active) {
                        ("down", Some(i)) => (i + 1) % n,
                        ("down", None) => 0,
                        (_, Some(0)) | (_, None) => n - 1,
                        (_, Some(i)) => i - 1,
                    });
                }
            }
            "enter" if self.open => {
                if let Some(name) = self.active.and_then(|i| self.names(cx).into_iter().nth(i)) {
                    cx.stop_propagation();
                    return self.pick(name, window, cx);
                }
                self.open = false;
            }
            _ => return,
        }
        cx.notify();
    }
}

impl Render for RevisionPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let width = self.width.clone();
        let measure = canvas(move |b, _, _| width.set(f32::from(b.size.width)), |_, _, _, _| {}).absolute().size_full();
        let mut el = div()
            .relative()
            .flex_1()
            .min_w(px(200.))
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| this.key(e, window, cx)))
            .child(div().mb(px(4.)).text_color(p.muted).child(self.label))
            .child(
                div()
                    .relative()
                    .flex()
                    .gap(px(4.))
                    .child(measure)
                    .child(div().flex_1().min_w_0().child(Input::new(&self.input)))
                    .child(btn("picker-open").icon_size(Icon::Down, px(14.)).tip(t.tools.select_revision).on_click(cx.listener(|this, _, window, cx| {
                        if this.open {
                            this.open = false;
                        } else {
                            this.input.update(cx, |s, cx| s.focus(window, cx));
                            this.show();
                        }
                        cx.notify();
                    }))),
            );
        if self.open {
            let Some(repo) = self.repo.upgrade() else { return el };
            let groups = revision_options(&repo.read(cx).refs, &self.query);
            let names = [("current", t.tools.current_branches), ("common", t.tools.common_branches), ("branches", t.branches), ("remotes", t.remotes), ("tags", t.tags), ("other", t.tools.other_refs)];
            let heading = |text: &str| div().p(px(8.)).text_size(FS_SM).text_color(p.muted).child(text.to_owned());
            let mut list = div()
                .id("picker-list")
                .w(px(self.width.get()))
                .max_h(px(320.))
                .overflow_y_scroll()
                .p(px(4.))
                .bg(p.bg)
                .border_1()
                .border_color(p.border)
                .rounded(R_MD)
                .shadow(crate::theme::overlay_shadow(&p))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }));
            if groups.is_empty() {
                list = list.child(heading(t.tools.no_matching_refs));
            }
            let mut index = 0;
            for (key, items) in groups {
                list = list.child(heading(names.iter().find(|(k, _)| *k == key).map_or(key, |(_, n)| n)));
                for o in items {
                    let active = self.active == Some(index);
                    index += 1;
                    let name = o.name.clone();
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("pick-{}", o.name)))
                            .px(px(12.))
                            .py(px(6.))
                            .rounded(R_SM)
                            .when(active, |s| s.bg(p.accent).text_color(p.on_accent))
                            .hover(|s| s.bg(p.accent).text_color(p.on_accent))
                            .cursor_pointer()
                            .tooltip({
                                let tip: SharedString = o.name.clone().into();
                                move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                            })
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.pick(name.clone(), window, cx);
                            }))
                            .child(o.label.clone()),
                    );
                }
            }
            el = el.child(deferred(anchored().child(div().mt(px(4.)).child(list))).with_priority(1));
        }
        el
    }
}
