//! 一个文件的 diff：上方一条工具栏（上一处 / 下一处、忽略空白、隐藏字符、换行、上下文行数、整个文件、并排），
//! 下面按区块显示；本地更改里可以按区块、按行暂存或丢弃，PR 里在行号上拖选几行发评论、已有的行评论显示在所在行下面。
//! 带文件列表的 diff 可以进禅模式：只剩文件列表和 diff 铺满窗口。只渲染看得见的行，几千行的 diff 滚动也不卡。

use crate::highlight::{highlight_hunk, Spans, Token};
use crate::i18n::{error_text, t};
use crate::icons::{icon, Icon};
use crate::layout;
use crate::store;
use crate::theme::{alpha, palette, Palette, FS_LG, FS_MD, FS_SM, LH_BODY, MONO, PROSE, R_MD};
use crate::time::fmt_time;
use crate::ui::{btn, label, sep};
use engine::diff::{Diff, Line, Options};
use engine::forge::{spot, Note, Spot};
use gpui::prelude::*;
use gpui::{div, img, list, px, AnyElement, AnyView, AnyWeakView, App, ClipboardItem, Context, Entity, FocusHandle, FontStyle, Global, HighlightStyle, KeyDownEvent, ListAlignment, ListOffset, ListState, MouseButton, Pixels, ScrollWheelEvent, StyledText, Task, Window};
use gpui_component::scroll::ScrollableElement;
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

/// 显示列换算成原文里的字节位置；落在一个字符中间时取近的那条边
fn byte_at(text: &str, col: usize) -> usize {
    let mut c = 0;
    for (i, ch) in text.char_indices() {
        let w = if ch == '\t' { 4 - c % 4 } else if ch as u32 >= 0x2e80 { 2 } else { 1 };
        if col <= c + w / 2 {
            return i;
        }
        c += w;
    }
    text.len()
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

/// 禅模式：铺满窗口的视图（文件列表 + diff），其余界面都不画；Esc 退出
pub struct Zen(pub Option<AnyView>);

impl Global for Zen {}

pub fn zen(cx: &App) -> Option<AnyView> {
    cx.try_global::<Zen>().and_then(|z| z.0.clone())
}

/// 进入（给出视图）或退出禅模式
pub fn set_zen(view: Option<AnyView>, window: &mut Window, cx: &mut App) {
    cx.set_global(Zen(view));
    window.refresh();
}

/// 鼠标选中的文字：起点和终点都是（行在 items 里的下标，显示列）；并排时只在一栏里选，`side` 同 `code_line` 的 `half`
#[derive(Clone, Copy, PartialEq)]
struct TextSel {
    side: Option<bool>,
    anchor: (usize, usize),
    head: (usize, usize),
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
    /// 挂在上一行下面的评论：notes 里的下标
    Note(usize),
}

/// 分页预览：给出从第几行开始取的后台任务
pub type Pager = Rc<dyn Fn(usize) -> Box<dyn FnOnce() -> engine::Result<Diff> + Send>>;
/// 按区块 / 按行暂存或丢弃：区块下标、区块标题、选中的行下标
pub type Apply = Rc<dyn Fn(usize, String, Vec<usize>, &mut Window, &mut App)>;
/// 对选中的行发评论：起止两行
pub type Comment = Rc<dyn Fn(Spot, Spot, &mut Window, &mut App)>;

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
    /// 这个文件的行评论，在 `set` 之前给
    pub notes: Vec<Note>,
    /// 给了就能在行号上拖选几行发评论（PR 改动的文件），在 `set` 之前给
    pub on_comment: Option<Comment>,
    /// 在行号上按住拖选要评论的行：（区块，起点行）
    picking: Option<(usize, usize)>,
    /// 给了就在工具栏放禅模式按钮，铺满窗口的是这个视图
    pub zen: Option<Rc<AnyWeakView>>,
    _colors: Option<Task<()>>,
    /// 上一次布局时的宽度，算并排分隔条的位置
    width: Rc<std::cell::Cell<f32>>,
    text_sel: Option<TextSel>,
    dragging: bool,
    /// 正文左边缘在窗口里的横坐标：单栏或并排左栏、并排右栏。按鼠标位置算列要用
    code_left: Rc<std::cell::Cell<[f32; 2]>>,
    /// 等宽字体一个字符的宽度
    char_w: f32,
    focus: FocusHandle,
}

