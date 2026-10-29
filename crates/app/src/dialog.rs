//! 确认对话框：标题、白话说明和示意图、提示、警告、若干输入项。确认返回各项的值，取消返回 None。
//! 整个窗口一个，盖在所有内容上；调用方 `ask` 之后等结果。

use crate::explain::explainer;
use crate::i18n::{t, Explain};
use crate::store;
use crate::theme::{alpha, palette, DISPLAY, FS_MD, FS_SM, FS_XL, LH_BODY, MONO, R_MD};
use crate::ui::{btn, label, Opt};
use futures::channel::oneshot;
use gpui::prelude::*;
use gpui::{div, px, relative, AnyElement, Animation, AnimationExt, App, Context, Entity, FocusHandle, FontWeight, Hsla, KeyDownEvent, SharedString, Subscription, Window};
use gpui_component::checkbox::Checkbox;
use gpui_component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::radio::Radio;
use gpui_component::select::{SearchableVec, Select, SelectState};
use gpui_component::IndexPath;
use std::collections::HashMap;
use std::time::Duration;

pub enum Kind {
    /// `pattern`：文本必须匹配才能确认；规则说明放在 message 里
    Text(Option<fn(&str) -> bool>),
    Textarea,
    Checkbox,
    /// 没有默认值时必须选一个才能确认
    Radio(Vec<(String, String)>),
    Select(Vec<(String, String)>),
    /// 取色器，值是六位 HEX；给的颜色作为色板
    Color(Vec<Hsla>),
}

pub struct Field {
    key: String,
    label: String,
    kind: Kind,
    text: String,
    checked: bool,
    /// 文本框是否允许留空
    optional: bool,
    /// 文本框显示成圆点（令牌、密码）
    masked: bool,
}

impl Field {
    fn new(key: impl Into<String>, label: impl Into<String>, kind: Kind) -> Field {
        Field { key: key.into(), label: label.into(), kind, text: String::new(), checked: false, optional: false, masked: false }
    }
    pub fn masked(mut self) -> Field {
        self.masked = true;
        self
    }
    pub fn text(key: impl Into<String>, label: impl Into<String>) -> Field {
        Field::new(key, label, Kind::Text(None))
    }
    pub fn textarea(key: impl Into<String>, label: impl Into<String>) -> Field {
        Field::new(key, label, Kind::Textarea)
    }
    pub fn checkbox(key: impl Into<String>, label: impl Into<String>, checked: bool) -> Field {
        Field { checked, ..Field::new(key, label, Kind::Checkbox) }
    }
    pub fn radio(key: impl Into<String>, label: impl Into<String>, options: Vec<(String, String)>) -> Field {
        Field::new(key, label, Kind::Radio(options))
    }
    pub fn select(key: impl Into<String>, label: impl Into<String>, options: Vec<(String, String)>) -> Field {
        Field::new(key, label, Kind::Select(options))
    }
    pub fn color(key: impl Into<String>, label: impl Into<String>, swatches: Vec<Hsla>) -> Field {
        Field::new(key, label, Kind::Color(swatches))
    }
    pub fn value(mut self, value: impl Into<String>) -> Field {
        self.text = value.into();
        self
    }
    pub fn optional(mut self) -> Field {
        self.optional = true;
        self
    }
    pub fn pattern(mut self, test: fn(&str) -> bool) -> Field {
        self.kind = Kind::Text(Some(test));
        self
    }
}

#[derive(Default)]
pub struct Spec {
    title: String,
    explain: Option<&'static Explain>,
    /// 给了这个键，对话框会多一个「下次不再显示」的勾选框，勾选后记在设置的 skip:<键>
    skip_key: Option<&'static str>,
    message: Option<String>,
    /// 醒目的警告，显示为红色
    warning: Option<String>,
    confirm: Option<String>,
    danger: bool,
    fields: Vec<Field>,
    /// 按当前各项的值画一块预览，改动时立即更新
    preview: Option<fn(&Values, &App) -> AnyElement>,
}

