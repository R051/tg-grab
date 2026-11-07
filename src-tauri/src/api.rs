//! UI-agnostic API layer: every operation the frontends can perform,
//! shared by the Tauri commands (desktop) and the axum invoke dispatcher (web).

use crate::config::{Config, WatchEntry};
use crate::state::{ActiveTask, SharedState};
use crate::tg::MediaKind;
use crate::tg::auth::{self, SignInOutcome};
use crate::tg::client::{ensure_client, logged_in_name, resolve_chat};
use crate::tg::history::HistoryPage;
use crate::tg::links::{make_msg_link, parse_chat_link};
use serde::Serialize;

fn save(st: &SharedState, cfg: &Config) -> Result<(), String> {
    cfg.save(&st.config_path)
}

// ── status & settings ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub configured: bool,
    pub connected: bool,
    pub authorized: bool,
    pub user: Option<String>,
    pub error: Option<String>,
}

pub async fn get_status(st: &SharedState) -> Result<Status, String> {
    let cfg = st.config.read().await.clone();
    if cfg.api_id.is_none() || cfg.api_hash.is_none() {
        return Ok(Status {
            configured: false,
            connected: false,
            authorized: false,
            user: None,
            error: None,
        });
    }
    match ensure_client(st).await {
        Ok(client) => match client.is_authorized().await {
            Ok(true) => {
                let user = logged_in_name(&client).await;
                Ok(Status {
                    configured: true,
                    connected: true,
                    authorized: true,
                    user,
                    error: None,
                })
            }
            Ok(false) => Ok(Status {
                configured: true,
                connected: true,
                authorized: false,
                user: None,
                error: None,
            }),
            Err(e) => Ok(Status {
                configured: true,
                connected: true,
                authorized: false,
                user: None,
                error: Some(e.to_string()),
            }),
        },
        Err(e) => Ok(Status {
            configured: true,
            connected: false,
            authorized: false,
            user: None,
            error: Some(e),
        }),
    }
}

pub async fn save_api_config(
    st: &SharedState,
    api_id: String,
    api_hash: String,
) -> Result<(), String> {
    let _auth = st.auth_lock.lock().await;
    let id: i32 = api_id.trim().parse().map_err(|_| "INVALID_API_ID")?;
    let hash = if api_hash.trim().is_empty() {
        let cfg = st.config.read().await;
        if cfg.api_id != Some(id) {
            return Err("INVALID_API_ID".into());
        }
        cfg.api_hash.clone().ok_or("INVALID_API_ID")?
    } else {
        api_hash.trim().to_string()
    };
    if id <= 0 || hash.len() != 32 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("INVALID_API_ID".into());
    }
    {
        let mut cfg = st.config.write().await;
        if cfg.api_id == Some(id) && cfg.api_hash.as_deref() == Some(hash.as_str()) {
            return Ok(());
        }
        cfg.api_id = Some(id);
        cfg.api_hash = Some(hash);
        save(st, &cfg)?;
    }
    // Credentials changed → drop the old connection and session.
    crate::tg::client::reset_client(st).await;
    *st.login_token.lock().await = None;
    *st.password_token.lock().await = None;
    let _ = std::fs::remove_file(&st.session_path);
    Ok(())
}

