//! 变更文件按目录层级显示：文件夹在前，只有一个子目录的目录并成一行，折起来的文件夹不展开。
//! 还有文件列表上方的一排工具（筛选、全部展开 / 折叠、按文件夹显示）和按类型区分的文件图标。

use crate::diff_view::{prefs, set_pref};
use crate::i18n::t;
use crate::icons::{icon, Icon};
use crate::theme::{palette, CONTROL_SM, FS_SM, MONO};
use crate::ui::btn;
use gpui::prelude::*;
use gpui::{div, px, App, Entity, Pixels};
use gpui_component::input::{Input, InputState};
use gpui_component::Sizable;
use std::collections::{BTreeMap, HashSet};

pub struct TreeRow<T> {
    /// 文件是它的路径；文件夹是目录路径加 `/`，折叠状态按这个键记
    pub key: String,
    pub depth: usize,
    pub name: String,
    /// 文件行
    pub item: Option<T>,
    /// 文件夹行：其下所有文件，批量暂存 / 丢弃用
    pub items: Vec<T>,
    pub open: bool,
}

#[derive(Default)]
struct Dir<T> {
    dirs: BTreeMap<String, Dir<T>>,
    files: Vec<T>,
}

fn all<T: Clone>(dir: &Dir<T>) -> Vec<T> {
    dir.dirs.values().flat_map(all).chain(dir.files.iter().cloned()).collect()
}

/// 把路径列表摊成带深度的行
pub fn tree_rows<T: Clone>(items: &[T], path: impl Fn(&T) -> &str, closed: &HashSet<String>) -> Vec<TreeRow<T>> {
    let mut root = Dir { dirs: BTreeMap::new(), files: Vec::new() };
    // 未跟踪的嵌套仓库在状态里是以 / 结尾的一条
    let parts = |item: &T| path(item).trim_end_matches('/').split('/').map(str::to_owned).collect::<Vec<_>>();
    for item in items {
        let p = parts(item);
        let mut dir = &mut root;
        for part in &p[..p.len() - 1] {
            dir = dir.dirs.entry(part.clone()).or_insert_with(|| Dir { dirs: BTreeMap::new(), files: Vec::new() });
        }
        dir.files.push(item.clone());
    }
    let mut rows = Vec::new();
    fn walk<T: Clone>(dir: &Dir<T>, prefix: &str, depth: usize, rows: &mut Vec<TreeRow<T>>, closed: &HashSet<String>, parts: &dyn Fn(&T) -> Vec<String>) {
        for (name, child) in &dir.dirs {
            let mut name = name.clone();
            let mut child = child;
            while child.files.is_empty() && child.dirs.len() == 1 {
                let (next, only) = child.dirs.iter().next().unwrap();
                name = format!("{name}/{next}");
                child = only;
            }
            let key = format!("{prefix}{name}/");
            let open = !closed.contains(&key);
            rows.push(TreeRow { key: key.clone(), depth, name, item: None, items: all(child), open });
            if open {
                walk(child, &key, depth + 1, rows, closed, parts);
            }
        }
        for item in &dir.files {
            let p = parts(item);
            rows.push(TreeRow { key: p.join("/"), depth, name: p.last().unwrap().clone(), item: Some(item.clone()), items: Vec::new(), open: false });
        }
    }
    walk(&root, "", 0, &mut rows, closed, &parts);
    rows
}

/// 树里文件行的左缩进，和同层文件夹的名字对齐
pub fn indent(depth: usize) -> Pixels {
    px(28. + depth as f32 * 16.)
}

