//! 一个文件的 diff：上方一条工具栏（上一处 / 下一处、忽略空白、隐藏字符、换行、上下文行数、整个文件、并排），
//! 下面按区块显示；本地更改里可以按区块、按行暂存或丢弃。只渲染看得见的行，几千行的 diff 滚动也不卡。

use crate::highlight::{highlight_hunk, Spans, Token};
use crate::i18n::{error_text, t};
use crate::icons::Icon;
use crate::layout;
use crate::store;
use crate::theme::{alpha, palette, Palette, FS_LG, FS_MD, FS_SM, MONO, PROSE};
use crate::ui::{btn, label, sep};
use engine::diff::{Diff, Line, Options};
use gpui::prelude::*;
use gpui::{div, img, list, px, AnyElement, App, Context, Entity, FontStyle, Global, HighlightStyle, ListAlignment, ListOffset, ListState, ScrollWheelEvent, StyledText, Task, Window};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

/// 上下文行数的档位，减少 / 增加按钮在两头禁用
pub const CONTEXT_STEPS: [u32; 7] = [0, 1, 3, 6, 12, 25, 50];
/// 大文件只显示前这么多行，其余走只读分页预览
const MAX_LINES: usize = 5000;
const LINE_H: f32 = 20.;

/// diff 工具栏和文件列表的显示偏好，存设置的 diffPrefs，所有页签共用一份
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DiffPrefs {
    pub ignore_ws: bool,
    pub hidden: bool,
    pub wrap: bool,
    pub side_by_side: bool,
    pub context: u32,
    /// 变更文件按文件夹显示
    pub tree: bool,
    /// 并排时左栏占的比例，拖中间的分隔条调整
    pub split_ratio: f32,
    /// 显示整个文件：只在本次运行内有效，免得每打开一个文件都取全文
    #[serde(skip)]
    pub whole: bool,
}

impl Default for DiffPrefs {
    fn default() -> DiffPrefs {
        DiffPrefs { ignore_ws: false, hidden: false, wrap: false, side_by_side: false, context: 3, tree: true, split_ratio: 0.5, whole: false }
    }
}

impl Global for DiffPrefs {}

pub fn init(cx: &mut App) {
    // 损坏的本地偏好不应阻止查看改动
    let mut p = store::get::<DiffPrefs>(cx, "diffPrefs").unwrap_or_default();
    if !CONTEXT_STEPS.contains(&p.context) {
        p.context = 3;
    }
    p.split_ratio = if p.split_ratio.is_finite() { p.split_ratio.clamp(0.2, 0.8) } else { 0.5 };
    cx.set_global(p);
}

pub fn prefs(cx: &App) -> &DiffPrefs {
    cx.global::<DiffPrefs>()
}

/// 改偏好并保存；会重新取 diff 的视图观察着这个全局值
pub fn set_pref(cx: &mut App, f: impl FnOnce(&mut DiffPrefs)) {
    f(cx.global_mut::<DiffPrefs>());
    let p = prefs(cx).clone();
    store::set(cx, "diffPrefs", p);
    cx.refresh_windows();
}

/// 传给引擎的参数
pub fn options(cx: &App) -> Options {
    let p = prefs(cx);
    Options { context: Some(if p.whole { 1_000_000 } else { p.context }), ignore_ws: p.ignore_ws }
}

/// 过滤过的视图和真实补丁对不上，不能按行 / 按区块暂存或丢弃
pub fn partial_blocked(cx: &App) -> bool {
    let p = prefs(cx);
    p.ignore_ws || (!p.whole && p.context == 0)
}

/// 一行占多少个等宽字符位：制表符按 4、中日韩字符按 2
fn columns(text: &str) -> usize {
    text.chars().map(|c| if c == '\t' { 4 } else if c as u32 >= 0x2e80 { 2 } else { 1 }).sum()
}