pub async fn save_proxy(st: &SharedState, proxy_url: String) -> Result<(), String> {
    let _auth = st.auth_lock.lock().await;
    {
        let mut cfg = st.config.write().await;
        let url = proxy_url.trim().to_string();
        cfg.proxy_url = if url.is_empty() { None } else { Some(url) };
        save(st, &cfg)?;
    }
    crate::tg::client::reset_client(st).await;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct AppSettings {
    pub api_id: Option<i32>,
    pub api_hash: Option<String>,
    pub api_hash_set: bool,
    pub proxy_url: Option<String>,
    pub concurrency: usize,
    pub watched: Vec<WatchEntry>,
    pub lang: String,
    pub downloads_dir: Option<std::path::PathBuf>,
    pub effective_downloads_dir: std::path::PathBuf,
    pub default_downloads_dir: std::path::PathBuf,
    pub media_kinds: Vec<String>,
    pub classify_by_type: bool,
    pub organize_by_channel: bool,
    pub close_to_tray: bool,
    pub close_action: String,
}

pub async fn save_settings(
    st: &SharedState,
    concurrency: Option<u32>,
    lang: Option<String>,
    downloads_dir: Option<String>,
    media_kinds: Option<Vec<String>>,
    classify_by_type: Option<bool>,
    organize_by_channel: Option<bool>,
    close_to_tray: Option<bool>,
    close_action: Option<String>,
) -> Result<(), String> {
    let mut cfg = st.config.write().await;
    let mut next = cfg.clone();
    if let Some(value) = concurrency {
        next.concurrency = value.clamp(1, 4) as usize;
    }
    if let Some(value) = lang {
        if value != "zh-CN" && value != "en" {
            return Err("INVALID_LANGUAGE".into());
        }
        next.lang = value;
    }
    if let Some(value) = downloads_dir {
        let value = value.trim();
        let requested = if value.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(value))
        };
        if requested.as_ref().is_some_and(|path| !path.is_absolute()) {
            return Err("INVALID_DOWNLOAD_DIR".into());
        }
        if requested != cfg.downloads_dir {
            if !st.queue.in_flight.lock().await.is_empty() {
                return Err("DOWNLOADS_ACTIVE".into());
            }
            let root = requested.as_ref().unwrap_or(&st.default_downloads_dir);
            tokio::fs::create_dir_all(root)
                .await
                .map_err(|e| format!("INVALID_DOWNLOAD_DIR: {e}"))?;
            let probe = root.join(format!(
                ".tg-grab-write-check-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ));
            tokio::fs::write(&probe, [])
                .await
                .map_err(|e| format!("INVALID_DOWNLOAD_DIR: {e}"))?;
            let _ = tokio::fs::remove_file(probe).await;
        }
        next.downloads_dir = requested;
    }
    if let Some(kinds) = media_kinds {
        if kinds.is_empty() || kinds.iter().any(|kind| MediaKind::parse(kind).is_none()) {
            return Err("INVALID_MEDIA_KINDS".into());
        }
        next.media_kinds = kinds;
        next.media_kinds.sort();
        next.media_kinds.dedup();
    }
    if let Some(value) = classify_by_type {
        next.classify_by_type = value;
    }
    if let Some(value) = organize_by_channel {
        next.organize_by_channel = value;
    }
    if let Some(value) = close_to_tray {
        next.close_to_tray = value;
        if close_action.is_none() {
            next.close_action = if value { "tray" } else { "exit" }.into();
        }
    }
    if let Some(value) = close_action {
        if !["ask", "tray", "exit"].contains(&value.as_str()) {
            return Err("INVALID_CLOSE_ACTION".into());
        }
        next.close_to_tray = value != "exit";
        next.close_action = value;
    }
    save(st, &next)?;
    *cfg = next;
    Ok(())
}

pub async fn get_settings(st: &SharedState) -> Result<AppSettings, String> {
    let cfg = st.config.read().await.clone();
    let effective_downloads_dir = cfg
        .downloads_dir
        .clone()
        .unwrap_or(st.default_downloads_dir.clone());
    Ok(AppSettings {
        api_id: cfg.api_id,
        api_hash: None,
        api_hash_set: cfg.api_hash.is_some(),
        proxy_url: cfg.proxy_url,
        concurrency: cfg.concurrency,
        watched: cfg.watched,
        lang: cfg.lang,
        downloads_dir: cfg.downloads_dir,
        effective_downloads_dir,
        default_downloads_dir: st.default_downloads_dir.clone(),
        media_kinds: cfg.media_kinds,
        classify_by_type: cfg.classify_by_type,
        organize_by_channel: cfg.organize_by_channel,
        close_to_tray: cfg.close_to_tray,
        close_action: cfg.close_action,
    })
}

// ── auth ──

pub async fn auth_send_code(st: &SharedState, phone: String) -> Result<(), String> {
    auth::send_code(st, phone.trim()).await
}

pub async fn auth_sign_in(st: &SharedState, code: String) -> Result<SignInOutcome, String> {
    auth::sign_in(st, code.trim()).await
}

pub async fn auth_check_password(st: &SharedState, password: String) -> Result<String, String> {
    auth::check_password(st, &password).await
}

pub async fn logout(st: &SharedState) -> Result<(), String> {
    auth::logout(st).await?;
    st.bus.emit("auth:changed", serde_json::Value::Bool(false));
    Ok(())
}

// ── download queue ──

pub async fn download_urls(st: &SharedState, urls: Vec<String>) -> Result<Vec<u64>, String> {
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.map_err(|e| e.to_string())? {
        return Err("NOT_LOGGED_IN".into());
    }
    let urls: Vec<String> = urls
        .into_iter()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .map(|url| crate::tg::links::canonical_msg_link(&url))
        .collect::<Result<_, _>>()?;
    if urls.is_empty() {
        return Err("INVALID_LINK".into());
    }
    let ids = st.queue.enqueue(urls).await;
    st.bus.emit("queue:changed", serde_json::Value::Bool(true));
    Ok(ids)
}

/// Cancel a queued or in-flight download.
pub async fn queue_cancel(st: &SharedState, id: u64) -> Result<(), String> {
    crate::tg::queue::cancel_job(st, id).await?;
    st.bus.emit("queue:changed", serde_json::Value::Bool(true));
    Ok(())
}

pub async fn get_queue(st: &SharedState) -> Result<serde_json::Value, String> {
    Ok(crate::tg::queue::snapshot(st).await)
}

/// Raise a queued job's priority: move it up one slot, or to the very front.
pub async fn queue_move(st: &SharedState, id: u64, to_front: bool) -> Result<(), String> {
    let mut pending = st.queue.pending.lock().await;
    let idx = pending.iter().position(|j| j.id == id).ok_or("NOT_FOUND")?;
    let job = pending.remove(idx).unwrap();
    if to_front {
        pending.push_front(job);
    } else {
        let new_idx = idx.saturating_sub(1);
        pending.insert(new_idx, job);
    }
    drop(pending);
    st.queue.persist(&st.queue_path).await;
    st.bus.emit("queue:changed", serde_json::Value::Bool(true));
    Ok(())
}

/// Delete all cached thumbnails, returning bytes freed.
#[derive(Debug, Serialize)]
pub struct ClearedThumbs {
    pub freed: u64,
}

pub async fn clear_thumbs(st: &SharedState) -> Result<ClearedThumbs, String> {
    let thumbs_dir = st.default_downloads_dir.join("thumbs");
    let mut freed: u64 = 0;
    let mut cleared = false;
    if let Ok(rd) = std::fs::read_dir(&thumbs_dir) {
        cleared = true;
        for entry in rd.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    freed += meta.len();
                }
            }
            let _ = std::fs::remove_file(entry.path());
        }
    }
    if cleared {
        let _ = std::fs::remove_dir(&thumbs_dir);
    }
    Ok(ClearedThumbs { freed })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedFile {
    pub name: String,
    pub size: u64,
    pub kind: String,
    pub source_url: Option<String>,
}

