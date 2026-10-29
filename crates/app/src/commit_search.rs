//! 提交图上方的搜索条：按说明、作者、文件、改动内容或提交号找提交，也用来显示单个文件的历史。
//! 右端是提交图的两个显示开关（标签、只看主线）。

use crate::author::author_color;
use crate::git::{self, Session};
use crate::i18n::{error_text, lookup, t};
use crate::icons::{icon, Icon};
use crate::repo_view::RepoView;
use crate::theme::{palette, CONTROL_SM, FS_SM, MONO, PROSE};
use crate::time::fmt_time;
use crate::ui::{btn, Opt};
use engine::history::{Commit as HistoryCommit, SearchKind};
use gpui::prelude::*;
use gpui::{div, px, relative, Context, Entity, SharedString, Subscription, WeakEntity, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{Select, SelectState};
use gpui_component::{IndexPath, Sizable};
use std::sync::Arc;

const PAGE: usize = 200;
const KINDS: [&str; 5] = ["message", "author", "file", "content", "id"];

pub struct CommitSearch {
    session: Arc<Session>,
    repo: WeakEntity<RepoView>,
    kind: Entity<SelectState<Vec<Opt>>>,
    query: Entity<InputState>,
    /// None 表示没有在显示结果
    results: Option<Vec<HistoryCommit>>,
    more: bool,
    loading: bool,
    error: String,
    /// 结果是某个文件的历史（跟随重命名）时，这里是它的路径
    history: String,
    request: usize,
    _subs: Vec<Subscription>,
}

impl CommitSearch {
    pub fn new(session: Arc<Session>, repo: WeakEntity<RepoView>, window: &mut Window, cx: &mut Context<Self>) -> CommitSearch {
        let t = t(cx);
        let opts = KINDS.iter().map(|k| Opt::new(*k, lookup(t.vscode.search_kinds, k).unwrap_or(k))).collect();
        let kind = cx.new(|cx| SelectState::new(opts, Some(IndexPath::new(0)), window, cx));
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(t.search.placeholder));
        let subs = vec![cx.subscribe_in(&query, window, |this, _, e: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = e {
                this.history.clear();
                if !this.query.read(cx).value().trim().is_empty() {
                    this.load(false, cx);
                }
            }
        })];
        CommitSearch { session, repo, kind, query, results: None, more: false, loading: false, error: String::new(), history: String::new(), request: 0, _subs: subs }
    }

    fn load(&mut self, append: bool, cx: &mut Context<Self>) {
        self.request += 1;
        let id = self.request;
        let skip = if append { self.results.as_ref().map_or(0, Vec::len) } else { 0 };
        self.loading = true;
        self.error.clear();
        cx.notify();
        let session = self.session.clone();
        let history = self.history.clone();
        let query = self.query.read(cx).value().trim().to_owned();
        let kind = match self.kind.read(cx).selected_value().map(|v| v.as_ref()) {
            Some("author") => SearchKind::Author,
            Some("file") => SearchKind::File,
            Some("content") => SearchKind::Content,
            Some("id") => SearchKind::Id,
            _ => SearchKind::Message,
        };
        // 多取一条用来判断还有没有下一页
        git::bg(
            cx,
            move || if history.is_empty() { engine::history::search_page(&session.repo, kind, &query, PAGE + 1, skip) } else { engine::history::file_page(&session.repo, &history, PAGE + 1, skip) },
            move |this, r, cx| {
                if id != this.request {
                    return;
                }
                this.loading = false;
                match r {
                    Ok(mut next) => {
                        this.more = next.len() > PAGE;
                        next.truncate(PAGE);
                        let mut list = if append { this.results.take().unwrap_or_default() } else { Vec::new() };
                        list.extend(next);
                        this.results = Some(list);
                    }
                    Err(e) => this.error = error_text(cx, &e),
                }
                cx.notify();
            },
        )
        .detach();
    }

    pub fn file_history(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.history = path.clone();
        self.query.update(cx, |q, cx| q.set_value(path, window, cx));
        self.load(false, cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request += 1;
        self.results = None;
        self.more = false;
        self.loading = false;
        self.error.clear();
        self.history.clear();
        self.query.update(cx, |q, cx| q.set_value("", window, cx));
        cx.notify();
    }
}