/// 并排 diff 的一行：连续的删除和紧跟的新增逐行对齐，上下文两边各放一份；数字是行在区块里的下标
pub fn split_rows(lines: &[Line]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut rows = Vec::new();
    let (mut dels, mut adds): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    // 「文件末尾无换行」的标记跟在它前一行所在的那一栏：true 是新增那栏
    let mut last: Option<bool> = None;
    let flush = |rows: &mut Vec<(Option<usize>, Option<usize>)>, dels: &mut Vec<usize>, adds: &mut Vec<usize>| {
        for i in 0..dels.len().max(adds.len()) {
            rows.push((dels.get(i).copied(), adds.get(i).copied()));
        }
        dels.clear();
        adds.clear();
    };
    for (i, line) in lines.iter().enumerate() {
        match line.kind {
            '-' => {
                if !adds.is_empty() {
                    flush(&mut rows, &mut dels, &mut adds);
                }
                dels.push(i);
                last = Some(false);
            }
            '+' => {
                adds.push(i);
                last = Some(true);
            }
            '\\' if last.is_some() => {
                if last == Some(true) { adds.push(i) } else { dels.push(i) }
            }
            _ => {
                flush(&mut rows, &mut dels, &mut adds);
                last = None;
                rows.push((Some(i), Some(i)));
            }
        }
    }
    flush(&mut rows, &mut dels, &mut adds);
    rows
}

/// 显示用的文本：制表符展开成空格（按 4 对齐）；开了「隐藏字符」时空格画成点、制表符画成箭头。
/// 着色范围跟着换算到新文本上
fn display(text: &str, spans: Option<&Spans>, hidden: bool, p: &Palette) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut marks = Vec::new();
    let mut column = 0;
    for (i, ch) in text.char_indices() {
        for _ in 0..ch.len_utf8() {
            map.push(out.len());
        }
        let _ = i;
        match ch {
            '\t' => {
                let width = 4 - column % 4;
                let start = out.len();
                if hidden {
                    out.push('→');
                    out.push_str(&" ".repeat(width - 1));
                    marks.push(start..start + '→'.len_utf8());
                } else {
                    out.push_str(&" ".repeat(width));
                }
                column += width;
            }
            ' ' if hidden => {
                let start = out.len();
                out.push('·');
                marks.push(start..out.len());
                column += 1;
            }
            _ => {
                out.push(ch);
                column += if ch as u32 >= 0x2e80 { 2 } else { 1 };
            }
        }
    }
    map.push(out.len());
    let mut styles = Vec::new();
    if let Some(spans) = spans {
        for (r, token) in spans {
            let color = match token {
                Token::Keyword => p.syn_keyword,
                Token::String => p.syn_string,
                Token::Comment => p.syn_comment,
                Token::Number => p.syn_number,
                Token::Function => p.syn_function,
                Token::Type => p.syn_type,
                Token::Variable => p.syn_variable,
                Token::Meta => p.syn_meta,
            };
            let range = map[r.start.min(text.len())]..map[r.end.min(text.len())];
            let style = HighlightStyle { color: Some(color), font_style: (*token == Token::Comment).then_some(FontStyle::Italic), ..Default::default() };
            if !range.is_empty() {
                styles.push((range, style));
            }
        }
    }
    // 隐藏字符的标记压在语法色之上
    let mark = HighlightStyle { color: Some(alpha(p.muted, 0.8)), ..Default::default() };
    let mut merged: Vec<_> = styles.into_iter().filter(|(r, _)| !marks.iter().any(|m| m.start < r.end && r.start < m.end)).collect();
    merged.extend(marks.into_iter().map(|m| (m, mark)));
    merged.sort_by_key(|(r, _)| r.start);
    (out, merged)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Readonly,
    Unstaged,
    Staged,
}

#[derive(Clone)]
enum Item {
    /// 区块标题和按钮
    Hunk(usize),
    Line(usize, usize),
    /// 并排的一行：左右各是区块里的行下标
    Split(usize, Option<usize>, Option<usize>),
}

/// 分页预览：给出从第几行开始取的后台任务
pub type Pager = Rc<dyn Fn(usize) -> Box<dyn FnOnce() -> engine::Result<Diff> + Send>>;
/// 按区块 / 按行暂存或丢弃：区块下标、区块标题、选中的行下标
pub type Apply = Rc<dyn Fn(usize, String, Vec<usize>, &mut Window, &mut App)>;