pub async fn list_files(st: &SharedState) -> Result<Vec<DownloadedFile>, String> {
    let mut files = Vec::new();
    let base = st.downloads_root().await;
    let records: std::collections::HashMap<String, crate::library::DownloadRecord> = st
        .library
        .lock()
        .await
        .data
        .downloads
        .values()
        .filter(|record| record.root == base)
        .map(|record| (record.relative_path.clone(), record.clone()))
        .collect();
    let mut stack = vec![base.clone()];
    while let Some(dir) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let Ok(kind) = entry.file_type().await else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                if entry.file_name() != "thumbs" {
                    stack.push(entry.path());
                }
                continue;
            }
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.ends_with(".part")
                || filename.ends_with(".progress")
                || filename.ends_with(".tmp")
            {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(&base)
                .unwrap_or(entry.path().as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let record = records.get(&rel);
            let extension = entry
                .path()
                .extension()
                .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let kind = record.map(|record| record.kind.clone()).unwrap_or_else(|| {
                match extension.as_str() {
                    "mp4" | "mkv" | "webm" | "mov" => "video",
                    "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" => "photo",
                    "mp3" | "ogg" | "wav" | "m4a" | "flac" => "audio",
                    _ => "document",
                }
                .into()
            });
            files.push(DownloadedFile {
                name: rel,
                size: meta.len(),
                kind,
                source_url: record.map(|record| record.source_url.clone()),
            });
        }
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(files)
}

