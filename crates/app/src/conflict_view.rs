//! 逐处解决冲突：每个冲突块选我方、对方、两边都要，或者手动写结果；全部选好后保存并标记已解决。

use crate::i18n::t;
use crate::icons::Icon;
use crate::theme::{alpha, palette, FS_MD, FS_SM, MONO, PROSE};
use crate::ui::{btn, label};
use engine::conflict::{Block, Conflict, Side};
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, FontWeight, ScrollHandle, Subscription, Window};
use gpui_component::input::{InputEvent, Textarea, TextareaState};

pub enum ConflictEvent {
    /// 每个冲突块选了哪一边，按出现顺序
    Resolve(Vec<Side>),
    /// 整个文件取一边：true 取对方
    Take(bool),
    /// 用外部合并工具打开
    External,
}

enum Choice {
    Ours,
    Theirs,
    Both,
    Custom(Entity<TextareaState>),
}

pub struct ConflictView {
    conflict: Option<Conflict>,
    pub rebasing: bool,
    /// 配了外部合并工具时才有那个按钮
    pub external: bool,
    choices: Vec<Option<Choice>>,
    scroll: ScrollHandle,
    /// 各冲突块在滚动区里是第几个子元素，上一处 / 下一处用
    sections: Vec<usize>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ConflictEvent> for ConflictView {}

impl ConflictView {
    pub fn new() -> ConflictView {
        ConflictView { conflict: None, rebasing: false, external: false, choices: Vec::new(), scroll: ScrollHandle::new(), sections: Vec::new(), _subs: Vec::new() }
    }

    pub fn set(&mut self, conflict: Option<Conflict>, cx: &mut Context<Self>) {
        let n = conflict.as_ref().map_or(0, |c| c.blocks.iter().filter(|b| matches!(b, Block::Conflict { .. })).count());
        self.choices = (0..n).map(|_| None).collect();
        self.sections = conflict.as_ref().map_or(Vec::new(), |c| c.blocks.iter().enumerate().filter(|(_, b)| matches!(b, Block::Conflict { .. })).map(|(i, _)| i).collect());
        self.conflict = conflict;
        self._subs.clear();
        cx.notify();
    }

    fn choose(&mut self, index: usize, choice: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Block::Conflict { ours, theirs, .. }) = self.conflict.as_ref().and_then(|c| c.blocks.get(self.sections[index])) else { return };
        self.choices[index] = Some(match choice {
            "ours" => Choice::Ours,
            "theirs" => Choice::Theirs,
            "both" => Choice::Both,
            _ => {
                if matches!(self.choices[index], Some(Choice::Custom(_))) {
                    return;
                }
                let text = format!("{ours}{theirs}");
                let rows = text.lines().count().min(19) + 1;
                let state = cx.new(|cx| {
                    let mut s = TextareaState::new(window, cx).rows(rows);
                    s.set_value(text, window, cx);
                    s
                });
                self._subs.push(cx.subscribe(&state, |_, _, _: &InputEvent, cx| cx.notify()));
                Choice::Custom(state)
            }
        });
        cx.notify();
    }

    fn jump(&mut self, forward: bool, cx: &mut Context<Self>) {
        let top = self.scroll.top_item();
        let target = if forward { self.sections.iter().find(|&&s| s > top) } else { self.sections.iter().rev().find(|&&s| s < top) };
        if let Some(&ix) = target {
            self.scroll.scroll_to_item(ix);
            cx.notify();
        }
    }

    fn resolve(&mut self, cx: &mut Context<Self>) {
        let sides: Option<Vec<Side>> = self
            .choices
            .iter()
            .map(|c| {
                c.as_ref().map(|c| match c {
                    Choice::Ours => Side::Ours,
                    Choice::Theirs => Side::Theirs,
                    Choice::Both => Side::Both,
                    Choice::Custom(s) => Side::Custom(s.read(cx).value().to_string()),
                })
            })
            .collect();
        if let Some(sides) = sides {
            cx.emit(ConflictEvent::Resolve(sides));
        }
    }
}

