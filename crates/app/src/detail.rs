//! 提交详情（也用于贮藏详情、版本比较和 PR 改动的文件）：左边改动的文件，右边是提交说明，点了文件就换成它的 diff。
//! PR 的 diff 里显示行评论，也能选行发评论。diff 工具栏能进禅模式，只留这个视图铺满窗口。

use crate::author::author_color;
use crate::diff_view::{self, DiffPrefs, DiffView, Mode, Pager};
use crate::file_tree::{file_icon, file_list_bar, filter_input, folder_row, indent, tree_rows, TreeRow};
use crate::git::{self, Session};
use crate::i18n::{error_text, lookup, t};
use crate::layout::{self, layout, Key};
use crate::repo_view::RepoView;
use crate::store;
use crate::theme::{palette, FS_LG, FS_MD, FS_SM, LH_BODY, MONO, PROSE};
use crate::time::fmt_time;
use engine::detail::{Detail, FileChange};
use engine::forge::{Anchor, Note};
use gpui::prelude::*;
use gpui::{div, px, relative, uniform_list, AnyView, App, Context, Entity, FontWeight, MouseButton, MouseDownEvent, SharedString, Subscription, UniformListScrollHandle, WeakEntity, Window};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

/// diff 从哪里取
#[derive(Clone, PartialEq)]
pub enum Source {
    Commit,
    Stash,
    /// 版本比较：两端
    Compare(String, String),
}

impl Source {
    /// 「已查看」记录的键里用的名字，和网页版一致
    pub fn mode(&self) -> &'static str {
        match self {
            Source::Commit => "commit",
            Source::Stash => "stash",
            Source::Compare(..) => "compare",
        }
    }
}

pub struct DetailView {
    session: Arc<Session>,
    repo: WeakEntity<RepoView>,
    detail: Option<Detail>,
    source: Source,
    /// 正在看 diff 的文件；None 时右侧显示提交信息
    file: Option<String>,
    diff: Entity<DiffView>,
    error: String,
    filter: Entity<InputState>,
    closed: HashSet<String>,
    scroll: UniformListScrollHandle,
    reviewed: Vec<String>,
    review_key: String,
    /// 版本比较里不显示作者、提交号这些
    pub comparison: bool,
    /// 嵌在仓库工具里（变基预览）：父提交只显示不跳转，跳到提交图会离开仓库工具
    pub embedded: bool,
    /// PR 所有文件的行评论
    pub notes: Vec<Note>,
    /// 给了就能在 diff 里选行发评论
    pub on_comment: Option<Rc<dyn Fn(Anchor, &mut Window, &mut App)>>,
    request: usize,
    _subs: Vec<Subscription>,
}

