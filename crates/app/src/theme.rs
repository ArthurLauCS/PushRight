//! 设计令牌（GitHub 风格 primer）。组件里的颜色、字号、间距、圆角都从这里取，不写裸值。
//! 间距 scale：4 8 16 24（细分 2、12）；字号 12 / 14 / 16 / 20 / 24。
//!
//! 中性色取 primer 的灰阶，功能色语义固定（绿 成功，橙 警告，红 危险，蓝 交互）；
//! 全应用唯一的强调色取应用图标的黄色（浅色 #8a5d00，深色 #f0c23a），只用于焦点、选中标记、主按钮和链接。
//! 字体按角色分四个字族（见下），Noto Serif SC 和霞鹜文楷随应用打包，找不到时退回系统字体。

use gpui::{px, rgb, App, BoxShadow, Global, Hsla, Pixels, Window, WindowAppearance};
use gpui_component::theme::{Theme, ThemeMode};

/// 四个字族各管一个区域：
/// UI 界面控件：Segoe UI Variable Text，中文由 DirectWrite 回退到微软雅黑 UI（只有 400 / 700 两档字重）；
/// DISPLAY 标题和名字（仓库名、分支名、页面和对话框标题、标签页名）：思源宋体，可变字重，600 / 700 都是真字重；
/// PROSE 人写的话（提交说明、操作说明、空状态）：霞鹜文楷；MONO 哈希、路径、diff、数字：Cascadia Code。
/// 视图代码只写这四个常量，不写字族名。
pub const UI: &str = "Segoe UI Variable Text";
pub const DISPLAY: &str = "Noto Serif SC";
pub const PROSE: &str = "LXGW WenKai";
pub const MONO: &str = "Cascadia Code";

pub const FS_SM: Pixels = px(12.);
pub const FS_MD: Pixels = px(14.);
pub const FS_LG: Pixels = px(16.);
pub const FS_XL: Pixels = px(20.);
pub const FS_2XL: Pixels = px(24.);
/// 段落行高；单行用 1.35
pub const LH_BODY: f32 = 1.6;

/// 小件（标签、芯片）4，盒子、按钮、输入框 6
pub const R_SM: Pixels = px(4.);
pub const R_MD: Pixels = px(6.);

pub const ROW: Pixels = px(28.);
/// 小号控件的高度：小按钮，以及和它们排在一行的输入框、下拉框（组件库的小号按字号算，只有 21px）
pub const CONTROL_SM: Pixels = px(24.);

#[derive(Clone)]
pub struct Palette {
    pub dark: bool,
    /// 底色三层：内容区 bg、面板 surface、浮起的控件 raised
    pub bg: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    /// 悬停底色：比 surface 再深一档，在 bg 和 surface 上都看得出
    pub hover: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    /// 强调色：焦点、选中标记、主按钮、链接。主按钮上的字用 on_accent
    pub accent: Hsla,
    pub on_accent: Hsla,
    /// 选中行的底色：强调色加透明度
    pub selection: Hsla,
    pub red: Hsla,
    pub orange: Hsla,
    pub green: Hsla,
    /// 蓝：冲突里的「我方」一侧
    pub blue: Hsla,
    pub on_danger: Hsla,
    /// diff 里的语法高亮
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
    /// 提交图泳道（连线），节点本身一律用 muted
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

/// 带透明度的十六进制颜色，写成 0xRRGGBBAA
fn ha(hex: u32) -> Hsla {
    gpui::rgba(hex).into()
}

/// 带透明度的颜色
pub fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla { a: color.a * a, ..color }
}

/// 浮在窗口上的东西（菜单、弹出层、对话框）的阴影；平铺的面板不用
pub fn overlay_shadow(p: &Palette) -> Vec<BoxShadow> {
    vec![BoxShadow { color: p.shadow, offset: gpui::point(px(0.), px(8.)), blur_radius: px(24.), spread_radius: px(0.), inset: false }]
}

/// 选中行左边 2px 的强调色条
pub fn select_bar(p: &Palette) -> Vec<BoxShadow> {
    vec![BoxShadow { color: p.accent, offset: gpui::point(px(2.), px(0.)), blur_radius: px(0.), spread_radius: px(0.), inset: true }]
}

impl Palette {
    pub fn dark() -> Palette {
        let green = h(0x3fb950);
        let red = h(0xf85149);
        Palette {
            dark: true,
            bg: h(0x0d1117),
            surface: h(0x151b23),
            raised: h(0x212830),
            hover: h(0x212830),
            border: h(0x3d444d),
            text: h(0xf0f6fc),
            muted: h(0x9198a1),
            accent: h(0xf0c23a),
            on_accent: h(0x1a1a1a),
            selection: ha(0xf0c23a4c),
            red,
            orange: h(0xdb6d28),
            green,
            blue: h(0x4493f8),
            on_danger: h(0x0d1117),
            syn_keyword: h(0xff7b72),
            syn_string: h(0xa5d6ff),
            syn_comment: h(0x9198a1),
            syn_number: h(0x79c0ff),
            syn_function: h(0xffa657),
            syn_type: h(0x7ee787),
            syn_variable: h(0xf0f6fc),
            syn_meta: h(0x79c0ff),
            diff_added: green,
            diff_deleted: red,
            lanes: [0x4493f8, 0x3fb950, 0xdb6d28, 0x39c5cf, 0x9198a1, 0xd29922, 0xdb61a2, 0x768390].map(h),
            authors: [0xe59a7f, 0x9cc281, 0xe3bd6e, 0xc1a6e3, 0x84b1dc, 0x6fc4b5, 0xe79abb, 0xc9b98a, 0x9fb0e8, 0xd6a36a].map(h),
            shadow: ha(0x01040966),
            backdrop: ha(0x01040999),
        }
    }

