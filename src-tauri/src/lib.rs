mod commands;
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
            commands::reveal_file,
            commands::watch_list,
            commands::watch_add,
            commands::watch_toggle,
            commands::watch_delete,
            commands::watch_catchup,
            commands::history_page,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir).ok();

            let downloads_dir = data_dir.join("downloads");
            std::fs::create_dir_all(&downloads_dir).ok();

            let config_path = data_dir.join("config.json");
            let session_path = data_dir.join("tggrab.session");

            let config = config::Config::load(&config_path);

            let st: SharedState = Arc::new(Inner {
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
                queue: state::QueueState::new(),
            });
            app.manage(st.clone());

            // Download workers run for the whole app lifetime.
            let worker_st = st.clone();
            let worker_app = app.handle().clone();
            tg::queue::spawn_workers(worker_st, worker_app);

            // The watcher waits for authorization, then streams updates.
            let watch_st = st.clone();
            let watch_app = app.handle().clone();
            tg::watcher::spawn(watch_st, watch_app);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
