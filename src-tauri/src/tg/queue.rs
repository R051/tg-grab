//! Download queue: pending jobs, workers, parallel chunked downloads, progress events.

use crate::state::{ActiveTask, InFlightControl, Job, PausedTask, SharedState, StopReason, now_ms};
use crate::tg::links::{ChatLink, make_msg_link, parse_msg_link, sanitize_filename};
use crate::tg::{client as tg_client, media_meta};
use grammers_client::media::Downloadable;
use grammers_client::{Client, InvocationError, tl};
use grammers_session::Session;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Notify, watch};

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
    /// Path relative to downloads_dir (may contain <channel>/<date> segments).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: i64,
}

impl TaskResult {
    pub fn failed(id: u64, url: &str, created_at: i64, error: impl Into<String>) -> Self {
        TaskResult {
            id,
            url: url.to_string(),
            status: "failed".into(),
            name: None,
            path: None,
            size: None,
            cached: None,
            error: Some(error.into()),
            created_at,
            completed_at: now_ms(),
        }
    }
}

fn journal_for_part(part_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.progress", part_path.display()))
}

fn journal_tmp_for_part(part_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.progress.tmp", part_path.display()))
}

fn remove_part_bundle(part_path: &Path) {
    let _ = std::fs::remove_file(part_path);
    let _ = std::fs::remove_file(journal_for_part(part_path));
    let _ = std::fs::remove_file(journal_tmp_for_part(part_path));
}

/// Remove `.part` / `.part.progress` / `.part.progress.tmp` files belonging to a cancelled task.
pub async fn cleanup_temp_files(st: &SharedState, part_path: Option<&Path>, url: &str) {
    let root = st.downloads_root().await;
    if let Some(p) = part_path {
        if p.starts_with(&root) {
            remove_part_bundle(p);
            return;
        }
    }
    if let Ok((ChatLink::ChannelRaw(raw_id), msg_id)) = parse_msg_link(url) {
        let marker = format!("_{raw_id}_{msg_id}.");
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in rd.flatten() {
                if entry.file_type().map(|k| k.is_symlink()).unwrap_or(true) {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().and_then(|n| n.to_str()) != Some("thumbs") {
                        stack.push(path);
                    }
                    continue;
                }
                let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if fname.ends_with(".part") && fname.contains(&marker) {
                    remove_part_bundle(&path);
                }
            }
        }
    }
}

async fn stop_in_flight(st: &SharedState, id: u64, reason: StopReason) -> bool {
    let notify = {
        let controls = st.queue.controls.lock().unwrap();
        controls.get(&id).map(|ctrl| {
            let _ = ctrl.stop_tx.send(Some(reason));
            ctrl.done_notify.clone()
        })
    };
    let Some(notify) = notify else {
        return false;
    };
    if reason == StopReason::Cancel {
        st.queue.cancel_flags.lock().unwrap().insert(id);
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), notify.notified()).await;
    true
}

/// Pause a queued or in-flight download, keeping partial `.part` + `.part.progress` chunks.
pub async fn pause_job(st: &SharedState, id: u64) -> Result<(), String> {
    {
        let mut pending = st.queue.pending.lock().await;
        if let Some(idx) = pending.iter().position(|j| j.id == id) {
            let job = pending.remove(idx).unwrap();
            let mut paused = st.queue.paused.lock().await;
            paused.insert(
                job.id,
                PausedTask {
                    id: job.id,
                    url: job.url,
                    name: job.name,
                    current: job.current,
                    total: job.total,
                    percent: job.percent,
                    created_at: job.created_at,
                    paused_at: now_ms(),
                    part_path: job.part_path,
                },
            );
            drop(paused);
            drop(pending);
            st.queue.persist(&st.queue_path).await;
            return Ok(());
        }
    }
    if st.queue.paused.lock().await.contains_key(&id) {
        return Ok(());
    }
    if stop_in_flight(st, id, StopReason::Pause).await {
        return Ok(());
    }
    Err("NOT_FOUND".into())
}

