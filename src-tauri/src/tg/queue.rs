//! Download queue: pending jobs, workers, parallel chunked downloads, progress events.

use crate::state::{ActiveTask, Job, SharedState};
use crate::tg::links::{make_msg_link, parse_msg_link, sanitize_filename};
use crate::tg::{client as tg_client, video_meta};
use grammers_client::media::Downloadable;
use grammers_client::{Client, InvocationError, tl};
use grammers_session::Session;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

/// A finished (or failed) queue item, pushed to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub id: u64,
    pub url: String,
    /// "success" | "failed"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TaskResult {
    pub fn failed(id: u64, url: &str, error: impl Into<String>) -> Self {
        TaskResult {
            id,
            url: url.to_string(),
            status: "failed".into(),
            name: None,
            size: None,
            cached: None,
            error: Some(error.into()),
        }
    }
}

/// Map raw MTProto errors to stable frontend error codes.
fn friendly_error(err: &str) -> String {
    if err.contains("FLOOD_WAIT") {
        "RATE_LIMITED".to_string()
    } else if err.contains("CHANNEL_INVALID") || err.contains("CHANNEL_PRIVATE") {
        "CHANNEL_UNAVAILABLE".to_string()
    } else {
        err.to_string()
    }
}

/// Mirror of grammers' crate-private `copy_auth_to_dc`: authorize our session
/// key on a foreign DC (auth.ExportAuthorization on home + ImportAuthorization
/// on the target) so the parallel workers can fetch chunks from it.
async fn copy_auth_to_dc(
    client: &Client,
    target_dc: i32,
    copied: Arc<std::sync::Mutex<HashSet<i32>>>,
) -> Result<(), String> {
    // NB: never hold the lock across an await (std::MutexGuard is !Send).
    if copied.lock().unwrap().contains(&target_dc) {
        return Ok(());
    }
    let exported = client
        .invoke(&tl::functions::auth::ExportAuthorization { dc_id: target_dc })
        .await
        .map_err(|e| e.to_string())?;
    let tl::enums::auth::ExportedAuthorization::Authorization(a) = exported
    else {
        return Err("unexpected auth.exportedAuthorization".into());
    };
    client
        .invoke_in_dc(
            target_dc,
            &tl::functions::auth::ImportAuthorization {
                id: a.id,
                bytes: a.bytes,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    copied.lock().unwrap().insert(target_dc);
    Ok(())
}

/// Spawn download workers (1 = sequential across files; each file itself
/// downloads with 4 parallel chunk connections). Capped at 4.
pub fn spawn_workers(st: SharedState, app: AppHandle) {
    let n = {
        let cfg = st.config.blocking_read();
        cfg.concurrency.clamp(1, 4)
    };
    for _ in 0..n {
        let st = st.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            worker_loop(st, app).await;
        });
    }
}

async fn worker_loop(st: SharedState, app: AppHandle) {
    loop {
        let job = {
            let mut pending = st.queue.pending.lock().await;
            let j = pending.pop_front();
            drop(pending);
            // Journal the remaining pending jobs after every take.
            st.queue.persist(&st.queue_path).await;
            j
        };
        let Some(job) = job else {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        };

        let result = process_job(&st, &app, &job).await;

        {
            let mut results = st.queue.results.lock().await;
            results.push(result.clone());
            let len = results.len();
            if len > 200 {
                results.drain(0..len - 200);
            }
        }
        *st.queue.active.lock().await = None;
        let _ = app.emit("queue:result", &result);
    }
}

