//! 24 网格的线性图标，只收用到的。gpui 按路径取图标，这里按名字现拼一个 SVG；
//! 别的路径交给组件库自带的图标。

use gpui::{svg, AssetSource, Hsla, IntoElement, Pixels, Result, SharedString, Styled};
use std::borrow::Cow;

macro_rules! icons {
    ($($name:ident $file:literal $path:literal,)*) => {
        #[derive(Clone, Copy, PartialEq, Debug)]
        pub enum Icon { $($name,)* }
        impl Icon {
            fn file(self) -> &'static str {
                match self { $(Icon::$name => concat!("pr/", $file, ".svg"),)* }
            }
        }
        fn path_of(file: &str) -> Option<&'static str> {
            match file { $(concat!("pr/", $file, ".svg") => Some($path),)* _ => None }
        }
    };
}

icons! {
    Undo "undo" "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
    Fetch "fetch" "M21 12a9 9 0 1 1-2.6-6.4M21 3v6h-6",
    Pull "pull" "M12 3v12m-5-5 5 5 5-5M5 21h14",
    Push "push" "M12 21V9m-5 5 5-5 5 5M5 3h14",
    Stash "stash" "M3 6h18v5H3zM5 11v9h14v-9M10 15h4",
    Branch "branch" "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9",
    Log "log" "m4 17 6-5-6-5M12 19h8",
    Help "help" "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01",
    Settings "settings" "M4 7h16M4 17h16M9 4v6M15 14v6",
    Plus "plus" "M12 5v14M5 12h14",
    Minus "minus" "M5 12h14",
    Close "close" "M18 6 6 18M6 6l12 12",
    Moon "moon" "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
    Sun "sun" "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
    Windows "windows" "M8 8h12v12H8zM4 16V4h12",
    Changes "changes" "M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z",
    History "history" "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l4 2",
    Folders "folders" "M3 7V5a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    Search "search" "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM21 21l-4.3-4.3",
    Sync "sync" "M21 12a9 9 0 0 0-15-6.7L3 8M3 3v5h5M3 12a9 9 0 0 0 15 6.7l3-2.7M21 21v-5h-5",
    Contrast "contrast" "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 2v20M12 7h7.5M12 12h10M12 17h7.5",
    Tools "tools" "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94z",
    Up "up" "m6 15 6-6 6 6",
    Down "down" "m6 9 6 6 6-6",
    Right "right" "m9 6 6 6-6 6",
    Left "left" "m15 6-6 6 6 6",
    Pin "pin" "M12 17v5M9 3h6l-1 6 3 3v2H7v-2l3-3z",
    Broom "broom" "M19 4 12 11M10 9l5 5-4 6H4l1-6zM7 14l5 5",
    Filter "filter" "M3 5h18l-7 8v6l-4 2v-8z",
    Space "space" "M4 10v4h16v-4",
    Pilcrow "pilcrow" "M13 4v16M17 4v16M19 4H9.5a4.5 4.5 0 0 0 0 9H13",
    Wrap "wrap" "M3 6h18M3 12h15a3 3 0 1 1 0 6h-4m2-2-2 2 2 2M3 18h7",
    ContextLess "context-less" "M3 5h18M3 19h18M8 12h8",
    ContextMore "context-more" "M3 5h18M3 19h18M8 12h8M12 8v8",
    WholeFile "whole-file" "M3 4h18M3 20h18M12 8v8M9 11l3-3 3 3M9 13l3 3 3-3",
    Split "split" "M3 4h18v16H3zM12 4v16",
    Tree "tree" "M21 12h-8M21 6H8M21 18h-8M3 6v4a2 2 0 0 0 2 2h3M3 10v6a2 2 0 0 0 2 2h3",
    Expand "expand" "m7 15 5 5 5-5M7 9l5-5 5 5",
    Collapse "collapse" "m7 20 5-5 5 5M7 4l5 5 5-5",
    // 文件列表里按类型区分的小图标：形状表示大类，颜色区分具体语言或格式（file_icons.rs）
    File "file" "M6 3h8l4 4v14H6zM14 3v4h4",
    Code "code" "m8 7-5 5 5 5M16 7l5 5-5 5",
    Doc "doc" "M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6",
    Braces "braces" "M9 4c-2 0-3 1-3 3v2c0 1-1 2-2 3 1 1 2 2 2 3v2c0 2 1 3 3 3M15 4c2 0 3 1 3 3v2c0 1 1 2 2 3-1 1-2 2-2 3v2c0 2-1 3-3 3",
    Table "table" "M4 5h16v14H4zM4 10h16M4 15h16M10 5v14",
    Image "image" "M4 5h16v14H4zM4 16l4-4 4 4 3-3 5 5M9 9h.01",
    Media "media" "M4 5h16v14H4zM10 9l5 3-5 3z",
    BoxIcon "box" "M4 8l8-4 8 4v8l-8 4-8-4zM4 8l8 4 8-4M12 12v8",
    Font "font" "M6 19 12 5l6 14M8.5 14h7",
    Terminal "terminal" "m5 8 4 4-4 4M12 17h7",
    Lock "lock" "M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z",
    Git "git" "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9",
    Roblox "roblox" "M7 4l13 3-3 13-13-3zM11 11l3 .7-.7 3-3-.7z",
    Folder "folder" "M3 7V5a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
}

/// 图标元素；颜色跟着传入的颜色走（相当于 CSS 的 currentColor）
pub fn icon(name: Icon, size: Pixels, color: Hsla) -> impl IntoElement {
    svg().path(name.file()).size(size).flex_none().text_color(color)
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path_of(path) {
            Some(d) => Ok(Some(Cow::Owned(
                format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="{d}"/></svg>"##)
                    .into_bytes(),
            ))),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit_assets::Assets.list(path)
    }
}
