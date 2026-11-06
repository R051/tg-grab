//! Download queue: pending jobs, workers, progress events.

use crate::state::{ActiveTask, Job, SharedState};
use crate::tg::links::{make_msg_link, parse_msg_link, sanitize_filename};
use crate::tg::{client as tg_client, video_meta};
use grammers_client::Client;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

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

/// Spawn download workers (1 = sequential, gentlest; capped at 4).
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
            pending.pop_front()
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

    let file_name = sanitize_filename(
        &meta
            .name
            .clone()
            .unwrap_or_else(|| format!("tg_video_{msg_id}.mp4")),
    );
    let target = st.downloads_dir.join(&file_name);
    let total = meta.size;

    // Skip complete files; delete corrupt leftovers from interrupted runs.
    if target.exists() {
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
        let _ = std::fs::remove_file(&target);
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

    // Progress closure: update the shared snapshot and throttle UI emits.
    let app_for_progress = app.clone();
    let mut progress = progress.clone();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let on_progress = &mut |current: u64, total: u64| {
        progress.current = current;
        progress.total = total;
        progress.percent = if total > 0 {
            (current as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        if last_emit.elapsed() >= Duration::from_millis(200) {
            last_emit = Instant::now();
            let _ = app_for_progress.emit("queue:progress", &progress);
        }
    };

    match download_to_file(&client, &media, &target, total, on_progress).await {
        Ok(bytes) => {
            progress.percent = 100.0;
            progress.current = bytes;
            *st.queue.active.lock().await = Some(progress);
            TaskResult {
                id: job.id,
                url: job.url.clone(),
                status: "success".into(),
                name: Some(file_name),
                size: Some(bytes),
                cached: Some(false),
                error: None,
            }
        }
        Err(e) => {
            // Remove the partial file so a retry starts clean.
            let _ = std::fs::remove_file(&target);
            fail_str(&e)
        }
    }
}

/// Chunked download with a progress callback (bytes_done, bytes_total).
async fn download_to_file(
    client: &Client,
    media: &grammers_client::media::Media,
    target: &std::path::Path,
    total: u64,
    on_progress: &mut impl FnMut(u64, u64),
) -> Result<u64, String> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|e| format!("IO: {e}"))?;
    let mut download = client.iter_download(media);
    let mut done: u64 = 0;

    while let Some(chunk) = download
        .next()
        .await
        .map_err(|e| friendly_error(&e.to_string()))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("IO: {e}"))?;
        done += chunk.len() as u64;
        on_progress(done, total);
    }
    file.flush().await.map_err(|e| format!("IO: {e}"))?;
    drop(file);

    // Sanity check against the advertised size.
    if total > 0 {
        let local = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
        if local != total {
            return Err(format!("SIZE_MISMATCH: {local} != {total}"));
        }
    }
    Ok(done)
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

#[allow(dead_code)]
fn unused(_v: VecDeque<Job>) {}
