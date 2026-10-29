//! 各页面共用的小部件：按钮的几种样式（对应原来的 .btn / .primary / .danger / .quiet / .small / .icon / .on）、
//! 文案末尾的 git 命令、分隔线、可点掉的筛选条件。

use crate::icons::{icon, Icon};
use crate::theme::{alpha, palette, CONTROL_SM, FS_SM, MONO, R_MD};
use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, ClickEvent, ElementId, Hsla, Pixels, SharedString, Window};
use gpui_component::tooltip::Tooltip;

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// 文案末尾的「（git …）」拆出来单独排：名字是主角，命令退后；地方不够时由调用方关掉
pub fn label(text: &str, show_cmd: bool) -> AnyElement {
    match text.strip_suffix('）').and_then(|t| t.rfind("（git ").map(|at| (&t[..at], &t[at + "（".len()..]))) {
        Some((name, cmd)) => {
            let row = div().flex().items_baseline().min_w_0().child(name.to_owned());
            if show_cmd {
                row.child(div().ml(px(8.)).font_family(MONO).text_size(FS_SM).opacity(0.7).child(cmd.to_owned())).into_any_element()
            } else {
                row.into_any_element()
            }
        }
        None => div().child(text.to_owned()).into_any_element(),
    }
}

/// 去掉末尾 git 命令的名字：工具栏只放名字，命令在悬停提示和确认框里
pub fn name_only(text: &str) -> &str {
    text.strip_suffix('）').and_then(|t| t.rfind("（git ").map(|at| &t[..at])).unwrap_or(text)
}

#[derive(IntoElement)]
pub struct Btn {
    id: ElementId,
    text: Option<SharedString>,
    icon: Option<(Icon, Pixels)>,
    primary: bool,
    danger: bool,
    quiet: bool,
    small: bool,
    on: bool,
    disabled: bool,
    show_cmd: bool,
    tip: Option<SharedString>,
    height: Option<Pixels>,
    pad: Option<Pixels>,
    on_click: Option<Handler>,
}

pub fn btn(id: impl Into<ElementId>) -> Btn {
    Btn {
        id: id.into(),
        text: None,
        icon: None,
        primary: false,
        danger: false,
        quiet: false,
        small: false,
        on: false,
        disabled: false,
        show_cmd: true,
        tip: None,
        height: None,
        pad: None,
        on_click: None,
    }
}

impl Btn {
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some((icon, px(16.)));
        self
    }
    pub fn icon_size(mut self, icon: Icon, size: Pixels) -> Self {
        self.icon = Some((icon, size));
        self
    }
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
    pub fn danger_if(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// 不显示文案末尾的 git 命令
    pub fn no_cmd(mut self) -> Self {
        self.show_cmd = false;
        self
    }
    pub fn tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tip = Some(tip.into());
        self
    }
    pub fn h(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }
    /// 左右内边距，默认普通 12px、小号 8px
    pub fn px(mut self, pad: Pixels) -> Self {
        self.pad = Some(pad);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Btn {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = palette(cx).clone();
        let icon_only = self.text.is_none();
        let h = self.height.unwrap_or(if self.small { CONTROL_SM } else { px(28.) });
        // 停用的主按钮按普通按钮画，强调色只留给能点的按钮；字重不变，启用时宽度不跳
        let primary = self.primary && !self.disabled;
        let (bg, fg, border) = match (primary, self.danger, self.quiet, self.on) {
            (true, true, ..) => (p.red, p.on_danger, p.red),
            (true, false, ..) => (p.accent, p.on_accent, p.accent),
            (_, _, _, true) => (p.raised, p.text, p.muted),
            (_, _, true, _) => (alpha(p.bg, 0.), p.text, alpha(p.bg, 0.)),
            _ => (p.raised, p.text, p.border),
        };
        let mut el = div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .h(h)
            .border_1()
            .border_color(border)
            .rounded(R_MD)
            .bg(bg)
            .text_color(fg)
            .whitespace_nowrap();
        el = if icon_only { el.w(h) } else { el.px(self.pad.unwrap_or(if self.small { px(8.) } else { px(12.) })) };
        if self.small {
            el = el.text_size(FS_SM);
        }
        if self.primary {
            el = el.font_weight(gpui::FontWeight::BOLD);
        }
        if let Some((name, size)) = self.icon {
            el = el.child(icon(name, size, fg));
        }
        if let Some(text) = &self.text {
            el = el.child(label(text, self.show_cmd));
        }
        if let Some(tip) = self.tip {
            el = el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));
        }
        if self.disabled {
            return el.opacity(0.45);
        }
        let (danger, quiet, on) = (self.danger, self.quiet, self.on);
        el = el.cursor_pointer().hover(move |s| match (primary, danger, quiet, on) {
            // 主操作提亮一档；危险操作描红；无边框的只铺一层悬停色
            (true, ..) => s.opacity(0.92),
            (_, true, ..) => s.border_color(p.red).text_color(p.red),
            (_, _, _, true) => s,
            (_, _, true, _) => s.bg(p.hover),
            _ => s.border_color(p.border),
        });
        if let Some(f) = self.on_click {
            el = el.on_click(move |e, window, cx| {
                cx.stop_propagation();
                f(e, window, cx)
            });
        }
        el
    }
}

/// 竖分隔线（按钮组之间）
pub fn sep(cx: &App) -> impl IntoElement {
    div().w(px(1.)).h(px(16.)).mx(px(4.)).flex_none().bg(palette(cx).border)
}

/// 可点掉的筛选条件：提交图的作者聚焦、分支筛选
pub fn chip(id: impl Into<ElementId>, text: impl Into<SharedString>, color: Hsla, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .px(px(8.))
        .border_1()
        .border_color(color)
        .rounded(R_MD)
        .text_size(FS_SM)
        .text_color(color)
        .cursor_pointer()
        .child(text.into())
        .on_click(f)
}

/// 下拉框的一项：显示的文字和存下来的值分开
#[derive(Clone)]
pub struct Opt {
    pub value: SharedString,
    pub label: SharedString,
}

impl Opt {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Opt {
        Opt { value: value.into(), label: label.into() }
    }
}

impl gpui_component::searchable_list::SearchableListItem for Opt {
    type Value = SharedString;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &SharedString {
        &self.value
    }
}
