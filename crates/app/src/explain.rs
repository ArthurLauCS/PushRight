//! 每个操作的白话说明（文案在 i18n）和操作前后的示意图。菜单提示、确认对话框、「操作说明」页都从这里取。

use crate::i18n::{t, Explain};
use crate::theme::{alpha, palette, Palette, FS_LG, FS_MD, FS_SM, LH_BODY, MONO, PROSE, R_MD};
use gpui::prelude::*;
use gpui::{canvas, div, point, px, App, Bounds, FontWeight, Hsla, PathBuilder, Pixels, Point};

pub struct Node {
    pub id: String,
    pub x: f32,
    pub y: f32,
    /// 0 当前这条线，1 另一条线，2 远程
    pub color: usize,
    /// 这次操作新产生的
    pub new: bool,
    /// 操作后从分支上消失的
    pub ghost: bool,
}

pub struct Label {
    pub at: String,
    pub text: String,
    pub color: usize,
    /// 你当前所在的分支，画成实心
    pub here: bool,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<(String, String)>,
    pub labels: Vec<Label>,
}

/// 示意图的紧凑写法：
/// 节点 `id:x,y[:颜色[:new|ghost]]`，连线 `父>子`，标签 `节点=文字[:颜色][*]`（* 表示当前分支）
fn g(nodes: &str, edges: &str, labels: &str) -> Graph {
    Graph {
        nodes: nodes
            .split_whitespace()
            .map(|n| {
                let mut parts = n.split(':');
                let id = parts.next().unwrap().to_owned();
                let (x, y) = parts.next().unwrap().split_once(',').unwrap();
                let color = parts.next().filter(|c| !c.is_empty()).map_or(0, |c| c.parse().unwrap());
                let kind = parts.next().unwrap_or("");
                Node { id, x: x.parse().unwrap(), y: y.parse().unwrap(), color, new: kind == "new", ghost: kind == "ghost" }
            })
            .collect(),
        edges: edges.split_whitespace().map(|e| e.split_once('>').map(|(a, b)| (a.to_owned(), b.to_owned())).unwrap()).collect(),
        labels: labels
            .split_whitespace()
            .map(|l| {
                let here = l.ends_with('*');
                let (at, rest) = l.trim_end_matches('*').split_once('=').unwrap();
                let (text, color) = rest.split_once(':').map_or((rest, 0), |(t, c)| (t, c.parse().unwrap()));
                Label { at: at.to_owned(), text: text.to_owned(), color, here }
            })
            .collect(),
    }
}

// 两条分支的公共起点：main 上 A-B-C，feat 从 B 分出 D-E
const FORK: &str = "A:0,0 B:1,0 C:2,0 D:2,1:1 E:3,1:1";
const FORK_EDGES: &str = "A>B B>C B>D D>E";
const LINE: &str = "A:0,0 B:1,0 C:2,0";

/// 操作前后的示意图；没有图的操作返回 None
pub fn diagram(key: &str) -> Option<(Graph, Graph)> {
    let cat = |a: &str, b: &str| format!("{a} {b}");
    Some(match key {
        "commit" => (g(LINE, "A>B B>C", "C=main*"), g(&cat(LINE, "D:3,0::new"), "A>B B>C C>D", "D=main*")),
        "checkout" => (g(FORK, FORK_EDGES, "C=main* E=feat:1"), g(FORK, FORK_EDGES, "C=main E=feat:1*")),
        "create_branch" => (g(LINE, "A>B B>C", "C=main*"), g(LINE, "A>B B>C", "C=main C=new-branch:1*")),
        "merge" => (g(FORK, FORK_EDGES, "C=main* E=feat:1"), g(&cat(FORK, "M:4,0::new"), &cat(FORK_EDGES, "C>M E>M"), "M=main* E=feat:1")),
        "rebase" => (
            g(FORK, FORK_EDGES, "C=main E=feat:1*"),
            g("A:0,0 B:1,0 C:2,0 D:2,1:1:ghost E:3,1:1:ghost D':3,0:1:new E':4,0:1:new", "A>B B>C B>D D>E C>D' D'>E'", "C=main E'=feat:1*"),
        ),
        "cherry_pick" => (g(FORK, FORK_EDGES, "C=main* E=feat:1"), g(&cat(FORK, "E':3,0:1:new"), &cat(FORK_EDGES, "C>E'"), "E'=main* E=feat:1")),
        "revert" => (g(LINE, "A>B B>C", "C=main*"), g(&cat(LINE, "-C:3,0::new"), "A>B B>C C>-C", "-C=main*")),
        "reset" => (g(LINE, "A>B B>C", "C=main*"), g("A:0,0 B:1,0 C:2,0::ghost", "A>B B>C", "B=main*")),
        "fetch" => (g("A:0,0 B:1,0", "A>B", "B=main* B=origin/main:2"), g("A:0,0 B:1,0 C:2,0:2:new", "A>B B>C", "B=main* C=origin/main:2")),
        "pull" => (g("A:0,0 B:1,0", "A>B", "B=main* B=origin/main:2"), g("A:0,0 B:1,0 C:2,0::new", "A>B B>C", "C=main* C=origin/main:2")),
        // B 和 C 隔开两格：相邻节点上的两个标签会叠在一起
        "fast_forward" => (g("A:0,0 B:1,0 C:3,0:2", "A>B B>C", "B=main* C=origin/main:2"), g("A:0,0 B:1,0 C:3,0", "A>B B>C", "C=main* C=origin/main:2")),
        "push" => (g(LINE, "A>B B>C", "C=main* B=origin/main:2"), g(LINE, "A>B B>C", "C=main* C=origin/main:2")),
        _ => return None,
    })
}