impl Render for CommitSearch {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let (hide_tags, first_parent, loading_all) = self.repo.upgrade().map_or((false, false, false), |r| {
            let r = r.read(cx);
            (r.graph_prefs.hide_tags, r.graph_prefs.first_parent, r.loading_all)
        });
        let showing = self.results.is_some() || !self.error.is_empty();
        let form = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .py(px(4.))
            .text_color(p.muted)
            .child(icon(Icon::Search, px(14.), p.muted))
            .child(div().flex_none().w(px(170.)).child(Select::new(&self.kind).small().h(CONTROL_SM)))
            // 输入框自带的 h() 只管多行，单行用最小高度撑到和按钮一样高
            .child(div().flex_1().min_w_0().child(Input::new(&self.query).small().min_h(CONTROL_SM).text_size(FS_SM)))
            .when(showing, |s| s.child(btn("search-close").small().quiet().icon_size(Icon::Close, px(14.)).tip(t.search.close).on_click(cx.listener(|this, _, window, cx| this.close(window, cx)))))
            .child(btn("tags").small().quiet().text(t.graph_view.tags).on(!hide_tags).tip(t.graph_view.tags_hint).on_click(cx.listener(|this, _, _, cx| {
                let _ = this.repo.update(cx, |r, cx| r.set_graph_pref(false, cx));
            })))
            .child(btn("first-parent").small().quiet().text(t.graph_view.first_parent).on(first_parent).disabled(loading_all).tip(t.graph_view.first_parent_hint).on_click(cx.listener(|this, _, _, cx| {
                let _ = this.repo.update(cx, |r, cx| r.set_graph_pref(true, cx));
            })));
        let note = |text: String| div().px(px(12.)).py(px(4.)).text_color(p.muted).child(text);
        let mut el = div().flex().flex_col().flex_none().max_h(relative(0.45)).border_b_1().border_color(p.border).text_size(FS_SM).child(form);
        if !self.error.is_empty() {
            el = el.child(note(self.error.clone()));
        }
        if let Some(results) = &self.results {
            let head = if results.is_empty() {
                if self.loading { t.working.to_owned() } else { t.search.none.to_owned() }
            } else {
                format!("{}{}", (t.search.results)(results.len()), if self.more { "+" } else { "" })
            };
            let head = if self.history.is_empty() { head } else { format!("{} · {head}", (t.search.file_history)(&self.history)) };
            let mut list = div().id("search-results").min_h_0().overflow_y_scroll().border_t_1().border_color(p.border).bg(p.surface).child(note(head));
            for c in results {
                let id = c.id.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("result-{}", c.id)))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .h(px(24.))
                        .px(px(12.))
                        .hover(|s| s.bg(p.hover))
                        .tooltip({
                            let tip: SharedString = c.subject.clone().into();
                            move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = id.clone();
                            let _ = this.repo.update(cx, |r, cx| r.reveal(id, cx));
                        }))
                        .child(div().flex_none().font_family(MONO).text_color(p.muted).child(c.id[..8.min(c.id.len())].to_owned()))
                        .child(div().flex_1().min_w_0().truncate().font_family(PROSE).child(c.subject.clone()))
                        .child(div().flex_none().whitespace_nowrap().text_color(author_color(&c.author, cx)).child(c.author.clone()))
                        .child(div().flex_none().whitespace_nowrap().text_color(p.muted).child(fmt_time(cx, c.time))),
                );
            }
            if self.more {
                list = list.child(div().mx(px(12.)).mt(px(4.)).mb(px(8.)).child(btn("load-more").small().text(t.tools.load_more).disabled(self.loading).on_click(cx.listener(|this, _, _, cx| this.load(true, cx)))));
            }
            el = el.child(list);
        }
        el
    }
}