/// Resolve a user-supplied relative file name inside downloads_dir,
/// rejecting path traversal.
pub async fn resolve_download_path(
    st: &SharedState,
    rel: &str,
) -> Result<std::path::PathBuf, String> {
    let normalized = rel.replace('\\', "/");
    let rel_path = std::path::Path::new(&normalized);
    if rel_path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err("INVALID_NAME".into());
    }
    let root = st.downloads_root().await;
    let base = root.canonicalize().unwrap_or_else(|_| root.clone());
    let full = root.join(rel_path);
    let canon = full.canonicalize().map_err(|_| "NOT_FOUND".to_string())?;
    if !canon.starts_with(&base) {
        return Err("INVALID_NAME".into());
    }
    Ok(canon)
}

pub async fn delete_file(st: &SharedState, name: String) -> Result<(), String> {
    let path = resolve_download_path(st, &name).await?;
    if !path.exists() {
        return Err("NOT_FOUND".into());
    }
    tokio::fs::remove_file(&path)
        .await
        .map_err(|e| e.to_string())
}

// ── watched channels ──

pub async fn watch_list(st: &SharedState) -> Result<Vec<WatchEntry>, String> {
    Ok(st.config.read().await.watched.clone())
}

pub async fn watch_add(
    st: &SharedState,
    link: String,
    backfill: bool,
) -> Result<WatchEntry, String> {
    let chat_link = parse_chat_link(&link)?;
    let client = ensure_client(st).await?;
    let rc = resolve_chat(st, &client, &chat_link).await?;
    let identity = &rc.identity;
    if identity.raw_id == 0 {
        return Err("NOT_A_CHANNEL".into());
    }

    if st
        .config
        .read()
        .await
        .watched
        .iter()
        .any(|w| w.raw_id == identity.raw_id)
    {
        return Err("ALREADY_WATCHED".into());
    }

    // Start point = newest message id (only content posted after this gets downloaded).
    let last_msg_id = {
        let mut it = client.iter_messages(rc.peer);
        match it.next().await {
            Ok(Some(m)) => m.id(),
            _ => 0,
        }
    };

    let entry = WatchEntry {
        chat_id: 0, // legacy field, kept for config compat
        raw_id: identity.raw_id,
        access_hash: identity.access_hash,
        title: identity.title.clone().unwrap_or_else(|| "…".into()),
        username: identity.username.clone(),
        enabled: true,
        last_msg_id,
    };
    let mut cfg = st.config.write().await;
    if cfg.watched.iter().any(|w| w.raw_id == identity.raw_id) {
        return Err("ALREADY_WATCHED".into());
    }
    cfg.watched.push(entry.clone());
    save(st, &cfg)?;
    drop(cfg);

    // Optional: pull the last 10 existing videos right away.
    if backfill {
        let media_kinds = st.config.read().await.media_kinds.clone();
        let mut urls = Vec::new();
        let mut messages = client.iter_messages(rc.peer);
        loop {
            let Some(msg) = messages.next().await.map_err(|e| e.to_string())? else {
                break;
            };
            if urls.len() >= 10 {
                break;
            }
            if let Some(media) = msg.media() {
                let allowed = crate::tg::media_meta(&media)
                    .is_some_and(|meta| media_kinds.iter().any(|kind| kind == meta.kind.as_str()));
                if allowed {
                    urls.push(make_msg_link(
                        entry.username.as_deref(),
                        entry.raw_id,
                        msg.id(),
                    ));
                }
            }
        }
        urls.reverse();
        if !urls.is_empty() {
            st.queue.enqueue(urls).await;
            st.bus.emit("queue:pending", serde_json::Value::Bool(true));
        }
    }

    st.bus.emit("watch:changed", serde_json::Value::Bool(true));
    Ok(entry)
}

pub async fn watch_toggle(st: &SharedState, raw_id: i64) -> Result<bool, String> {
    let mut cfg = st.config.write().await;
    let Some(idx) = cfg.watched.iter().position(|w| w.raw_id == raw_id) else {
        return Err("NOT_FOUND".into());
    };
    cfg.watched[idx].enabled = !cfg.watched[idx].enabled;
    let enabled = cfg.watched[idx].enabled;
    save(st, &cfg)?;
    drop(cfg);
    st.bus.emit("watch:changed", serde_json::Value::Bool(true));
    Ok(enabled)
}