const DX: f32 = 36.;
const DY: f32 = 46.;
const PAD: f32 = 32.;
const TOP: f32 = 60.;
const R: f32 = 9.;
const PILL_H: f32 = 18.;

/// 标签药丸的宽度：汉字按两个字符算
fn pill_w(text: &str) -> f32 {
    text.chars().map(|c| if (c as u32) > 255 { 11. } else { 6.2 }).sum::<f32>() + 14.
}

fn lane(p: &Palette, color: usize) -> Hsla {
    p.lanes[color.min(2)]
}

/// 一张示意图。两张图用同一个尺寸，前后对比时节点位置不跳
fn panel(title: &str, graph: Graph, width: f32, height: f32, p: &Palette) -> impl IntoElement {
    let x = |n: &Node| PAD + n.x * DX;
    let y = |n: &Node| TOP + n.y * DY;
    let at = |id: &str| graph.nodes.iter().find(|n| n.id == id).unwrap();
    // 第一行的标签放在节点上方，其余行放下方；同一个节点上的多个标签向外堆叠
    let mut stacked = std::collections::HashMap::<&str, usize>::new();
    let pills: Vec<_> = graph
        .labels
        .iter()
        .map(|l| {
            let n = at(&l.at);
            let i = stacked.entry(&l.at).or_default();
            let k = *i as f32;
            *i += 1;
            let up = n.y == 0.;
            let cy = if up { y(n) - 26. - k * (PILL_H + 4.) } else { y(n) + 26. + k * (PILL_H + 4.) };
            let stem = (k == 0.).then(|| if up { (cy + PILL_H / 2., y(n) - R - 2.) } else { (y(n) + R + 2., cy - PILL_H / 2.) });
            (l, x(n), cy, stem)
        })
        .collect();
    let edges: Vec<_> = graph.edges.iter().map(|(a, b)| {
        let (a, b) = (at(a), at(b));
        // 新提交从另一条线并过来时用来路的颜色
        let color = if b.new && a.y != b.y { a.color } else { b.color };
        ((x(a), y(a)), (x(b), y(b)), color, a.ghost || b.ghost)
    }).collect();
    let nodes: Vec<_> = graph.nodes.iter().map(|n| (x(n), y(n), n.color, n.new, n.ghost)).collect();
    let stems: Vec<_> = pills.iter().filter_map(|(l, cx, _, stem)| stem.map(|s| (*cx, s, l.color))).collect();
    let pc = p.clone();
    let paint = move |bounds: Bounds<Pixels>, window: &mut gpui::Window| {
        let o = bounds.origin;
        let pt = |x: f32, y: f32| o + point(px(x), px(y));
        // 分叉和汇合走曲线，不走斜线
        for &((x0, y0), (x1, y1), color, ghost) in &edges {
            let mut b = PathBuilder::stroke(px(2.));
            if ghost {
                b = b.dash_array(&[px(3.), px(4.)]);
            }
            b.move_to(pt(x0, y0));
            if y0 == y1 {
                b.line_to(pt(x1, y1));
            } else {
                b.cubic_bezier_to(pt(x1, y1), pt(x0 + DX * 0.6, y0), pt(x1 - DX * 0.6, y1));
            }
            if let Ok(path) = b.build() {
                window.paint_path(path, alpha(lane(&pc, color), if ghost { 0.4 } else { 1. }));
            }
        }
        for &(x, (y0, y1), color) in &stems {
            let mut b = PathBuilder::stroke(px(1.));
            b.move_to(pt(x, y0));
            b.line_to(pt(x, y1));
            if let Ok(path) = b.build() {
                window.paint_path(path, alpha(lane(&pc, color), 0.6));
            }
        }
        for &(x, y, color, new, ghost) in &nodes {
            let c = lane(&pc, color);
            if new {
                circle(window, pt(x, y), R + 5., alpha(c, 0.25), None);
            }
            // 消失的提交只剩虚线轮廓；其余是实心点
            if ghost {
                circle(window, pt(x, y), R, pc.bg, Some((alpha(c, 0.6), true)));
            } else {
                circle(window, pt(x, y), R, c, Some((c, false)));
            }
        }
    };
    let mut figure = div()
        .relative()
        .w(px(width))
        .h(px(height))
        .child(canvas(|_, _, _| {}, move |bounds, _, window, _| paint(bounds, window)).absolute().size_full());
    for n in &graph.nodes {
        figure = figure.child(
            div()
                .absolute()
                .left(px(x(n) - 12.))
                .top(px(y(n) - 8.))
                .w(px(24.))
                .h(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .font_family(MONO)
                .text_size(FS_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if n.ghost { p.muted } else { p.bg })
                .child(n.id.clone()),
        );
    }
    for (l, cx, cy, _) in pills {
        let c = lane(p, l.color);
        let w = pill_w(&l.text);
        figure = figure.child(
            div()
                .absolute()
                .left(px(cx - w / 2.))
                .top(px(cy - PILL_H / 2.))
                .w(px(w))
                .h(px(PILL_H))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(alpha(c, if l.here { 1. } else { 0.5 }))
                .bg(alpha(c, if l.here { 1. } else { 0.14 }))
                .font_family(MONO)
                .text_size(FS_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if l.here { p.bg } else { c })
                .child(l.text.clone()),
        );
    }
    div()
        .pt(px(8.))
        .px(px(4.))
        .pb(px(4.))
        .border_1()
        .border_color(p.border)
        .rounded(R_MD)
        .bg(p.bg)
        .child(div().pl(px(8.)).text_size(FS_SM).text_color(p.muted).child(title.to_owned()))
        .child(figure)
}

fn circle(window: &mut gpui::Window, c: Point<Pixels>, r: f32, fill: Hsla, stroke: Option<(Hsla, bool)>) {
    let ring = |b: &mut PathBuilder| {
        b.move_to(c + point(px(r), px(0.)));
        b.arc_to(point(px(r), px(r)), px(0.), false, false, c - point(px(r), px(0.)));
        b.arc_to(point(px(r), px(r)), px(0.), false, false, c + point(px(r), px(0.)));
        b.close();
    };
    let mut b = PathBuilder::fill();
    ring(&mut b);
    if let Ok(path) = b.build() {
        window.paint_path(path, fill);
    }
    if let Some((color, dashed)) = stroke {
        let mut b = PathBuilder::stroke(px(2.));
        if dashed {
            b = b.dash_array(&[px(3.), px(3.)]);
        }
        ring(&mut b);
        if let Ok(path) = b.build() {
            window.paint_path(path, color);
        }
    }
}

/// 操作前 → 操作后的两张示意图
pub fn op_diagram(before: Graph, after: Graph, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let t = t(cx);
    let all = || before.nodes.iter().chain(&after.nodes);
    let width = all().map(|n| n.x).fold(0., f32::max) * DX + PAD * 2.;
    let rows = all().map(|n| n.y).fold(0., f32::max) + 1.;
    let height = TOP + (rows - 1.) * DY + if rows > 1. { 52. } else { 26. };
    let arrow = canvas(|_, _, _| {}, {
        let muted = p.muted;
        move |b, _, window, _| {
            let o = b.origin;
            let mut path = PathBuilder::stroke(px(2.));
            path.move_to(o + point(px(2.), px(8.)));
            path.line_to(o + point(px(24.), px(8.)));
            path.move_to(o + point(px(18.), px(2.)));
            path.line_to(o + point(px(24.), px(8.)));
            path.line_to(o + point(px(18.), px(14.)));
            if let Ok(path) = path.build() {
                window.paint_path(path, muted);
            }
        }
    })
    .w(px(28.))
    .h(px(16.))
    .flex_none();
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(panel(t.before, before, width, height, p))
        .child(arrow)
        .child(panel(t.after, after, width, height, p))
}

/// 说明文字是写给人慢慢读的，用文楷和控件区分开
pub fn explainer(e: &Explain, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let lh = gpui::relative(LH_BODY);
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .font_family(PROSE)
        .text_size(FS_MD)
        .line_height(lh)
        .child(div().max_w(gpui::rems(36.)).text_size(FS_LG).child(e.what))
        .children(diagram(e.key).map(|(before, after)| op_diagram(before, after, cx)))
        .child(div().flex().flex_col().gap(px(4.)).pl(px(16.)).text_color(p.muted).children(e.result.iter().map(|r| div().flex().gap(px(6.)).child("•").child(div().flex_1().min_w_0().child(*r)))))
        .children(e.undo.map(|undo| {
            div()
                .px(px(12.))
                .py(px(8.))
                .border_l_2()
                .border_color(p.border)
                .text_color(p.muted)
                .child(div().child(gpui::StyledText::new(format!("{}{}", t(cx).if_wrong, undo)).with_highlights(vec![(0..t(cx).if_wrong.len(), gpui::HighlightStyle { color: Some(p.text), font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() })])))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compact_notation() {
        let (before, after) = diagram("rebase").unwrap();
        assert_eq!(before.nodes.len(), 5);
        let d = after.nodes.iter().find(|n| n.id == "D").unwrap();
        assert!(d.ghost && d.color == 1 && d.y == 1.);
        let e = after.nodes.iter().find(|n| n.id == "E'").unwrap();
        assert!(e.new && e.x == 4.);
        let l = &after.labels[1];
        assert_eq!((l.at.as_str(), l.text.as_str(), l.color, l.here), ("E'", "feat", 1, true));
        assert!(diagram("stash").is_none());
    }
}