/// 文件列表上方的一排：按路径筛选、全部展开 / 折叠、树 / 平铺切换
pub fn file_list_bar(filter: &Entity<InputState>, on_fold: impl Fn(bool, &mut App) + Clone + 'static, cx: &App) -> impl IntoElement {
    let t = t(cx);
    let tree = prefs(cx).tree;
    let (f1, f2) = (on_fold.clone(), on_fold);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.))
        .px(px(8.))
        .py(px(4.))
        // 输入框自带的 h() 只管多行，单行用最小高度撑到和按钮一样高
        .child(div().flex_1().min_w_0().mr(px(4.)).child(Input::new(filter).small().min_h(CONTROL_SM).text_size(FS_SM).cleanable(true)))
        .child(btn("expand-all").small().quiet().icon_size(Icon::Expand, px(14.)).disabled(!tree).tip(t.file_list.expand_all).on_click(move |_, _, cx| f1(true, cx)))
        .child(btn("collapse-all").small().quiet().icon_size(Icon::Collapse, px(14.)).disabled(!tree).tip(t.file_list.collapse_all).on_click(move |_, _, cx| f2(false, cx)))
        .child(btn("tree").small().quiet().icon_size(Icon::Tree, px(14.)).on(tree).tip(t.file_list.tree).on_click(|_, _, cx| set_pref(cx, |p| p.tree = !p.tree)))
}

