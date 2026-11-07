//! Tauri command wrappers — thin shells over the shared api layer.

use crate::api;
use crate::config::WatchEntry;
use crate::state::SharedState;
use crate::tg::auth::SignInOutcome;
use crate::tg::history::HistoryPage;
use tauri::State;

#[tauri::command]
pub fn window_close_pending(st: State<'_, SharedState>) -> bool {
    st.window_close_pending
        .load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
pub async fn window_action(
    st: State<'_, SharedState>,
    app: tauri::AppHandle,
    action: String,
    remember: bool,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use tauri::Manager;
    if !["tray", "exit", "cancel"].contains(&action.as_str()) {
        return Err("INVALID_CLOSE_ACTION".into());
    }
    if remember && action != "cancel" {
        api::save_settings(
            &st,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(action.clone()),
        )
        .await?;
    }
    match action.as_str() {
        "exit" => {
            st.queue.persist(&st.queue_path).await;
            app.exit(0);
        }
        "tray" => {
            app.get_webview_window("main")
                .ok_or("WINDOW_NOT_FOUND")?
                .hide()
                .map_err(|error| error.to_string())?;
        }
        _ => {}
    }
    st.window_close_pending.store(false, Ordering::SeqCst);
    Ok(())
}

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
pub async fn save_proxy(st: State<'_, SharedState>, proxy_url: String) -> Result<(), String> {
    api::save_proxy(&st, proxy_url).await
}

#[tauri::command]
pub async fn save_settings(
    st: State<'_, SharedState>,
    concurrency: Option<u32>,
    lang: Option<String>,
    downloads_dir: Option<String>,
    media_kinds: Option<Vec<String>>,
    classify_by_type: Option<bool>,
    organize_by_channel: Option<bool>,
    close_to_tray: Option<bool>,
    close_action: Option<String>,
) -> Result<(), String> {
    api::save_settings(
        &st,
        concurrency,
        lang,
        downloads_dir,
        media_kinds,
        classify_by_type,
        organize_by_channel,
        close_to_tray,
        close_action,
    )
    .await
}

#[tauri::command]
pub async fn get_settings(st: State<'_, SharedState>) -> Result<api::AppSettings, String> {
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
pub async fn queue_move(st: State<'_, SharedState>, id: u64, to_front: bool) -> Result<(), String> {
    api::queue_move(&st, id, to_front).await
}

#[tauri::command]
pub async fn clear_thumbs(st: State<'_, SharedState>) -> Result<api::ClearedThumbs, String> {
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
    day_start: Option<i64>,
    day_end: Option<i64>,
) -> Result<HistoryPage, String> {
    api::history_page(&st, link, offset_id, date, day_start, day_end).await
}

#[tauri::command]
pub async fn joined_channels(
    st: State<'_, SharedState>,
) -> Result<crate::tg::directory::ChannelDirectory, String> {
    api::joined_channels(&st).await
}
#[tauri::command]
pub async fn history_search(
    st: State<'_, SharedState>,
    link: String,
    query: String,
    kind: String,
    offset_id: i32,
    day_start: Option<i64>,
    day_end: Option<i64>,
) -> Result<HistoryPage, String> {
    api::history_search(&st, link, query, kind, offset_id, day_start, day_end).await
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
pub async fn history_recent_remove(st: State<'_, SharedState>, raw_id: i64) -> Result<(), String> {
    api::history_recent_remove(&st, raw_id).await
}

/// Desktop-only: reveal a downloaded file in the system file manager.
#[tauri::command]
pub async fn reveal_file(
    st: State<'_, SharedState>,
    app: tauri::AppHandle,
    name: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let canon = api::resolve_download_path(&st, &name).await?;
    app.opener()
        .reveal_item_in_dir(&canon)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn library_get(
    st: State<'_, SharedState>,
) -> Result<crate::library::LibraryView, String> {
    crate::library::get(&st).await
}
#[tauri::command]
pub async fn favorite_save(
    st: State<'_, SharedState>,
    item: crate::library::FavoriteDraft,
) -> Result<crate::library::Favorite, String> {
    crate::library::save_favorite(&st, item).await
}
#[tauri::command]
pub async fn favorite_remove(st: State<'_, SharedState>, id: String) -> Result<(), String> {
    crate::library::remove_favorite(&st, id).await
}
#[tauri::command]
pub async fn folder_save(
    st: State<'_, SharedState>,
    id: Option<String>,
    name: String,
) -> Result<crate::library::Folder, String> {
    crate::library::save_folder(&st, id, name).await
}
#[tauri::command]
pub async fn folder_remove(st: State<'_, SharedState>, id: String) -> Result<(), String> {
    crate::library::remove_folder(&st, id).await
}
#[tauri::command]
pub async fn library_export(
    st: State<'_, SharedState>,
) -> Result<crate::library::LibraryExport, String> {
    crate::library::export(&st).await
}
#[tauri::command]
pub async fn library_import(
    st: State<'_, SharedState>,
    data: crate::library::LibraryExport,
) -> Result<crate::library::ImportSummary, String> {
    crate::library::import(&st, data).await
}

#[tauri::command]
pub async fn choose_download_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|path| {
                path.into_path()
                    .map(|path| path.to_string_lossy().to_string())
                    .map_err(|error| error.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn media_preview_path(
    st: State<'_, SharedState>,
    app: tauri::AppHandle,
    name: String,
) -> Result<String, String> {
    use tauri::Manager;
    let path = api::resolve_download_path(&st, &name).await?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn library_export_file(
    st: State<'_, SharedState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    let data = serde_json::to_vec_pretty(&crate::library::export(&st).await?)
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name("tg-grab-collections.json")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        std::fs::write(path.into_path().map_err(|error| error.to_string())?, data)
            .map_err(|error| error.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|error| error.to_string())?
}
