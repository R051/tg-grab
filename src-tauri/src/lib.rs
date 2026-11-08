pub mod api;
pub mod config;
pub mod events;
pub mod library;
pub mod state;
pub mod tg;

#[cfg(feature = "desktop")]
mod commands;

use state::{Inner, SharedState};
use std::sync::Arc;

/// Shared bootstrap for both shells: paths, state, queue restore, workers.
pub async fn init_core(data_dir: std::path::PathBuf) -> SharedState {
    tokio::fs::create_dir_all(&data_dir).await.ok();
    let config_path = data_dir.join("config.json");
    let config = config::Config::load(&config_path);
    let default_downloads_dir = data_dir.join("downloads");
    let downloads_dir = config
        .downloads_dir
        .clone()
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| default_downloads_dir.clone());
    tokio::fs::create_dir_all(&downloads_dir).await.ok();
    let session_path = data_dir.join("tggrab.session");
    let queue_path = data_dir.join("queue.json");

    // Leftover .part files without a chunk journal are junk; paired
    // ones (.part + .part.progress) are kept for resume.
    let mut directories = vec![downloads_dir.clone()];
    while let Some(directory) = directories.pop() {
        if let Ok(rd) = std::fs::read_dir(directory) {
            for entry in rd.flatten() {
                if entry
                    .file_type()
                    .map(|kind| kind.is_symlink())
                    .unwrap_or(true)
                {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    directories.push(path);
                    continue;
                }
                let ext = path.extension().map(|e| e.to_string_lossy().to_string());
                match ext.as_deref() {
                    Some("part") => {
                        let journal = path.with_extension("part.progress");
                        if !journal.exists() {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                    Some("progress") => {
                        let part_file = path.with_extension("");
                        if !part_file.exists() {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let st: SharedState = Arc::new(Inner {
        bus: events::EventBus::new(),
        queue_path: queue_path.clone(),
        config_path,
        session_path,
        default_downloads_dir,
        library: tokio::sync::Mutex::new(library::LibraryStore::load(
            data_dir.join("library.json"),
        )),
        file_locks: std::sync::Mutex::new(std::collections::HashMap::new()),
        config: tokio::sync::RwLock::new(config),
        client: tokio::sync::RwLock::new(None),
        session: tokio::sync::RwLock::new(None),
        pool_handle: tokio::sync::Mutex::new(None),
        updates_rx: tokio::sync::Mutex::new(None),
        login_token: tokio::sync::Mutex::new(None),
        password_token: tokio::sync::Mutex::new(None),
        auth_lock: tokio::sync::Mutex::new(()),
        connect_lock: tokio::sync::Mutex::new(()),
        catchup_lock: tokio::sync::Mutex::new(()),
        watcher_running: std::sync::atomic::AtomicBool::new(false),
        sweeper_running: std::sync::atomic::AtomicBool::new(false),
        window_close_pending: std::sync::atomic::AtomicBool::new(false),
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
pub mod desktop {
    use super::{SharedState, init_core};
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::{Manager, WindowEvent};

    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub fn run() {
        tauri::Builder::default()
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                // Second launch: focus the existing window instead.
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }))
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_dialog::init())
            .invoke_handler(tauri::generate_handler![
                crate::commands::get_status,
                crate::commands::save_api_config,
                crate::commands::save_proxy,
                crate::commands::save_settings,
                crate::commands::get_settings,
                crate::commands::auth_send_code,
                crate::commands::auth_sign_in,
                crate::commands::auth_check_password,
                crate::commands::logout,
                crate::commands::download_urls,
                crate::commands::get_queue,
                crate::commands::queue_move,
                crate::commands::queue_cancel,
                crate::commands::clear_thumbs,
                crate::commands::list_files,
                crate::commands::delete_file,
                crate::commands::reveal_file,
                crate::commands::watch_list,
                crate::commands::watch_add,
                crate::commands::watch_toggle,
                crate::commands::watch_delete,
                crate::commands::watch_catchup,
                crate::commands::history_page,
                crate::commands::joined_channels,
                crate::commands::history_search,
                crate::commands::history_thumbs,
                crate::commands::history_recent,
                crate::commands::history_recent_remove,
                crate::commands::library_get,
                crate::commands::favorite_save,
                crate::commands::favorite_remove,
                crate::commands::folder_save,
                crate::commands::folder_remove,
                crate::commands::library_export,
                crate::commands::library_import,
                crate::commands::choose_download_directory,
                crate::commands::media_preview_path,
                crate::commands::library_export_file,
                crate::commands::window_action,
                crate::commands::window_close_pending,
            ])
            .on_window_event(|window, event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let window = window.clone();
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let Some(st) = app.try_state::<SharedState>() else {
                            return;
                        };
                        use std::sync::atomic::Ordering;
                        if st.window_close_pending.swap(true, Ordering::SeqCst) {
                            return;
                        }
                        let action = st.config.read().await.close_action.clone();
                        match action.as_str() {
                            "tray" => {
                                let _ = window.hide();
                                st.window_close_pending.store(false, Ordering::SeqCst);
                            }
                            "exit" => {
                                st.queue.persist(&st.queue_path).await;
                                app.exit(0);
                            }
                            _ => {
                                use tauri::Emitter;
                                let _ = window.show();
                                let _ = window.set_focus();
                                if window.emit("desktop:close-requested", true).is_err() {
                                    st.window_close_pending.store(false, Ordering::SeqCst);
                                }
                            }
                        }
                    });
                }
            })
            .setup(|app| {
                // Use the same embedded brand asset for the window/taskbar and tray.
                let icon = tauri::include_image!("icons/128x128.png");
                if let Some(window) = app.get_webview_window("main") {
                    window.set_icon(icon.clone())?;
                }
                // Tray icon: the app keeps downloading while hidden.
                let show = MenuItem::with_id(app, "show", "显示 tg-grab", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let tray_menu = Menu::with_items(app, &[&show, &quit])?;
                TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&tray_menu)
                    .tooltip("tg-grab")
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(|tray, event| {
                        // Left click toggles window visibility.
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let visible = w.is_visible().unwrap_or(false);
                                if visible {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                    })
                    .on_menu_event(|app, ev| match ev.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .build(app)?;

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
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn startup_keeps_paired_resume_journals_in_organized_folders() {
        let root = std::env::temp_dir().join(format!(
            "tg-grab-resume-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let directory = root.join("downloads/channel/day");
        std::fs::create_dir_all(&directory).unwrap();
        let part = directory.join("clip.mp4.part");
        let journal = directory.join("clip.mp4.part.progress");
        std::fs::write(&part, b"partial").unwrap();
        std::fs::write(&journal, b"{}").unwrap();
        let state = super::init_core(root.clone()).await;
        assert!(part.exists());
        assert!(journal.exists());
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }
}