impl Render for ConflictView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let Some(conflict) = &self.conflict else {
            return div().flex_1().p(px(16.)).text_color(p.muted).bg(p.bg).child(t.pick_file).into_any_element();
        };
        let total = self.choices.len();
        let chosen = self.choices.iter().filter(|c| c.is_some()).count();
        let done = total > 0 && chosen == total;
        let hint = format!("{}{}", if conflict.binary { t.conflict_binary.to_owned() } else { (t.conflict_hint)(total) }, if self.rebasing { format!(" {}", t.rebase_conflict_hint) } else { String::new() });
        // 说明独占一行，按钮在下面一行并可换行：按钮再长也不会把说明挤成一列
        let mut header = div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .min_h(px(44.))
            .px(px(12.))
            .py(px(8.))
            .border_b_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(div().w_full().text_color(p.orange).font_family(PROSE).text_size(FS_MD).child(hint));
        if !conflict.binary {
            header = header
                .child(div().text_size(FS_SM).text_color(p.muted).whitespace_nowrap().child((t.conflict_chosen)(chosen, total)))
                .child(btn("conflict-prev").small().quiet().icon_size(Icon::Up, px(14.)).tip(t.conflict_previous).on_click(cx.listener(|this, _, _, cx| this.jump(false, cx))))
                .child(btn("conflict-next").small().quiet().icon_size(Icon::Down, px(14.)).tip(t.conflict_next).on_click(cx.listener(|this, _, _, cx| this.jump(true, cx))));
        }
        if self.external {
            header = header.child(btn("external-merge").small().text(t.external_tools.merge).on_click(cx.listener(|_, _, _, cx| cx.emit(ConflictEvent::External))));
        }
        header = header
            .child(btn("take-ours").small().text(t.take_ours).on_click(cx.listener(|_, _, _, cx| cx.emit(ConflictEvent::Take(false)))))
            .child(btn("take-theirs").small().text(t.take_theirs).on_click(cx.listener(|_, _, _, cx| cx.emit(ConflictEvent::Take(true)))));
        if !conflict.binary {
            header = header.child(btn("resolve").small().primary().text(t.resolve).disabled(!done).on_click(cx.listener(|this, _, _, cx| this.resolve(cx))));
        }

        let pre = |text: &str| div().px(px(16.)).py(px(4.)).font_family(MONO).text_size(FS_SM).line_height(px(20.)).whitespace_nowrap().child(text.to_owned());
        let mut body = div().id("conflict-body").track_scroll(&self.scroll).flex_1().min_h_0().overflow_y_scroll();
        let mut index = 0;
        for block in &conflict.blocks {
            body = match block {
                Block::Text { text } => body.child(pre(text).text_color(p.muted)),
                Block::Conflict { ours, theirs, ours_label, theirs_label } => {
                    let i = index;
                    index += 1;
                    let choice = &self.choices[i];
                    let custom = matches!(choice, Some(Choice::Custom(_)));
                    let dropped_ours = matches!(choice, Some(Choice::Theirs)) || custom;
                    let dropped_theirs = matches!(choice, Some(Choice::Ours)) || custom;
                    let side = |color, dropped: bool| div().border_l(px(3.)).border_color(color).bg(alpha(color, 0.08)).when(dropped, |s| s.opacity(0.35));
                    let bar = |name: &str, small: &str| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(12.))
                            .py(px(4.))
                            .text_size(FS_SM)
                            .child(div().flex_1().flex().items_baseline().font_weight(FontWeight::SEMIBOLD).child(label(name, true)).child(div().ml(px(8.)).font_family(MONO).font_weight(FontWeight::NORMAL).text_color(p.muted).child(small.to_owned())))
                    };
                    let shown = |text: &str| if text.is_empty() { t.empty_side.to_owned() } else { text.to_owned() };
                    let pick = |id: &'static str, text: &'static str, on: bool, which: &'static str, cx: &mut Context<Self>| btn((id, i)).small().text(text).on(on).on_click(cx.listener(move |this, _, window, cx| this.choose(i, which, window, cx)));
                    let mut section = div()
                        .my(px(8.))
                        .border_t_1()
                        .border_b_1()
                        .border_color(p.border)
                        .child(side(p.blue, dropped_ours).child(bar(t.ours, ours_label).child(pick("use-ours", t.use_this, matches!(choice, Some(Choice::Ours)), "ours", cx))).child(pre(&shown(ours)).when(dropped_ours, |s| s.line_through())))
                        .child(
                            side(p.green, dropped_theirs)
                                .child(
                                    bar(t.theirs, theirs_label)
                                        .child(pick("use-theirs", t.use_this, matches!(choice, Some(Choice::Theirs)), "theirs", cx))
                                        .child(pick("use-both", t.use_both, matches!(choice, Some(Choice::Both)), "both", cx))
                                        .child(btn(("edit", i)).small().text(t.edit_result).on(custom).tip(t.edit_result_hint).on_click(cx.listener(move |this, _, window, cx| this.choose(i, "custom", window, cx)))),
                                )
                                .child(pre(&shown(theirs)).when(dropped_theirs, |s| s.line_through())),
                        );
                    if let Some(Choice::Custom(state)) = choice {
                        section = section.child(side(p.orange, false).child(bar(t.edit_result_label, "")).child(div().mx(px(16.)).mb(px(8.)).child(Textarea::new(state).font_family(MONO).text_size(FS_SM))));
                    }
                    body.child(section)
                }
            };
        }
        div().flex().flex_col().flex_1().min_w_0().min_h_0().bg(p.bg).child(header).child(body).into_any_element()
    }
}
