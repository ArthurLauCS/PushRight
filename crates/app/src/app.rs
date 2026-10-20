//! 窗口的最外层：顶上一排仓库页签和全局设置，下面是当前页签的仓库页或「打开仓库」页。

use crate::diff_colors::{DiffColors, PRESETS};
use crate::dialog::{Dialog, Field, Spec};
use crate::git::{self, Session};
use crate::help::Help;
use crate::menu::Menu;
use crate::repo_view::AutoSync;
use crate::i18n::{self, t, I18n, Locale};
use crate::icons::{icon, Icon};
use crate::repo_view::RepoView;
use crate::theme::{self, palette, DISPLAY, FS_SM, FS_XL, MONO, PROSE, R_MD, R_SM};
use crate::time::{self, Zone};
use crate::ui::{btn, Opt};
use crate::store;
use gpui::prelude::*;
use gpui::{actions, div, px, App, Context, Entity, FocusHandle, FontWeight, MouseButton, PathPromptOptions, SharedString, Subscription, Window};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::{IndexPath, Sizable};
use std::sync::Arc;

actions!(pushright, [NextTab, PrevTab, CloseTab, NewTab]);

struct Tab {
    id: u32,
    path: String,
    view: Entity<RepoView>,
}

pub struct PushRight {
    focus: FocusHandle,
    recent: Vec<String>,
    tabs: Vec<Tab>,
    /// 当前页签；None 时显示打开仓库页
    active: Option<u32>,
    next_id: u32,
    error: String,
    creating: bool,
    zones: Entity<SelectState<Vec<Opt>>>,
    languages: Entity<SelectState<Vec<Opt>>>,
    dialog: Entity<Dialog>,
    menu: Entity<Menu>,
    help: Entity<Help>,
    _subscriptions: Vec<Subscription>,
}

const RECENT: &str = "recent";

