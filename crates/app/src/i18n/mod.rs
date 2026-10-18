//! 界面文案：中英两份，切换语言时整体换掉。只翻译应用自己的错误码，Git 输出和仓库内容原样显示。

mod types;
pub mod en;
pub mod zh;

pub use types::*;

use gpui::{App, Global};

/// 按数据取文案的表（状态字母、错误码、操作名…）
pub type Dict = &'static [(&'static str, &'static str)];

pub fn lookup(dict: Dict, key: &str) -> Option<&'static str> {
    dict.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// 一个操作的白话说明；示意图在 explain.rs，和语言无关
pub struct Explain {
    pub key: &'static str,
    pub name: &'static str,
    /// 一行以内，用在菜单和按钮提示里
    pub short: &'static str,
    pub what: &'static str,
    pub result: &'static [&'static str],
    /// 做错了怎么办
    pub undo: Option<&'static str>,
}

impl Explanations {
    pub fn get(&self, key: &str) -> &Explain {
        self.all().into_iter().find(|e| e.key == key).expect("unknown explanation")
    }
}

/// 千分位：1234567 → 1,234,567（中英文界面都用逗号）
pub fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    /// 存进设置里的名字，和网页版 localStorage 的取值一致
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Zh => "zh-CN",
        }
    }
    pub fn from_code(code: &str) -> Locale {
        if code == "zh-CN" { Locale::Zh } else { Locale::En }
    }
}

pub struct I18n {
    pub locale: Locale,
    pub t: &'static Messages,
    pub explain: &'static Explanations,
}

impl Global for I18n {}

impl I18n {
    pub fn new(locale: Locale) -> I18n {
        match locale {
            Locale::En => I18n { locale, t: &en::MESSAGES, explain: &en::EXPLANATIONS },
            Locale::Zh => I18n { locale, t: &zh::MESSAGES, explain: &zh::EXPLANATIONS },
        }
    }
}

pub fn t(cx: &App) -> &'static Messages {
    cx.global::<I18n>().t
}

pub fn explain(cx: &App) -> &'static Explanations {
    cx.global::<I18n>().explain
}

/// 应用自己的错误码（`PR_…: 细节`）换成当前语言的说明，其余原样返回
pub fn error_text(cx: &App, error: &str) -> String {
    let (code, detail) = error.split_once(": ").map_or((error, None), |(c, d)| (c, Some(d)));
    match lookup(t(cx).errors, code) {
        Some(text) => detail.map_or_else(|| text.to_owned(), |d| format!("{text}: {d}")),
        None => error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(group(0), "0");
        assert_eq!(group(999), "999");
        assert_eq!(group(1000), "1,000");
        assert_eq!(group(1234567), "1,234,567");
    }

    #[test]
    fn both_locales_have_every_explanation() {
        assert_eq!(en::EXPLANATIONS.all().len(), zh::EXPLANATIONS.all().len());
        assert_eq!(en::EXPLANATIONS.get("merge").key, "merge");
        assert_eq!(lookup(zh::MESSAGES.status, "M"), Some("修改"));
    }
}