impl DetailView {
    pub fn new(session: Arc<Session>, repo: WeakEntity<RepoView>, window: &mut Window, cx: &mut Context<Self>) -> DetailView {
        let diff = cx.new(DiffView::new);
        let me: AnyView = cx.entity().into();
        diff.update(cx, |d, _| d.zen = Some(Rc::new(me.downgrade())));
        let filter = filter_input(window, cx);
        let subs = vec![
            // 只换 diff 选项时留着旧内容等新结果，不先清空
            cx.observe_global::<DiffPrefs>(|this, cx| this.load_diff(cx)),
            cx.subscribe(&filter, |_, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        DetailView {
            session,
            repo,
            detail: None,
            source: Source::Commit,
            file: None,
            diff,
            error: String::new(),
            filter,
            closed: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            reviewed: Vec::new(),
            review_key: String::new(),
            comparison: false,
            embedded: false,
            notes: Vec::new(),
            on_comment: None,
            request: 0,
            _subs: subs,
        }
    }

    pub fn file_count(&self) -> usize {
        self.detail.as_ref().map_or(0, |d| d.files.len())
    }

    pub fn set(&mut self, detail: Option<Detail>, source: Source, review_key: String, cx: &mut Context<Self>) {
        self.reviewed = store::get(cx, &review_key).unwrap_or_default();
        self.review_key = review_key;
        self.detail = detail;
        self.source = source;
        self.file = None;
        self.error.clear();
        self.diff.update(cx, |d, cx| d.clear(cx));
        cx.notify();
    }

    /// 当前文件的行评论
    fn file_notes(&self) -> Vec<Note> {
        self.notes.iter().filter(|n| self.file.as_ref() == Some(&n.path)).cloned().collect()
    }

    /// 只换评论（刚发了一条）：diff 停在原处
    pub fn set_notes(&mut self, notes: Vec<Note>, cx: &mut Context<Self>) {
        self.notes = notes;
        let notes = self.file_notes();
        self.diff.update(cx, |d, cx| d.set_notes(notes, cx));
    }

    fn open(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        self.file = path;
        self.diff.update(cx, |d, cx| d.clear(cx));
        self.load_diff(cx);
    }

    fn load_diff(&mut self, cx: &mut Context<Self>) {
        let (Some(detail), Some(file)) = (self.detail.as_ref(), self.file.clone()) else { return };
        self.request += 1;
        let request = self.request;
        self.error.clear();
        let session = self.session.clone();
        let id = detail.id.clone();
        let old_path = detail.files.iter().find(|f| f.path == file).and_then(|f| f.old_path.clone());
        let opts = diff_view::options(cx);
        let source = self.source.clone();
        let anchor = Anchor { path: file.clone(), old_path: old_path.clone().unwrap_or_else(|| file.clone()), ..Default::default() };
        let (s, src, f, op, i) = (session.clone(), source.clone(), file.clone(), old_path.clone(), id.clone());
        let job = move || match &src {
            Source::Commit => engine::diff::commit_with(&s.repo, &i, &f, opts),
            Source::Stash => engine::diff::stash_with(&s.repo, &i, &f, opts),
            Source::Compare(left, right) => engine::diff::between_with(&s.repo, left, right, &f, op.as_deref(), opts),
        };
        let pager: Pager = std::rc::Rc::new(move |skip| {
            let (s, source, file, old_path, id) = (session.clone(), source.clone(), file.clone(), old_path.clone(), id.clone());
            Box::new(move || match &source {
                Source::Compare(left, right) => engine::diff::page_with(&s.repo, "compare", left, right, &file, old_path.as_deref(), skip, opts),
                other => engine::diff::page_with(&s.repo, other.mode(), "", &id, &file, None, skip, opts),
            })
        });
        git::bg(cx, job, move |this, r, cx| {
            if request != this.request {
                return;
            }
            match r {
                Ok(d) => {
                    let path = this.file.clone().unwrap_or_default();
                    let notes = this.file_notes();
                    let on_comment = this.on_comment.clone().map(|f| Rc::new(move |from, to, window: &mut Window, cx: &mut App| f(Anchor { from, to, ..anchor.clone() }, window, cx)) as diff_view::Comment);
                    this.diff.update(cx, |v, cx| {
                        v.notes = notes;
                        v.on_comment = on_comment;
                        v.set(Some(d), &path, Mode::Readonly, Some(pager), None, None, cx)
                    });
                }
                Err(e) => this.error = error_text(cx, &e),
            }
            cx.notify();
        })
        .detach();
    }

    fn review(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.reviewed.iter().any(|p| p == path) {
            self.reviewed.retain(|p| p != path);
        } else {
            self.reviewed.push(path.to_owned());
        }
        if !self.review_key.is_empty() {
            store::set(cx, &self.review_key, &self.reviewed);
        }
        cx.notify();
    }

    fn file_row(&mut self, f: &FileChange, depth: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let base = |s: &str| s.rsplit('/').next().unwrap_or(s).to_owned();
        let shown = |s: &str| if depth.is_some() { base(s) } else { s.to_owned() };
        let selected = self.file.as_deref() == Some(&f.path);
        let status_color = match f.status {
            'A' => p.diff_added,
            'D' => p.diff_deleted,
            'R' | 'C' => p.accent,
            _ => p.yellow,
        };
        let path = f.path.clone();
        let (p1, p2, p3) = (path.clone(), path.clone(), path.clone());
        let gone = f.status == 'D';
        let tip: SharedString = match &f.old_path {
            Some(old) => format!("{old} → {}", f.path).into(),
            None => f.path.clone().into(),
        };
        let status = lookup(t.status, &f.status.to_string()).unwrap_or_default();
        div()
            .flex()
            .items_center()
            .w_full()
            .pl(depth.map_or(px(8.), |d| indent(d) - px(20.)))
            .child(Checkbox::new(SharedString::from(format!("reviewed-{path}"))).checked(self.reviewed.contains(&path)).tooltip(t.tools.reviewed).on_click(cx.listener(move |this, _, _, cx| this.review(&p1, cx))))
            .child(
                div()
                    .id(SharedString::from(format!("file-{path}")))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.))
                    .h(px(24.))
                    .px(px(12.))
                    .text_size(FS_SM)
                    .when(selected, |s| s.bg(p.accent_soft))
                    .when(!selected, |s| s.hover(|s| s.bg(p.hover)))
                    .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let next = if this.file.as_deref() == Some(&p2) { None } else { Some(p2.clone()) };
                        this.open(next, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                        // 文件菜单按单个提交取补丁，和比较的两端对不上
                        let Some(id) = this.detail.as_ref().filter(|_| !this.comparison).map(|d| d.id.clone()) else { return };
                        let (at, file) = (e.position, p3.clone());
                        let _ = this.repo.update(cx, |r, cx| r.commit_file_menu(at, id, file, gone, window, cx));
                    }))
                    .child(div().id("status").flex_none().w(px(16.)).flex().justify_center().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(status_color).tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(status).build(window, cx)).child(f.status.to_string()))
                    .child(file_icon(&f.path, px(14.), cx))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .truncate()
                            .when_some(f.old_path.clone(), |s, old| s.child(div().text_color(p.muted).child(format!("{} → ", shown(&old)))))
                            .child(shown(&f.path)),
                    ),
            )
    }
}

