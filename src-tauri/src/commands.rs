//! Tauri command wrappers — thin shells over the shared api layer.

use crate::api;
use crate::config::{Config, WatchEntry};
use crate::state::SharedState;
use crate::tg::auth::SignInOutcome;
use crate::tg::history::HistoryPage;
use tauri::State;

#[tauri::command]
pub async fn get_status(st: State<'_, SharedState>) -> Result<api::Status, String> {
    api::get_status(&st).await
}

#[tauri::command]
pub async fn save_api_config(
    st: State<'_, SharedState>,
    api_id: String,
    api_hash: String,
) -> Result<(), String> {
    api::save_api_config(&st, api_id, api_hash).await
}

#[tauri::command]
pub async fn save_proxy(
    st: State<'_, SharedState>,
    proxy_url: String,
) -> Result<(), String> {
    api::save_proxy(&st, proxy_url).await
}

#[tauri::command]
pub async fn save_settings(
    st: State<'_, SharedState>,
    concurrency: Option<u32>,
    lang: Option<String>,
) -> Result<(), String> {
    api::save_settings(&st, concurrency, lang).await
}

#[tauri::command]
pub async fn get_settings(st: State<'_, SharedState>) -> Result<Config, String> {
    api::get_settings(&st).await
}

#[tauri::command]
pub async fn auth_send_code(st: State<'_, SharedState>, phone: String) -> Result<(), String> {
    api::auth_send_code(&st, phone).await
}

#[tauri::command]
pub async fn auth_sign_in(
    st: State<'_, SharedState>,
    code: String,
) -> Result<SignInOutcome, String> {
    api::auth_sign_in(&st, code).await
}

#[tauri::command]
pub async fn auth_check_password(
    st: State<'_, SharedState>,
    password: String,
) -> Result<String, String> {
    api::auth_check_password(&st, password).await
}

#[tauri::command]
pub async fn logout(st: State<'_, SharedState>) -> Result<(), String> {
    api::logout(&st).await
}

#[tauri::command]
pub async fn download_urls(
    st: State<'_, SharedState>,
    urls: Vec<String>,
) -> Result<Vec<u64>, String> {
    api::download_urls(&st, urls).await
}

#[tauri::command]
pub async fn get_queue(st: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    api::get_queue(&st).await
}

#[tauri::command]
pub async fn queue_move(
    st: State<'_, SharedState>,
    id: u64,
    to_front: bool,
) -> Result<(), String> {
    api::queue_move(&st, id, to_front).await
}

#[tauri::command]
pub async fn clear_thumbs(st: State<'_, SharedState>) -> Result<u64, String> {
    api::clear_thumbs(&st).await
}

#[tauri::command]
pub async fn queue_cancel(st: State<'_, SharedState>, id: u64) -> Result<(), String> {
    api::queue_cancel(&st, id).await
}

#[tauri::command]
pub async fn list_files(st: State<'_, SharedState>) -> Result<Vec<api::DownloadedFile>, String> {
    api::list_files(&st).await
}

#[tauri::command]
pub async fn delete_file(st: State<'_, SharedState>, name: String) -> Result<(), String> {
    api::delete_file(&st, name).await
}

#[tauri::command]
pub async fn watch_list(st: State<'_, SharedState>) -> Result<Vec<WatchEntry>, String> {
    api::watch_list(&st).await
}

#[tauri::command]
pub async fn watch_add(
    st: State<'_, SharedState>,
    link: String,
    backfill: bool,
) -> Result<WatchEntry, String> {
    api::watch_add(&st, link, backfill).await
}

#[tauri::command]
pub async fn watch_toggle(st: State<'_, SharedState>, raw_id: i64) -> Result<bool, String> {
    api::watch_toggle(&st, raw_id).await
}

#[tauri::command]
pub async fn watch_delete(st: State<'_, SharedState>, raw_id: i64) -> Result<(), String> {
    api::watch_delete(&st, raw_id).await
}

#[tauri::command]
pub async fn watch_catchup(st: State<'_, SharedState>) -> Result<(), String> {
    api::watch_catchup(&st).await
}

#[tauri::command]
pub async fn history_page(
    st: State<'_, SharedState>,
    link: String,
    offset_id: i32,
    date: Option<String>,
) -> Result<HistoryPage, String> {
    api::history_page(&st, link, offset_id, date).await
}

#[tauri::command]
pub async fn history_thumbs(
    st: State<'_, SharedState>,
    link: String,
    msg_ids: Vec<i32>,
) -> Result<Vec<api::ThumbData>, String> {
    api::history_thumbs(&st, link, msg_ids).await
}

#[tauri::command]
pub async fn history_recent(st: State<'_, SharedState>) -> Result<Vec<WatchEntry>, String> {
    api::history_recent(&st).await
}

#[tauri::command]
pub async fn history_recent_remove(
    st: State<'_, SharedState>,
    raw_id: i64,
) -> Result<(), String> {
    api::history_recent_remove(&st, raw_id).await
}

/// Desktop-only: reveal a downloaded file in the system file manager.
#[tauri::command]
pub async fn reveal_file(
    st: State<'_, SharedState>,
    app: tauri::AppHandle,
    name: String,
) -> Result<(), String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let rel_path = std::path::Path::new(&name);
    if rel_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("INVALID_NAME".into());
    }
    let base = st.downloads_dir.canonicalize().unwrap_or(st.downloads_dir.clone());
    let full = st.downloads_dir.join(rel_path);
    let canon = full.canonicalize().map_err(|_| "NOT_FOUND".to_string())?;
    if !canon.starts_with(&base) {
        return Err("INVALID_NAME".into());
    }
    app.opener()
        .reveal_item_in_dir(&canon)
        .map_err(|e| e.to_string())
}
