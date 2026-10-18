//! 差异配色：新增、删除各一个六位 HEX 颜色，存在设置的 diffColors 里；没设置时用主题自带的绿和红。

use gpui::{rgb, Hsla};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffColors {
    pub added: String,
    pub deleted: String,
}

/// 色盲友好的几组预设：名字是文案里的键
pub const PRESETS: [(&str, &str, &str); 3] = [
    ("blueOrange", "#0072b2", "#e69f00"),
    ("purpleGold", "#aa88dd", "#d9a400"),
    ("cyanRose", "#00a6b2", "#cc79a7"),
];

pub fn is_diff_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn hex(value: &str) -> Option<Hsla> {
    is_diff_color(value).then(|| rgb(u32::from_str_radix(&value[1..], 16).unwrap()).into())
}

impl DiffColors {
    pub fn hsla(&self) -> Option<(Hsla, Hsla)> {
        Some((hex(&self.added)?, hex(&self.deleted)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_six_digit_hex() {
        assert!(is_diff_color("#0072b2"));
        assert!(is_diff_color("#ABCDEF"));
        assert!(!is_diff_color("0072b2"));
        assert!(!is_diff_color("#0072b"));
        assert!(!is_diff_color("#0072bz"));
        assert!(DiffColors { added: "#000000".into(), deleted: "red".into() }.hsla().is_none());
    }
}