impl Spec {
    pub fn new(title: impl Into<String>) -> Spec {
        Spec { title: title.into(), ..Default::default() }
    }
    pub fn explain(mut self, e: &'static Explain) -> Spec {
        self.explain = Some(e);
        self
    }
    pub fn skip_key(mut self, key: &'static str) -> Spec {
        self.skip_key = Some(key);
        self
    }
    pub fn message(mut self, m: impl Into<String>) -> Spec {
        self.message = Some(m.into());
        self
    }
    pub fn warning(mut self, w: impl Into<String>) -> Spec {
        self.warning = Some(w.into());
        self
    }
    pub fn maybe_warning(mut self, w: Option<String>) -> Spec {
        self.warning = w;
        self
    }
    pub fn confirm(mut self, c: impl Into<String>) -> Spec {
        self.confirm = Some(c.into());
        self
    }
    pub fn danger(mut self) -> Spec {
        self.danger = true;
        self
    }
    pub fn preview(mut self, f: fn(&Values, &App) -> AnyElement) -> Spec {
        self.preview = Some(f);
        self
    }
    pub fn field(mut self, f: Field) -> Spec {
        self.fields.push(f);
        self
    }
    pub fn fields(mut self, fs: impl IntoIterator<Item = Field>) -> Spec {
        self.fields.extend(fs);
        self
    }
}

/// 确认时各项的值：文本框、单选、下拉是字符串，勾选框是布尔
#[derive(Default, Debug)]
pub struct Values(HashMap<String, (String, bool)>);

impl Values {
    pub fn text(&self, key: &str) -> String {
        self.0.get(key).map(|v| v.0.clone()).unwrap_or_default()
    }
    pub fn bool(&self, key: &str) -> bool {
        self.0.get(key).is_some_and(|v| v.1)
    }
}

enum Control {
    Input(Entity<InputState>),
    Area(Entity<TextareaState>),
    Check(bool),
    Choice(Option<usize>),
    /// `Vec` 不实现搜索，选项多时显示的搜索框不起作用，所以用 `SearchableVec`
    Select(Entity<SelectState<SearchableVec<Opt>>>),
    Color(Entity<ColorPickerState>),
}

struct Open {
    spec: Spec,
    controls: Vec<Control>,
    skip: bool,
    reply: oneshot::Sender<Option<Values>>,
    /// 每次打开换一个，弹出动画才会重新播放
    serial: usize,
    _subs: Vec<Subscription>,
}

pub struct Dialog {
    open: Option<Open>,
    focus: FocusHandle,
    serial: usize,
}

