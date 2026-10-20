//! 设计令牌。组件里的颜色、字号、间距、圆角都从这里取，不写裸值。
//! 间距 scale：2 4 8 12 16 24 32 48；字号音阶 1.2：10 12 14 17 21 25。
//!
//! 配色和气质参照 Claude 的公开界面：暖调的象牙白与炭黑、陶土色强调、衬线标题。
//! Claude 自用的字体不公开授权，这里用开源字体替代（思源宋体、霞鹜文楷）。

use gpui::{px, rgb, App, Global, Hsla, Pixels, Window, WindowAppearance};
use gpui_component::theme::{Theme, ThemeMode};

/// 四个字族各管一个区域：
/// UI 界面控件：无衬线，小字号下最清楚；DISPLAY 标题和名字：思源宋体，衬线带来书卷气；
/// PROSE 给人读的整段话（操作说明、提交说明、空状态）：霞鹜文楷，手写感，和控件区分开；MONO 代码、哈希、路径。
/// 中日韩字符由 DirectWrite 自动回退到系统字体（微软雅黑）。
pub const UI: &str = "Segoe UI Variable Text";
pub const DISPLAY: &str = "Noto Serif SC";
pub const PROSE: &str = "LXGW WenKai";
pub const MONO: &str = "Cascadia Code";

pub const FS_XS: Pixels = px(10.);
pub const FS_SM: Pixels = px(12.);
pub const FS_MD: Pixels = px(14.);
pub const FS_LG: Pixels = px(17.);
pub const FS_XL: Pixels = px(21.);
pub const FS_2XL: Pixels = px(25.);
/// 正文行高（--lh-body）
pub const LH_BODY: f32 = 1.7;

pub const R_SM: Pixels = px(6.);
pub const R_MD: Pixels = px(8.);
pub const R_LG: Pixels = px(14.);

pub const ROW: Pixels = px(28.);

#[derive(Clone)]
pub struct Palette {
    pub dark: bool,
    /// 底色由深到浅三层：内容区、面板、浮起的控件。全部带一点暖调
    pub bg: Hsla,
    pub panel: Hsla,
    pub raised: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub hover: Hsla,
    /// 陶土色：全应用唯一的强调色
    pub accent: Hsla,
    pub accent_soft: Hsla,
    /// 提交图里本地分支标签的底色，比选中行的 accent_soft 重一档
    pub ref_bg: Hsla,
    pub on_accent: Hsla,
    pub green: Hsla,
    pub yellow: Hsla,
    pub red: Hsla,
    pub on_danger: Hsla,
    /// diff 里的语法高亮，取 VS Code 默认主题的配色
    pub syn_keyword: Hsla,
    pub syn_string: Hsla,
    pub syn_comment: Hsla,
    pub syn_number: Hsla,
    pub syn_function: Hsla,
    pub syn_type: Hsla,
    pub syn_variable: Hsla,
    pub syn_meta: Hsla,
    /// 用户在「差异配色」里改过就是用户选的颜色
    pub diff_added: Hsla,
    pub diff_deleted: Hsla,
    /// 提交图泳道
    pub lanes: [Hsla; 8],
    /// 作者色：按名字哈希取一个，同一个人在任何仓库里颜色都一样
    pub authors: [Hsla; 10],
    pub shadow: Hsla,
    pub backdrop: Hsla,
}

impl Global for Palette {}

fn h(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// 带透明度的颜色，等同 CSS 的 color-mix(in srgb, 颜色 p%, transparent)
pub fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla { a: color.a * a, ..color }
}

/// 两色按比例混合，等同 CSS 的 color-mix(in srgb, a t%, b)
pub fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let l = |x: f32, y: f32| x * t + y * (1. - t);
    gpui::Rgba { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }.into()
}

impl Palette {
    pub fn dark() -> Palette {
        let accent = h(0xe08363);
        let green = h(0x91b876);
        let red = h(0xef7b88);
        Palette {
            dark: true,
            bg: h(0x1f1e1d),
            panel: h(0x262624),
            raised: h(0x30302e),
            border: h(0x3a3936),
            border_strong: h(0x5a5852),
            text: h(0xf0eee6),
            muted: h(0xa9a69c),
            hover: alpha(h(0xf0eee6), 0.06),
            accent,
            accent_soft: alpha(accent, 0.16),
            ref_bg: alpha(accent, 0.3),
            on_accent: h(0x1f1e1d),
            green,
            yellow: h(0xe0b35a),
            red,
            on_danger: h(0x1f1e1d),
            syn_keyword: h(0x569cd6),
            syn_string: h(0xce9178),
            syn_comment: h(0x6a9955),
            syn_number: h(0xb5cea8),
            syn_function: h(0xdcdcaa),
            syn_type: h(0x4ec9b0),
            syn_variable: h(0x9cdcfe),
            syn_meta: h(0xc586c0),
            diff_added: green,
            diff_deleted: red,
            lanes: [0xe08363, 0x91b876, 0xe0b35a, 0xb79ad9, 0x6a9bcc, 0x5fb8a8, 0xe08fb0, 0xa9a69c].map(h),
            authors: [0xe59a7f, 0x9cc281, 0xe3bd6e, 0xc1a6e3, 0x84b1dc, 0x6fc4b5, 0xe79abb, 0xc9b98a, 0x9fb0e8, 0xd6a36a].map(h),
            shadow: alpha(h(0x000000), 0.5),
            backdrop: alpha(h(0x141312), 0.55),
        }
    }

