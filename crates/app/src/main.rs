// 发布版不弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod author;
mod commit_search;
mod conflict_view;
mod credential;
mod detail;
mod diff_view;
mod file_tree;
mod help;
mod requests;
mod highlight;
mod revision_picker;
mod rules;
mod sidebar;
mod tools;
mod working_copy;
mod worktrees;
mod dialog;
mod explain;
mod graph;
mod layout;
mod menu;
mod diff_colors;
mod git;
mod i18n;
mod icons;
mod instance;
mod repo_view;
mod store;
mod theme;
mod time;
mod ui;

use gpui::{px, size, App, AppContext, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions};
use std::borrow::Cow;
use std::path::PathBuf;

/// 两套中文字体：安装版在 exe 旁的 fonts 目录，开发时在 crates/app/assets/fonts（scripts/fonts.ps1 下载）。
/// 找不到时不报错，界面退回系统字体。
fn load_fonts(cx: &mut App) {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("fonts")));
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    let fonts: Vec<Cow<'static, [u8]>> = ["NotoSerifSC-VF.ttf", "LXGWWenKai-Regular.ttf"]
        .iter()
        .filter_map(|name| exe.iter().chain([&dev]).find_map(|dir| std::fs::read(dir.join(name)).ok()))
        .map(Cow::Owned)
        .collect();
    let _ = cx.text_system().add_fonts(fonts);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // 已经开着一个：把要打开的仓库交给它，自己退出
    if instance::forward(&args) {
        return;
    }
    let (tx, forwarded) = futures::channel::mpsc::unbounded();
    instance::serve(move |args| {
        let _ = tx.unbounded_send(args);
    });
    gpui_platform::application().with_assets(icons::Assets).run(move |cx: &mut App| {
        load_fonts(cx);
        gpui_component::init(cx);
        let store = store::Store::load();
        let locale = i18n::Locale::from_code(&store.get::<String>("locale").unwrap_or_default());
        let zone = time::Zone::new(&store.get::<String>("timeZone").unwrap_or_default());
        cx.set_global(store);
        cx.set_global(i18n::I18n::new(locale));
        cx.set_global(zone);
        layout::init(cx);
        author::init(cx);
        diff_view::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-tab", app::NextTab, None),
            KeyBinding::new("ctrl-shift-tab", app::PrevTab, None),
            KeyBinding::new("ctrl-w", app::CloseTab, None),
            KeyBinding::new("ctrl-t", app::NewTab, None),
        ]);
        let bounds = Bounds::centered(None, size(px(1440.), px(860.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(1024.), px(640.))),
            titlebar: Some(TitlebarOptions { title: Some("PushRight".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| app::PushRight::new(args, forwarded, window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("open window");
        cx.activate(true);
    });
}
