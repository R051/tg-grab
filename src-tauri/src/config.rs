//! App configuration persistence (api credentials, proxy, watched channels, prefs).

use crate::tg::MediaKind;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One watched channel/supergroup for automatic downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEntry {
    /// Marked chat id (-100xxxx for channels/supergroups).
    pub chat_id: i64,
    /// Raw channel id (without the -100 prefix).
    pub raw_id: i64,
    /// Access hash captured when the entry was added (needed to re-resolve).
    #[serde(default)]
    pub access_hash: i64,
    pub title: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Only download messages newer than this id.
    #[serde(default)]
    pub last_msg_id: i32,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub api_id: Option<i32>,
    #[serde(default)]
    pub api_hash: Option<String>,
    /// socks5://host:port or http://host:port (empty = direct connection).
    #[serde(default)]
    pub proxy_url: Option<String>,
    /// Parallel download jobs (1 = sequential, gentlest).
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default)]
    pub watched: Vec<WatchEntry>,
    /// Recently browsed channels for the history page (auto-remembered).
    #[serde(default)]
    pub recent: Vec<WatchEntry>,
    /// UI language: "zh-CN" | "en".
    #[serde(default = "default_lang")]
    pub lang: String,
    /// Web mode: access token (auto-generated on first `--serve` run).
    #[serde(default)]
    pub server_token: Option<String>,
    /// Web mode: listen port.
    #[serde(default)]
    pub server_port: Option<u16>,
    /// Save downloads into <channel>/<date>/ subdirectories.
    #[serde(default)]
    pub organize_by_channel: bool,
    /// Further split organized downloads by media type (video/photo/...).
    #[serde(default)]
    pub classify_by_type: bool,
    /// Media kinds to auto-download: video/photo/audio/document.
    #[serde(default = "default_media_kinds")]
    pub media_kinds: Vec<String>,
    /// Custom downloads root (falls back to <data>/downloads).
    #[serde(default)]
    pub downloads_dir: Option<PathBuf>,
}

fn default_concurrency() -> usize {
    1
}

fn default_lang() -> String {
    "zh-CN".into()
}

fn default_media_kinds() -> Vec<String> {
    vec!["video".into()]
}

impl Config {
    pub fn load(path: &Path) -> Config {
        let mut cfg = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Config>(&bytes).ok())
            .unwrap_or_default();
        cfg.normalize();
        cfg
    }

    /// Derive-`Default` gives 0/empty for some fields; clamp to sane values.
    pub fn normalize(&mut self) {
        if self.concurrency == 0 {
            self.concurrency = 1;
        }
        if self.lang.is_empty() {
            self.lang = "zh-CN".into();
        }
        if self.media_kinds.is_empty() {
            self.media_kinds = default_media_kinds();
        }
        self.media_kinds
            .retain(|k| MediaKind::parse(k).is_some());
        self.concurrency = self.concurrency.clamp(1, 4);
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .and_then(|_| std::fs::rename(tmp, path))
            .map_err(|e| e.to_string())
    }
}