    pub fn light() -> Palette {
        let accent = h(0xa04423);
        let green = h(0x3f6b2f);
        let red = h(0xa82835);
        Palette {
            dark: false,
            bg: h(0xf4f2ea),
            panel: h(0xebe8de),
            raised: h(0xe0dccf),
            border: h(0xd4d0c2),
            border_strong: h(0xada999),
            text: h(0x1f1e1c),
            muted: h(0x5e5d57),
            hover: alpha(h(0x141413), 0.05),
            accent,
            accent_soft: alpha(accent, 0.12),
            ref_bg: alpha(accent, 0.12),
            on_accent: h(0xfaf9f5),
            green,
            yellow: h(0x7a5607),
            red,
            on_danger: h(0xfaf9f5),
            syn_keyword: h(0x0000ff),
            syn_string: h(0xa31515),
            syn_comment: h(0x008000),
            syn_number: h(0x098658),
            syn_function: h(0x795e26),
            syn_type: h(0x267f99),
            syn_variable: h(0x001080),
            syn_meta: h(0xaf00db),
            diff_added: green,
            diff_deleted: red,
            lanes: [0xb5532d, 0x5a8548, 0xa87a1c, 0x7b5bb0, 0x3f79b5, 0x2f8a7c, 0xb8527f, 0x76746c].map(h),
            authors: [0x9a4522, 0x40682f, 0x77540a, 0x63469a, 0x2b5e93, 0x1c6c60, 0x973a66, 0x63552a, 0x4250a3, 0x84511a].map(h),
            shadow: alpha(h(0x3c321e), 0.18),
            backdrop: alpha(h(0x28241c), 0.32),
        }
    }
}

pub fn palette(cx: &App) -> &Palette {
    cx.global::<Palette>()
}

/// 没手动选过主题时跟随系统
pub fn system_dark(window: &Window) -> bool {
    matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark)
}

/// 换主题或差异配色后重建调色板，并让组件库的输入框、菜单、滚动条用同一套颜色
pub fn apply(dark: bool, diff: Option<(Hsla, Hsla)>, window: Option<&mut Window>, cx: &mut App) {
    let mut p = if dark { Palette::dark() } else { Palette::light() };
    if let Some((added, deleted)) = diff {
        p.diff_added = added;
        p.diff_deleted = deleted;
    }
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, window, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = UI.into();
    theme.font_size = FS_MD;
    theme.mono_font_family = MONO.into();
    theme.mono_font_size = FS_SM;
    theme.radius = R_MD;
    theme.radius_lg = R_LG;
    // 滚动条常驻，长列表和长 diff 一眼看出能滚、滚到了哪
    theme.scrollbar_mode = gpui_component::scroll::ScrollbarMode::Always;
    let k = &mut theme.colors;
    k.background = p.bg;
    k.foreground = p.text;
    k.border = p.border;
    k.input = p.border;
    k.ring = p.accent;
    k.caret = p.text;
    k.selection = alpha(p.accent, 0.35);
    k.primary = p.accent;
    k.primary_hover = p.accent;
    k.primary_active = p.accent;
    k.primary_foreground = p.on_accent;
    k.secondary = p.raised;
    k.secondary_hover = p.raised;
    k.secondary_active = p.raised;
    k.secondary_foreground = p.text;
    k.muted = p.raised;
    k.muted_foreground = p.muted;
    k.accent = p.accent;
    k.accent_foreground = p.on_accent;
    k.popover = p.raised;
    k.popover_foreground = p.text;
    k.list = p.panel;
    k.list_hover = p.hover;
    k.list_active = p.accent_soft;
    k.list_active_border = p.accent;
    k.danger = p.red;
    k.danger_foreground = p.on_danger;
    k.scrollbar = alpha(p.bg, 0.);
    k.scrollbar_thumb = p.border;
    k.scrollbar_thumb_hover = p.border_strong;
    k.overlay = p.backdrop;
    k.window_border = p.border_strong;
    cx.set_global(p);
}