pub struct DiffView {
    source: Option<Arc<Diff>>,
    page: Option<Arc<Diff>>,
    offset: usize,
    loading: bool,
    error: String,
    request: usize,
    path: String,
    mode: Mode,
    /// 选中的行：（区块，行）
    selected: HashSet<(usize, usize)>,
    /// 每个区块各行的着色，后台算好再填
    colors: Rc<Vec<Option<Vec<Spans>>>>,
    color_gen: usize,
    items: Rc<Vec<Item>>,
    list: ListState,
    /// 不换行时的横向偏移；并排时两栏一起动
    scroll_x: f32,
    widest: usize,
    pager: Option<Pager>,
    on_apply: Option<Apply>,
    on_discard: Option<Apply>,
    _colors: Option<Task<()>>,
    /// 上一次布局时的宽度，算并排分隔条的位置
    width: Rc<std::cell::Cell<f32>>,
}

impl DiffView {
    pub fn new(_cx: &mut Context<Self>) -> DiffView {
        DiffView {
            source: None,
            page: None,
            offset: 0,
            loading: false,
            error: String::new(),
            request: 0,
            path: String::new(),
            mode: Mode::Readonly,
            selected: HashSet::new(),
            colors: Rc::new(Vec::new()),
            color_gen: 0,
            items: Rc::new(Vec::new()),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            scroll_x: 0.,
            widest: 0,
            pager: None,
            on_apply: None,
            on_discard: None,
            _colors: None,
            width: Rc::new(std::cell::Cell::new(800.)),
        }
    }

