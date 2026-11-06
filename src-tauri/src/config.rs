//! App configuration persistence (api credentials, proxy, watched channels, prefs).

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
    /// UI language: "zh-CN" | "en".
    #[serde(default = "default_lang")]
    pub lang: String,
    /// Where downloaded files go (absolute path, set on first run).
    #[serde(default)]
    pub downloads_dir: Option<PathBuf>,
}

fn default_concurrency() -> usize {
    1
}

fn default_lang() -> String {
    "zh-CN".into()
}

impl Config {
    pub fn load(path: &Path) -> Config {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}