impl DiffView {
    pub fn new(cx: &mut Context<Self>) -> DiffView {
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
            notes: Vec::new(),
            on_comment: None,
            picking: None,
            zen: None,
            _colors: None,
            width: Rc::new(std::cell::Cell::new(800.)),
            text_sel: None,
            dragging: false,
            code_left: Rc::new(std::cell::Cell::new([0.; 2])),
            char_w: 7.2,
            focus: cx.focus_handle(),
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
        self.recolor(cx);
    }

    /// 只换评论：停在原处，不重新着色
    pub fn set_notes(&mut self, notes: Vec<Note>, cx: &mut Context<Self>) {
        self.notes = notes;
        self.rebuild(false, cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set(None, "", Mode::Readonly, None, None, None, cx);
    }

    fn diff(&self) -> Option<&Arc<Diff>> {
        self.page.as_ref().or(self.source.as_ref())
    }

    /// 内容、评论或显示方式变了：重排行、清掉选择；内容变了的由调用方再重新着色
    fn rebuild(&mut self, reset_scroll: bool, cx: &mut Context<Self>) {
        self.selected.clear();
        self.picking = None;
        self.text_sel = None;
        self.scroll_x = 0.;
        let side = prefs(cx).side_by_side;
        let mut items = Vec::new();
        let mut budget = MAX_LINES;
        let mut widest = 0;
        // 评论挂在它那一侧行号相同的行下面
        let notes_at = |items: &mut Vec<Item>, line: &Line| items.extend(self.notes.iter().enumerate().filter(|(_, n)| (if n.old { line.old_no } else { line.new_no }) == Some(n.line)).map(|(k, _)| Item::Note(k)));
        if let Some(d) = self.diff().filter(|d| !d.binary && !d.too_large) {
            for (h, hunk) in d.hunks.iter().enumerate() {
                items.push(Item::Hunk(h));
                let n = hunk.lines.len().min(budget);
                budget -= n;
                if side {
                    for (l, r) in split_rows(&hunk.lines[..n]) {
                        items.push(Item::Split(h, l, r));
                        // 上下文行两栏是同一行，只挂一次
                        for i in [l, r.filter(|&r| Some(r) != l)].into_iter().flatten() {
                            notes_at(&mut items, &hunk.lines[i]);
                        }
                    }
                } else {
                    for i in 0..n {
                        items.push(Item::Line(h, i));
                        notes_at(&mut items, &hunk.lines[i]);
                    }
                }
                widest = hunk.lines[..n].iter().map(|l| columns(&l.text)).fold(widest, usize::max);
            }
        }
        self.widest = widest;
        self.items = Rc::new(items);
        // 没画过的行按一行高估算，滚动条从一开始就按整份 diff 的长度算
        let top = self.list.logical_scroll_top();
        self.list.reset_with_uniform_height(self.items.len(), px(LINE_H));
        if !reset_scroll {
            self.list.scroll_to(top);
        }
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
                    this.recolor(cx);
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

    /// 在行号上按下：从这一行开始选要评论的行
    fn pick_start(&mut self, h: usize, i: usize, cx: &mut Context<Self>) {
        self.picking = Some((h, i));
        self.selected = HashSet::from([(h, i)]);
        self.text_sel = None;
        cx.notify();
    }

    /// 拖到区块里第 i 行：选中起点到这里的整段，不跨出起点所在的区块；按键已经松开（松在了 diff 外面）就作废
    fn pick_to(&mut self, h: usize, i: usize, pressed: bool, cx: &mut Context<Self>) {
        let Some((ph, from)) = self.picking else { return };
        if !pressed {
            self.picking = None;
            self.selected.clear();
            return cx.notify();
        }
        let Some(lines) = self.diff().filter(|_| h == ph).map(|d| &d.hunks[h].lines) else { return };
        let picked: HashSet<_> = (from.min(i)..=from.max(i)).filter(|&k| lines[k].kind != '\\').map(|k| (h, k)).collect();
        if picked != self.selected {
            self.selected = picked;
            cx.notify();
        }
    }

    /// 松开：对选中的整段发评论
    fn pick_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((h, _)) = self.picking.take() else { return };
        let lines = self.picked(h);
        self.selected.clear();
        cx.notify();
        let (Some(d), Some(on_comment), Some(&a), Some(&b)) = (self.diff(), self.on_comment.clone(), lines.first(), lines.last()) else { return };
        let (from, to) = (spot(&d.hunks[h].lines, a), spot(&d.hunks[h].lines, b));
        on_comment(from, to, window, cx);
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
            Item::Hunk(_) | Item::Note(_) => false,
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

    /// 不换行时最多能往右滚多少
    fn max_scroll(&self, cx: &App) -> f32 {
        let p = prefs(cx);
        let view = self.width.get() * if p.side_by_side { p.split_ratio.min(1. - p.split_ratio) } else { 1. };
        (self.widest as f32 * self.char_w + 80. - view).max(0.)
    }

    fn line_at(&self, ix: usize, side: Option<bool>) -> Option<&Line> {
        let d = self.diff()?;
        let line = match (self.items.get(ix)?, side) {
            (Item::Line(h, i), None) => &d.hunks[*h].lines[*i],
            (Item::Split(h, l, r), Some(old)) => &d.hunks[*h].lines[(if old { *l } else { *r })?],
            _ => return None,
        };
        (line.kind != '\\').then_some(line)
    }

    /// 这一行被选中的显示列范围，选到行尾是 usize::MAX；换行显示时按整行选
    fn sel_cols(&self, ix: usize, side: Option<bool>, cx: &App) -> Option<(usize, usize)> {
        let s = self.text_sel.filter(|s| s.side == side && s.anchor != s.head)?;
        let (a, b) = if s.anchor <= s.head { (s.anchor, s.head) } else { (s.head, s.anchor) };
        if ix < a.0 || ix > b.0 {
            return None;
        }
        if prefs(cx).wrap {
            return Some((0, usize::MAX));
        }
        Some((if ix == a.0 { a.1 } else { 0 }, if ix == b.0 { b.1 } else { usize::MAX }))
    }

    /// 鼠标横坐标对应的显示列；换行显示时没法按横坐标算，起点取行首、终点取行尾
    fn col_at(&self, side: Option<bool>, x: Pixels, end: bool, cx: &App) -> usize {
        if prefs(cx).wrap {
            return if end { usize::MAX } else { 0 };
        }
        let left = self.code_left.get()[(side == Some(false)) as usize];
        ((f32::from(x) - left + self.scroll_x) / self.char_w).round().max(0.) as usize
    }

    fn press(&mut self, ix: usize, side: Option<bool>, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let col = self.col_at(side, x, false, cx);
        self.text_sel = Some(TextSel { side, anchor: (ix, col), head: (ix, col) });
        self.dragging = true;
        cx.notify();
    }

    fn drag(&mut self, ix: usize, side: Option<bool>, x: Pixels, cx: &mut Context<Self>) {
        let col = self.col_at(side, x, true, cx);
        if let Some(s) = self.text_sel.as_mut().filter(|s| s.side == side) {
            if s.head != (ix, col) {
                s.head = (ix, col);
                cx.notify();
            }
        }
    }

    /// 复制选中的原文：制表符和空格是真实字符，不是显示用的箭头和点
    fn copy(&self, cx: &mut App) {
        let Some(s) = self.text_sel else { return };
        let (a, b) = (s.anchor.0.min(s.head.0), s.anchor.0.max(s.head.0));
        let lines: Vec<&str> = (a..=b)
            .filter_map(|ix| {
                let (c0, c1) = self.sel_cols(ix, s.side, cx)?;
                let text = self.line_at(ix, s.side)?.text.as_str();
                let start = byte_at(text, c0);
                Some(&text[start..byte_at(text, c1).max(start)])
            })
            .collect();
        if !lines.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
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
fn code_line(this: &Entity<DiffView>, v: &DiffView, ix: usize, h: usize, i: usize, half: Option<bool>, p: &Palette, cx: &App) -> AnyElement {
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
    // 选中的文字底下垫一块强调色；等宽字体按列算位置
    let highlight = v.sel_cols(ix, half, cx).map(|(c0, c1)| {
        let cols = columns(&line.text);
        let (a, b) = (c0.min(cols), c1.min(cols));
        let w = (b.saturating_sub(a) as f32 + if c1 > cols { 0.5 } else { 0. }) * v.char_w;
        div().absolute().top_0().bottom_0().bg(alpha(p.accent, 0.35)).when(wrap, |s| s.left_0().right_0()).when(!wrap, |s| s.left(px(a as f32 * v.char_w - shift)).w(px(w)))
    });
    let left = v.code_left.clone();
    let side = (half == Some(false)) as usize;
    let code = div()
        .relative()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .cursor_text()
        .when(line.kind == '\\', |s| s.text_color(p.muted))
        .child(
            gpui::canvas(
                move |b, _, _| {
                    let mut l = left.get();
                    l[side] = f32::from(b.origin.x);
                    left.set(l);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .children(highlight)
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
    let numbers = match half {
        None => vec![no(line.old_no), no(line.new_no)],
        Some(old) => vec![no(if old { line.old_no } else { line.new_no })],
    };
    // 能评论的行：悬停时行号上出现「+」，在行号上按住拖选几行，松开就评论；在正文上拖仍是选字
    let mut gutter = div().relative().flex().flex_none().children(numbers);
    if v.on_comment.is_some() && line.kind != '\\' {
        let t3 = this.clone();
        gutter = gutter
            .cursor_pointer()
            .child(div().absolute().left(px(4.)).top(px(2.)).size(px(16.)).rounded(px(4.)).flex().items_center().justify_center().bg(p.accent).opacity(0.).group_hover(group.clone(), |s| s.opacity(1.)).child(icon(Icon::Plus, px(12.), p.on_accent)))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                t3.update(cx, |v, cx| v.pick_start(h, i, cx));
            });
    }
    row = row
        .child(gutter)
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
    let (t1, t2) = (this.clone(), this.clone());
    row = row
        .on_mouse_down(MouseButton::Left, move |e, window, cx| t1.update(cx, |v, cx| v.press(ix, half, e.position.x, window, cx)))
        .on_mouse_move(move |e, _, cx| {
            t2.update(cx, |v, cx| {
                let pressed = e.pressed_button == Some(MouseButton::Left);
                if v.dragging && pressed {
                    v.drag(ix, half, e.position.x, cx);
                }
                v.pick_to(h, i, pressed, cx);
            })
        });
    if editable {
        let this = this.clone();
        // 拖出了一段文字是在选字，不切换这一行的暂存选择
        row = row.cursor_pointer().on_click(move |_, _, cx| {
            this.update(cx, |v, cx| {
                if v.text_sel.is_none_or(|s| s.anchor == s.head) {
                    v.toggle(h, i, cx)
                }
            })
        });
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
        Item::Note(k) => {
            let n = &v.notes[k];
            let time = n.created.parse::<jiff::Timestamp>().map(|t| fmt_time(cx, t.as_second())).unwrap_or_default();
            div()
                .py(px(4.))
                .px(px(12.))
                .font_family(crate::theme::UI)
                .child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(R_MD)
                        .border_1()
                        .border_color(p.border)
                        .bg(p.panel)
                        .child(div().text_color(p.muted).child(format!("{} · {time}", n.author)))
                        .child(div().font_family(PROSE).text_size(FS_MD).line_height(gpui::relative(LH_BODY)).child(n.body.clone())),
                )
                .into_any_element()
        }
        Item::Line(h, i) => code_line(this, v, ix, h, i, None, &p, cx),
        Item::Split(h, l, r) => {
            let ratio = prefs(cx).split_ratio;
            let half = |side: Option<usize>, old: bool| match side {
                Some(i) => code_line(this, v, ix, h, i, Some(old), &p, cx),
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
        let font = window.text_system().resolve_font(&gpui::font(MONO));
        self.char_w = window.text_system().advance(font, FS_SM, '0').map_or(7.2, |s| f32::from(s.width));
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
        let in_zen = zen(cx).is_some();
        let bar = match self.zen.clone() {
            Some(view) => bar.child(sep(cx)).child(tool("zen", Icon::Maximize, t.diff_tools.zen.into(), in_zen).on_click(move |_, window, cx| set_zen(if in_zen { None } else { view.upgrade() }, window, cx))),
            None => bar,
        };

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
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                        if e.keystroke.modifiers.control && e.keystroke.key == "c" {
                            this.copy(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.dragging = false;
                            this.pick_end(window, cx);
                        }),
                    )
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
                            let max = this.max_scroll(cx);
                            this.scroll_x = (this.scroll_x - f32::from(dx)).clamp(0., max);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .vertical_scrollbar(&self.list);
                // 不换行且有超出的长行时，底部一条横向滚动条，可拖动；并排时两栏一起滚。
                // 底部留出滚动条的高度，滚到底时最后一行不被它挡住
                let max = self.max_scroll(cx);
                if !dp.wrap && max > 0. {
                    let width = self.width.get();
                    let view = width * if dp.side_by_side { dp.split_ratio.min(1. - dp.split_ratio) } else { 1. };
                    let thumb = (width * view / (view + max)).max(24.);
                    let (from, me) = (self.scroll_x, cx.entity());
                    body = body.pb(px(10.)).child(
                        div().absolute().left_0().right_0().bottom_0().h(px(10.)).bg(alpha(p.panel, 0.85)).border_t_1().border_color(p.border).child(
                            div()
                                .id("hscroll")
                                .absolute()
                                .top(px(2.))
                                .bottom(px(2.))
                                .left(px((width - thumb) * from / max))
                                .w(px(thumb))
                                .rounded(px(3.))
                                .bg(p.border_strong)
                                .hover(|s| s.bg(p.muted))
                                .on_mouse_down(gpui::MouseButton::Left, move |e, _, cx| {
                                    cx.stop_propagation();
                                    let me = me.clone();
                                    layout::start(
                                        e.position,
                                        true,
                                        move |delta, cx| {
                                            me.update(cx, |v, cx| {
                                                v.scroll_x = (from + delta * max / (width - thumb).max(1.)).clamp(0., max);
                                                cx.notify();
                                            })
                                        },
                                        cx,
                                    );
                                }),
                        ),
                    );
                }
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

    #[test]
    fn selection_columns_map_to_bytes() {
        // a 占第 0 列，制表符补到第 4 列，中占 4、5 两列
        let s = "a\t中b";
        assert_eq!(byte_at(s, 0), 0);
        assert_eq!(byte_at(s, 1), 1);
        assert_eq!(byte_at(s, 2), 1);
        assert_eq!(byte_at(s, 3), 2);
        assert_eq!(byte_at(s, 4), 2);
        assert_eq!(byte_at(s, 5), 2);
        assert_eq!(byte_at(s, 6), 5);
        assert_eq!(byte_at(s, usize::MAX), s.len());
    }
}