impl PushRight {
    pub fn new(args: Vec<String>, forwarded: futures::channel::mpsc::UnboundedReceiver<Vec<String>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        apply_theme(window, cx);
        // 没手动选过主题时跟随系统
        let follow = cx.observe_window_appearance(window, |_, window, cx| {
            if store::get::<String>(cx, "theme").is_none() {
                apply_theme(window, cx);
            }
        });

        let t = t(cx);
        let mut zone_opts = vec![Opt::new("", t.system_time_zone)];
        zone_opts.extend(time::zone_names().into_iter().map(|z| Opt::new(z.clone(), z.replace('_', " "))));
        let current = cx.global::<Zone>().name.clone();
        let selected = zone_opts.iter().position(|o| o.value.as_ref() == current).unwrap_or(0);
        let zones = cx.new(|cx| SelectState::new(zone_opts, Some(IndexPath::new(selected)), window, cx).searchable(true));
        let lang_opts = vec![Opt::new("en", "English"), Opt::new("zh-CN", "简体中文")];
        let selected = if cx.global::<I18n>().locale == Locale::Zh { 1 } else { 0 };
        let languages = cx.new(|cx| SelectState::new(lang_opts, Some(IndexPath::new(selected)), window, cx));
        let subscriptions = vec![
            follow,
            cx.subscribe(&zones, |_, _, e: &SelectEvent<Vec<Opt>>, cx| {
                let SelectEvent::Confirm(Some(name)) = e else { return };
                let zone = Zone::new(name);
                store::set(cx, "timeZone", &zone.name);
                cx.set_global(zone);
                cx.refresh_windows();
            }),
            cx.subscribe(&languages, |_, _, e: &SelectEvent<Vec<Opt>>, cx| {
                let SelectEvent::Confirm(Some(code)) = e else { return };
                let locale = Locale::from_code(code);
                store::set(cx, "locale", locale.code());
                cx.set_global(I18n::new(locale));
                cx.refresh_windows();
            }),
        ];

        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // 切回这个窗口时刷新当前页签
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                if let Some(tab) = this.active.and_then(|id| this.tabs.iter().find(|t| t.id == id)) {
                    tab.view.update(cx, |v, cx| {
                        if !v.busy {
                            let _ = v.refresh(false, cx);
                        }
                    });
                }
            }
        });
        let mut subscriptions = subscriptions;
        subscriptions.push(activation);
        let mut this = PushRight {
            dialog: cx.new(Dialog::new),
            menu: cx.new(Menu::new),
            help: cx.new(Help::new),
            focus,
            recent: store::get(cx, RECENT).unwrap_or_default(),
            tabs: Vec::new(),
            active: None,
            next_id: 0,
            error: String::new(),
            creating: false,
            zones,
            languages,
            _subscriptions: subscriptions,
        };
        // 恢复上次的页签，再打开命令行传入的仓库
        let saved: Vec<String> = store::get(cx, "tabs").unwrap_or_default();
        let (paths, reveal) = crate::instance::parse(&args);
        this.open_repos(saved.into_iter().chain(paths).collect(), reveal, window.window_handle(), cx);
        // 再次启动的桌面版（或 VS Code 扩展）把参数交给这个窗口：打开仓库、切到前台
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            use futures::StreamExt;
            let mut forwarded = forwarded;
            while let Some(args) = forwarded.next().await {
                let (paths, reveal) = crate::instance::parse(&args);
                let _ = this.update(cx, |this, cx| this.open_repos(paths, reveal, handle, cx));
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            }
        })
        .detach();
        this
    }

    pub fn open_repo(&mut self, path: String, window: gpui::AnyWindowHandle, cx: &mut Context<Self>) {
        self.open_repos(vec![path], None, window, cx);
    }

    /// 按顺序打开一批仓库（恢复页签时顺序不变）；已经开着的同一个工作区直接切过去。
    /// `reveal`：打开后在最后一个仓库里定位到这个提交
    fn open_repos(&mut self, paths: Vec<String>, reveal: Option<String>, window: gpui::AnyWindowHandle, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        self.error.clear();
        git::bg(cx, move || paths.iter().map(|p| Session::open(p)).collect::<Vec<_>>(), move |this, results, cx| {
            let n = results.len();
            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Err(e) => this.error = e,
                    Ok(session) => {
                        let view = this.add_tab(session, window, cx);
                        if let (Some(id), true) = (reveal.clone(), i + 1 == n) {
                            view.update(cx, |v, cx| v.reveal_later(id, cx));
                        }
                    }
                }
            }
            this.save_tabs(cx);
            cx.notify();
        })
        .detach();
    }

    fn add_tab(&mut self, session: Session, window: gpui::AnyWindowHandle, cx: &mut Context<Self>) -> Entity<RepoView> {
                let real = session.path();
                if let Some(tab) = self.tabs.iter().find(|t| t.path == real) {
                    self.active = Some(tab.id);
                    tab.view.clone()
                } else {
                    let id = self.next_id;
                    self.next_id += 1;
                    let session = Arc::new(session);
                    let opener = cx.entity().downgrade();
                    let handles = (self.dialog.clone(), self.menu.clone(), self.help.clone());
                    let view = window
                        .update(cx, |_, window, cx| cx.new(|cx| RepoView::new(session, opener, handles, window, cx)))
                        .expect("window");
                    self.tabs.push(Tab { id, path: real.clone(), view: view.clone() });
                    self.active = Some(id);
                    self.recent.retain(|p| *p != real);
                    self.recent.insert(0, real);
                    self.recent.truncate(10);
                    store::set(cx, RECENT, &self.recent);
                    view
                }
    }

    fn save_tabs(&self, cx: &mut App) {
        store::set(cx, "tabs", self.tabs.iter().map(|t| &t.path).collect::<Vec<_>>());
    }

    fn close(&mut self, id: u32, cx: &mut Context<Self>) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else { return };
        self.tabs.remove(i);
        if self.active == Some(id) {
            self.active = self.tabs.get(i).or_else(|| i.checked_sub(1).and_then(|j| self.tabs.get(j))).map(|t| t.id);
        }
        self.save_tabs(cx);
        cx.notify();
    }

    fn pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(t(cx).open_repo.into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| this.open_repo(path.display().to_string(), window.window_handle(), cx));
                }
            }
        })
        .detach();
    }

    /// 克隆或初始化仓库：先选父文件夹（初始化时就是仓库本身），再填地址和新文件夹名
    fn create(&mut self, clone: bool, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let title = if clone { t.tools.clone_parent } else { t.tools.init_repo };
        let paths = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(title.into()) });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(dir) = paths.into_iter().next().map(|p| p.display().to_string()) else { return };
            let Ok(rx) = this.update_in(cx, |this, window, cx| {
                let mut spec = Spec::new(if clone { t.tools.clone_repo } else { t.tools.init_repo }).message(dir.clone());
                if clone {
                    spec = spec.fields([Field::text("url", t.tools.url), Field::text("name", t.tools.folder_name)]);
                }
                this.dialog.update(cx, |d, cx| d.ask(spec, window, cx))
            }) else { return };
            let Ok(Some(v)) = rx.await else { return };
            let name = v.text("name");
            let bad = name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) || name == "." || name == "..";
            if clone && bad {
                let _ = this.update(cx, |this, cx| {
                    this.error = "PR_INVALID_NAME".into();
                    cx.notify();
                });
                return;
            }
            let target = if clone { format!("{dir}/{name}") } else { dir };
            let url = clone.then(|| v.text("url"));
            let _ = this.update_in(cx, |this, window, cx| {
                this.creating = true;
                this.error.clear();
                cx.notify();
                let w = window.window_handle();
                git::bg(cx, move || engine::tools::create(&target, url.as_deref()), move |this, r, cx| {
                    this.creating = false;
                    match r {
                        Ok(path) => {
                            this.open_repo(path, w, cx);
                        }
                        Err(e) => this.error = e,
                    }
                    cx.notify();
                })
                .detach();
            });
        })
        .detach();
    }

    fn auto_sync_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let current = store::get::<AutoSync>(cx, "autoSync").unwrap_or_default();
        let spec = Spec::new(t.auto_sync_settings).message(t.auto_sync_hint).fields([Field::checkbox("refresh", t.auto_refresh, current.refresh), Field::checkbox("fetch", t.auto_fetch, current.fetch)]);
        let rx = self.dialog.update(cx, |d, cx| d.ask(spec, window, cx));
        cx.spawn(async move |_, cx| {
            if let Ok(Some(v)) = rx.await {
                cx.update(|cx| store::set(cx, "autoSync", AutoSync { refresh: v.bool("refresh"), fetch: v.bool("fetch") }));
            }
        })
        .detach();
    }

    /// 差异配色：预设或自定义的两个 HEX 颜色；选「恢复原配色」回到主题自带的绿和红
    fn diff_colors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = t(cx);
        let current = store::get::<DiffColors>(cx, "diffColors");
        let preset = match &current {
            None => "default".to_owned(),
            Some(c) => PRESETS.iter().find(|(_, a, d)| *a == c.added && *d == c.deleted).map_or("custom".to_owned(), |(k, _, _)| (*k).to_owned()),
        };
        let names = [t.diff_colors.blue_orange, t.diff_colors.purple_gold, t.diff_colors.cyan_rose];
        let mut options = vec![("default".to_owned(), t.diff_colors.default.to_owned())];
        options.extend(PRESETS.iter().zip(names).map(|((k, _, _), n)| ((*k).to_owned(), n.to_owned())));
        options.push(("custom".to_owned(), t.diff_colors.custom.to_owned()));
        let c = current.unwrap_or(DiffColors { added: PRESETS[0].1.into(), deleted: PRESETS[0].2.into() });
        let spec = Spec::new(t.diff_colors.title).message(format!("{}\n{}", t.diff_colors.hint, t.diff_colors.invalid)).fields([
            Field::select("preset", t.diff_colors.preset, options).value(preset),
            Field::text("added", format!("{} HEX", t.diff_colors.added)).value(c.added).pattern(crate::diff_colors::is_diff_color),
            Field::text("deleted", format!("{} HEX", t.diff_colors.deleted)).value(c.deleted).pattern(crate::diff_colors::is_diff_color),
        ]);
        let rx = self.dialog.update(cx, |d, cx| d.ask(spec, window, cx));
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Some(v)) = rx.await else { return };
            let colors = match v.text("preset").as_str() {
                "default" => None,
                "custom" => Some(DiffColors { added: v.text("added").to_lowercase(), deleted: v.text("deleted").to_lowercase() }),
                key => PRESETS.iter().find(|(k, _, _)| *k == key).map(|(_, a, d)| DiffColors { added: (*a).into(), deleted: (*d).into() }),
            };
            let _ = cx.update(|window, cx| {
                store::set(cx, "diffColors", colors);
                apply_theme(window, cx);
                cx.refresh_windows();
            });
        })
        .detach();
    }

    fn cycle(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let n = self.tabs.len() as isize;
        let i = self.active.and_then(|id| self.tabs.iter().position(|t| t.id == id)).map_or(0, |i| i as isize);
        self.active = Some(self.tabs[((i + step).rem_euclid(n)) as usize].id);
        cx.notify();
    }

    fn render_tabs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let mut nav = div()
            .flex()
            .flex_none()
            .items_end()
            .gap(px(2.))
            .h(px(40.))
            .px(px(8.))
            .bg(p.bg)
            .border_b_1()
            .border_color(p.border);
        for tab in &self.tabs {
            let id = tab.id;
            let active = self.active == Some(id);
            let name = tab.path.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(&tab.path).to_owned();
            let branch = tab.view.read(cx).head();
            let group: SharedString = format!("tab-{id}").into();
            nav = nav.child(
                div()
                    .id(("tab", id))
                    .group(group.clone())
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .h(px(32.))
                    .min_w_0()
                    .max_w(px(320.))
                    // 左右留出一样宽的位置，关闭按钮浮在右边，仓库名和分支名在标签正中
                    .px(px(32.))
                    .mb(px(-1.))
                    .border_1()
                    .border_b_0()
                    .rounded_t(R_MD)
                    .font_family(DISPLAY)
                    .font_weight(FontWeight::BOLD)
                    .when(active, |s| s.bg(p.panel).border_color(p.border).text_color(p.text))
                    .when(!active, |s| s.border_color(theme::alpha(p.border, 0.)).text_color(p.muted).hover(|s| s.bg(p.hover).text_color(p.text)))
                    .tooltip({
                        let path: SharedString = tab.path.clone().into();
                        move |window, cx| gpui_component::tooltip::Tooltip::new(path.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = Some(id);
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, _, cx| this.close(id, cx)))
                    .child(div().min_w_0().truncate().child(name))
                    .when_some(branch, |s, b| s.child(icon(Icon::Branch, px(12.), p.muted)).child(div().min_w_0().truncate().text_color(if active { p.accent } else { p.muted }).child(b)))
                    .child(
                        div()
                            .id(("close", id))
                            .absolute()
                            .right(px(6.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(20.))
                            .rounded(R_SM)
                            .text_color(p.muted)
                            .when(!active, |s| s.invisible().group_hover(group, |s| s.visible()))
                            .hover(|s| s.bg(p.raised).text_color(p.text))
                            .cursor_pointer()
                            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.close_tab).build(window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close(id, cx);
                            }))
                            .child(icon(Icon::Close, px(12.), p.muted)),
                    ),
            );
        }
        let dark = p.dark;
        nav.child(
            div().mb(px(4.)).child(
                btn("new-tab").quiet().icon(Icon::Plus).on(self.active.is_none()).tip(t.new_tab).on_click(cx.listener(|this, _, _, cx| {
                    this.active = None;
                    cx.notify();
                })),
            ),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .mb(px(4.))
                .child(btn("diff-colors").quiet().icon(Icon::Contrast).tip(t.diff_colors.title).on_click(cx.listener(|this, _, window, cx| this.diff_colors(window, cx))))
                .child(btn("auto-sync").quiet().icon(Icon::Sync).tip(t.auto_sync_settings).on_click(cx.listener(|this, _, window, cx| this.auto_sync_settings(window, cx))))
                .child(div().mx(px(8.)).w(px(190.)).child(Select::new(&self.zones).small().menu_width(px(260.))))
                .child(div().mr(px(8.)).w(px(120.)).child(Select::new(&self.languages).small()))
                .child(
                    btn("theme")
                        .quiet()
                        .icon(if dark { Icon::Sun } else { Icon::Moon })
                        .tip(if dark { t.theme_to_light } else { t.theme_to_dark })
                        .on_click(cx.listener(move |_, _, window, cx| {
                            store::set(cx, "theme", if dark { "light" } else { "dark" });
                            apply_theme(window, cx);
                            cx.refresh_windows();
                        })),
                ),
        )
    }

    fn render_welcome(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let creating = self.creating;
        div()
            .m_auto()
            .w(px(520.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.))
            .child(div().font_family(DISPLAY).text_size(px(48.)).font_weight(FontWeight::EXTRA_BOLD).text_color(p.accent).line_height(px(62.)).child(t.app_name))
            .child(div().mb(px(16.)).font_family(PROSE).text_size(FS_XL).child(t.welcome_lead))
            .child(div().mb(px(16.)).text_color(p.muted).child(t.open_repo_hint))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(btn("open").primary().h(px(40.)).text(t.open_repo).disabled(creating).on_click(cx.listener(|this, _, window, cx| this.pick(window, cx))))
                    .child(btn("clone").h(px(40.)).text(t.tools.clone_repo).disabled(creating).on_click(cx.listener(|this, _, window, cx| this.create(true, window, cx))))
                    .child(btn("init").h(px(40.)).text(t.tools.init_repo).disabled(creating).on_click(cx.listener(|this, _, window, cx| this.create(false, window, cx)))),
            )
            .when(creating, |s| s.child(t.working))
            .when(!self.error.is_empty(), |s| s.child(div().text_color(p.red).child(i18n::error_text(cx, &self.error))))
            .when(!self.recent.is_empty(), |s| {
                s.child(div().mt(px(32.)).mb(px(4.)).text_size(FS_SM).text_color(p.muted).child(t.recent)).children(self.recent.iter().enumerate().map(|(i, path)| {
                    let path = path.clone();
                    div()
                        .id(("recent", i))
                        .w_full()
                        .ml(px(-12.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(R_MD)
                        .font_family(MONO)
                        .text_size(FS_SM)
                        .truncate()
                        .cursor_pointer()
                        .hover(|s| s.bg(p.hover))
                        .child(path.clone())
                        .on_click(cx.listener(move |this, _, window, cx| this.open_repo(path.clone(), window.window_handle(), cx)))
                }))
            })
    }
}

