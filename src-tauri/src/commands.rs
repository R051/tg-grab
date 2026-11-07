//! Tauri commands exposed to the frontend via `invoke`.

use crate::config::{Config, WatchEntry};
use crate::state::SharedState;
use crate::tg::auth::{self, SignInOutcome};
use crate::tg::client::{ensure_client, logged_in_name, resolve_chat};
use crate::tg::history::HistoryPage;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbData {
    pub msg_id: i32,
    /// base64-encoded image bytes (jpeg).
    pub data: String,
}
use crate::tg::links::{make_msg_link, parse_chat_link};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

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

#[tauri::command]
pub async fn get_status(st: State<'_, SharedState>) -> Result<Status, String> {
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
    match ensure_client(&st).await {
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

#[tauri::command]
pub async fn save_api_config(
    st: State<'_, SharedState>,
    app: AppHandle,
    api_id: String,
    api_hash: String,
) -> Result<(), String> {
    let id: i32 = api_id.trim().parse().map_err(|_| "INVALID_API_ID")?;
    let hash = api_hash.trim().to_string();
    if hash.is_empty() {
        return Err("INVALID_API_ID".into());
    }
    {
        let mut cfg = st.config.write().await;
        cfg.api_id = Some(id);
        cfg.api_hash = Some(hash);
        save(&st, &cfg)?;
    }
    // Credentials changed → drop the old connection and session.
    crate::tg::client::reset_client(&st).await;
    let _ = std::fs::remove_file(&st.session_path);
    let _ = app;
    Ok(())
}

#[tauri::command]
pub async fn save_proxy(st: State<'_, SharedState>, app: AppHandle, proxy_url: String) -> Result<(), String> {
    {
        let mut cfg = st.config.write().await;
        let url = proxy_url.trim().to_string();
        cfg.proxy_url = if url.is_empty() { None } else { Some(url) };
        save(&st, &cfg)?;
    }
    crate::tg::client::reset_client(&st).await;
    let _ = app;
    Ok(())
}

#[tauri::command]
pub async fn save_settings(
    st: State<'_, SharedState>,
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
    save(&st, &cfg)
}

#[tauri::command]
pub async fn get_settings(st: State<'_, SharedState>) -> Result<Config, String> {
    Ok(st.config.read().await.clone())
}

// ── auth ──

#[tauri::command]
pub async fn auth_send_code(st: State<'_, SharedState>, phone: String) -> Result<(), String> {
    auth::send_code(&st, phone.trim()).await
}

#[tauri::command]
pub async fn auth_sign_in(
    st: State<'_, SharedState>,
    app: AppHandle,
    code: String,
) -> Result<SignInOutcome, String> {
    auth::sign_in(&st, &app, code.trim()).await
}

#[tauri::command]
pub async fn auth_check_password(
    st: State<'_, SharedState>,
    app: AppHandle,
    password: String,
) -> Result<String, String> {
    auth::check_password(&st, &app, password.trim()).await
}

#[tauri::command]
pub async fn logout(st: State<'_, SharedState>, app: AppHandle) -> Result<(), String> {
    auth::logout(&st).await?;
    let _ = app.emit("auth:changed", false);
    Ok(())
}

// ── download queue ──

#[tauri::command]
pub async fn download_urls(
    st: State<'_, SharedState>,
    urls: Vec<String>,
) -> Result<Vec<u64>, String> {
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

#[tauri::command]
pub async fn get_queue(st: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    Ok(crate::tg::queue::snapshot(&st).await)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedFile {
    pub name: String,
    pub size: u64,
}

#[tauri::command]
pub async fn list_files(st: State<'_, SharedState>) -> Result<Vec<DownloadedFile>, String> {
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

/// Reveal a downloaded file in the system file manager.
#[tauri::command]
pub async fn reveal_file(
    st: State<'_, SharedState>,
    app: AppHandle,
    name: String,
) -> Result<(), String> {
    // keep the name inside the downloads dir
    let safe = std::path::Path::new(&name)
        .file_name()
        .ok_or("INVALID_NAME")?;
    let path = st.downloads_dir.join(safe);
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|err| err.to_string())
}

/// Fetch (and cache) thumbnails for a set of history messages.
#[tauri::command]
pub async fn history_thumbs(
    st: State<'_, SharedState>,
    link: String,
    msg_ids: Vec<i32>,
) -> Result<Vec<ThumbData>, String> {
    let client = ensure_client(&st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    let chat_link = crate::tg::links::parse_chat_link(&link)?;
    let rc = resolve_chat(&st, &client, &chat_link).await?;

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
            crate::tg::history::fetch_thumb(&st, &client, &media, raw_id, msg_id).await
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

// ── watched channels ──

#[tauri::command]
pub async fn watch_list(st: State<'_, SharedState>) -> Result<Vec<WatchEntry>, String> {
    Ok(st.config.read().await.watched.clone())
}

#[tauri::command]
pub async fn watch_add(
    st: State<'_, SharedState>,
    app: AppHandle,
    link: String,
    backfill: bool,
) -> Result<WatchEntry, String> {
    let chat_link = parse_chat_link(&link)?;
    let client = ensure_client(&st).await?;
    let rc = resolve_chat(&st, &client, &chat_link).await?;
    let identity = &rc.identity;
    if identity.raw_id == 0 {
        return Err("NOT_A_CHANNEL".into());
    }

    let mut cfg = st.config.write().await;
    if cfg
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
        raw_id: identity.raw_id,
        access_hash: identity.access_hash,
        title: identity.title.clone().unwrap_or_else(|| "…".into()),
        username: identity.username.clone(),
        enabled: true,
        last_msg_id,
        chat_id: 0, // legacy field, kept for config compat
    };
    cfg.watched.push(entry.clone());
    save(&st, &cfg)?;
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
            let _ = app.emit("queue:pending", true);
        }
    }

    let _ = app.emit("watch:changed", true);
    Ok(entry)
}

#[tauri::command]
pub async fn watch_toggle(
    st: State<'_, SharedState>,
    app: AppHandle,
    raw_id: i64,
) -> Result<bool, String> {
    let mut cfg = st.config.write().await;
    let Some(entry) = cfg.watched.iter_mut().find(|w| w.raw_id == raw_id) else {
        return Err("NOT_FOUND".into());
    };
    entry.enabled = !entry.enabled;
    let enabled = entry.enabled;
    save(&st, &cfg)?;
    drop(cfg);
    let _ = app.emit("watch:changed", true);
    Ok(enabled)
}

#[tauri::command]
pub async fn watch_delete(
    st: State<'_, SharedState>,
    app: AppHandle,
    raw_id: i64,
) -> Result<(), String> {
    let mut cfg = st.config.write().await;
    let before = cfg.watched.len();
    cfg.watched.retain(|w| w.raw_id != raw_id);
    if cfg.watched.len() == before {
        return Err("NOT_FOUND".into());
    }
    save(&st, &cfg)?;
    drop(cfg);
    let _ = app.emit("watch:changed", true);
    Ok(())
}

/// Catch up now (manual trigger from the UI).
#[tauri::command]
pub async fn watch_catchup(st: State<'_, SharedState>, app: AppHandle) -> Result<(), String> {
    let client = ensure_client(&st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    crate::tg::watcher::catchup_watched(&st, &client, &app).await
}

// ── history ──

#[tauri::command]
pub async fn history_page(
    st: State<'_, SharedState>,
    link: String,
    offset_id: i32,
    date: Option<String>,
) -> Result<HistoryPage, String> {
    let client = ensure_client(&st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    let page = crate::tg::history::browse(&st, &client, &link, offset_id, date).await?;

    // Auto-remember the channel for the history page's quick chips.
    if page.chat.raw_id != 0 && offset_id == 0 {
        let mut cfg = st.config.write().await;
        if !cfg.recent.iter().any(|w| w.raw_id == page.chat.raw_id) {
            let entry = WatchEntry {
                chat_id: 0,
                raw_id: page.chat.raw_id,
                access_hash: 0,
                title: page.chat.title.clone(),
                username: crate::tg::links::parse_chat_link(&link)
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

/// Recently browsed channels (history page quick chips).
#[tauri::command]
pub async fn history_recent(st: State<'_, SharedState>) -> Result<Vec<WatchEntry>, String> {
    Ok(st.config.read().await.recent.clone())
}

/// Remove one channel from the history quick chips.
#[tauri::command]
pub async fn history_recent_remove(
    st: State<'_, SharedState>,
    raw_id: i64,
) -> Result<(), String> {
    let mut cfg = st.config.write().await;
    cfg.recent.retain(|w| w.raw_id != raw_id);
    save(&st, &cfg)
}