impl Render for DetailView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = window;
        let p = palette(cx).clone();
        let t = t(cx);
        let Some(detail) = self.detail.clone() else {
            return div().flex_1().flex().items_center().justify_center().bg(p.panel).text_color(p.muted).font_family(PROSE).text_size(FS_LG).child(t.no_selection).into_any_element();
        };
        let l = layout(cx);
        let query = self.filter.read(cx).value().trim().to_lowercase();
        let files: Vec<FileChange> = detail.files.iter().filter(|f| query.is_empty() || f.path.to_lowercase().contains(&query)).cloned().collect();
        let me = cx.entity().downgrade();
        let fold = {
            let me = me.clone();
            move |open: bool, cx: &mut gpui::App| {
                let _ = me.update(cx, |this, cx| {
                    this.closed = if open { HashSet::new() } else { tree_rows(&this.detail.as_ref().map(|d| d.files.clone()).unwrap_or_default(), |f| &f.path, &HashSet::new()).into_iter().filter(|r| r.item.is_none()).map(|r| r.key).collect() };
                    cx.notify();
                });
            }
        };
        // 只渲染看得见的行，上千个文件的提交和分支对比也不拖慢每一帧
        let tree = diff_view::prefs(cx).tree;
        let rows = Rc::new(if tree {
            tree_rows(&files, |f| &f.path, &self.closed)
        } else {
            files.into_iter().map(|f| TreeRow { key: f.path.clone(), depth: 0, name: String::new(), item: Some(f), items: Vec::new(), open: false }).collect()
        });
        let rows = uniform_list(
            "detail-files",
            rows.len(),
            cx.processor(move |this: &mut DetailView, range: Range<usize>, _window: &mut Window, cx: &mut Context<DetailView>| {
                range
                    .map(|ix| {
                        let row = &rows[ix];
                        match &row.item {
                            Some(f) => this.file_row(f, tree.then_some(row.depth), cx).into_any_element(),
                            None => {
                                let key = row.key.clone();
                                folder_row(format!("folder-{}", row.key).into(), &row.key, &row.name, row.items.len(), row.open, row.depth, true, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.closed.remove(&key) {
                                            this.closed.insert(key.clone());
                                        }
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        let list = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(l.files))
            .max_w(relative(0.45))
            .py(px(12.))
            .child(div().mx(px(12.)).mb(px(8.)).text_size(FS_SM).text_color(p.muted).child((t.files)(detail.files.len())))
            .child(file_list_bar(&self.filter, fold, cx))
            .child(div().relative().flex_1().min_h_0().child(rows).vertical_scrollbar(&self.scroll));

        let right = if self.file.is_some() {
            if self.error.is_empty() {
                self.diff.clone().into_any_element()
            } else {
                div().m_auto().text_color(p.muted).font_family(PROSE).text_size(FS_LG).child(self.error.clone()).into_any_element()
            }
        } else {
            // 提交说明是这块的主角：标题字族、最大行宽 72 个字符
            let (subject, body) = detail.message.split_once('\n').map_or((detail.message.as_str(), ""), |(s, b)| (s, b.trim()));
            let mut info = div()
                .id("detail-info")
                .flex_1()
                .min_w_0()
                .overflow_y_scroll()
                .px(px(24.))
                .py(px(16.))
                .child(div().max_w(gpui::rems(40.)).mb(px(12.)).font_family(crate::theme::DISPLAY).text_size(FS_LG).font_weight(FontWeight::SEMIBOLD).child(subject.to_owned()));
            if !body.is_empty() {
                info = info.child(div().max_w(px(72. * 8.)).mb(px(16.)).font_family(PROSE).text_size(FS_MD).line_height(relative(LH_BODY)).child(body.to_owned()));
            }
            if !self.comparison {
                let row = |k: &str, v: gpui::AnyElement| div().flex().gap(px(16.)).child(div().w(px(72.)).flex_none().text_color(p.muted).child(k.to_owned())).child(div().flex_1().min_w_0().child(v));
                let mut dl = div().flex().flex_col().gap(px(4.)).pt(px(12.)).border_t_1().border_color(p.border).text_size(FS_SM);
                dl = dl.child(row(t.col_commit, div().font_family(MONO).child(detail.id.clone()).into_any_element()));
                dl = dl.child(row(
                    t.author,
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.))
                        .child(div().font_weight(FontWeight::BOLD).text_color(author_color(&detail.author, cx)).child(detail.author.clone()))
                        .child(div().text_color(p.muted).child(format!("<{}> · {}", detail.author_email, fmt_time(cx, detail.author_time))))
                        .into_any_element(),
                ));
                if detail.committer != detail.author || detail.committer_time != detail.author_time {
                    dl = dl.child(row(t.committer, div().flex().gap(px(4.)).child(detail.committer.clone()).child(div().text_color(p.muted).child(format!("· {}", fmt_time(cx, detail.committer_time)))).into_any_element()));
                }
                if !detail.parents.is_empty() {
                    let parents = div().flex().gap(px(8.)).children(detail.parents.iter().map(|parent| {
                        let id = parent.clone();
                        let repo = self.repo.clone();
                        let embedded = self.embedded;
                        div()
                            .id(SharedString::from(format!("parent-{parent}")))
                            .font_family(MONO)
                            .text_color(if embedded { p.muted } else { p.accent })
                            .when(!embedded, |s| s.cursor_pointer().hover(|s| s.underline()))
                            .child(parent[..8.min(parent.len())].to_owned())
                            .on_click(move |_, _, cx| {
                                if !embedded {
                                    let _ = repo.update(cx, |r, cx| r.jump(&id, cx));
                                }
                            })
                    }));
                    dl = dl.child(row(t.parents, parents.into_any_element()));
                }
                info = info.child(dl);
            }
            info.into_any_element()
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .bg(p.panel)
            .child(list)
            .child(layout::splitter("split-files", Key::Files, true, 200., 640., false, cx))
            .child(right)
            .into_any_element()
    }
}
