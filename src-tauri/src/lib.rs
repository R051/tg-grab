mod api;
mod commands;
mod events;
mod config;
mod state;
mod tg;

use state::{Inner, SharedState};
use std::sync::Arc;
use tauri::Manager;

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
            std::fs::create_dir_all(&data_dir).ok();

            let downloads_dir = data_dir.join("downloads");
            std::fs::create_dir_all(&downloads_dir).ok();

            let config_path = data_dir.join("config.json");
            let session_path = data_dir.join("tggrab.session");

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
                queue_path: data_dir.join("queue.json"),
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
                queue: state::QueueState::new(data_dir.join("queue.json")),
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
                            tauri::async_runtime::block_on(st.queue.enqueue(urls));
                            log::info!("restored pending download tasks");
                        }
                    }
                }
            }

            app.manage(st.clone());

            // Download workers run for the whole app lifetime.
            let worker_st = st.clone();
            let watch_st = st.clone();
            tauri::async_runtime::block_on(async move {
                tg::queue::spawn_workers(worker_st);
                tg::watcher::spawn(watch_st);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
