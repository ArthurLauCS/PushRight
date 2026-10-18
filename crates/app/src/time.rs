//! 提交时间的显示：按 IANA 时区名换算，每个提交按它自己那天的夏令时规则。空名字表示跟随系统时区。

use gpui::{App, Global};
use jiff::tz::TimeZone;
use jiff::Timestamp;

pub struct Zone {
    /// 存进设置里的时区名；空表示跟随系统
    pub name: String,
    tz: TimeZone,
}

impl Global for Zone {}

impl Zone {
    /// 认不出的名字（手改过设置、或时区库里没有）退回跟随系统
    pub fn new(name: &str) -> Zone {
        match TimeZone::get(name) {
            Ok(tz) if !name.is_empty() => Zone { name: name.to_owned(), tz },
            _ => Zone { name: String::new(), tz: TimeZone::system() },
        }
    }

    /// 秒级时间戳 → 2026-10-18 09:30
    pub fn format(&self, sec: i64) -> String {
        Timestamp::from_second(sec).map_or_else(|_| String::new(), |t| t.to_zoned(self.tz.clone()).strftime("%Y-%m-%d %H:%M").to_string())
    }

    fn date(&self, sec: i64) -> String {
        let mut s = self.format(sec);
        s.truncate(10);
        s
    }
}

pub fn fmt_time(cx: &App, sec: i64) -> String {
    cx.global::<Zone>().format(sec)
}

/// 时区下拉框的选项：UTC、本机时区、其余按名字排
pub fn zone_names() -> Vec<String> {
    let mut names: Vec<String> = jiff::tz::db().available().map(|n| n.to_string()).collect();
    names.sort();
    let local = TimeZone::system().iana_name().map(str::to_owned);
    let mut out = vec!["UTC".to_owned()];
    out.extend(local);
    for n in names {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

pub struct Month {
    pub name: String,
    pub count: usize,
}

pub struct Calendar {
    pub first: String,
    pub last: String,
    /// 从第一个提交的月份到最后一个，没有提交的月份也占一格
    pub months: Vec<Month>,
}

/// 仓库统计里的日期：最早、最晚的提交日期和每月提交数
pub fn calendar(times: &[i64], zone: &Zone) -> Calendar {
    let mut first = String::new();
    let mut last = String::new();
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for &sec in times {
        let date = zone.date(sec);
        *counts.entry(date[..7].to_owned()).or_default() += 1;
        if first.is_empty() || date < first {
            first = date.clone();
        }
        if date > last {
            last = date;
        }
    }
    let index = |date: &str| date[..4].parse::<i32>().unwrap() * 12 + date[5..7].parse::<i32>().unwrap() - 1;
    let mut months = Vec::new();
    if !first.is_empty() {
        for m in index(&first)..=index(&last) {
            let name = format!("{}-{:02}", m / 12, m % 12 + 1);
            let count = counts.get(&name).copied().unwrap_or(0);
            months.push(Month { name, count });
        }
    }
    Calendar { first, last, months }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_in_zone_with_daylight_saving() {
        let la = Zone::new("America/Los_Angeles");
        // 2026-07-01 00:00 UTC 是夏令时（UTC-7），2026-01-01 00:00 UTC 是标准时（UTC-8）
        assert_eq!(la.format(1_782_864_000), "2026-06-30 17:00");
        assert_eq!(la.format(1_767_225_600), "2025-12-31 16:00");
        assert_eq!(Zone::new("UTC").format(0), "1970-01-01 00:00");
    }

    #[test]
    fn unknown_zone_follows_system() {
        assert_eq!(Zone::new("Mars/Olympus").name, "");
        assert_eq!(Zone::new("").name, "");
    }

    #[test]
    fn calendar_fills_empty_months() {
        let utc = Zone::new("UTC");
        // 2026-01-15 和 2026-03-02
        let c = calendar(&[1_768_435_200, 1_772_409_600], &utc);
        assert_eq!((c.first.as_str(), c.last.as_str()), ("2026-01-15", "2026-03-02"));
        let months: Vec<_> = c.months.iter().map(|m| (m.name.as_str(), m.count)).collect();
        assert_eq!(months, [("2026-01", 1), ("2026-02", 0), ("2026-03", 1)]);
    }
}
