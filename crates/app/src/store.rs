//! 界面偏好，存在 %APPDATA%\PushRight\settings.json。键名沿用网页版 localStorage 的，所有页签共用一份。

use gpui::{App, Global};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

pub struct Store {
    path: PathBuf,
    values: Map<String, Value>,
}

impl Global for Store {}

impl Store {
    pub fn load() -> Store {
        let dir = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("PushRight");
        let path = dir.join("settings.json");
        // 损坏的偏好不应阻止打开应用，当作空的
        let values = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Store { path, values }
    }

    /// 取不到或格式不对时返回 None
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.values.get(key).and_then(|v| T::deserialize(v).ok())
    }

    pub fn set(&mut self, key: &str, value: impl Serialize) {
        match serde_json::to_value(value) {
            Ok(Value::Null) | Err(_) => self.values.remove(key),
            Ok(v) => self.values.insert(key.to_owned(), v),
        };
        self.save();
    }

    /// 以 `prefix` 开头的键，「不再显示」的说明要一起清掉
    pub fn remove_prefix(&mut self, prefix: &str) {
        self.values.retain(|k, _| !k.starts_with(prefix));
        self.save();
    }

    fn save(&self) {
        // 先写临时文件再换名，写到一半被结束也不会留下半个文件
        let tmp = self.path.with_extension("tmp");
        let ok = std::fs::create_dir_all(self.path.parent().unwrap()).is_ok()
            && std::fs::write(&tmp, serde_json::to_vec_pretty(&self.values).unwrap()).is_ok();
        if ok {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

pub fn get<T: DeserializeOwned>(cx: &App, key: &str) -> Option<T> {
    cx.global::<Store>().get(key)
}

pub fn set(cx: &mut App, key: &str, value: impl Serialize) {
    cx.global_mut::<Store>().set(key, value);
}
