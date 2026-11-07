pub mod api;
#[cfg(feature = "desktop")]
use tauri::Manager;

#[cfg(feature = "desktop")]
mod commands;
pub mod events;
pub mod config;
pub mod state;
pub mod tg;

use state::{Inner, SharedState};
use std::sync::Arc;

/// Shared bootstrap for both shells: paths, state, queue restore, workers.
pub async fn init_core(data_dir: std::path::PathBuf) -> SharedState {
    tokio::fs::create_dir_all(&data_dir).await.ok();
    let downloads_dir = data_dir.join("downloads");
    tokio::fs::create_dir_all(&downloads_dir).await.ok();

    let config_path = data_dir.join("config.json");
    let session_path = data_dir.join("tggrab.session");
    let queue_path = data_dir.join("queue.json");

    // Leftover .part files from an interrupted run are always junk.
    if let Ok(rd) = std::fs::read_dir(&downloads_dir) {
        for entry in rd.flatten() {
            if entry.path().extension().map(|e| e == "part").unwrap_or(false) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let config = config::Config::load(&config_path);

    let st: SharedState = Arc::new(Inner {
        bus: events::EventBus::new(),
        queue_path: queue_path.clone(),
        config_path,
        session_path,
        downloads_dir,
        config: tokio::sync::RwLock::new(config),
        client: tokio::sync::RwLock::new(None),
        session: tokio::sync::RwLock::new(None),
        pool_handle: tokio::sync::Mutex::new(None),
        updates_rx: tokio::sync::Mutex::new(None),
        login_token: tokio::sync::Mutex::new(None),
        password_token: tokio::sync::Mutex::new(None),
        queue: state::QueueState::new(queue_path),
    });

    // Restore pending download tasks from the previous session.
    if let Ok(bytes) = std::fs::read(&st.queue_path) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(jobs) = v.get("jobs").and_then(|j| j.as_array()) {
                let urls: Vec<String> = jobs
                    .iter()
                    .filter_map(|j| j.get("url").and_then(|u| u.as_str()).map(String::from))
                    .collect();
                if !urls.is_empty() {
                    st.queue.enqueue(urls).await;
                    log::info!("restored pending download tasks");
                }
            }
        }
    }

    tg::queue::spawn_workers(st.clone()).await;
    tg::watcher::spawn(st.clone());
    st
}

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::save_api_config,
            commands::save_proxy,
            commands::save_settings,
            commands::get_settings,
            commands::auth_send_code,
            commands::auth_sign_in,
            commands::auth_check_password,
            commands::logout,
            commands::download_urls,
            commands::get_queue,
            commands::list_files,
            commands::delete_file,
            commands::reveal_file,
            commands::watch_list,
            commands::watch_add,
            commands::watch_toggle,
            commands::watch_delete,
            commands::watch_catchup,
            commands::history_page,
            commands::history_thumbs,
            commands::history_recent,
            commands::history_recent_remove,
            commands::history_thumbs,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let st = tauri::async_runtime::block_on(async {
                let st = init_core(data_dir).await;
                app.manage(st.clone());
                Ok::<_, Box<dyn std::error::Error>>(st)
            })?;

            // Forward core events to the webview as window events.
            let fwd_app = app.handle().clone();
            let mut rx = st.bus.subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok((event, payload)) => {
                            let _ = tauri::Emitter::emit(&fwd_app, &event, payload);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });

            Ok(())
        })
                .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