/// 新建筛选框
pub fn filter_input(window: &mut gpui::Window, cx: &mut App) -> Entity<InputState> {
    let placeholder = t(cx).file_list.filter;
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

/// 文件夹行的公共部分：箭头、文件夹图标、名字、文件数
pub fn folder_row(id: gpui::SharedString, row_key: &str, name: &str, count: usize, open: bool, depth: usize, compact: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let p = palette(cx);
    let _ = row_key;
    div()
        .id(id.clone())
        .group(id)
        .flex()
        .items_center()
        .gap(px(4.))
        .w_full()
        .h(if compact { px(24.) } else { crate::theme::ROW })
        .pl(px(12. + depth as f32 * 16.))
        .pr(px(8.))
        .text_color(p.muted)
        .when(compact, |s| s.text_size(FS_SM))
        .hover(|s| s.bg(p.hover).text_color(p.text))
        .child(icon(if open { Icon::Down } else { Icon::Right }, px(12.), p.muted))
        .child(icon(Icon::Folder, px(14.), p.muted))
        .child(div().min_w_0().truncate().font_family(MONO).child(name.to_owned()))
        .child(div().flex_1().ml(px(4.)).text_size(FS_SM).opacity(0.7).child(count.to_string()))
}

// ---------- 文件图标 ----------

/// 「形状 颜色: 扩展名…」；primer 里文件图标一律用 muted，颜色列只保留作识别用，不再画出来
const TABLE: &[(Icon, u32, &str)] = &[
    (Icon::Code, 0x00a2ff, "luau"),
    (Icon::Code, 0x7986cb, "lua"),
    (Icon::Roblox, 0x00a2ff, "rbxl rbxlx rbxm rbxmx"),
    (Icon::Code, 0x3b82c4, "ts mts cts tsx"),
    (Icon::Code, 0xd4a72c, "js mjs cjs jsx"),
    (Icon::Code, 0x4b8bbe, "py pyw"),
    (Icon::Code, 0xdea584, "rs"),
    (Icon::Code, 0x00add8, "go"),
    (Icon::Code, 0x659ad2, "c h"),
    (Icon::Code, 0xf34b7d, "cpp cc cxx hpp hh"),
    (Icon::Code, 0x9b4f96, "cs"),
    (Icon::Code, 0xe76f00, "java"),
    (Icon::Code, 0xa97bff, "kt kts"),
    (Icon::Code, 0xf05138, "swift"),
    (Icon::Code, 0xcc342d, "rb"),
    (Icon::Code, 0x8892bf, "php"),
    (Icon::Code, 0xff3e00, "svelte"),
    (Icon::Code, 0x41b883, "vue"),
    (Icon::Code, 0xe44d26, "html htm"),
    (Icon::Code, 0x8a63d2, "css"),
    (Icon::Code, 0xcf649a, "scss sass less"),
    (Icon::Code, 0x6e9f18, "glsl hlsl shader"),
    (Icon::Doc, 0x519aba, "md markdown mdx"),
    (Icon::Doc, 0, "txt log rst"),
    (Icon::Doc, 0xb7472a, "pdf doc docx ppt pptx"),
    (Icon::Braces, 0xcbcb41, "json jsonc json5"),
    (Icon::Braces, 0xe37933, "xml plist"),
    (Icon::Braces, 0x6d8086, "yml yaml"),
    (Icon::Settings, 0x9c8b6e, "toml"),
    (Icon::Settings, 0x6d8086, "ini cfg conf env editorconfig properties"),
    (Icon::Table, 0x4caf50, "csv tsv xls xlsx"),
    (Icon::Table, 0xe38c00, "sql sqlite db"),
    (Icon::Image, 0xa074c4, "png jpg jpeg gif webp bmp ico tga psd"),
    (Icon::Image, 0xffb13b, "svg"),
    (Icon::Media, 0xd16d9e, "mp3 wav ogg flac mp4 mov webm avi mkv"),
    (Icon::BoxIcon, 0xafb42b, "zip rar 7z gz tar tgz"),
    (Icon::BoxIcon, 0, "exe dll so dylib bin dat wasm"),
    (Icon::Font, 0xec5f67, "ttf otf woff woff2"),
    (Icon::Terminal, 0x89e051, "sh bash zsh fish"),
    (Icon::Terminal, 0x5391fe, "ps1 bat cmd"),
    (Icon::Lock, 0, "lock pem key crt"),
];

/// 没有扩展名或扩展名说明不了问题的文件，按整个文件名认
const BY_NAME: &[(&str, Icon, u32)] = &[
    (".gitignore", Icon::Git, 0xf05033),
    (".gitattributes", Icon::Git, 0xf05033),
    (".gitmodules", Icon::Git, 0xf05033),
    ("dockerfile", Icon::BoxIcon, 0x2496ed),
    ("makefile", Icon::Terminal, 0xe37933),
    ("license", Icon::Doc, 0xcbcb41),
];

pub fn file_icon_of(path: &str) -> (Icon, Option<u32>) {
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_lowercase();
    if let Some((_, i, c)) = BY_NAME.iter().find(|(n, _, _)| *n == name) {
        return (*i, Some(*c));
    }
    let ext = name.rsplit_once('.').map_or(name.as_str(), |(_, e)| e);
    TABLE.iter().find(|(_, _, exts)| exts.split(' ').any(|e| e == ext)).map_or((Icon::File, None), |(i, c, _)| (*i, (*c != 0).then_some(*c)))
}

pub fn file_icon(path: &str, size: Pixels, cx: &App) -> impl IntoElement {
    let (glyph, _) = file_icon_of(path);
    icon(glyph, size, palette(cx).muted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_first_and_chains_merged() {
        let items = ["src/lib/locales/en.ts", "src/lib/locales/zh.ts", "README.md", "src/main.ts"];
        let rows = tree_rows(&items, |s| s, &HashSet::new());
        let shape: Vec<_> = rows.iter().map(|r| (r.depth, r.name.as_str(), r.item.is_some())).collect();
        assert_eq!(shape, [(0, "src", false), (1, "lib/locales", false), (2, "en.ts", true), (2, "zh.ts", true), (1, "main.ts", true), (0, "README.md", true)]);
        assert_eq!(rows[0].items.len(), 3);
    }

    #[test]
    fn closed_folders_hide_children() {
        let items = ["a/x", "a/y", "b"];
        let closed = HashSet::from(["a/".to_owned()]);
        let rows = tree_rows(&items, |s| s, &closed);
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["a/", "b"]);
        assert!(!rows[0].open);
    }

    #[test]
    fn icons_by_name_and_extension() {
        assert_eq!(file_icon_of("x/.gitignore").0, Icon::Git);
        assert_eq!(file_icon_of("game/init.LUAU"), (Icon::Code, Some(0x00a2ff)));
        assert_eq!(file_icon_of("notes"), (Icon::File, None));
        assert_eq!(file_icon_of("a.txt"), (Icon::Doc, None));
    }
}
