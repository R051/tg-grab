//! tg-grab web server: serves the SPA + invoke dispatcher + WS event stream.
//!
//! Run with `--no-default-features --features server`. Data lives in
//! `$TG_GRAB_DATA_DIR` (default ./data); frontend build in `$TG_GRAB_DIST`
//! (default ./dist). Access requires the token from config (auto-generated
//! on first run and printed to the console).

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State, WebSocketUpgrade, ws::Message},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use tg_grab_lib::state::SharedState;
use tower_http::services::{ServeDir, ServeFile};

type St = SharedState;

#[cfg(test)]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("secure randomness unavailable");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn to_json<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn s_arg(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn i32_arg(v: &Value, k: &str) -> i32 {
    v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) as i32
}

fn i64_arg(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Generic invoke dispatcher — mirrors the Tauri command surface 1:1.
async fn invoke_dispatch(st: St, cmd: String, body: Value) -> Result<Value, String> {
    Ok(match cmd.as_str() {
        "get_status" => to_json(tg_grab_lib::api::get_status(&st).await?)?,
        "save_api_config" => {
            tg_grab_lib::api::save_api_config(&st, s_arg(&body, "apiId"), s_arg(&body, "apiHash"))
                .await?;
            Value::Null
        }
        "save_proxy" => {
            tg_grab_lib::api::save_proxy(&st, s_arg(&body, "proxyUrl")).await?;
            Value::Null
        }
        "save_settings" => {
            tg_grab_lib::api::save_settings(
                &st,
                body.get("concurrency")
                    .and_then(|x| x.as_u64())
                    .map(|x| x as u32),
                body.get("lang").and_then(|x| x.as_str()).map(String::from),
                body.get("downloadsDir")
                    .and_then(|x| x.as_str())
                    .map(String::from),
                body.get("mediaKinds").and_then(|x| x.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|k| k.as_str())
                        .map(String::from)
                        .collect()
                }),
                body.get("classifyByType").and_then(|x| x.as_bool()),
                body.get("organizeByChannel").and_then(|x| x.as_bool()),
                body.get("closeToTray").and_then(|x| x.as_bool()),
                body.get("closeAction")
                    .and_then(|x| x.as_str())
                    .map(String::from),
            )
            .await?;
            Value::Null
        }
        "get_settings" => to_json(tg_grab_lib::api::get_settings(&st).await?)?,
        "auth_send_code" => {
            tg_grab_lib::api::auth_send_code(&st, s_arg(&body, "phone")).await?;
            Value::Null
        }
        "auth_sign_in" => {
            to_json(tg_grab_lib::api::auth_sign_in(&st, s_arg(&body, "code")).await?)?
        }
        "auth_check_password" => {
            to_json(tg_grab_lib::api::auth_check_password(&st, s_arg(&body, "password")).await?)?
        }
        "logout" => {
            tg_grab_lib::api::logout(&st).await?;
            Value::Null
        }
        "download_urls" => to_json(
            tg_grab_lib::api::download_urls(
                &st,
                body.get("urls")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|u| u.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
            )
            .await?,
        )?,
        "get_queue" => tg_grab_lib::api::get_queue(&st).await?,
        "queue_move" => {
            tg_grab_lib::api::queue_move(
                &st,
                i64_arg(&body, "id") as u64,
                body.get("toFront")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
            )
            .await?;
            Value::Null
        }
        "queue_cancel" => {
            tg_grab_lib::api::queue_cancel(&st, i64_arg(&body, "id") as u64).await?;
            Value::Null
        }
        "clear_thumbs" => to_json(tg_grab_lib::api::clear_thumbs(&st).await?)?,
        "list_files" => to_json(tg_grab_lib::api::list_files(&st).await?)?,
        "delete_file" => {
            tg_grab_lib::api::delete_file(&st, s_arg(&body, "name")).await?;
            Value::Null
        }
        "watch_list" => to_json(tg_grab_lib::api::watch_list(&st).await?)?,
        "watch_add" => to_json(
            tg_grab_lib::api::watch_add(
                &st,
                s_arg(&body, "link"),
                body.get("backfill")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
            )
            .await?,
        )?,
        "watch_toggle" => {
            to_json(tg_grab_lib::api::watch_toggle(&st, i64_arg(&body, "rawId")).await?)?
        }
        "watch_delete" => {
            tg_grab_lib::api::watch_delete(&st, i64_arg(&body, "rawId")).await?;
            Value::Null
        }
        "watch_catchup" => {
            tg_grab_lib::api::watch_catchup(&st).await?;
            Value::Null
        }
        "joined_channels" => to_json(tg_grab_lib::api::joined_channels(&st).await?)?,
        "history_search" => to_json(
            tg_grab_lib::api::history_search(
                &st,
                s_arg(&body, "link"),
                s_arg(&body, "query"),
                s_arg(&body, "kind"),
                i32_arg(&body, "offsetId"),
                body.get("dayStart").and_then(Value::as_i64),
                body.get("dayEnd").and_then(Value::as_i64),
            )
            .await?,
        )?,
        "history_page" => to_json(
            tg_grab_lib::api::history_page(
                &st,
                s_arg(&body, "link"),
                i32_arg(&body, "offsetId"),
                body.get("date").and_then(|x| x.as_str()).map(String::from),
                body.get("dayStart").and_then(|x| x.as_i64()),
                body.get("dayEnd").and_then(|x| x.as_i64()),
            )
            .await?,
        )?,
        "history_thumbs" => to_json(
            tg_grab_lib::api::history_thumbs(
                &st,
                s_arg(&body, "link"),
                body.get("msgIds")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| n.as_i64())
                            .map(|n| n as i32)
                            .collect()
                    })
                    .unwrap_or_default(),
            )
            .await?,
        )?,
        "history_recent" => to_json(tg_grab_lib::api::history_recent(&st).await?)?,
        "history_recent_remove" => {
            tg_grab_lib::api::history_recent_remove(&st, i64_arg(&body, "rawId")).await?;
            Value::Null
        }
        "library_get" => to_json(tg_grab_lib::library::get(&st).await?)?,
        "favorite_save" => to_json(
            tg_grab_lib::library::save_favorite(
                &st,
                serde_json::from_value(body.get("item").cloned().unwrap_or(Value::Null))
                    .map_err(|_| "INVALID_LIBRARY_ITEM")?,
            )
            .await?,
        )?,
        "favorite_remove" => {
            tg_grab_lib::library::remove_favorite(&st, s_arg(&body, "id")).await?;
            Value::Null
        }
        "folder_save" => to_json(
            tg_grab_lib::library::save_folder(
                &st,
                body.get("id")
                    .and_then(|value| value.as_str())
                    .map(String::from),
                s_arg(&body, "name"),
            )
            .await?,
        )?,
        "folder_remove" => {
            tg_grab_lib::library::remove_folder(&st, s_arg(&body, "id")).await?;
            Value::Null
        }
        "library_export" => to_json(tg_grab_lib::library::export(&st).await?)?,
        "library_import" => to_json(
            tg_grab_lib::library::import(
                &st,
                serde_json::from_value(body.get("data").cloned().unwrap_or(Value::Null))
                    .map_err(|_| "INVALID_LIBRARY_BACKUP")?,
            )
            .await?,
        )?,
        _ => return Err("UNKNOWN_COMMAND".into()),
    })
}

