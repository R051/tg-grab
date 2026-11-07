//! UI-agnostic API layer: every operation the frontends can perform,
//! shared by the Tauri commands (desktop) and the axum invoke dispatcher (web).

use crate::config::{Config, WatchEntry};
use crate::state::{ActiveTask, SharedState};
use crate::tg::auth::{self, SignInOutcome};
use crate::tg::client::{ensure_client, logged_in_name, resolve_chat};
use crate::tg::history::HistoryPage;
use crate::tg::links::{make_msg_link, parse_chat_link};
use crate::tg::queue::TaskResult;
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

pub async fn save_api_config(st: &SharedState, api_id: String, api_hash: String) -> Result<(), String> {
    let id: i32 = api_id.trim().parse().map_err(|_| "INVALID_API_ID")?;
    let hash = api_hash.trim().to_string();
    if hash.is_empty() {
        return Err("INVALID_API_ID".into());
    }
    {
        let mut cfg = st.config.write().await;
        cfg.api_id = Some(id);
        cfg.api_hash = Some(hash);
        save(st, &cfg)?;
    }
    // Credentials changed → drop the old connection and session.
    crate::tg::client::reset_client(st).await;
    let _ = std::fs::remove_file(&st.session_path);
    Ok(())
}

pub async fn save_proxy(st: &SharedState, proxy_url: String) -> Result<(), String> {
    {
        let mut cfg = st.config.write().await;
        let url = proxy_url.trim().to_string();
        cfg.proxy_url = if url.is_empty() { None } else { Some(url) };
        save(st, &cfg)?;
    }
    crate::tg::client::reset_client(st).await;
    Ok(())
}

pub async fn save_settings(
    st: &SharedState,
    concurrency: Option<u32>,
    lang: Option<String>,
) -> Result<(), String> {
    let mut cfg = st.config.write().await;
    if let Some(c) = concurrency {
        cfg.concurrency = c.clamp(1, 4) as usize;
    }
    if let Some(l) = lang {
        cfg.lang = l;
    }
    save(st, &cfg)
}

pub async fn get_settings(st: &SharedState) -> Result<Config, String> {
    Ok(st.config.read().await.clone())
}

// ── auth ──

pub async fn auth_send_code(st: &SharedState, phone: String) -> Result<(), String> {
    auth::send_code(st, phone.trim()).await
}

pub async fn auth_sign_in(st: &SharedState, code: String) -> Result<SignInOutcome, String> {
    auth::sign_in(st, code.trim()).await
}

pub async fn auth_check_password(st: &SharedState, password: String) -> Result<String, String> {
    auth::check_password(st, password.trim()).await
}

pub async fn logout(st: &SharedState) -> Result<(), String> {
    auth::logout(st).await?;
    st.bus.emit("auth:changed", serde_json::Value::Bool(false));
    Ok(())
}

// ── download queue ──

pub async fn download_urls(st: &SharedState, urls: Vec<String>) -> Result<Vec<u64>, String> {
    if st.client.read().await.is_none() {
        return Err("NOT_LOGGED_IN".into());
    }
    let urls: Vec<String> = urls
        .into_iter()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect();
    if urls.is_empty() {
        return Err("INVALID_LINK".into());
    }
    Ok(st.queue.enqueue(urls).await)
}

pub async fn get_queue(st: &SharedState) -> Result<serde_json::Value, String> {
    Ok(crate::tg::queue::snapshot(st).await)
}

/// Raise a queued job's priority: move it up one slot, or to the very front.
pub async fn queue_move(st: &SharedState, id: u64, to_front: bool) -> Result<(), String> {
    let mut pending = st.queue.pending.lock().await;
    let idx = pending
        .iter()
        .position(|j| j.id == id)
        .ok_or("NOT_FOUND")?;
    let job = pending.remove(idx).unwrap();
    if to_front {
        pending.push_front(job);
    } else {
        let new_idx = idx.saturating_sub(1);
        pending.insert(new_idx, job);
    }
    drop(pending);
    st.queue.persist(&st.queue_path).await;
    Ok(())
}

/// Delete all cached thumbnails, returning bytes freed.
pub async fn clear_thumbs(st: &SharedState) -> Result<u64, String> {
    let thumbs_dir = st.downloads_dir.join("thumbs");
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
    Ok(freed)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedFile {
    pub name: String,
    pub size: u64,
}

pub async fn list_files(st: &SharedState) -> Result<Vec<DownloadedFile>, String> {
    let mut files = Vec::new();
    let mut rd = tokio::fs::read_dir(&st.downloads_dir)
        .await
        .map_err(|e| e.to_string())?;
    while let Ok(Some(entry)) = rd.next_entry().await {
        let Ok(meta) = entry.metadata().await else { continue };
        if meta.is_file() {
            files.push(DownloadedFile {
                name: entry.file_name().to_string_lossy().to_string(),
                size: meta.len(),
            });
        }
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(files)
}

pub async fn delete_file(st: &SharedState, name: String) -> Result<(), String> {
    let safe = std::path::Path::new(&name)
        .file_name()
        .ok_or("INVALID_NAME")?
        .to_string_lossy()
        .to_string();
    let path = st.downloads_dir.join(&safe);
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

    let mut cfg = st.config.write().await;
    if cfg.watched.iter().any(|w| w.raw_id == identity.raw_id) {
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
    cfg.watched.push(entry.clone());
    save(st, &cfg)?;
    drop(cfg);

    // Optional: pull the last 10 existing videos right away.
    if backfill {
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
                if crate::tg::video_meta(&media).is_some() {
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

pub async fn history_page(
    st: &SharedState,
    link: String,
    offset_id: i32,
    date: Option<String>,
) -> Result<HistoryPage, String> {
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    let page = crate::tg::history::browse(st, &client, &link, offset_id, date).await?;

    // Auto-remember the channel for the history page's quick chips.
    if page.chat.raw_id != 0 {
        let mut cfg = st.config.write().await;
        if !cfg.recent.iter().any(|w| w.raw_id == page.chat.raw_id) {
            let entry = WatchEntry {
                chat_id: 0,
                raw_id: page.chat.raw_id,
                access_hash: 0,
                title: page.chat.title.clone(),
                username: parse_chat_link(&link)
                    .ok()
                    .and_then(|l| match l {
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
            let snapshot = cfg.clone();
            drop(cfg);
            let _ = snapshot.save(&st.config_path);
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
