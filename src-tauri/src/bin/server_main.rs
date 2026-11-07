//! tg-grab web server: serves the SPA + invoke dispatcher + WS event stream.
//!
//! Run with `--no-default-features --features server`. Data lives in
//! `$TG_GRAB_DATA_DIR` (default ./data); frontend build in `$TG_GRAB_DIST`
//! (default ./dist). Access requires the token from config (auto-generated
//! on first run and printed to the console).

use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, State, WebSocketUpgrade, ws::Message},
    http::{HeaderMap, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use tokio::io::{AsyncSeekExt as _, AsyncWriteExt as _};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use tg_grab_lib::state::SharedState;
use tower_http::services::{ServeDir, ServeFile};

type St = SharedState;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn random_token() -> String {
    let mut seed = now_ms() as u64 ^ 0x9E3779B97F4A7C15;
    let mut out = String::new();
    for _ in 0..8 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        out += &format!("{:08x}", seed as u32);
    }
    out
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
        "save_api_config" => json!({
            "ok": tg_grab_lib::api::save_api_config(&st, s_arg(&body, "apiId"), s_arg(&body, "apiHash")).await.is_ok()
        }),
        "save_proxy" => {
            tg_grab_lib::api::save_proxy(&st, s_arg(&body, "proxyUrl")).await?;
            json!({"ok": true})
        }
        "save_settings" => {
            tg_grab_lib::api::save_settings(
                &st,
                body.get("concurrency").and_then(|x| x.as_u64()).map(|x| x as u32),
                body.get("lang").and_then(|x| x.as_str()).map(String::from),
            )
            .await?;
            json!({"ok": true})
        }
        "get_settings" => to_json(tg_grab_lib::api::get_settings(&st).await?)?,
        "auth_send_code" => {
            tg_grab_lib::api::auth_send_code(&st, s_arg(&body, "phone")).await?;
            json!({"ok": true})
        }
        "auth_sign_in" => to_json(tg_grab_lib::api::auth_sign_in(&st, s_arg(&body, "code")).await?)?,
        "auth_check_password" => {
            json!({"user": tg_grab_lib::api::auth_check_password(&st, s_arg(&body, "password")).await?})
        }
        "logout" => {
            tg_grab_lib::api::logout(&st).await?;
            json!({"ok": true})
        }
        "download_urls" => to_json(tg_grab_lib::api::download_urls(
            &st,
            body.get("urls")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|u| u.as_str()).map(String::from).collect())
                .unwrap_or_default(),
        ).await?)?,
        "get_queue" => tg_grab_lib::api::get_queue(&st).await?,
        "queue_move" => {
            tg_grab_lib::api::queue_move(&st, i64_arg(&body, "id") as u64, body.get("toFront").and_then(|x| x.as_bool()).unwrap_or(false)).await?;
            json!({"ok": true})
        }
        "queue_cancel" => {
            tg_grab_lib::api::queue_cancel(&st, i64_arg(&body, "id") as u64).await?;
            json!({"ok": true})
        }
        "clear_thumbs" => json!({"freed": tg_grab_lib::api::clear_thumbs(&st).await?}),
        "list_files" => to_json(tg_grab_lib::api::list_files(&st).await?)?,
        "delete_file" => {
            tg_grab_lib::api::delete_file(&st, s_arg(&body, "name")).await?;
            json!({"ok": true})
        }
        "watch_list" => to_json(tg_grab_lib::api::watch_list(&st).await?)?,
        "watch_add" => to_json(tg_grab_lib::api::watch_add(
            &st,
            s_arg(&body, "link"),
            body.get("backfill").and_then(|x| x.as_bool()).unwrap_or(false),
        ).await?)?,
        "watch_toggle" => {
            json!({"enabled": tg_grab_lib::api::watch_toggle(&st, i64_arg(&body, "rawId")).await?})
        }
        "watch_delete" => {
            tg_grab_lib::api::watch_delete(&st, i64_arg(&body, "rawId")).await?;
            json!({"ok": true})
        }
        "watch_catchup" => {
            tg_grab_lib::api::watch_catchup(&st).await?;
            json!({"ok": true})
        }
        "history_page" => to_json(tg_grab_lib::api::history_page(
            &st,
            s_arg(&body, "link"),
            i32_arg(&body, "offsetId"),
            body.get("date").and_then(|x| x.as_str()).map(String::from),
        ).await?)?,
        "history_thumbs" => to_json(tg_grab_lib::api::history_thumbs(
            &st,
            s_arg(&body, "link"),
            body.get("msgIds")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|n| n.as_i64()).map(|n| n as i32).collect())
                .unwrap_or_default(),
        ).await?)?,
        "history_recent" => to_json(tg_grab_lib::api::history_recent(&st).await?)?,
        "history_recent_remove" => {
            tg_grab_lib::api::history_recent_remove(&st, i64_arg(&body, "rawId")).await?;
            json!({"ok": true})
        }
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