async fn invoke_handler(
    State(st): State<St>,
    Path(cmd): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let body = body.map(|Json(v)| v).unwrap_or(Value::Null);
    match invoke_dispatch(st, cmd, body).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    }
}

/// WebSocket: forwards every core event as {event, payload} JSON text.
async fn ws_handler(
    State(st): State<St>,
    Query(q): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    let token_ok = {
        let cfg = st.config.read().await;
        cfg.server_token
            .as_deref()
            .map(|t| q.get("token").map(|g| g == t).unwrap_or(false))
            .unwrap_or(false)
    };
    if !token_ok {
        return (StatusCode::UNAUTHORIZED, "UNAUTHORIZED").into_response();
    }
    ws.on_upgrade(move |socket| async move { forward_events(st, socket).await })
}

async fn forward_events(st: SharedState, mut socket: axum::extract::ws::WebSocket) {
    let mut rx = st.bus.subscribe();
    loop {
        match rx.recv().await {
            Ok((event, payload)) => {
                let msg = json!({"event": event, "payload": payload});
                if socket
                    .send(Message::Text(msg.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
}

/// Serve a downloaded file with Range support (streaming, seek-friendly).
async fn file_handler(
    State(st): State<St>,
    Path(path): Path<String>,
    request: axum::extract::Request,
) -> Response {
    let full = match tg_grab_lib::api::resolve_download_path(&st, &path).await {
        Ok(path) => path,
        Err(e) => return (StatusCode::NOT_FOUND, e).into_response(),
    };
    // tower-http streams bounded chunks and implements GET/HEAD, 206 and 416.
    match ServeFile::new(full).try_call(request).await {
        Ok(response) => {
            let mut response = response.map(Body::new);
            response
                .headers_mut()
                .insert("x-content-type-options", "nosniff".parse().unwrap());
            let extension = std::path::Path::new(&path)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ![
                "mp4", "webm", "mov", "mp3", "ogg", "wav", "m4a", "flac", "jpg", "jpeg", "png",
                "webp", "gif", "avif",
            ]
            .contains(&extension.as_str())
            {
                response
                    .headers_mut()
                    .insert("content-disposition", "attachment".parse().unwrap());
                response
                    .headers_mut()
                    .insert("content-security-policy", "sandbox".parse().unwrap());
            }
            response
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Bearer/query token auth for /api, /ws and /files.
async fn auth_mw(
    State(st): State<St>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let expected = st
        .config
        .read()
        .await
        .server_token
        .clone()
        .unwrap_or_default();
    let given = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(String::from)
        .or_else(|| {
            req.uri().query().and_then(|q| {
                q.split('&').find_map(|kv| {
                    let mut it = kv.splitn(2, '=');
                    match (it.next(), it.next()) {
                        (Some("token"), Some(v)) => Some(v.to_string()),
                        _ => None,
                    }
                })
            })
        });
    if !expected.is_empty() && given.as_deref() == Some(expected.as_str()) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "UNAUTHORIZED").into_response()
    }
}

#[tokio::main]
async fn main() {
    let data_dir = std::path::PathBuf::from(
        std::env::var("TG_GRAB_DATA_DIR").unwrap_or_else(|_| "data".into()),
    );
    let dist_dir = std::env::var("TG_GRAB_DIST").unwrap_or_else(|_| "dist".into());
    let port: u16 = std::env::var("TG_GRAB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let st = tg_grab_lib::init_core(data_dir).await;

    // Ensure an access token exists; print it once for the operator.
    let token = {
        let mut cfg = st.config.write().await;
        if cfg
            .server_token
            .as_deref()
            .map(|t| t.is_empty())
            .unwrap_or(true)
        {
            let t = random_token();
            cfg.server_token = Some(t.clone());
            if let Err(error) = cfg.save(&st.config_path) {
                eprintln!("could not persist access token: {error}");
            }
            Some(t)
        } else {
            cfg.server_token.clone()
        }
    };
    match &token {
        Some(t)
            if st.config.read().await.server_token.as_deref() == Some(t.as_str()) && {
                // only print when freshly generated is undetectable; printing on
                // every boot is convenient for headless setups
                true
            } =>
        {
            println!("────────────────────────────────────────────");
            println!("  tg-grab web server: http://0.0.0.0:{port}");
            println!("  access token: {t}");
            println!("────────────────────────────────────────────");
        }
        _ => {}
    }

    let dist = std::path::PathBuf::from(&dist_dir);
    let spa = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));

    let app = Router::new()
        .route("/api/invoke/{cmd}", post(invoke_handler))
        .route("/ws", get(ws_handler))
        .route("/files/{*path}", get(file_handler))
        .layer(middleware::from_fn_with_state(st.clone(), auth_mw))
        .fallback_service(spa)
        .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(st);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    println!("listening on {addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}

// Silence unused warnings for types kept for parity with the desktop shell.
#[allow(unused)]
fn _unused(h: HeaderMap, _s: SharedState) {}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        state: St,
    }
    impl Fixture {
        async fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "tg-grab-test-{}-{}-{}",
                std::process::id(),
                now_ms(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let state = tg_grab_lib::init_core(root.clone()).await;
            Self { root, state }
        }
        async fn get(&self, path: &str, method: &str, range: Option<&str>) -> Response {
            let mut request = Request::builder().method(method).uri("/files/video.mp4");
            if let Some(range) = range {
                request = request.header("range", range);
            }
            file_handler(
                State(self.state.clone()),
                Path(path.into()),
                request.body(Body::empty()).unwrap(),
            )
            .await
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn organized_files_stream_ranges_and_head() {
        let fixture = Fixture::new().await;
        let folder = fixture.state.default_downloads_dir.join("频道/2026-11-07");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("video.mp4"), b"0123456789").unwrap();
        let path = "频道/2026-11-07/video.mp4";
        let response = fixture.get(path, "GET", Some("bytes=2-5")).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-range"], "bytes 2-5/10");
        assert_eq!(response.headers()["accept-ranges"], "bytes");
        assert_eq!(
            &to_bytes(response.into_body(), 100).await.unwrap()[..],
            b"2345"
        );
        let suffix = fixture.get(path, "GET", Some("bytes=-3")).await;
        assert_eq!(suffix.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            &to_bytes(suffix.into_body(), 100).await.unwrap()[..],
            b"789"
        );
        let head = fixture.get(path, "HEAD", None).await;
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers()["content-length"], "10");
        assert!(to_bytes(head.into_body(), 100).await.unwrap().is_empty());
        let invalid = fixture.get(path, "GET", Some("bytes=20-")).await;
        assert_eq!(invalid.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(invalid.headers()["content-range"], "bytes */10");
    }

    #[tokio::test]
    async fn empty_files_and_traversal_are_handled_without_panics() {
        let fixture = Fixture::new().await;
        std::fs::write(fixture.state.default_downloads_dir.join("empty.mp4"), []).unwrap();
        let response = fixture.get("empty.mp4", "GET", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            to_bytes(response.into_body(), 100)
                .await
                .unwrap()
                .is_empty()
        );
        for path in [
            "../config.json",
            "..\\config.json",
            "/etc/passwd",
            "C:/Windows/win.ini",
        ] {
            assert_eq!(
                fixture.get(path, "GET", None).await.status(),
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn command_errors_and_thumbnail_results_match_the_shared_api() {
        let fixture = Fixture::new().await;
        let response = invoke_handler(
            State(fixture.state.clone()),
            Path("save_api_config".into()),
            Some(Json(json!({"apiId":"invalid", "apiHash":"bad"}))),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1000).await.unwrap()).unwrap();
        assert_eq!(error["error"], "INVALID_API_ID");
        let thumbs = fixture.state.default_downloads_dir.join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        std::fs::write(thumbs.join("thumbnail.jpg"), b"image").unwrap();
        let result = invoke_dispatch(fixture.state.clone(), "clear_thumbs".into(), Value::Null)
            .await
            .unwrap();
        assert_eq!(result, json!({"freed":5}));
        assert_eq!(
            serde_json::to_value(
                tg_grab_lib::api::clear_thumbs(&fixture.state)
                    .await
                    .unwrap()
            )
            .unwrap(),
            json!({"freed":0})
        );
    }

    #[tokio::test]
    async fn file_listing_excludes_partial_files_and_normalizes_paths() {
        let fixture = Fixture::new().await;
        let root = &fixture.state.default_downloads_dir;
        std::fs::create_dir_all(root.join("channel/day")).unwrap();
        for name in [
            "partial.mp4.part",
            "partial.mp4.part.progress",
            "partial.progress.tmp",
            "channel/day/clip.mp4",
        ] {
            std::fs::write(root.join(name), b"data").unwrap();
        }
        let files = tg_grab_lib::api::list_files(&fixture.state).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "channel/day/clip.mp4");
    }

    #[tokio::test]
    async fn settings_persist_all_preferences_without_exposing_credentials() {
        let fixture = Fixture::new().await;
        {
            let mut cfg = fixture.state.config.write().await;
            cfg.api_id = Some(12345);
            cfg.api_hash = Some("private-api-hash".into());
            cfg.server_token = Some("private-server-token".into());
        }
        let custom_root = fixture.root.join("custom-media");
        let saved = json!({
            "concurrency": 3, "lang":"en", "downloadsDir":custom_root,
            "mediaKinds":["photo","video","photo"], "classifyByType":true,
            "organizeByChannel":true, "closeToTray":false
        });
        invoke_dispatch(fixture.state.clone(), "save_settings".into(), saved)
            .await
            .unwrap();
        let settings = invoke_dispatch(fixture.state.clone(), "get_settings".into(), Value::Null)
            .await
            .unwrap();
        assert_eq!(settings["effective_downloads_dir"], json!(custom_root));
        assert_eq!(settings["media_kinds"], json!(["photo", "video"]));
        assert_eq!(settings["api_hash"], Value::Null);
        assert_eq!(settings["api_hash_set"], true);
        let text = settings.to_string();
        assert!(!text.contains("private-api-hash"));
        assert!(!text.contains("private-server-token"));
        let persisted = tg_grab_lib::config::Config::load(&fixture.state.config_path);
        assert_eq!(persisted.concurrency, 3);
        assert_eq!(persisted.lang, "en");
        assert!(persisted.organize_by_channel && persisted.classify_by_type);
        assert!(!persisted.close_to_tray);
        let invalid = invoke_dispatch(
            fixture.state.clone(),
            "save_settings".into(),
            json!({"concurrency":1,"lang":"zh-CN","mediaKinds":[]}),
        )
        .await;
        assert_eq!(invalid.unwrap_err(), "INVALID_MEDIA_KINDS");
        assert_eq!(fixture.state.config.read().await.lang, "en");
        assert_eq!(fixture.state.config.read().await.concurrency, 3);
        std::fs::create_dir_all(custom_root.join("频道/day/photo")).unwrap();
        std::fs::write(custom_root.join("频道/day/photo/image.jpg"), b"image").unwrap();
        let files = tg_grab_lib::api::list_files(&fixture.state).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "频道/day/photo/image.jpg");
        assert_eq!(files[0].kind, "photo");
        let response = fixture.get("频道/day/photo/image.jpg", "GET", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let thumbs = fixture.state.default_downloads_dir.join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        std::fs::write(thumbs.join("sample.jpg"), b"thumb").unwrap();
        assert_eq!(
            tg_grab_lib::api::clear_thumbs(&fixture.state)
                .await
                .unwrap()
                .freed,
            5
        );
    }

    #[tokio::test]
    async fn collection_commands_roundtrip_and_documents_are_downloaded_safely() {
        let fixture = Fixture::new().await;
        let folder = invoke_dispatch(
            fixture.state.clone(),
            "folder_save".into(),
            json!({"name":"Reference"}),
        )
        .await
        .unwrap();
        let favorite = invoke_dispatch(fixture.state.clone(), "favorite_save".into(), json!({"item":{
            "kind":"channel", "title":"Telegram", "url":"https://t.me/telegram", "folderId":folder["id"]
        }})).await.unwrap();
        assert_eq!(favorite["folderId"], folder["id"]);
        let exported = invoke_dispatch(fixture.state.clone(), "library_export".into(), Value::Null)
            .await
            .unwrap();
        assert_eq!(exported["format"], "tg-grab-library");
        let imported = invoke_dispatch(
            fixture.state.clone(),
            "library_import".into(),
            json!({"data":exported}),
        )
        .await
        .unwrap();
        assert_eq!(imported["added"], 0);
        assert_eq!(imported["skipped"], 1);
        invoke_dispatch(
            fixture.state.clone(),
            "folder_remove".into(),
            json!({"id":folder["id"]}),
        )
        .await
        .unwrap();
        let view = invoke_dispatch(fixture.state.clone(), "library_get".into(), Value::Null)
            .await
            .unwrap();
        assert_eq!(view["items"].as_array().unwrap().len(), 1);
        assert_eq!(view["items"][0]["folderId"], Value::Null);
        std::fs::write(
            fixture.state.default_downloads_dir.join("document.html"),
            b"<script>alert(1)</script>",
        )
        .unwrap();
        let response = fixture.get("document.html", "GET", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("attachment")
        );
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    }

    #[tokio::test]
    async fn close_preference_defaults_to_asking_and_rejects_invalid_changes() {
        let fixture = Fixture::new().await;
        assert_eq!(fixture.state.config.read().await.close_action, "ask");
        let settings = invoke_dispatch(fixture.state.clone(), "get_settings".into(), Value::Null)
            .await
            .unwrap();
        assert_eq!(settings["close_action"], "ask");
        invoke_dispatch(
            fixture.state.clone(),
            "save_settings".into(),
            json!({"closeAction":"tray"}),
        )
        .await
        .unwrap();
        assert_eq!(
            tg_grab_lib::config::Config::load(&fixture.state.config_path).close_action,
            "tray"
        );
        let error = invoke_dispatch(
            fixture.state.clone(),
            "save_settings".into(),
            json!({"lang":"en","closeAction":"unknown"}),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "INVALID_CLOSE_ACTION");
        assert_eq!(fixture.state.config.read().await.lang, "zh-CN");
        let legacy: tg_grab_lib::config::Config =
            serde_json::from_value(json!({"close_to_tray":true})).unwrap();
        assert_eq!(legacy.close_action, "ask");
    }

    #[tokio::test]
    async fn search_validates_inputs_before_connecting_and_channel_picker_requires_login() {
        let fixture = Fixture::new().await;
        for (body, expected) in [
            (
                json!({"link":"https://t.me/telegram","query":"test","kind":"unknown"}),
                "INVALID_MEDIA_KINDS",
            ),
            (
                json!({"link":"https://t.me/telegram","query":"test","kind":"all","dayStart":500,"dayEnd":400}),
                "INVALID_DATE",
            ),
            (
                json!({"link":"https://t.me/telegram","query":"a".repeat(257),"kind":"all"}),
                "INVALID_SEARCH",
            ),
        ] {
            assert_eq!(
                invoke_dispatch(fixture.state.clone(), "history_search".into(), body)
                    .await
                    .unwrap_err(),
                expected
            );
        }
        assert_eq!(
            invoke_dispatch(fixture.state.clone(), "joined_channels".into(), Value::Null)
                .await
                .unwrap_err(),
            "API_NOT_CONFIGURED"
        );
    }
}