impl Dialog {
    pub fn new(cx: &mut Context<Self>) -> Dialog {
        Dialog { open: None, focus: cx.focus_handle(), serial: 0 }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// 弹出对话框；确认返回各字段的值，取消返回 None。焦点放到第一个输入框；没有输入框时回车即确认，
    /// 危险操作除外（回车不确认）
    pub fn ask(&mut self, spec: Spec, window: &mut Window, cx: &mut Context<Self>) -> oneshot::Receiver<Option<Values>> {
        if let Some(prev) = self.open.take() {
            let _ = prev.reply.send(None);
        }
        let (tx, rx) = oneshot::channel();
        let mut subs = Vec::new();
        let mut first_input = None;
        let controls = spec
            .fields
            .iter()
            .map(|f| match &f.kind {
                Kind::Text(_) => {
                    let value = f.text.clone();
                    let state = cx.new(|cx| {
                        let mut s = InputState::new(window, cx).masked(f.masked);
                        s.set_value(value, window, cx);
                        s
                    });
                    subs.push(cx.subscribe_in(&state, window, |this, _, e: &InputEvent, window, cx| match e {
                        InputEvent::PressEnter { .. } => this.close(true, window, cx),
                        InputEvent::Change => cx.notify(),
                        _ => {}
                    }));
                    if first_input.is_none() {
                        first_input = Some(state.clone());
                    }
                    Control::Input(state)
                }
                Kind::Textarea => {
                    let value = f.text.clone();
                    let state = cx.new(|cx| {
                        let mut s = TextareaState::new(window, cx).rows(4);
                        s.set_value(value, window, cx);
                        s
                    });
                    subs.push(cx.subscribe(&state, |_, _, _: &InputEvent, cx| cx.notify()));
                    Control::Area(state)
                }
                Kind::Checkbox => Control::Check(f.checked),
                Kind::Radio(options) => Control::Choice(options.iter().position(|(v, _)| *v == f.text)),
                Kind::Select(options) => {
                    let opts: Vec<Opt> = options.iter().map(|(v, l)| Opt::new(v.clone(), l.clone())).collect();
                    let selected = options.iter().position(|(v, _)| *v == f.text).unwrap_or(0);
                    let state = cx.new(|cx| SelectState::new(SearchableVec::new(opts), Some(IndexPath::new(selected)), window, cx).searchable(options.len() > 12));
                    subs.push(cx.observe(&state, |_, _, cx| cx.notify()));
                    Control::Select(state)
                }
                Kind::Color(_) => {
                    let value = crate::diff_colors::hex(&f.text);
                    let state = cx.new(|cx| {
                        let s = ColorPickerState::new(window, cx);
                        match value {
                            Some(v) => s.default_value(v),
                            None => s,
                        }
                    });
                    subs.push(cx.subscribe(&state, |_, _, _: &ColorPickerEvent, cx| cx.notify()));
                    Control::Color(state)
                }
            })
            .collect();
        self.serial += 1;
        self.open = Some(Open { spec, controls, skip: false, reply: tx, serial: self.serial, _subs: subs });
        match first_input {
            Some(input) => input.update(cx, |s, cx| s.focus(window, cx)),
            None => window.focus(&self.focus, cx),
        }
        cx.notify();
        rx
    }

    fn ready(&self, cx: &gpui::App) -> bool {
        let Some(open) = &self.open else { return false };
        open.spec.fields.iter().zip(&open.controls).all(|(f, c)| match c {
            Control::Check(_) | Control::Select(_) | Control::Color(_) => true,
            Control::Choice(choice) => choice.is_some(),
            Control::Input(state) => {
                let v = state.read(cx).value();
                f.optional || (!v.is_empty() && !matches!(f.kind, Kind::Text(Some(test)) if !test(&v)))
            }
            Control::Area(state) => f.optional || !state.read(cx).value().is_empty(),
        })
    }

    /// 各项当前的值
    fn values(&self, cx: &App) -> Values {
        let mut values = Values::default();
        let Some(open) = &self.open else { return values };
        for (f, c) in open.spec.fields.iter().zip(&open.controls) {
            let v = match c {
                Control::Input(s) => (s.read(cx).value().to_string(), false),
                Control::Area(s) => (s.read(cx).value().to_string(), false),
                Control::Check(b) => (String::new(), *b),
                Control::Choice(i) => match &f.kind {
                    Kind::Radio(options) => (i.map(|i| options[i].0.clone()).unwrap_or_default(), false),
                    _ => unreachable!(),
                },
                Control::Select(s) => (s.read(cx).selected_value().map(|v| v.to_string()).unwrap_or_default(), false),
                Control::Color(s) => (s.read(cx).value().map(crate::diff_colors::to_hex).unwrap_or_default(), false),
            };
            values.0.insert(f.key.clone(), v);
        }
        values
    }

    fn close(&mut self, ok: bool, window: &mut Window, cx: &mut Context<Self>) {
        if ok && !self.ready(cx) {
            return;
        }
        let values = ok.then(|| self.values(cx));
        let Some(open) = self.open.take() else { return };
        if ok && open.skip {
            if let Some(key) = open.spec.skip_key {
                store::set(cx, &format!("skip:{key}"), "1");
            }
        }
        let _ = open.reply.send(values);
        window.blur(cx);
        cx.notify();
    }
}

impl Render for Dialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.open else { return div().into_any_element() };
        let p = palette(cx).clone();
        let t = t(cx);
        let ready = self.ready(cx);
        let spec = &open.spec;
        let entity = cx.entity();
        let mut body = div().flex().flex_col().gap(px(16.)).p(px(24.)).pb(px(12.));
        body = body.child(div().font_family(DISPLAY).text_size(FS_XL).font_weight(FontWeight::SEMIBOLD).line_height(relative(1.3)).child(label(&spec.title, true)));
        if let Some(e) = spec.explain {
            body = body.child(explainer(e, cx));
        }
        let prose = |text: &str| div().text_color(p.muted).line_height(relative(LH_BODY)).child(text.to_owned());
        if let Some(m) = &spec.message {
            body = body.child(prose(m));
        }
        if let Some(w) = &spec.warning {
            body = body.child(div().px(px(12.)).py(px(8.)).rounded(R_MD).text_color(p.red).bg(alpha(p.red, 0.12)).line_height(relative(LH_BODY)).child(w.clone()));
        }
        for (i, (f, c)) in spec.fields.iter().zip(&open.controls).enumerate() {
            let caption = || div().text_size(FS_SM).text_color(p.muted).child(f.label.clone());
            body = body.child(match c {
                Control::Input(state) => {
                    let invalid = matches!(f.kind, Kind::Text(Some(test)) if { let v = state.read(cx).value(); !v.is_empty() && !test(&v) });
                    let input = Input::new(state).text_size(FS_MD).when(invalid, |s| s.border_color(p.red));
                    div().flex().flex_col().gap(px(4.)).child(caption()).child(input).into_any_element()
                }
                Control::Area(state) => div().flex().flex_col().gap(px(4.)).child(caption()).child(Textarea::new(state).font_family(MONO)).into_any_element(),
                Control::Check(checked) => Checkbox::new(("dlg-check", i))
                    .checked(*checked)
                    .label(SharedString::from(f.label.clone()))
                    .on_click({
                        let entity = entity.clone();
                        move |v, _, cx| {
                            entity.update(cx, |d, cx| {
                                if let Some(Control::Check(b)) = d.open.as_mut().map(|o| &mut o.controls[i]) {
                                    *b = *v;
                                }
                                cx.notify();
                            })
                        }
                    })
                    .into_any_element(),
                Control::Choice(choice) => {
                    let Kind::Radio(options) = &f.kind else { unreachable!() };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(caption().pb(px(8.)))
                        .children(options.iter().enumerate().map(|(j, (_, text))| {
                            let entity = entity.clone();
                            Radio::new(("dlg-radio", i * 100 + j)).checked(*choice == Some(j)).label(SharedString::from(text.clone())).on_click(move |_, _, cx| {
                                entity.update(cx, |d, cx| {
                                    if let Some(Control::Choice(c)) = d.open.as_mut().map(|o| &mut o.controls[i]) {
                                        *c = Some(j);
                                    }
                                    cx.notify();
                                })
                            })
                        }))
                        .into_any_element()
                }
                Control::Select(state) => div().flex().flex_col().gap(px(4.)).child(caption()).child(Select::new(state)).into_any_element(),
                Control::Color(state) => {
                    let Kind::Color(swatches) = &f.kind else { unreachable!() };
                    div().flex().items_center().gap(px(12.)).child(ColorPicker::new(state).featured_colors(swatches.clone())).child(f.label.clone()).into_any_element()
                }
            });
        }
        if let Some(preview) = spec.preview {
            body = body.child(preview(&self.values(cx), cx));
        }
        if spec.skip_key.is_some() {
            let entity = entity.clone();
            body = body.child(
                div().text_size(FS_SM).text_color(p.muted).child(Checkbox::new("dlg-skip").checked(open.skip).label(t.dont_show_again).on_click(move |v, _, cx| {
                    entity.update(cx, |d, cx| {
                        if let Some(o) = d.open.as_mut() {
                            o.skip = *v;
                        }
                        cx.notify();
                    })
                })),
            );
        }
        // 窗口矮、内容要滚动时按钮钉在底部，不用滚到底才能确认
        let buttons = div()
            .flex()
            .flex_none()
            .justify_end()
            .gap(px(8.))
            .px(px(24.))
            .pt(px(12.))
            .pb(px(24.))
            .child(btn("dlg-cancel").text(t.cancel).h(px(32.)).on_click(cx.listener(|this, _, window, cx| this.close(false, window, cx))))
            .child(
                btn("dlg-ok")
                    .primary()
                    .danger_if(spec.danger)
                    .h(px(32.))
                    .text(spec.confirm.clone().unwrap_or_else(|| t.ok.to_owned()))
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, window, cx| this.close(true, window, cx))),
            );
        let danger = spec.danger;
        let card = div()
            .id(("dialog", open.serial))
            .w(px(560.))
            .max_h(relative(0.9))
            .flex()
            .flex_col()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .rounded(R_MD)
            .shadow(crate::theme::overlay_shadow(&p))
            .child(div().id("dialog-body").flex_1().min_h_0().overflow_y_scroll().child(body))
            .child(buttons)
            .with_animation(("pop", open.serial), Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()), |el, d| el.opacity(d).mt(px(8. * (1. - d))));
        let _ = window;
        div()
            .id("dialog-layer")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.backdrop)
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| match e.keystroke.key.as_str() {
                "escape" => {
                    this.close(false, window, cx);
                    cx.stop_propagation();
                }
                "enter" if !danger && this.focus.is_focused(window) => this.close(true, window, cx),
                _ => {}
            }))
            .child(card)
            .into_any_element()
    }
}

/// 设置里记过「不再显示」的说明，直接执行不再确认
pub fn skipped(cx: &gpui::App, key: &str) -> bool {
    store::get::<String>(cx, &format!("skip:{key}")).is_some()
}