/// Full pipeline for one job: parse link → fetch message → download video.
async fn process_job(st: &SharedState, app: &AppHandle, job: &Job) -> TaskResult {
    let fail_str = |msg: &str| TaskResult::failed(job.id, &job.url, msg);

    let (chat, msg_id) = match parse_msg_link(&job.url) {
        Ok(v) => v,
        Err(code) => return fail_str(&code),
    };

    let client = match st.client.read().await.clone() {
        Some(c) => c,
        None => return fail_str("NOT_LOGGED_IN"),
    };

    let rc = match tg_client::resolve_chat(st, &client, &chat).await {
        Ok(rc) => rc,
        Err(e) => return fail_str(&e),
    };

    let msg = match client.get_messages_by_id(rc.peer, &[msg_id]).await {
        Ok(mut v) => match v.pop().flatten() {
            Some(m) => m,
            None => return fail_str("MESSAGE_NOT_FOUND"),
        },
        Err(e) => return fail_str(&friendly_error(&e.to_string())),
    };

    let media = match msg.media() {
        Some(m) => m,
        None => return fail_str("NOT_A_VIDEO"),
    };
    let meta = match video_meta(&media) {
        Some(m) => m,
        None => return fail_str("NOT_A_VIDEO"),
    };

    let mut file_name = sanitize_filename(
        &meta
            .name
            .clone()
            .unwrap_or_else(|| format!("tg_video_{msg_id}.mp4")),
    );
    let mut target = st.downloads_dir.join(&file_name);
    let total = meta.size;

    // Skip complete files; on a same-name/size-mismatch collision (channels
    // reuse names like proc_0.mp4) suffix with the message id instead of
    // overwriting the earlier download.
    let mut attempts = 0;
    while target.exists() {
        let local = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
        if total > 0 && local == total {
            return TaskResult {
                id: job.id,
                url: job.url.clone(),
                status: "success".into(),
                name: Some(file_name),
                size: Some(local),
                cached: Some(true),
                error: None,
            };
        }
        attempts += 1;
        if attempts > 1 {
            let _ = std::fs::remove_file(&target);
            break;
        }
        let p = std::path::Path::new(&file_name);
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.clone());
        let ext = p
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        file_name = format!("{stem}_{msg_id}{ext}");
        target = st.downloads_dir.join(&file_name);
    }

    // Download into a .part temp file; only a verified-complete file gets
    // the real name, so an interrupted run can never leave a broken
    // "final" file behind.
    let part_path = st.downloads_dir.join(format!("{file_name}.part"));
    if part_path.exists() {
        let _ = std::fs::remove_file(&part_path);
    }

    // Publish the active task before starting so the UI can show the name.
    let progress = ActiveTask {
        id: job.id,
        url: job.url.clone(),
        name: file_name.clone(),
        current: 0,
        total,
        percent: 0.0,
    };
    *st.queue.active.lock().await = Some(progress.clone());
    let _ = app.emit("queue:progress", &progress);

    let done = Arc::new(AtomicU64::new(0));
    let emitting = Arc::new(AtomicBool::new(true));

    // Progress emitter: polls the shared byte counter ~5x/sec.
    let emit_st = st.clone();
    let emit_app = app.clone();
    let emit_done = done.clone();
    let emit_flag = emitting.clone();
    let mut emit_progress = progress;
    tauri::async_runtime::spawn(async move {
        while emit_flag.load(Ordering::Relaxed) {
            let current = emit_done.load(Ordering::Relaxed);
            emit_progress.current = current;
            emit_progress.percent = if total > 0 {
                (current as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            *emit_st.queue.active.lock().await = Some(emit_progress.clone());
            let _ = emit_app.emit("queue:progress", &emit_progress);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });

    let result = match download_parallel(st, &client, &media, &part_path, total, done).await {
        Ok(()) => match tokio::fs::rename(&part_path, &target).await {
            Ok(()) => TaskResult {
                id: job.id,
                url: job.url.clone(),
                status: "success".into(),
                name: Some(file_name),
                size: Some(total),
                cached: Some(false),
                error: None,
            },
            Err(e) => TaskResult::failed(job.id, &job.url, e.to_string()),
        },
        Err(e) => {
            // Remove the partial file so a retry starts clean.
            let _ = std::fs::remove_file(&part_path);
            TaskResult::failed(job.id, &job.url, e)
        }
    };
    emitting.store(false, Ordering::Relaxed);
    result
}

/// Parallel download (mirrors grammers' own concurrent downloader): N workers
/// each fetch 512 KiB chunks by offset via `upload.GetFile`; a writer task
/// persists every chunk at its offset. Progress = shared byte counter.
const PARALLEL_WORKERS: usize = 4;
const CHUNK: i32 = 512 * 1024;
const FILE_MIGRATE_ERROR: i32 = 303;

async fn download_parallel(
    st: &SharedState,
    client: &Client,
    media: &grammers_client::media::Media,
    target: &std::path::Path,
    total: u64,
    done: Arc<AtomicU64>,
) -> Result<(), String> {
    let location = media.to_raw_input_location().ok_or("NOT_A_VIDEO")?;
    let session = st.session.read().await.clone().ok_or("NOT_LOGGED_IN")?;
    let home_dc = session.home_dc_id();

    // Pre-allocate so workers can seek-write concurrently.
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|e| format!("IO: {e}"))?;
    file.set_len(total).await.map_err(|e| format!("IO: {e}"))?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u64, Vec<u8>)>();
    let next_part = Arc::new(tokio::sync::Mutex::new(0u64));
    let auth_copied: Arc<std::sync::Mutex<HashSet<i32>>> = Arc::new(std::sync::Mutex::new(HashSet::new()));

    let mut tasks = Vec::new();
    for _ in 0..PARALLEL_WORKERS {
        let location = location.clone();
        let tx = tx.clone();
        let next_part = next_part.clone();
        let client = client.clone();
        let done = done.clone();
        let auth_copied = auth_copied.clone();
        tasks.push(tauri::async_runtime::spawn(async move {
            let mut dc = home_dc;
            let mut retry_offset: Option<i64> = None;
            loop {
                // Claim the next chunk offset.
                let offset: i64 = match retry_offset.take() {
                    Some(o) => o,
                    None => {
                        let mut i = next_part.lock().await;
                        *i += 1;
                        (CHUNK as u64 * (*i - 1)) as i64
                    }
                };
                if offset >= total as i64 {
                    break;
                }
                let request = tl::functions::upload::GetFile {
                    precise: true,
                    cdn_supported: false,
                    location: location.clone(),
                    offset,
                    limit: CHUNK,
                };
                match client.invoke_in_dc(dc, &request).await {
                    Ok(tl::enums::upload::File::File(f)) => {
                        let n = f.bytes.len() as u64;
                        done.fetch_add(n, Ordering::Relaxed);
                        if tx.send((offset as u64, f.bytes)).is_err() {
                            break; // writer gone (abort/shutdown)
                        }
                        if n == 0 {
                            break; // past-EOF safeguard
                        }
                    }
                    Ok(tl::enums::upload::File::CdnRedirect(_)) => {
                        return Err("CDN_REDIRECT".into());
                    }
                    Err(InvocationError::Rpc(err)) => {
                        if err.code == FILE_MIGRATE_ERROR {
                            dc = err.value.unwrap_or_default() as i32;
                            retry_offset = Some(offset);
                            continue;
                        }
                        if err.name == "AUTH_KEY_UNREGISTERED" {
                            // Session key not authorized on this DC yet — export/import it.
                            match copy_auth_to_dc(&client, dc, auth_copied.clone()).await {
                                Ok(()) => {
                                    retry_offset = Some(offset);
                                    continue;
                                }
                                Err(e) => return Err(format!("AUTH_COPY: {e}")),
                            }
                        }
                        return Err(err.name.clone());
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
            Ok::<(), String>(())
        }));
    }
    drop(tx);

    // Writer: place every chunk at its offset.
    let mut pos: u64 = 0;
    let mut write_err: Option<String> = None;
    while let Some((offset, data)) = rx.recv().await {
        if offset != pos {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| format!("IO: {e}"))?;
        }
        if let Err(e) = file.write_all(&data).await {
            write_err = Some(format!("IO: {e}"));
            break;
        }
        pos = offset + data.len() as u64;
    }
    let _ = file.sync_all().await;
    drop(file);

    // Propagate worker errors (writer error takes precedence).
    for t in tasks {
        match t.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) if write_err.is_none() => write_err = Some(e),
            Err(e) if write_err.is_none() => write_err = Some(format!("JOIN: {e}")),
            _ => {}
        }
    }
    if let Some(e) = write_err {
        return Err(friendly_error(&e));
    }

    // Verify against the advertised size.
    let local = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
    if total > 0 && local != total {
        return Err(format!("SIZE_MISMATCH: {local} != {total}"));
    }
    Ok(())
}

/// Build the canonical link for a watched-channel message.
pub fn link_for_entry(entry: &crate::config::WatchEntry, msg_id: i32) -> String {
    make_msg_link(entry.username.as_deref(), entry.raw_id, msg_id)
}

/// Initial snapshot for the frontend (after a reload or on start).
pub async fn snapshot(st: &SharedState) -> serde_json::Value {
    let pending: Vec<Job> = st.queue.pending.lock().await.iter().cloned().collect();
    let results: Vec<TaskResult> = st.queue.results.lock().await.clone();
    let active = st.queue.active.lock().await.clone();
    serde_json::json!({
        "pending": pending
            .iter()
            .map(|j| serde_json::json!({"id": j.id, "url": j.url}))
            .collect::<Vec<_>>(),
        "active": active,
        "results": results,
    })
}
