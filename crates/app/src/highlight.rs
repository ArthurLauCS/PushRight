//! diff 里的语法高亮：按文件扩展名选语言，区块的旧侧和新侧各自作为一段完整代码高亮，再拆回逐行。
//! 整段高亮才能认出跨行的字符串和注释；区块以外的上文看不到，从区块中间开始的多行注释会着色不准（和 GitHub 一样）。
//! 语法定义第一次用到时在后台线程加载。

use std::ops::Range;
use std::sync::OnceLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// 记号的种类，对应主题里的 syn_* 颜色
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Token {
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Variable,
    Meta,
}

/// 一行里着色的片段（字节范围）
pub type Spans = Vec<(Range<usize>, Token)>;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// 语法定义里没有的扩展名
const EXTRA: &[(&str, &str)] = &[
    ("luau", "lua"),
    ("svelte", "html"),
    ("vue", "html"),
    ("jsonc", "json"),
    ("json5", "json"),
    ("mts", "ts"),
    ("cts", "ts"),
    ("rbxlx", "xml"),
    ("rbxmx", "xml"),
];

/// 路径对应的语法；认不出的返回 None，按纯文本显示
fn syntax_of(path: &str) -> Option<&'static SyntaxReference> {
    let name = path.rsplit('/').next()?.to_lowercase();
    let ext = name.rsplit_once('.').map_or(name.as_str(), |(_, e)| e);
    let ext = EXTRA.iter().find(|(e, _)| *e == ext).map_or(ext, |(_, to)| *to);
    let set = syntaxes();
    set.find_syntax_by_extension(ext).or_else(|| set.find_syntax_by_extension(&name)).filter(|s| s.name != "Plain Text")
}

struct Classes {
    rules: Vec<(Scope, Token)>,
}

fn classes() -> &'static Classes {
    static C: OnceLock<Classes> = OnceLock::new();
    C.get_or_init(|| {
        // 从最内层往外找第一个认得的 scope；顺序靠前的优先
        let rules = [
            ("comment", Token::Comment),
            ("string", Token::String),
            ("constant.character.escape", Token::String),
            ("constant.numeric", Token::Number),
            ("constant.language", Token::Keyword),
            ("keyword.operator.word", Token::Keyword),
            ("keyword.operator", Token::Keyword),
            ("keyword", Token::Keyword),
            // VS Code 的默认主题把 storage.type（fn、let、int、class）也染成关键字色
            ("storage", Token::Keyword),
            ("entity.name.function", Token::Function),
            ("support.function", Token::Function),
            ("meta.function-call", Token::Function),
            ("entity.name.tag", Token::Keyword),
            ("entity.name.type", Token::Type),
            ("entity.name.class", Token::Type),
            ("entity.other.inherited-class", Token::Type),
            ("support.type", Token::Type),
            ("support.class", Token::Type),
            ("entity.other.attribute-name", Token::Variable),
            ("variable.parameter", Token::Variable),
            ("variable.other.member", Token::Variable),
            ("variable.other.property", Token::Variable),
            ("support.variable", Token::Variable),
            ("meta.preprocessor", Token::Meta),
            ("punctuation.definition.annotation", Token::Meta),
            ("entity.name.section", Token::Function),
        ];
        Classes { rules: rules.into_iter().map(|(s, t)| (Scope::new(s).unwrap(), t)).collect() }
    })
}

fn classify(stack: &ScopeStack) -> Option<Token> {
    let rules = &classes().rules;
    stack.as_slice().iter().rev().find_map(|scope| rules.iter().find(|(r, _)| r.is_prefix_of(*scope)).map(|(_, t)| *t))
}

/// 超过这个长度的行多半是压缩过的代码，高亮既慢又没意义
const MAX_LINE: usize = 2000;

/// 一段连续代码逐行着色
fn highlight_block(lines: &[&str], syntax: &SyntaxReference) -> Vec<Spans> {
    let set = syntaxes();
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let text = format!("{line}\n");
        let mut spans = Spans::new();
        let Ok(ops) = state.parse_line(&text, set) else {
            out.push(spans);
            continue;
        };
        let mut start = 0;
        let mut push = |stack: &ScopeStack, end: usize, spans: &mut Spans| {
            let end = end.min(line.len());
            if end > start {
                if let Some(token) = classify(stack) {
                    match spans.last_mut() {
                        Some((r, t)) if *t == token && r.end == start => r.end = end,
                        _ => spans.push((start..end, token)),
                    }
                }
                start = end;
            }
        };
        for (pos, op) in ops {
            push(&stack, pos, &mut spans);
            let _ = stack.apply(&op);
        }
        push(&stack, line.len(), &mut spans);
        out.push(spans);
    }
    out
}

/// 一个区块各行的着色，下标与 `lines` 一一对应；不高亮时返回 None。
/// 删除行取自旧侧，新增行和上下文取自新侧
pub fn highlight_hunk(lines: &[engine::diff::Line], path: &str) -> Option<Vec<Spans>> {
    let syntax = syntax_of(path)?;
    if lines.iter().any(|l| l.text.len() > MAX_LINE) {
        return None;
    }
    let side = |skip: char| {
        let picked: Vec<&str> = lines.iter().filter(|l| l.kind != skip && l.kind != '\\').map(|l| l.text.as_str()).collect();
        highlight_block(&picked, syntax)
    };
    let (older, newer) = (side('+'), side('-'));
    let (mut o, mut n) = (0, 0);
    Some(
        lines
            .iter()
            .map(|l| match l.kind {
                '\\' => Spans::new(),
                '-' => {
                    o += 1;
                    older[o - 1].clone()
                }
                kind => {
                    if kind == ' ' {
                        o += 1;
                    }
                    n += 1;
                    newer[n - 1].clone()
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: char, text: &str) -> engine::diff::Line {
        engine::diff::Line { kind, text: text.into(), old_no: None, new_no: None }
    }

    #[test]
    fn colors_rust_tokens() {
        let spans = highlight_hunk(&[line(' ', "fn main() { let s = \"hi\"; // note")], "src/main.rs").unwrap();
        let text = "fn main() { let s = \"hi\"; // note";
        let of = |t: Token| spans[0].iter().filter(|(_, k)| *k == t).map(|(r, _)| &text[r.clone()]).collect::<Vec<_>>();
        assert!(of(Token::Keyword).iter().any(|s| s.contains("fn") || s.contains("let")), "{:?}", spans);
        assert!(of(Token::String).iter().any(|s| s.contains("hi")));
        assert!(of(Token::Comment).iter().any(|s| s.contains("note")));
    }

    #[test]
    fn deleted_lines_come_from_the_old_side() {
        // 删除行在旧侧开了一个多行注释，新侧没有：新增行不能被染成注释色
        let lines = [line('-', "/* old"), line('+', "let x = 1;"), line(' ', "*/")];
        let spans = highlight_hunk(&lines, "a.ts").unwrap();
        assert!(spans[0].iter().all(|(_, t)| *t == Token::Comment));
        assert!(spans[1].iter().all(|(_, t)| *t != Token::Comment), "{:?}", spans[1]);
    }

    #[test]
    fn unknown_and_minified_are_plain() {
        assert!(highlight_hunk(&[line(' ', "x")], "notes.unknownext").is_none());
        assert!(highlight_hunk(&[line(' ', &"a".repeat(MAX_LINE + 1))], "a.js").is_none());
        assert!(syntax_of("game/init.luau").is_some());
    }
}