/// Resume a paused download by pushing it back to the front of the pending queue.
pub async fn resume_job(st: &SharedState, id: u64) -> Result<(), String> {
    let task = {
        let mut paused = st.queue.paused.lock().await;
        paused.remove(&id)
    };
    let Some(task) = task else {
        let in_pending = st.queue.pending.lock().await.iter().any(|j| j.id == id);
        let in_flight = st.queue.in_flight.lock().await.contains_key(&id);
        if in_pending || in_flight {
            return Ok(());
        }
        return Err("NOT_FOUND".into());
    };
    {
        let mut pending = st.queue.pending.lock().await;
        let inflight = st.queue.in_flight.lock().await;
        let already = pending
            .iter()
            .chain(inflight.values())
            .any(|j| j.url == task.url);
        if !already {
            pending.push_front(Job {
                id: task.id,
                url: task.url,
                created_at: task.created_at,
                name: task.name,
                current: task.current,
                total: task.total,
                percent: task.percent,
                part_path: task.part_path,
            });
        }
    }
    st.queue.persist(&st.queue_path).await;
    st.queue.wake.notify_waiters();
    Ok(())
}

/// Cancel a queued, paused, in-flight, or failed download, removing it and deleting temp files.
pub async fn cancel_job(st: &SharedState, id: u64) -> Result<(), String> {
    {
        let mut pending = st.queue.pending.lock().await;
        if let Some(idx) = pending.iter().position(|j| j.id == id) {
            let job = pending.remove(idx).unwrap();
            drop(pending);
            st.queue.results.lock().await.retain(|r| r.id != id);
            st.queue.persist(&st.queue_path).await;
            cleanup_temp_files(st, job.part_path.as_deref(), &job.url).await;
            return Ok(());
        }
    }
    {
        let mut paused = st.queue.paused.lock().await;
        if let Some(task) = paused.remove(&id) {
            drop(paused);
            st.queue.results.lock().await.retain(|r| r.id != id);
            st.queue.persist(&st.queue_path).await;
            cleanup_temp_files(st, task.part_path.as_deref(), &task.url).await;
            return Ok(());
        }
    }
    if stop_in_flight(st, id, StopReason::Cancel).await {
        return Ok(());
    }
    {
        let mut results = st.queue.results.lock().await;
        if let Some(pos) = results
            .iter()
            .position(|r| r.id == id && r.status == "failed")
        {
            let removed = results.remove(pos);
            drop(results);
            cleanup_temp_files(st, None, &removed.url).await;
            return Ok(());
        }
    }
    Err("NOT_FOUND".into())
}

fn is_cancelled(st: &SharedState, id: u64) -> bool {
    st.queue.cancel_flags.lock().unwrap().contains(&id)
}

/// yt-dlp style download archive: one `<channelRaw>_<msgId>` key per line.
/// Even if the file is deleted or moved, the archive remembers it.
fn archive_key(raw_id: i64, msg_id: i32) -> String {
    format!("{raw_id}_{msg_id}")
}