/// 设置里选过主题就用它，否则跟随系统；差异配色叠在主题上
pub fn apply_theme(window: &mut Window, cx: &mut App) {
    let dark = match store::get::<String>(cx, "theme").as_deref() {
        Some("light") => false,
        Some("dark") => true,
        _ => theme::system_dark(window),
    };
    let diff = store::get::<DiffColors>(cx, "diffColors").and_then(|c| c.hsla());
    theme::apply(dark, diff, Some(window), cx);
}

impl Render for PushRight {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let _ = window;
        let body = div().flex_1().min_h_0().relative().flex().flex_col();
        let body = match self.active.and_then(|id| self.tabs.iter().find(|t| t.id == id)) {
            Some(tab) => body.child(tab.view.clone()),
            None => body.child(self.render_welcome(cx)),
        };
        div()
            .id("pushright")
            .key_context("PushRight")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.cycle(1, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, _, cx| this.cycle(-1, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                if let Some(id) = this.active {
                    this.close(id, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewTab, _, cx| {
                this.active = None;
                cx.notify();
            }))
            .on_mouse_move(|e, window, cx| crate::layout::on_move(e, window, cx))
            .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| crate::layout::end(cx))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(theme::UI)
            .text_size(theme::FS_MD)
            .line_height(gpui::relative(1.5))
            .child(self.render_tabs(cx))
            .child(body)
            .child(self.help.clone())
            .child(self.dialog.clone())
            .child(self.menu.clone())
    }
}
