//! 作者色和作者样式。颜色按名字哈希固定，同一个名字在任何仓库、任何时候都是同一个颜色。

use crate::store;
use crate::theme::palette;
use gpui::{App, Global, Hsla};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 用邮箱区分同名作者；没有邮箱的历史提交退回到姓名
pub fn author_key(name: &str, email: &str) -> String {
    if email.is_empty() { format!("name:{name}") } else { format!("email:{email}") }
}

/// 和网页版同一个哈希（按 UTF-16 码元），换版本后每个人的颜色不变
pub fn author_index(name: &str) -> usize {
    let mut h: u32 = 0;
    for unit in name.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(unit as u32);
    }
    (h % 10) as usize
}

pub fn author_color(name: &str, cx: &App) -> Hsla {
    palette(cx).authors[author_index(name)]
}

/// 可选：给某个作者的名字加边框和（或）底色，颜色都跟着作者色走
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorStyle {
    pub border: bool,
    pub fill: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(rename = "authorStyles", default)]
    pub author_styles: HashMap<String, AuthorStyle>,
}

impl Global for Prefs {}

pub fn init(cx: &mut App) {
    // 损坏的本地偏好不应阻止打开仓库
    let prefs = store::get::<Prefs>(cx, "prefs").unwrap_or_default();
    cx.set_global(prefs);
}

pub fn style(cx: &App, key: &str) -> AuthorStyle {
    cx.global::<Prefs>().author_styles.get(key).copied().unwrap_or_default()
}

pub fn set_style(cx: &mut App, key: String, style: AuthorStyle) {
    let prefs = cx.global_mut::<Prefs>();
    if style.border || style.fill {
        prefs.author_styles.insert(key, style);
    } else {
        prefs.author_styles.remove(&key);
    }
    let value = serde_json::to_value(&*cx.global::<Prefs>()).unwrap();
    store::set(cx, "prefs", value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_web_hash() {
        // 网页版：for (i of name) h = (h * 31 + name.charCodeAt(i)) >>> 0; h % 10
        assert_eq!(author_index("a"), 97 % 10);
        assert_eq!(author_index("ab"), ((97 * 31 + 98) % 10) as usize);
        // 超过 32 位要回绕，和 >>> 0 一致
        let long = "LiuBingqi ArthurLau 刘";
        let mut h: u64 = 0;
        for u in long.encode_utf16() {
            h = (h * 31 + u as u64) % (1 << 32);
        }
        assert_eq!(author_index(long), (h % 10) as usize);
    }

    #[test]
    fn keys_prefer_email() {
        assert_eq!(author_key("A", "a@x"), "email:a@x");
        assert_eq!(author_key("A", ""), "name:A");
    }
}