fn archive_append(st: &SharedState, key: &str) {
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&st.queue.archive_path)
    {
        let _ = writeln!(f, "{key}");
    }
}

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
    let tl::enums::auth::ExportedAuthorization::Authorization(a) = exported else {
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
pub async fn spawn_workers(st: SharedState) {
    for worker in 0..4 {
        let st = st.clone();
        tokio::spawn(async move {
            worker_loop(st, worker).await;
        });
    }
}

async fn worker_loop(st: SharedState, worker: usize) {
    loop {
        if worker >= st.config.read().await.concurrency.clamp(1, 4)
            || st.queue.pending.lock().await.is_empty()
        {
            tokio::select! {
                _ = st.queue.wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_millis(500)) => {},
            }
            continue;
        }
        // Authentication loss pauses the durable queue instead of draining it.
        let Some(client) = tg_client::ensure_client(&st).await.ok() else {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        };
        if !client.is_authorized().await.unwrap_or(false) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }

        let taken = {
            let mut pending = st.queue.pending.lock().await;
            let j = pending.pop_front();
            if let Some(job) = &j {
                let (stop_tx, stop_rx) = watch::channel(None);
                let done_notify = Arc::new(Notify::new());
                st.queue.in_flight.lock().await.insert(job.id, job.clone());
                st.queue.controls.lock().unwrap().insert(
                    job.id,
                    InFlightControl {
                        stop_tx,
                        done_notify: done_notify.clone(),
                    },
                );
                drop(pending);
                st.queue.persist(&st.queue_path).await;
                st.bus.emit("queue:changed", serde_json::Value::Bool(true));
                Some((job.clone(), stop_rx, done_notify))
            } else {
                drop(pending);
                None
            }
        };
        let Some((job, stop_rx, done_notify)) = taken else {
            tokio::select! {
                _ = st.queue.wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_millis(500)) => {},
            }
            continue;
        };

        let outcome = process_job(&st, &job, &client, stop_rx).await;

        st.queue.active.lock().await.remove(&job.id);
        st.queue.in_flight.lock().await.remove(&job.id);
        st.queue.controls.lock().unwrap().remove(&job.id);
        st.queue.cancel_flags.lock().unwrap().remove(&job.id);

        match outcome {
            JobOutcome::Finished(result) => {
                {
                    let mut results = st.queue.results.lock().await;
                    results.push(result.clone());
                    let len = results.len();
                    if len > 200 {
                        results.drain(0..len - 200);
                    }
                }
                st.queue.persist(&st.queue_path).await;
                done_notify.notify_waiters();
                st.bus.emit(
                    "queue:result",
                    serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
                );
                st.bus.emit("queue:changed", serde_json::Value::Bool(true));
            }
            JobOutcome::Paused(task) => {
                st.queue.paused.lock().await.insert(task.id, task);
                st.queue.persist(&st.queue_path).await;
                done_notify.notify_waiters();
                st.bus.emit("queue:changed", serde_json::Value::Bool(true));
            }
            JobOutcome::Cancelled { part_path } => {
                cleanup_temp_files(&st, part_path.as_deref(), &job.url).await;
                st.queue.results.lock().await.retain(|r| r.id != job.id);
                st.queue.persist(&st.queue_path).await;
                done_notify.notify_waiters();
                st.bus.emit("queue:changed", serde_json::Value::Bool(true));
            }
        }
    }
}

enum JobOutcome {
    Finished(TaskResult),
    Paused(PausedTask),
    Cancelled { part_path: Option<PathBuf> },
}

async fn wait_stop_reason(stop_rx: &mut watch::Receiver<Option<StopReason>>) -> StopReason {
    if let Some(reason) = *stop_rx.borrow() {
        return reason;
    }
    while stop_rx.changed().await.is_ok() {
        if let Some(reason) = *stop_rx.borrow() {
            return reason;
        }
    }
    StopReason::Cancel
}

fn paused_from_job(
    job: &Job,
    name: Option<String>,
    current: u64,
    total: u64,
    part_path: Option<PathBuf>,
) -> PausedTask {
    let total = if total > 0 { total } else { job.total };
    let current = if current > 0 { current } else { job.current };
    let percent = if total > 0 {
        (current as f64 / total as f64) * 100.0
    } else {
        job.percent
    };
    PausedTask {
        id: job.id,
        url: job.url.clone(),
        name: name.or_else(|| job.name.clone()),
        current,
        total,
        percent,
        created_at: job.created_at,
        paused_at: now_ms(),
        part_path: part_path.or_else(|| job.part_path.clone()),
    }
}