pub async fn watch_delete(st: &SharedState, raw_id: i64) -> Result<(), String> {
    let mut cfg = st.config.write().await;
    let before = cfg.watched.len();
    cfg.watched.retain(|w| w.raw_id != raw_id);
    if cfg.watched.len() == before {
        return Err("NOT_FOUND".into());
    }
    save(st, &cfg)?;
    drop(cfg);
    st.bus.emit("watch:changed", serde_json::Value::Bool(true));
    Ok(())
}

/// Catch up now (manual trigger from the UI).
pub async fn watch_catchup(st: &SharedState) -> Result<(), String> {
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    crate::tg::watcher::catchup_watched(st, &client).await
}

// ── history ──
pub async fn joined_channels(
    st: &SharedState,
) -> Result<crate::tg::directory::ChannelDirectory, String> {
    crate::tg::directory::list(st).await
}
pub async fn history_search(
    st: &SharedState,
    link: String,
    query: String,
    kind: String,
    offset_id: i32,
    day_start: Option<i64>,
    day_end: Option<i64>,
) -> Result<HistoryPage, String> {
    crate::tg::history::validate_search(&query, &kind, day_start, day_end)?;
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.map_err(|e| e.to_string())? {
        return Err("NOT_LOGGED_IN".into());
    }
    crate::tg::history::search(
        st, &client, &link, &query, &kind, offset_id, day_start, day_end,
    )
    .await
}

pub async fn history_page(
    st: &SharedState,
    link: String,
    offset_id: i32,
    date: Option<String>,
    day_start: Option<i64>,
    day_end: Option<i64>,
) -> Result<HistoryPage, String> {
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    let page =
        crate::tg::history::browse(st, &client, &link, offset_id, date, day_start, day_end).await?;

    // Auto-remember the channel for the history page's quick chips.
    if page.chat.raw_id != 0 {
        let mut cfg = st.config.write().await;
        if !cfg.recent.iter().any(|w| w.raw_id == page.chat.raw_id) {
            let entry = WatchEntry {
                chat_id: 0,
                raw_id: page.chat.raw_id,
                access_hash: 0,
                title: page.chat.title.clone(),
                username: parse_chat_link(&link).ok().and_then(|l| match l {
                    crate::tg::links::ChatLink::Username(u) => Some(u),
                    _ => None,
                }),
                enabled: false,
                last_msg_id: 0,
            };
            cfg.recent.insert(0, entry);
            let len = cfg.recent.len();
            if len > 12 {
                cfg.recent.truncate(12);
            }
            cfg.save(&st.config_path)?;
        }
    }
    Ok(page)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbData {
    pub msg_id: i32,
    /// base64-encoded image bytes (jpeg).
    pub data: String,
}

pub async fn history_thumbs(
    st: &SharedState,
    link: String,
    msg_ids: Vec<i32>,
) -> Result<Vec<ThumbData>, String> {
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    let chat_link = parse_chat_link(&link)?;
    let rc = resolve_chat(st, &client, &chat_link).await?;

    let messages = client
        .get_messages_by_id(rc.peer, &msg_ids)
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for (i, msg) in messages.into_iter().enumerate() {
        let Some(msg) = msg else { continue };
        let Some(media) = msg.media() else { continue };
        let raw_id = rc.identity.raw_id;
        let msg_id = msg_ids[i];
        if let Some(bytes) =
            crate::tg::history::fetch_thumb(st, &client, &media, raw_id, msg_id).await
        {
            use base64::Engine as _;
            out.push(ThumbData {
                msg_id,
                data: base64::engine::general_purpose::STANDARD.encode(&bytes),
            });
        }
    }
    Ok(out)
}

pub async fn history_recent(st: &SharedState) -> Result<Vec<WatchEntry>, String> {
    Ok(st.config.read().await.recent.clone())
}

pub async fn history_recent_remove(st: &SharedState, raw_id: i64) -> Result<(), String> {
    let mut cfg = st.config.write().await;
    cfg.recent.retain(|w| w.raw_id != raw_id);
    save(st, &cfg)
}

/// Desktop-only helper kept out of the web API surface.
#[allow(dead_code)]
pub fn unused_active_task_alias(_t: ActiveTask) {}