    pub fn light() -> Palette {
        let green = h(0x1a7f37);
        let red = h(0xcf222e);
        Palette {
            dark: false,
            bg: h(0xffffff),
            surface: h(0xf6f8fa),
            raised: h(0xeff2f5),
            hover: h(0xeff2f5),
            border: h(0xd1d9e0),
            text: h(0x1f2328),
            muted: h(0x59636e),
            accent: h(0x8a5d00),
            on_accent: h(0xffffff),
            selection: ha(0x8a5d002e),
            red,
            orange: h(0xbc4c00),
            green,
            blue: h(0x0969da),
            on_danger: h(0xffffff),
            syn_keyword: h(0xcf222e),
            syn_string: h(0x0a3069),
            syn_comment: h(0x59636e),
            syn_number: h(0x0550ae),
            syn_function: h(0x953800),
            syn_type: h(0x116329),
            syn_variable: h(0x1f2328),
            syn_meta: h(0x0550ae),
            diff_added: green,
            diff_deleted: red,
            lanes: [0x0969da, 0x1a7f37, 0xbc4c00, 0x0e7c86, 0x6e7781, 0x9a6700, 0xbf3989, 0x57606a].map(h),
            authors: [0x9a4522, 0x40682f, 0x77540a, 0x63469a, 0x2b5e93, 0x1c6c60, 0x973a66, 0x63552a, 0x4250a3, 0x84511a].map(h),
            shadow: ha(0x8c959f33),
            backdrop: ha(0x1f232852),
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

/// 换主题或差异配色后重建调色板，并让组件库的输入框、菜单、滚动条、勾选框、悬停提示和窗口标题栏用同一套颜色
pub fn apply(dark: bool, diff: Option<(Hsla, Hsla)>, mut window: Option<&mut Window>, cx: &mut App) {
    let mut p = if dark { Palette::dark() } else { Palette::light() };
    if let Some((added, deleted)) = diff {
        p.diff_added = added;
        p.diff_deleted = deleted;
    }
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, window.as_deref_mut(), cx);
    // 组件库的颜色存两份（colors 和勾选框、悬停提示读的 tokens），用 update 改 colors，tokens 跟着改
    Theme::update(cx, |theme| {
        theme.font_family = UI.into();
        theme.font_size = FS_MD;
        theme.mono_font_family = MONO.into();
        theme.mono_font_size = FS_SM;
        theme.radius = R_MD;
        theme.radius_lg = R_MD;
        // 滚动条常驻，长列表和长 diff 一眼看出能滚、滚到了哪
        theme.scrollbar_mode = gpui_component::scroll::ScrollbarMode::Always;
        let k = &mut theme.colors;
        k.background = p.bg;
        k.foreground = p.text;
        k.border = p.border;
        k.input = p.border;
        k.ring = p.accent;
        k.caret = p.text;
        k.selection = p.selection;
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
        // 下拉框的悬停和选中项：上面是正文色的字，用淡的那档
        k.accent = p.selection;
        k.accent_foreground = p.text;
        k.popover = p.bg;
        k.popover_foreground = p.text;
        k.list = p.surface;
        k.list_hover = p.hover;
        k.list_active = p.selection;
        k.list_active_border = p.accent;
        k.danger = p.red;
        k.danger_foreground = p.on_danger;
        k.scrollbar = alpha(p.bg, 0.);
        k.scrollbar_thumb = p.border;
        k.scrollbar_thumb_hover = p.muted;
        k.overlay = p.backdrop;
        k.window_border = p.border;
    });
    if let Some(window) = window {
        title_bar(dark, window);
    }
    cx.set_global(p);
}

/// 系统画的标题栏只按系统的深浅色走，这里改成跟应用主题一致
pub fn title_bar(dark: bool, window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let Ok(RawWindowHandle::Win32(handle)) = HasWindowHandle::window_handle(window).map(|h| h.as_raw()) else { return };
    let on = windows::core::BOOL::from(dark);
    // 设置失败只是标题栏颜色不对，不影响使用
    let _ = unsafe { DwmSetWindowAttribute(HWND(handle.hwnd.get() as _), DWMWA_USE_IMMERSIVE_DARK_MODE, &on as *const _ as _, std::mem::size_of_val(&on) as u32) };
}