    /// 换内容。`keep` 为 true 时（只换了显示参数）不重置滚动位置
    pub fn set(&mut self, diff: Option<Diff>, path: &str, mode: Mode, pager: Option<Pager>, on_apply: Option<Apply>, on_discard: Option<Apply>, cx: &mut Context<Self>) {
        let same_file = self.path == path;
        self.source = diff.map(Arc::new);
        self.path = path.to_owned();
        self.mode = mode;
        self.pager = pager;
        self.on_apply = on_apply;
        self.on_discard = on_discard;
        self.page = None;
        self.offset = 0;
        self.error.clear();
        self.loading = false;
        self.request += 1;
        self.rebuild(!same_file, cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set(None, "", Mode::Readonly, None, None, None, cx);
    }

    fn diff(&self) -> Option<&Arc<Diff>> {
        self.page.as_ref().or(self.source.as_ref())
    }

    /// 内容或显示方式变了：重排行、清掉选择、后台重新着色
    fn rebuild(&mut self, reset_scroll: bool, cx: &mut Context<Self>) {
        self.selected.clear();
        self.scroll_x = 0.;
        let side = prefs(cx).side_by_side;
        let mut items = Vec::new();
        let mut budget = MAX_LINES;
        let mut widest = 0;
        if let Some(d) = self.diff().filter(|d| !d.binary && !d.too_large) {
            for (h, hunk) in d.hunks.iter().enumerate() {
                items.push(Item::Hunk(h));
                let n = hunk.lines.len().min(budget);
                budget -= n;
                if side {
                    for (l, r) in split_rows(&hunk.lines[..n]) {
                        items.push(Item::Split(h, l, r));
                    }
                } else {
                    items.extend((0..n).map(|i| Item::Line(h, i)));
                }
                widest = hunk.lines[..n].iter().map(|l| columns(&l.text)).fold(widest, usize::max);
            }
        }
        self.widest = widest;
        self.items = Rc::new(items);
        if reset_scroll {
            self.list.reset(self.items.len());
        } else {
            let top = self.list.logical_scroll_top();
            self.list.reset(self.items.len());
            self.list.scroll_to(top);
        }
        self.recolor(cx);
        cx.notify();
    }

    fn recolor(&mut self, cx: &mut Context<Self>) {
        self.colors = Rc::new(Vec::new());
        self.color_gen += 1;
        let gen = self.color_gen;
        let Some(d) = self.diff().cloned() else { return };
        let path = self.path.clone();
        self._colors = Some(cx.spawn(async move |this, cx| {
            let colors = cx
                .background_spawn(async move {
                    let mut budget = MAX_LINES;
                    d.hunks
                        .iter()
                        .map(|h| {
                            let n = h.lines.len().min(budget);
                            budget -= n;
                            highlight_hunk(&h.lines[..n], &path)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.color_gen == gen {
                    this.colors = Rc::new(colors);
                    cx.notify();
                }
            });
        }));
    }

    /// 并排、换行之类只换显示方式的开关
    pub fn prefs_changed(&mut self, cx: &mut Context<Self>) {
        self.rebuild(false, cx);
    }

    fn load_page(&mut self, skip: usize, cx: &mut Context<Self>) {
        let Some(pager) = self.pager.clone() else { return };
        self.request += 1;
        let id = self.request;
        self.loading = true;
        self.error.clear();
        cx.notify();
        let job = pager(skip);
        crate::git::bg(cx, job, move |this, r, cx| {
            if id != this.request {
                return;
            }
            this.loading = false;
            match r {
                Ok(d) => {
                    this.page = Some(Arc::new(d));
                    this.offset = skip;
                    this.rebuild(true, cx);
                }
                Err(e) => {
                    this.error = error_text(cx, &e);
                    cx.notify();
                }
            }
        })
        .detach();
    }

    fn editable(&self, cx: &App) -> bool {
        self.mode != Mode::Readonly && self.page.is_none() && !partial_blocked(cx)
    }

    fn picked(&self, h: usize) -> Vec<usize> {
        let mut v: Vec<usize> = self.selected.iter().filter(|(hh, _)| *hh == h).map(|(_, i)| *i).collect();
        v.sort();
        v
    }

    fn toggle(&mut self, h: usize, i: usize, cx: &mut Context<Self>) {
        let Some(d) = self.diff() else { return };
        if !self.editable(cx) || !matches!(d.hunks[h].lines[i].kind, '+' | '-') {
            return;
        }
        if !self.selected.remove(&(h, i)) {
            self.selected.insert((h, i));
        }
        cx.notify();
    }

    /// 没选行就是整个区块
    fn apply(&mut self, h: usize, discard: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.diff() else { return };
        let hunk = &d.hunks[h];
        let mut lines = self.picked(h);
        if lines.is_empty() {
            lines = hunk.lines.iter().enumerate().filter(|(_, l)| matches!(l.kind, '+' | '-')).map(|(i, _)| i).collect();
        }
        let action = if discard { self.on_discard.clone() } else { self.on_apply.clone() };
        if let Some(action) = action {
            action(h, hunk.header.clone(), lines, window, cx);
        }
    }

    /// 跳到上一处 / 下一处改动：一段连续的增删行算一处，以它的第一行为准
    fn jump(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(d) = self.diff().cloned() else { return };
        let changed = |item: &Item| match item {
            Item::Line(h, i) => matches!(d.hunks[*h].lines[*i].kind, '+' | '-'),
            Item::Split(h, l, r) => [l, r].iter().any(|x| x.is_some_and(|i| matches!(d.hunks[*h].lines[i].kind, '+' | '-'))),
            Item::Hunk(_) => false,
        };
        let starts: Vec<usize> = (0..self.items.len()).filter(|&i| changed(&self.items[i]) && (i == 0 || !changed(&self.items[i - 1]))).collect();
        let top = self.list.logical_scroll_top().item_ix;
        let target = if forward { starts.iter().find(|&&s| s > top + 1) } else { starts.iter().rev().find(|&&s| s + 1 < top) };
        if let Some(&ix) = target {
            // 留出一行，看得到改动前的上下文
            self.list.scroll_to(ListOffset { item_ix: ix.saturating_sub(1), offset_in_item: px(0.) });
            cx.notify();
        }
    }

    fn label(&self, h: usize, cx: &App) -> String {
        let t = t(cx);
        let n = self.picked(h).len();
        match (self.mode, n) {
            (Mode::Unstaged, 0) => t.stage_hunk.into(),
            (Mode::Unstaged, n) => (t.stage_lines)(n),
            (_, 0) => t.unstage_hunk.into(),
            (_, n) => (t.unstage_lines)(n),
        }
    }
}

/// 一行代码：行号、增删记号、正文。`old`：并排时左栏只显示旧行号
fn code_line(this: &Entity<DiffView>, v: &DiffView, h: usize, i: usize, half: Option<bool>, p: &Palette, cx: &App) -> AnyElement {
    let d = v.diff().unwrap();
    let line = &d.hunks[h].lines[i];
    let prefs = prefs(cx);
    let editable = v.editable(cx) && matches!(line.kind, '+' | '-');
    let selected = v.selected.contains(&(h, i));
    let (bar, bg) = match line.kind {
        '+' => (p.diff_added, alpha(p.diff_added, 0.22)),
        '-' => (p.diff_deleted, alpha(p.diff_deleted, 0.22)),
        _ => (alpha(p.bg, 0.), alpha(p.bg, 0.)),
    };
    let spans = v.colors.get(h).and_then(|c| c.as_ref()).and_then(|c| c.get(i));
    let (text, styles) = display(&line.text, spans, prefs.hidden, p);
    let no = |n: Option<u32>| div().flex_none().w(px(48.)).pr(px(8.)).flex().justify_end().text_color(p.muted).opacity(0.6).child(n.map(|n| n.to_string()).unwrap_or_default());
    let sign_color = if matches!(line.kind, '+' | '-') { p.text } else { p.muted };
    let wrap = prefs.wrap;
    let shift = if wrap { 0. } else { v.scroll_x };
    let group: gpui::SharedString = format!("line-{h}-{i}-{}", half.map_or(2, |b| b as u8)).into();
    let code = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .when(line.kind == '\\', |s| s.text_color(p.muted))
        .child(div().ml(px(-shift)).when(!wrap, |s| s.whitespace_nowrap()).child(StyledText::new(text).with_highlights(styles)));
    let mut row = div()
        .id(gpui::ElementId::Name(group.clone()))
        .group(group.clone())
        .relative()
        .flex()
        .w_full()
        .min_h(px(LINE_H))
        .pl(px(3.))
        .bg(if selected { p.accent_soft } else { bg });
    row = match half {
        None => row.child(no(line.old_no)).child(no(line.new_no)),
        Some(old) => row.child(no(if old { line.old_no } else { line.new_no })),
    };
    row = row
        .child(div().flex_none().w(px(24.)).flex().justify_center().text_color(sign_color).when(matches!(line.kind, '+' | '-'), |s| s.font_weight(gpui::FontWeight::BOLD)).child(line.kind.to_string()))
        .child(code)
        // 左边 3px 的竖条：增删色；可选的行悬停时加深，选中时是强调色
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(3.))
                .bg(if selected { p.accent } else { bar })
                .when(editable && !selected, |s| s.group_hover(group, |s| s.bg(p.border_strong))),
        );
    if editable {
        let this = this.clone();
        row = row.cursor_pointer().on_click(move |_, _, cx| this.update(cx, |v, cx| v.toggle(h, i, cx)));
    }
    row.into_any_element()
}

fn render_item(this: &Entity<DiffView>, ix: usize, cx: &App) -> AnyElement {
    let v = this.read(cx);
    let p = palette(cx).clone();
    let Some(item) = v.items.get(ix).cloned() else { return div().into_any_element() };
    match item {
        Item::Hunk(h) => {
            let d = v.diff().unwrap();
            let editable = v.editable(cx);
            let t = t(cx);
            let picked = v.picked(h).len();
            let mut bar = div()
                .flex()
                .items_center()
                .gap(px(8.))
                .w_full()
                .h(px(32.))
                .pl(px(12.))
                .pr(px(8.))
                .text_color(p.muted)
                .bg(p.panel)
                .border_t_1()
                .border_b_1()
                .border_color(p.border)
                .child(div().flex_1().min_w_0().truncate().child(d.hunks[h].header.clone()));
            if editable && v.mode == Mode::Unstaged && v.on_discard.is_some() {
                let text = if picked > 0 { (t.discard_lines)(picked) } else { t.discard_hunk.to_owned() };
                let this = this.clone();
                bar = bar.child(btn(("discard-hunk", h)).small().danger().text(text.clone()).tip(text).on_click(move |_, window, cx| this.update(cx, |v, cx| v.apply(h, true, window, cx))));
            }
            if editable {
                let text = v.label(h, cx);
                let this = this.clone();
                bar = bar.child(btn(("apply-hunk", h)).small().text(text.clone()).tip(text).on_click(move |_, window, cx| this.update(cx, |v, cx| v.apply(h, false, window, cx))));
            }
            bar.font_family(crate::theme::UI).into_any_element()
        }
        Item::Line(h, i) => code_line(this, v, h, i, None, &p, cx),
        Item::Split(h, l, r) => {
            let ratio = prefs(cx).split_ratio;
            let half = |side: Option<usize>, old: bool| match side {
                Some(i) => code_line(this, v, h, i, Some(old), &p, cx),
                None => div().size_full().min_h(px(LINE_H)).bg(p.hover).into_any_element(),
            };
            // 两栏按比例占满视窗，不随内容变宽；右栏左边一条分隔线
            div()
                .flex()
                .w_full()
                .child(div().flex_none().w(gpui::relative(ratio)).overflow_hidden().child(half(l, true)))
                .child(div().flex_1().min_w_0().overflow_hidden().border_l_1().border_color(p.border).child(half(r, false)))
                .into_any_element()
        }
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let dp = prefs(cx).clone();
        let step = CONTEXT_STEPS.iter().position(|&c| c == dp.context).unwrap_or(2);
        let has_hunks = self.diff().is_some_and(|d| !d.hunks.is_empty());
        let tool = |id: &'static str, icn: Icon, tip: String, on: bool| btn(id).small().quiet().icon_size(icn, px(14.)).on(on).tip(tip);
        let bar = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .gap(px(2.))
            .px(px(8.))
            .py(px(4.))
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .child(tool("prev", Icon::Up, t.diff_tools.previous.into(), false).disabled(!has_hunks).on_click(cx.listener(|this, _, _, cx| this.jump(false, cx))))
            .child(tool("next", Icon::Down, t.diff_tools.next.into(), false).disabled(!has_hunks).on_click(cx.listener(|this, _, _, cx| this.jump(true, cx))))
            .child(sep(cx))
            .child(tool("ws", Icon::Space, t.diff_tools.ignore_ws.into(), dp.ignore_ws).on_click(|_, _, cx| set_pref(cx, |p| p.ignore_ws = !p.ignore_ws)))
            .child(tool("hidden", Icon::Pilcrow, t.diff_tools.hidden.into(), dp.hidden).on_click(|_, _, cx| set_pref(cx, |p| p.hidden = !p.hidden)))
            .child(tool("wrap", Icon::Wrap, t.diff_tools.wrap.into(), dp.wrap).on_click(cx.listener(|this, _, _, cx| {
                set_pref(cx, |p| p.wrap = !p.wrap);
                this.prefs_changed(cx);
            })))
            .child(sep(cx))
            .child(tool("less", Icon::ContextLess, (t.diff_tools.less)(dp.context as usize), false).disabled(dp.whole || step == 0).on_click(move |_, _, cx| set_pref(cx, |p| p.context = CONTEXT_STEPS[step - 1])))
            .child(tool("more", Icon::ContextMore, (t.diff_tools.more)(dp.context as usize), false).disabled(dp.whole || step == CONTEXT_STEPS.len() - 1).on_click(move |_, _, cx| set_pref(cx, |p| p.context = CONTEXT_STEPS[step + 1])))
            .child(tool("whole", Icon::WholeFile, t.diff_tools.whole.into(), dp.whole).on_click(|_, _, cx| set_pref(cx, |p| p.whole = !p.whole)))
            .child(sep(cx))
            .child(tool("split", Icon::Split, t.diff_tools.side_by_side.into(), dp.side_by_side).on_click(cx.listener(|this, _, _, cx| {
                set_pref(cx, |p| p.side_by_side = !p.side_by_side);
                this.prefs_changed(cx);
            })));

        let note = |text: String| div().py(px(48.)).px(px(24.)).flex().justify_center().text_color(p.muted).font_family(PROSE).text_size(FS_LG).child(text);
        let mut area = div().relative().flex().flex_col().flex_1().min_h_0().bg(p.bg).font_family(MONO).text_size(FS_SM).line_height(px(LINE_H));
        if !self.error.is_empty() {
            area = area.child(note(self.error.clone()));
        }
        if self.page.is_some() {
            let next = self.page.as_ref().and_then(|d| d.next);
            let (offset, loading) = (self.offset, self.loading);
            area = area.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .h(px(32.))
                    .px(px(12.))
                    .bg(p.panel)
                    .border_b_1()
                    .border_color(p.border)
                    .text_color(p.muted)
                    .font_family(crate::theme::UI)
                    .child(btn("page-prev").small().text(t.tools.previous_page).disabled(loading || offset == 0).on_click(cx.listener(move |this, _, _, cx| this.load_page(offset.saturating_sub(1000), cx))))
                    .child(format!("{} · {}", offset + 1, t.tools.readonly_page))
                    .child(btn("page-next").small().text(t.tools.next_page).disabled(loading || next.is_none()).on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(n) = next {
                            this.load_page(n, cx);
                        }
                    }))),
            );
        }
        let paged = self.pager.is_some();
        let paged_button = |cx: &mut Context<Self>| btn("paged").text(t.tools.paged_preview).disabled(self.loading).on_click(cx.listener(|this, _, _, cx| this.load_page(0, cx)));
        match self.diff().cloned() {
            None => area = area.child(note(t.pick_file.into())),
            Some(d) if d.binary => {
                area = match &d.images {
                    Some(images) if images.iter().any(Option::is_some) => area.child(
                        div().flex().gap(px(16.)).p(px(16.)).font_family(crate::theme::UI).children(images.iter().enumerate().map(|(i, src)| {
                            let caption = if i > 0 { t.tools.after } else { t.tools.before };
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(div().mb(px(12.)).child(caption))
                                .child(match src.as_deref().and_then(data_image) {
                                    Some(image) => img(image).max_w_full().into_any_element(),
                                    None => div().text_color(p.muted).child(t.tools.absent).into_any_element(),
                                })
                        })),
                    ),
                    _ => area.child(note(t.binary.into())),
                }
            }
            Some(d) if d.too_large => {
                area = area.child(note(t.too_large.into()));
                if paged {
                    area = area.child(div().flex().justify_center().font_family(crate::theme::UI).child(paged_button(cx)));
                }
            }
            Some(d) if d.hunks.is_empty() => area = area.child(note(t.no_diff.into())),
            Some(d) => {
                if self.mode != Mode::Readonly && self.page.is_none() && partial_blocked(cx) {
                    area = area.child(div().px(px(12.)).py(px(8.)).text_color(p.yellow).font_family(PROSE).text_size(FS_MD).child(t.diff_tools.filtered));
                }
                let this = cx.entity();
                let width = self.width.clone();
                let lines = list(self.list.clone(), move |ix, _, cx| render_item(&this, ix, cx)).size_full();
                // 记下宽度，并排分隔条和横向滚动要用
                let measure = gpui::canvas(move |b, _, _| width.set(f32::from(b.size.width)), |_, _, _, _| {}).absolute().size_full();
                let mut body = div()
                    .id("diff-body")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(measure)
                    .child(lines)
                    .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, window, cx| {
                        // 横着滚：触控板横扫或按住 Shift 滚轮
                        if prefs(cx).wrap {
                            return;
                        }
                        let delta = e.delta.pixel_delta(window.line_height());
                        let dx = if e.shift { delta.y } else { delta.x };
                        if f32::from(dx) != 0. {
                            let em = 7.2;
                            let max = (this.widest as f32 * em + 80. - this.width.get() * if prefs(cx).side_by_side { prefs(cx).split_ratio.min(1. - prefs(cx).split_ratio) } else { 1. }).max(0.);
                            this.scroll_x = (this.scroll_x - f32::from(dx)).clamp(0., max);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }));
                if dp.side_by_side {
                    let ratio = dp.split_ratio;
                    let width = self.width.get();
                    body = body.child(
                        div()
                            .id("split-divider")
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(ratio * width - 3.))
                            .w(px(6.))
                            .cursor_col_resize()
                            .hover(|s| s.bg(alpha(p.accent, 0.5)))
                            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(t.drag_hint).build(window, cx))
                            .on_mouse_down(gpui::MouseButton::Left, move |e, _, cx| {
                                cx.stop_propagation();
                                if e.click_count == 2 {
                                    return set_pref(cx, |p| p.split_ratio = 0.5);
                                }
                                // 拖的过程只改内存里的比例，松手才保存（布局保存时一并写入）
                                let from = prefs(cx).split_ratio;
                                layout::start(e.position, true, move |delta, cx| {
                                    let r = (from + delta / width).clamp(0.2, 0.8);
                                    cx.global_mut::<DiffPrefs>().split_ratio = r;
                                    let p = prefs(cx).clone();
                                    store::set(cx, "diffPrefs", p);
                                }, cx);
                            }),
                    );
                }
                area = area.child(body);
                let total: usize = d.hunks.iter().map(|h| h.lines.len()).sum();
                if total > MAX_LINES {
                    area = area.child(div().flex().items_center().justify_center().gap(px(12.)).py(px(8.)).text_color(p.muted).font_family(PROSE).child((t.truncated)(MAX_LINES)).when(paged, |s| s.child(paged_button(cx))));
                }
                if d.clipped {
                    area = area.child(div().py(px(8.)).flex().justify_center().text_color(p.muted).font_family(PROSE).child(t.tools.clipped_line));
                }
            }
        }
        let _ = (window, label);
        div().flex().flex_col().flex_1().min_w_0().min_h_0().child(bar).child(area)
    }
}