async fn forward_events(
    st: SharedState,
    mut socket: axum::extract::ws::WebSocket,
) {
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
    headers: HeaderMap,
) -> Response {
    // Reject traversal, then resolve under downloads_dir.
    if path.split('\\').chain(path.split('/')).any(|seg| seg == "..") {
        return (StatusCode::BAD_REQUEST, "BAD_PATH").into_response();
    }
    let full = st.downloads_dir.join(&path);
    let meta = match tokio::fs::metadata(&full).await {
        Ok(m) if m.is_file() => m,
        _ => return (StatusCode::NOT_FOUND, "NOT_FOUND").into_response(),
    };
    let size = meta.len();
    let mime = if path.to_lowercase().ends_with(".mp4") {
        "video/mp4"
    } else {
        "application/octet-stream"
    };

    // Single-range support (suffix "bytes=-N" also handled).
    let mut start: u64 = 0;
    let mut end: u64 = size.saturating_sub(1);
    let mut partial = false;
    if let Some(range) = headers
        .get(header::RANGE)
        .and_then(|h| h.to_str().ok())
    {
        if let Some(spec) = range.strip_prefix("bytes=") {
            let mut it = spec.splitn(2, '-');
            let first = it.next().unwrap_or("");
            let last = it.next().unwrap_or("");
            if first.is_empty() {
                // bytes=-N : final N bytes
                if let Ok(n) = last.parse::<u64>() {
                    let n = n.min(size);
                    start = size - n;
                    end = size.saturating_sub(1);
                    partial = true;
                }
            } else if let Ok(s) = first.parse::<u64>() {
                start = s.min(size);
                end = if last.is_empty() {
                    size.saturating_sub(1)
                } else {
                    last.parse::<u64>().unwrap_or(size - 1).min(size.saturating_sub(1))
                };
                if start > end {
                    return (
                        StatusCode::RANGE_NOT_SATISFIABLE,
                        [("content-range", format!("bytes */{size}"))],
                    )
                        .into_response();
                }
                partial = true;
            }
        }
    }
    let len = end - start + 1;

    let mut f = match tokio::fs::File::open(&full).await {
        Ok(f) => f,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    if start > 0 {
        use std::io::SeekFrom;
        if f.seek(SeekFrom::Start(start)).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "seek failed").into_response();
        }
    }
    use tokio::io::AsyncReadExt as _;
    let mut buf = vec![0u8; len as usize];
    if let Err(e) = f.read_exact(&mut buf).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let mut resp = (StatusCode::OK, [(header::CONTENT_TYPE, mime)], buf).into_response();
    if partial {
        resp.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{size}")
                .parse()
                .unwrap(),
        );
    }
    resp
}

/// Bearer/query token auth for /api, /ws and /files.
async fn auth_mw(
    State(st): State<St>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let expected = st.config.read().await.server_token.clone().unwrap_or_default();
    let given = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(String::from)
        .or_else(|| {
            req.uri()
                .query()
                .and_then(|q| {
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
            let snapshot = cfg.clone();
            let path = st.config_path.clone();
            drop(cfg);
            let _ = snapshot.save(&path);
            Some(t)
        } else {
            cfg.server_token.clone()
        }
    };
    match &token {
        Some(t) if st.config.read().await.server_token.as_deref() == Some(t.as_str()) && {
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
    let spa = ServeDir::new(&dist)
        .fallback(ServeFile::new(dist.join("index.html")));

    let app = Router::new()
        .route("/api/invoke/{cmd}", post(invoke_handler))
        .route("/ws", get(ws_handler))
        .route("/files/{*path}", get(file_handler))
        .layer(middleware::from_fn_with_state(st.clone(), auth_mw))
        .fallback_service(spa)
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
