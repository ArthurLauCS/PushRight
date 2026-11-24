//! 工作树总览：本仓库或所有仓库（打开的页签和最近打开的）的工作树，每个的文件夹、分支、未提交改动、
//! 和远程的差异、最后活动；已合并的可以一键清理。

use crate::i18n::{explain, t};
use crate::repo_view::RepoView;
use crate::rules::short;
use crate::theme::{palette, DISPLAY, FS_2XL, FS_LG, FS_SM, LH_BODY, MONO, PROSE, R_MD, R_SM};
use crate::time::fmt_time;
use crate::ui::btn;
use engine::overview::Group;
use gpui::prelude::*;
use gpui::{div, px, relative, Context, FontWeight, SharedString};
use gpui_component::tooltip::Tooltip;

/// 超过这么久（秒）没有提交也没有改文件，最后活动标成橙色
const STALE: i64 = 7 * 24 * 3600;

/// 路径的最后一段，即文件夹名
pub fn folder(path: &str) -> &str {
    let path = path.trim_end_matches(['\\', '/']);
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

pub fn render(this: &mut RepoView, cx: &mut Context<RepoView>) -> impl IntoElement {
    let p = palette(cx).clone();
    let t = t(cx);
    // 列：文件夹、分支、改动、同步、最后活动、操作。固定宽度的列，其余按比例分剩下的宽度，不窄于最小宽度
    let widths = [1.3, 1.0, 0.0, 0.0, 1.4, 0.0];
    let fixed = [None, None, Some(136.), Some(160.), None, Some(168.)];
    let mins = [160., 120., 0., 0., 160., 0.];
    let cols = |cells: [gpui::AnyElement; 6]| {
        div().flex().items_center().gap(px(16.)).children(cells.into_iter().enumerate().map(move |(i, c)| {
            let cell = div().min_w_0().overflow_hidden();
            match fixed[i] {
                Some(w) => cell.flex_none().w(px(w)),
                None => cell.flex_grow(widths[i]).flex_basis(px(0.)).min_w(px(mins[i])),
            }
            .child(c)
        }))
    };
    // 表格最窄的宽度：各列加列间距、行的左右留白和边框；窗口更窄时表格左右滚动
    let table_min = (0..6).map(|i| fixed[i].unwrap_or(mins[i])).sum::<f32>() + 16. * 5. + 32. + 2.;
    let s = |text: &str| div().truncate().child(text.to_owned()).into_any_element();
    let small = |text: String| div().text_size(FS_SM).text_color(p.muted).truncate().child(text);
    let mut table = div()
        .min_w(px(table_min))
        .border_1()
        .border_color(p.border)
        .rounded(R_MD)
        .bg(p.surface)
        .overflow_hidden()
        .child(cols([s(t.wt_folder), s(t.wt_branch), s(t.wt_changes), s(t.wt_sync), s(t.wt_last), s("")]).min_h(px(32.)).px(px(16.)).text_size(FS_SM).text_color(p.muted));
    // 总览还没读出来时先用侧栏刷新读到的本仓库工作树（没有未推送、已合并、最后活动）
    let fallback;
    let groups: &[Group] = if this.overview.is_empty() {
        fallback = [Group { path: this.path.clone(), worktrees: this.worktrees.clone() }];
        &fallback
    } else {
        &this.overview
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // 第一组是本仓库：只有它的工作树能在这里删除，其他仓库的到它们自己的页签里删
    for (gi, g) in groups.iter().enumerate() {
        if this.overview_all {
            table = table.child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(p.border)
                    .bg(p.bg)
                    .px(px(16.))
                    .py(px(6.))
                    .child(div().flex_none().font_family(DISPLAY).font_weight(FontWeight::SEMIBOLD).child(folder(&g.path).to_owned()))
                    .child(small(g.path.clone()).font_family(MONO)),
            );
        }
        for (i, w) in g.worktrees.iter().enumerate() {
            let current = gi == 0 && w.current;
            let folder_cell = div().child(div().font_family(DISPLAY).font_weight(FontWeight::SEMIBOLD).truncate().child(folder(&w.path).to_owned())).child(small(w.path.clone()).font_family(MONO)).into_any_element();
            let branch = div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().min_w_0().px(px(8.)).py(px(2.)).rounded(R_SM).text_color(p.accent).bg(p.selection).text_size(FS_SM).font_weight(FontWeight::BOLD).truncate().child(w.branch.as_deref().map(short).unwrap_or(t.detached).to_owned()))
                .when(w.merged, |s| {
                    s.child(
                        div()
                            .id(SharedString::from(format!("merged-{}", w.path)))
                            .flex_none()
                            .text_size(FS_SM)
                            .text_color(p.green)
                            .tooltip(move |window, cx| Tooltip::new(t.wt_merged_tip).build(window, cx))
                            .child(t.wt_merged),
                    )
                })
                .when(current, |s| s.child(small(t.wt_current.into()).flex_none()))
                .into_any_element();
            let changes = div().when(w.changes > 0, |s| s.text_color(p.orange)).child(if w.changes > 0 { (t.wt_dirty)(w.changes) } else { t.wt_clean.into() }).into_any_element();
            let sync = match w.ahead_behind {
                None if w.unpushed > 0 => s(&(t.wt_unpushed)(w.unpushed as usize)),
                None => small(t.wt_no_upstream.into()).into_any_element(),
                Some((0, 0)) => s(t.wt_synced),
                Some((a, b)) => s(&format!("{} · {}", (t.to_push)(a as usize), (t.to_pull)(b as usize))),
            };
            // 侧栏数据里没有最后活动，用最后一次提交的时间
            let active = w.active.max(w.time);
            let stale = active > 0 && now - active > STALE;
            let when = div()
                .id(SharedString::from(format!("active-{}", w.path)))
                .text_size(FS_SM)
                .text_color(if stale { p.orange } else { p.muted })
                .truncate()
                .when(stale, |s| s.tooltip(move |window, cx| Tooltip::new(t.wt_stale_tip).build(window, cx)))
                .child(if active > 0 { fmt_time(cx, active) } else { String::new() });
            let last = div().child(div().truncate().child(w.subject.clone())).child(when).into_any_element();
            let path = w.path.clone();
            let mut actions = div().flex().justify_end().gap(px(8.));
            if !current {
                let p1 = path.clone();
                actions = actions.child(btn(SharedString::from(format!("open-{path}"))).small().text(t.open_in_tab).on_click(cx.listener(move |this, _, window, cx| this.open_path(p1.clone(), window, cx))));
                // 第一个是主工作树，git 不允许删除
                if gi == 0 && i > 0 {
                    let wt = w.clone();
                    actions = actions.child(btn(SharedString::from(format!("remove-{path}"))).small().danger().text(t.wt_remove).no_cmd().tip(t.wt_remove).on_click(cx.listener(move |this, _, window, cx| this.remove_worktree(&wt, window, cx))));
                }
            }
            let p2 = path.clone();
            table = table.child(
                div()
                    .id(SharedString::from(format!("wt-row-{path}")))
                    .border_t_1()
                    .border_color(p.border)
                    .min_h(px(56.))
                    .px(px(16.))
                    .py(px(8.))
                    .when(current, |s| s.bg(p.selection).shadow(crate::theme::select_bar(&p)))
                    .when(!current, |s| s.hover(|s| s.bg(p.hover)))
                    .on_click(cx.listener(move |this, e: &gpui::ClickEvent, window, cx| {
                        if e.click_count() >= 2 && !current {
                            this.open_path(p2.clone(), window, cx);
                        }
                    }))
                    .child(cols([folder_cell, branch, changes, sync, last, actions.into_any_element()]).min_h(px(40.))),
            );
        }
    }
    let all = this.overview_all;
    let scope = |id: &'static str, text: &'static str, on: bool| {
        btn(id).small().text(text).on(on == all).tip(t.wt_scope_tip).on_click(cx.listener(move |this, _, _, cx| {
            if this.overview_all != on {
                this.overview_all = on;
                this.load_overview(cx);
            }
        }))
    };
    let cleanable = this.cleanable().len();
    div()
        .id("worktrees")
        .flex_1()
        .min_w_0()
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
                        .min_w_0()
                        .child(div().mb(px(8.)).font_family(DISPLAY).text_size(FS_2XL).font_weight(FontWeight::SEMIBOLD).child(t.worktrees))
                        .child(div().max_w(gpui::rems(38.)).text_color(p.muted).font_family(PROSE).text_size(FS_LG).line_height(relative(LH_BODY)).child(explain(cx).worktree.what)),
                )
                .child(btn("new-worktree").primary().text(t.new_worktree).on_click(cx.listener(|this, _, window, cx| this.add_worktree("HEAD".into(), false, window, cx)))),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .mb(px(12.))
                .child(scope("scope-this", t.wt_scope_this, false))
                .child(scope("scope-all", t.wt_scope_all, true))
                .child(div().flex_1())
                .child(btn("clean-worktrees").small().danger().text((t.wt_cleanup)(cleanable)).tip(t.wt_cleanup_tip).disabled(cleanable == 0).on_click(cx.listener(|this, _, window, cx| this.clean_worktrees(window, cx)))),
        )
        // 窗口窄时表格左右滚动，标题和按钮留在页面里
        .child(div().id("wt-table").overflow_x_scroll().child(table))
}