/// data:image/png;base64,... → 图片
fn data_image(src: &str) -> Option<Arc<gpui::Image>> {
    use base64::Engine;
    let (head, data) = src.strip_prefix("data:")?.split_once(";base64,")?;
    let format = match head {
        "image/png" => gpui::ImageFormat::Png,
        "image/jpeg" => gpui::ImageFormat::Jpeg,
        "image/gif" => gpui::ImageFormat::Gif,
        "image/webp" => gpui::ImageFormat::Webp,
        "image/bmp" => gpui::ImageFormat::Bmp,
        "image/svg+xml" => gpui::ImageFormat::Svg,
        "image/tiff" => gpui::ImageFormat::Tiff,
        "image/x-icon" | "image/vnd.microsoft.icon" => gpui::ImageFormat::Ico,
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
    Some(Arc::new(gpui::Image::from_bytes(format, bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(kind: char) -> Line {
        Line { kind, text: String::new(), old_no: None, new_no: None }
    }

    #[test]
    fn pairs_deletions_with_following_additions() {
        let lines = [l(' '), l('-'), l('-'), l('+'), l(' '), l('+'), l('\\')];
        assert_eq!(split_rows(&lines), [(Some(0), Some(0)), (Some(1), Some(3)), (Some(2), None), (Some(4), Some(4)), (None, Some(5)), (None, Some(6))]);
    }

    #[test]
    fn hidden_marks_and_tabs() {
        let p = Palette::dark();
        let (text, styles) = display("a\tb c", None, true, &p);
        assert_eq!(text, "a→  b·c");
        assert_eq!(styles.len(), 2);
        let (plain, _) = display("\tx", None, false, &p);
        assert_eq!(plain, "    x");
    }

    #[test]
    fn columns_count_wide_chars() {
        assert_eq!(columns("a\t中"), 1 + 4 + 2);
    }
}
