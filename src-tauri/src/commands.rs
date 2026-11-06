//! Tauri commands exposed to the frontend via `invoke`.

use crate::config::{Config, WatchEntry};
use crate::state::SharedState;
use crate::tg::auth::{self, SignInOutcome};
use crate::tg::client::{ensure_client, logged_in_name, resolve_chat};
use crate::tg::history::HistoryPage;
use crate::tg::links::{make_msg_link, parse_chat_link};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
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
) -> Result<HistoryPage, String> {
    let client = ensure_client(&st).await?;
    if !client.is_authorized().await.unwrap_or(false) {
        return Err("NOT_LOGGED_IN".into());
    }
    crate::tg::history::browse(&st, &client, &link, offset_id).await
}