/// Full pipeline for one job: parse link → fetch message → download video.
async fn process_job(
    st: &SharedState,
    job: &Job,
    client: &Client,
    mut stop_rx: watch::Receiver<Option<StopReason>>,
) -> JobOutcome {
    let fail_str =
        |msg: &str| JobOutcome::Finished(TaskResult::failed(job.id, &job.url, job.created_at, msg));
    let stop_outcome = |reason: StopReason,
                        name: Option<String>,
                        current: u64,
                        total: u64,
                        part_path: Option<PathBuf>| match reason {
        StopReason::Pause => {
            JobOutcome::Paused(paused_from_job(job, name, current, total, part_path))
        }
        StopReason::Cancel => JobOutcome::Cancelled { part_path },
    };

    if let Some(reason) = *stop_rx.borrow() {
        return stop_outcome(reason, None, 0, 0, None);
    }

    let (chat, msg_id) = match parse_msg_link(&job.url) {
        Ok(v) => v,
        Err(code) => return fail_str(&code),
    };

    let rc = tokio::select! {
        reason = wait_stop_reason(&mut stop_rx) => return stop_outcome(reason, None, 0, 0, None),
        res = tg_client::resolve_chat(st, client, &chat) => match res {
            Ok(rc) => rc,
            Err(e) => return fail_str(&e),
        }
    };

    let msg_ids = [msg_id];
    let msg = tokio::select! {
        reason = wait_stop_reason(&mut stop_rx) => return stop_outcome(reason, None, 0, 0, None),
        res = client.get_messages_by_id(rc.peer, &msg_ids) => match res {
            Ok(mut v) => match v.pop().flatten() {
                Some(m) => m,
                None => return fail_str("MESSAGE_NOT_FOUND"),
            },
            Err(e) => return fail_str(&friendly_error(&e.to_string())),
        }
    };

    let media = match msg.media() {
        Some(m) => m,
        None => return fail_str("UNSUPPORTED_MEDIA"),
    };
    let meta = match media_meta(&media) {
        Some(m) => m,
        None => return fail_str("UNSUPPORTED_MEDIA"),
    };
    let source_id = archive_key(rc.identity.raw_id, msg_id);
    let lock = st.file_lock(&source_id);
    let _exclusive = tokio::select! {
        reason = wait_stop_reason(&mut stop_rx) => return stop_outcome(reason, meta.name.clone(), 0, meta.size, None),
        guard = lock.lock() => guard,
    };
    if let Some(reason) = *stop_rx.borrow() {
        return stop_outcome(reason, meta.name.clone(), 0, meta.size, None);
    }

    // Optional per-channel/date/type subdirectories + effective root.
    let cfg_dl = st.config.read().await.clone();
    let root = st.downloads_root().await;
    let mut base_dir = root.clone();
    if cfg_dl.organize_by_channel {
        let day = msg
            .date()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        let channel = sanitize_filename(rc.identity.title.as_deref().unwrap_or("channel"));
        base_dir = base_dir.join(channel).join(day);
    }
    if cfg_dl.classify_by_type {
        base_dir = base_dir.join(meta.kind.folder());
    }
    if let Err(error) = tokio::fs::create_dir_all(&base_dir).await {
        return fail_str(&format!("IO: {error}"));
    }
    drop(cfg_dl);

    let total = meta.size;
    let existing = st
        .library
        .lock()
        .await
        .data
        .downloads
        .get(&source_id)
        .cloned();
    if let Some(record) = existing {
        if record.root == root {
            if let Ok(path) = crate::api::resolve_download_path(st, &record.relative_path).await {
                if std::fs::metadata(path)
                    .map(|metadata| metadata.is_file() && metadata.len() == total)
                    .unwrap_or(false)
                {
                    return JobOutcome::Finished(TaskResult {
                        id: job.id,
                        url: job.url.clone(),
                        status: "success".into(),
                        name: Some(record.relative_path.clone()),
                        path: Some(record.relative_path),
                        size: Some(total),
                        cached: Some(true),
                        error: None,
                        created_at: job.created_at,
                        completed_at: now_ms(),
                    });
                }
            }
        }
    }
    let original = meta
        .name
        .clone()
        .unwrap_or_else(|| format!("media{}", meta.ext));
    let (file_name, target) =
        crate::library::download_target(&base_dir, &original, &source_id, &meta.ext);

    // Download into a .part temp file with a chunk journal (.part.progress).
    // A verified-complete file is renamed to the real name; an interrupted
    // one is RESUMED from its completed chunks on the next attempt.
    let part_path = base_dir.join(format!("{file_name}.part"));
    let journal_path = journal_for_part(&part_path);

    if let Some(inflight_job) = st.queue.in_flight.lock().await.get_mut(&job.id) {
        inflight_job.part_path = Some(part_path.clone());
        inflight_job.name = Some(file_name.clone());
        inflight_job.total = total;
    }

    // Resume detection: keep the partial only if its journal matches.
    let mut resume_done: Option<std::collections::HashSet<u64>> = None;
    let mut resumed_bytes: u64 = 0;
    if part_path.exists() {
        let mut usable = false;
        if let Ok(bytes) = std::fs::read(&journal_path) {
            if let Ok(j) = serde_json::from_slice::<PartJournal>(&bytes) {
                if j.total == total && j.chunk == CHUNK {
                    let set: std::collections::HashSet<u64> = j.done.into_iter().collect();
                    let chunks = total.div_ceil(CHUNK as u64);
                    let bytes_already: u64 = set
                        .iter()
                        .filter(|&&i| i < chunks)
                        .map(|&i| (total - i * CHUNK as u64).min(CHUNK as u64))
                        .sum();
                    if set.iter().all(|&i| i < chunks)
                        && std::fs::metadata(&part_path)
                            .map(|m| m.len() == total)
                            .unwrap_or(false)
                    {
                        usable = true;
                        resume_done = Some(set);
                        resumed_bytes = bytes_already;
                    }
                }
            }
        }
        if !usable {
            remove_part_bundle(&part_path);
        }
    }

    if let Some(reason) = *stop_rx.borrow() {
        return stop_outcome(
            reason,
            Some(file_name),
            resumed_bytes,
            total,
            Some(part_path),
        );
    }

    let initial_percent = if total > 0 {
        (resumed_bytes as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let started_at = now_ms();

    // Publish the active task before starting so the UI can show the name and resumed progress.
    let progress = ActiveTask {
        id: job.id,
        url: job.url.clone(),
        name: file_name.clone(),
        current: resumed_bytes,
        total,
        percent: initial_percent,
        created_at: job.created_at,
        started_at,
    };
    st.queue
        .active
        .lock()
        .await
        .insert(job.id, progress.clone());
    st.bus.emit(
        "queue:progress",
        serde_json::to_value(&progress).unwrap_or(serde_json::Value::Null),
    );

    let done = Arc::new(AtomicU64::new(resumed_bytes));
    let emitting = Arc::new(AtomicBool::new(true));

    // Progress emitter: polls the shared byte counter ~5x/sec.
    let emit_st = st.clone();
    let emit_done = done.clone();
    let emit_flag = emitting.clone();
    let mut emit_progress = progress;
    let emitter = tokio::spawn(async move {
        while emit_flag.load(Ordering::Relaxed) {
            let current = emit_done.load(Ordering::Relaxed);
            emit_progress.current = current;
            emit_progress.percent = if total > 0 {
                (current as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            emit_st
                .queue
                .active
                .lock()
                .await
                .insert(emit_progress.id, emit_progress.clone());
            emit_st.bus.emit(
                "queue:progress",
                serde_json::to_value(&emit_progress).unwrap_or(serde_json::Value::Null),
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });

    let dl_result = download_parallel(
        st,
        client,
        &media,
        &part_path,
        total,
        done,
        resume_done,
        &journal_path,
        job.id,
        stop_rx,
    )
    .await;

    emitting.store(false, Ordering::Relaxed);
    let _ = emitter.await;

    match dl_result {
        Ok(ParallelOutcome::Completed) => match tokio::fs::rename(&part_path, &target).await {
            Ok(()) => {
                let rel = target
                    .strip_prefix(&root)
                    .unwrap_or(target.as_path())
                    .to_string_lossy()
                    .replace('\\', "/");
                let _ = tokio::fs::remove_file(&journal_path).await;
                let completed_at = now_ms();
                let record = crate::library::DownloadRecord {
                    source_id: source_id.clone(),
                    source_url: job.url.clone(),
                    relative_path: rel.clone(),
                    root: root.clone(),
                    size: total,
                    kind: meta.kind.as_str().into(),
                    downloaded_at: Some(completed_at),
                };
                if let Err(error) = crate::library::record_download(st, record).await {
                    JobOutcome::Finished(TaskResult::failed(
                        job.id,
                        &job.url,
                        job.created_at,
                        format!("LIBRARY_WRITE_FAILED: {error}"),
                    ))
                } else {
                    archive_append(st, &source_id);
                    JobOutcome::Finished(TaskResult {
                        id: job.id,
                        url: job.url.clone(),
                        status: "success".into(),
                        name: Some(rel.clone()),
                        path: Some(rel),
                        size: Some(total),
                        cached: Some(false),
                        error: None,
                        created_at: job.created_at,
                        completed_at,
                    })
                }
            }
            Err(e) => JobOutcome::Finished(TaskResult::failed(
                job.id,
                &job.url,
                job.created_at,
                e.to_string(),
            )),
        },
        Ok(ParallelOutcome::Paused { written_bytes }) => JobOutcome::Paused(paused_from_job(
            job,
            Some(file_name),
            written_bytes,
            total,
            Some(part_path),
        )),
        Ok(ParallelOutcome::Cancelled) => JobOutcome::Cancelled {
            part_path: Some(part_path),
        },
        Err(e) => {
            // Keep .part + journal (unless the error is unrecoverable) so the
            // next attempt resumes from completed chunks.
            if e.starts_with("IO:") {
                remove_part_bundle(&part_path);
            }
            JobOutcome::Finished(TaskResult::failed(job.id, &job.url, job.created_at, e))
        }
    }
}

/// Chunk journal for resumable downloads: records which 512 KiB chunks of
/// the .part file are already on disk. Rewritten periodically (small file).
#[derive(serde::Serialize, serde::Deserialize)]
struct PartJournal {
    total: u64,
    chunk: i32,
    done: Vec<u64>,
}

fn write_journal(path: &std::path::Path, total: u64, written: &std::collections::HashSet<u64>) {
    let j = PartJournal {
        total,
        chunk: CHUNK,
        done: written.iter().copied().collect(),
    };
    // Atomic write: a hard kill mid-write must never truncate the journal.
    if let Ok(bytes) = serde_json::to_vec(&j) {
        let tmp = path.with_extension("progress.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

enum ParallelOutcome {
    Completed,
    Paused { written_bytes: u64 },
    Cancelled,
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
    resume_done: Option<std::collections::HashSet<u64>>,
    journal_path: &std::path::Path,
    job_id: u64,
    mut stop_rx: watch::Receiver<Option<StopReason>>,
) -> Result<ParallelOutcome, String> {
    let location = media.to_raw_input_location().ok_or("NOT_A_VIDEO")?;
    let session = st.session.read().await.clone().ok_or("NOT_LOGGED_IN")?;
    let home_dc = session.home_dc_id();

    // Open (or pre-allocate) the .part file — harmless on resume.
    let mut file = if target.exists() {
        tokio::fs::OpenOptions::new()
            .write(true)
            .open(target)
            .await
            .map_err(|e| format!("IO: {e}"))?
    } else {
        let f = tokio::fs::File::create(target)
            .await
            .map_err(|e| format!("IO: {e}"))?;
        f.set_len(total).await.map_err(|e| format!("IO: {e}"))?;
        f
    };

    // Chunk queue: everything not already on disk from a previous attempt.
    let total_chunks = total.div_ceil(CHUNK as u64);
    let skip = resume_done.unwrap_or_default();
    let remaining: std::collections::VecDeque<u64> =
        (0..total_chunks).filter(|i| !skip.contains(i)).collect();
    let remaining = Arc::new(tokio::sync::Mutex::new(remaining));

    let (tx, mut rx) = tokio::sync::mpsc::channel::<(u64, Vec<u8>, u64)>(8);

    let mut tasks = Vec::new();
    let st_for_workers = st.clone();
    for _ in 0..PARALLEL_WORKERS {
        let location = location.clone();
        let tx = tx.clone();
        let remaining = remaining.clone();
        let client = client.clone();
        let done = done.clone();
        let st_w = st_for_workers.clone();
        let mut w_stop_rx = stop_rx.clone();
        tasks.push(tokio::spawn(async move {
            let mut dc = home_dc;
            let mut retry_index: Option<u64> = None;
            loop {
                if w_stop_rx.borrow().is_some() || is_cancelled(&st_w, job_id) {
                    break;
                }
                // Claim the next chunk index.
                let index: u64 = match retry_index.take() {
                    Some(i) => i,
                    None => {
                        let mut q = remaining.lock().await;
                        match q.pop_front() {
                            Some(i) => i,
                            None => break,
                        }
                    }
                };
                let offset = (index * CHUNK as u64) as i64;
                let request = tl::functions::upload::GetFile {
                    precise: true,
                    cdn_supported: false,
                    location: location.clone(),
                    offset,
                    limit: CHUNK,
                };
                let resp = tokio::select! {
                    _ = wait_stop_reason(&mut w_stop_rx) => break,
                    res = client.invoke_in_dc(dc, &request) => res,
                };
                match resp {
                    Ok(tl::enums::upload::File::File(f)) => {
                        let n = f.bytes.len() as u64;
                        let expected = (total - offset as u64).min(CHUNK as u64);
                        if n != expected {
                            return Err(format!("SIZE_MISMATCH: chunk {index}: {n} != {expected}"));
                        }
                        if *w_stop_rx.borrow() == Some(StopReason::Cancel)
                            || is_cancelled(&st_w, job_id)
                        {
                            break;
                        }
                        done.fetch_add(n, Ordering::Relaxed);
                        if tx.send((offset as u64, f.bytes, index)).await.is_err() {
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
                            retry_index = Some(index);
                            continue;
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

    // Writer: place every chunk at its offset, journal progress periodically.
    let mut pos: u64 = 0;
    let mut write_err: Option<String> = None;
    let mut written: std::collections::HashSet<u64> = skip;
    let mut since_journal: u64 = 0;
    log::info!("[writer] starting, resume chunks: {}", written.len());
    loop {
        if *stop_rx.borrow() == Some(StopReason::Cancel) || is_cancelled(st, job_id) {
            break;
        }
        let item = tokio::select! {
            reason = wait_stop_reason(&mut stop_rx), if stop_rx.borrow().is_none() => {
                if reason == StopReason::Cancel {
                    break;
                }
                // On pause, keep draining buffered chunks already sent before workers exit.
                continue;
            }
            msg = rx.recv() => msg,
        };
        let Some((offset, data, index)) = item else {
            break;
        };
        if offset != pos {
            if let Err(e) = file.seek(std::io::SeekFrom::Start(offset)).await {
                write_err = Some(format!("IO: {e}"));
                break;
            }
        }
        if let Err(e) = file.write_all(&data).await {
            write_err = Some(format!("IO: {e}"));
            break;
        }
        pos = offset + data.len() as u64;
        written.insert(index);
        since_journal += 1;
        if since_journal >= 8 {
            since_journal = 0;
            write_journal(journal_path, total, &written);
            log::info!("[writer] {} chunks done, {} bytes", written.len(), pos);
        }
    }
    drop(rx);

    let stop_reason = *stop_rx.borrow();
    let cancelled = stop_reason == Some(StopReason::Cancel) || is_cancelled(st, job_id);

    if !cancelled {
        if let Err(e) = file.sync_all().await {
            write_err = Some(format!("IO: {e}"));
        }
    }
    // Always close the file handle before deleting or returning (critical on Windows).
    drop(file);

    for t in tasks {
        match t.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) if write_err.is_none() => write_err = Some(e),
            Err(e) if write_err.is_none() => write_err = Some(format!("JOIN: {e}")),
            _ => {}
        }
    }

    if cancelled {
        remove_part_bundle(target);
        return Ok(ParallelOutcome::Cancelled);
    }

    write_journal(journal_path, total, &written);

    if stop_reason == Some(StopReason::Pause) {
        let written_bytes: u64 = written
            .iter()
            .filter(|&&i| i < total_chunks)
            .map(|&i| (total - i * CHUNK as u64).min(CHUNK as u64))
            .sum();
        return Ok(ParallelOutcome::Paused { written_bytes });
    }

    if let Some(e) = write_err {
        return Err(friendly_error(&e));
    }

    // Verify against the advertised size.
    let local = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
    if written.len() as u64 != total_chunks || (total > 0 && local != total) {
        return Err(format!("SIZE_MISMATCH: {local} != {total}"));
    }
    Ok(ParallelOutcome::Completed)
}

/// Build the canonical link for a watched-channel message.
pub fn link_for_entry(entry: &crate::config::WatchEntry, msg_id: i32) -> String {
    make_msg_link(entry.username.as_deref(), entry.raw_id, msg_id)
}

/// Initial snapshot for the frontend (after a reload or on start).
pub async fn snapshot(st: &SharedState) -> serde_json::Value {
    let pending = st.queue.pending.lock().await;
    let inflight = st.queue.in_flight.lock().await;
    let paused = st.queue.paused.lock().await;
    let results: Vec<TaskResult> = st.queue.results.lock().await.clone();
    let active = st.queue.active.lock().await;
    let active_tasks: Vec<ActiveTask> = inflight
        .values()
        .map(|job| {
            active.get(&job.id).cloned().unwrap_or_else(|| ActiveTask {
                id: job.id,
                url: job.url.clone(),
                name: job.name.clone().unwrap_or_default(),
                current: job.current,
                total: job.total,
                percent: job.percent,
                created_at: job.created_at,
                started_at: job.created_at,
            })
        })
        .collect();
    serde_json::json!({
        "pending": pending
            .iter()
            .map(|j| serde_json::json!({
                "id": j.id,
                "url": j.url,
                "createdAt": j.created_at,
                "name": j.name,
                "current": j.current,
                "total": j.total,
                "percent": j.percent,
            }))
            .collect::<Vec<_>>(),
        "paused": paused
            .values()
            .map(|p| serde_json::json!({
                "id": p.id,
                "url": p.url,
                "name": p.name,
                "current": p.current,
                "total": p.total,
                "percent": p.percent,
                "createdAt": p.created_at,
                "pausedAt": p.paused_at,
            }))
            .collect::<Vec<_>>(),
        "active": active_tasks.first(),
        "activeTasks": active_tasks,
        "results": results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    async fn test_state() -> (PathBuf, SharedState) {
        let root = std::env::temp_dir().join(format!(
            "tg-grab-queue-ops-{}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        let st = crate::init_core(root.clone()).await;
        (root, st)
    }

    #[tokio::test]
    async fn pause_persists_across_restore_and_resume_preserves_id_and_progress() {
        let (root, st) = test_state().await;
        let ids = st.queue.enqueue(vec!["https://t.me/demo/101".into()]).await;
        let id = ids[0];
        let created_at = {
            let mut pending = st.queue.pending.lock().await;
            let job = pending.front_mut().unwrap();
            job.name = Some("clip.mp4".into());
            job.current = 512 * 1024;
            job.total = 1024 * 1024;
            job.percent = 50.0;
            job.created_at
        };

        pause_job(&st, id).await.unwrap();
        assert!(st.queue.pending.lock().await.is_empty());
        assert_eq!(st.queue.paused.lock().await.len(), 1);

        let restored = crate::state::QueueState::new(st.queue_path.clone());
        restored.restore(&st.queue_path).await;
        let paused_entry = restored.paused.lock().await.get(&id).cloned().unwrap();
        assert_eq!(paused_entry.url, "https://t.me/demo/101");
        assert_eq!(paused_entry.created_at, created_at);
        assert_eq!(paused_entry.current, 512 * 1024);
        assert_eq!(paused_entry.percent, 50.0);

        resume_job(&st, id).await.unwrap();
        assert!(st.queue.paused.lock().await.is_empty());
        let resumed = st.queue.pending.lock().await.pop_front().unwrap();
        assert_eq!(resumed.id, id);
        assert_eq!(resumed.created_at, created_at);
        assert_eq!(resumed.current, 512 * 1024);

        drop(st);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cancel_removes_paused_task_and_deletes_part_files() {
        let (root, st) = test_state().await;
        let dl_root = st.downloads_root().await;
        let part = dl_root.join("video_123_42.mp4.part");
        let journal = journal_for_part(&part);
        let tmp = journal_tmp_for_part(&part);
        std::fs::write(&part, b"partial").unwrap();
        std::fs::write(&journal, b"{}").unwrap();
        std::fs::write(&tmp, b"{}").unwrap();

        let ids = st.queue.enqueue(vec!["https://t.me/c/123/42".into()]).await;
        let id = ids[0];
        st.queue.pending.lock().await.front_mut().unwrap().part_path = Some(part.clone());
        pause_job(&st, id).await.unwrap();
        assert!(part.exists() && journal.exists());

        cancel_job(&st, id).await.unwrap();
        assert!(st.queue.paused.lock().await.is_empty());
        assert!(st.queue.pending.lock().await.is_empty());
        assert!(st.queue.results.lock().await.is_empty());
        assert!(!part.exists());
        assert!(!journal.exists());
        assert!(!tmp.exists());

        drop(st);
        std::fs::remove_dir_all(root).unwrap();
    }
}
