//! 工作树总览：每个工作树的文件夹、分支、未提交改动、和上游的同步状态、最后一次提交。

use crate::i18n::{explain, t};
use crate::repo_view::RepoView;
use crate::rules::short;
use crate::theme::{palette, FS_2XL, FS_LG, FS_SM, LH_BODY, MONO, PROSE, R_MD, R_SM};
use crate::time::fmt_time;
use crate::ui::btn;
use gpui::prelude::*;
use gpui::{div, px, relative, BoxShadow, Context, FontWeight, SharedString};

pub fn render(this: &mut RepoView, cx: &mut Context<RepoView>) -> impl IntoElement {
    let p = palette(cx).clone();
    let t = t(cx);
    // 列宽按比例：文件夹、分支、改动、同步、最后提交、操作
    let cols = |cells: [gpui::AnyElement; 6]| {
        let widths = [1.3, 1.0, 0.0, 0.0, 1.4, 0.0];
        let fixed = [None, None, Some(136.), Some(160.), None, Some(168.)];
        let mins = [160., 120., 0., 0., 160., 0.];
        div().flex().items_center().gap(px(16.)).children(cells.into_iter().enumerate().map(move |(i, c)| {
            let cell = div().min_w_0().overflow_hidden();
            match fixed[i] {
                Some(w) => cell.flex_none().w(px(w)),
                None => cell.flex_grow(widths[i]).flex_basis(px(0.)).min_w(px(mins[i])),
            }
            .child(c)
        }))
    };
    let s = |text: &str| div().truncate().child(text.to_owned()).into_any_element();
    let mut table = div()
        .border_1()
        .border_color(p.border)
        .rounded(R_MD)
        .bg(p.panel)
        .overflow_hidden()
        .child(cols([s(t.wt_folder), s(t.wt_branch), s(t.wt_changes), s(t.wt_sync), s(t.wt_last), s("")]).min_h(px(32.)).px(px(16.)).text_size(FS_SM).text_color(p.muted));
    for (i, w) in this.worktrees.iter().enumerate() {
        let name = w.path.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(&w.path).to_owned();
        let small = |text: String| div().text_size(FS_SM).text_color(p.muted).truncate().child(text);
        let folder = div().child(div().font_weight(FontWeight::BOLD).truncate().child(name)).child(small(w.path.clone()).font_family(MONO)).into_any_element();
        let branch = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().min_w_0().px(px(8.)).py(px(2.)).rounded(R_SM).text_color(p.accent).bg(p.accent_soft).text_size(FS_SM).font_weight(FontWeight::SEMIBOLD).truncate().child(w.branch.as_deref().map(short).unwrap_or(t.detached).to_owned()))
            .when(w.current, |s| s.child(small(t.wt_current.into()).flex_none()))
            .into_any_element();
        let changes = div().when(w.changes > 0, |s| s.text_color(p.yellow)).child(if w.changes > 0 { (t.wt_dirty)(w.changes) } else { t.wt_clean.into() }).into_any_element();
        let sync = match w.ahead_behind {
            None => small(t.wt_no_upstream.into()).into_any_element(),
            Some((0, 0)) => s(t.wt_synced),
            Some((a, b)) => s(&format!("{} · {}", (t.to_push)(a as usize), (t.to_pull)(b as usize))),
        };
        let last = div().child(div().truncate().child(w.subject.clone())).child(small(if w.time > 0 { fmt_time(cx, w.time) } else { String::new() })).into_any_element();
        let path = w.path.clone();
        let mut actions = div().flex().justify_end().gap(px(8.));
        if !w.current {
            let p1 = path.clone();
            actions = actions.child(btn(SharedString::from(format!("open-{path}"))).small().text(t.open_in_tab).on_click(cx.listener(move |this, _, window, cx| this.open_path(p1.clone(), window, cx))));
            // 第一个是主工作树，git 不允许删除
            if i > 0 {
                let wt = w.clone();
                actions = actions.child(btn(SharedString::from(format!("remove-{path}"))).small().danger().text(t.wt_remove).no_cmd().tip(t.wt_remove).on_click(cx.listener(move |this, _, window, cx| this.remove_worktree(&wt, window, cx))));
            }
        }
        let current = w.current;
        let p2 = path.clone();
        table = table.child(
            div()
                .id(SharedString::from(format!("wt-row-{path}")))
                .border_t_1()
                .border_color(p.border)
                .min_h(px(56.))
                .px(px(16.))
                .py(px(8.))
                .when(current, |s| s.bg(p.accent_soft).shadow(vec![BoxShadow { color: p.accent, offset: gpui::point(px(2.), px(0.)), blur_radius: px(0.), spread_radius: px(0.), inset: true }]))
                .when(!current, |s| s.hover(|s| s.bg(p.hover)))
                .on_click(cx.listener(move |this, e: &gpui::ClickEvent, window, cx| {
                    if e.click_count() >= 2 && !current {
                        this.open_path(p2.clone(), window, cx);
                    }
                }))
                .child(cols([folder, branch, changes, sync, last, actions.into_any_element()]).min_h(px(40.))),
        );
    }
    div()
        .id("worktrees")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(32.))
        .py(px(24.))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(24.))
                .mb(px(24.))
                .child(
                    div()
                        .flex_1()
                        .child(div().mb(px(8.)).font_family(crate::theme::DISPLAY).text_size(FS_2XL).font_weight(FontWeight::BOLD).child(t.worktrees))
                        .child(div().max_w(gpui::rems(38.)).text_color(p.muted).font_family(PROSE).text_size(FS_LG).line_height(relative(LH_BODY)).child(explain(cx).worktree.what)),
                )
                .child(btn("new-worktree").primary().text(t.new_worktree).on_click(cx.listener(|this, _, window, cx| this.add_worktree("HEAD".into(), false, window, cx)))),
        )
        .child(table)
}
