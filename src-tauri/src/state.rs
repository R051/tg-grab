//! Shared application state managed by Tauri (or the web server shell).

use crate::config::Config;
use crate::tg::queue::TaskResult;
use grammers_client::Client;
use grammers_client::client::{LoginToken, PasswordToken};
use grammers_mtsender::SenderPoolFatHandle;
use grammers_session::storages::SqliteSession;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use tokio::sync::{Mutex, Notify, RwLock, mpsc, watch};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Pause,
    Cancel,
}

/// One job waiting in (or being processed by) the download queue.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub url: String,
    #[serde(default = "now_ms")]
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub current: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_path: Option<PathBuf>,
}

/// A paused job persisted with partial progress so it can be resumed later.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PausedTask {
    pub id: u64,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub current: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub percent: f64,
    #[serde(default = "now_ms")]
    pub created_at: i64,
    #[serde(default = "now_ms")]
    pub paused_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_path: Option<PathBuf>,
}

/// Snapshot of the currently downloading item, pushed to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTask {
    pub id: u64,
    pub url: String,
    pub name: String,
    pub current: u64,
    pub total: u64,
    pub percent: f64,
    pub created_at: i64,
    pub started_at: i64,
}

pub struct InFlightControl {
    pub stop_tx: watch::Sender<Option<StopReason>>,
    pub done_notify: Arc<Notify>,
}

pub struct QueueState {
    pub next_id: AtomicU64,
    pub pending: Mutex<VecDeque<Job>>,
    /// The job currently being downloaded (journalled so a restart can
    /// re-queue it instead of losing it).
    pub in_flight: Mutex<BTreeMap<u64, Job>>,
    /// Jobs paused by the user, persisted alongside pending jobs.
    pub paused: Mutex<BTreeMap<u64, PausedTask>>,
    /// Stop signal & completion notifier for active in-flight tasks.
    pub controls: std::sync::Mutex<HashMap<u64, InFlightControl>>,
    /// Legacy cancel flag fallback (if needed); controls is primary.
    pub cancel_flags: std::sync::Mutex<std::collections::HashSet<u64>>,
    pub wake: Notify,
    /// Where the pending-journal lives (set at startup).
    pub path: PathBuf,
    /// yt-dlp style archive of downloaded items (one key per line).
    pub archive_path: PathBuf,
    pub active: Mutex<BTreeMap<u64, ActiveTask>>,
    pub results: Mutex<Vec<TaskResult>>,
    persist_lock: Mutex<()>,
    /// Unix-ms deadline: jobs fail with NOT_LOGGED_IN only after this passes
    /// without a login (gives restored tasks time for the startup login).
    pub no_auth_until: AtomicU64,
}

impl QueueState {
    pub fn new(path: PathBuf) -> Self {
        let archive_path = path.with_extension("archive.txt");
        Self {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(VecDeque::new()),
            in_flight: Mutex::new(BTreeMap::new()),
            paused: Mutex::new(BTreeMap::new()),
            controls: std::sync::Mutex::new(HashMap::new()),
            cancel_flags: std::sync::Mutex::new(std::collections::HashSet::new()),
            wake: Notify::new(),
            path,
            active: Mutex::new(BTreeMap::new()),
            results: Mutex::new(Vec::new()),
            persist_lock: Mutex::new(()),
            no_auth_until: AtomicU64::new(0),
            archive_path,
        }
    }

    /// Write the pending + in-flight + paused jobs to disk so a restart can restore them.
    pub async fn persist(&self, path: &std::path::Path) {
        let _writer = self.persist_lock.lock().await;
        let pending = self.pending.lock().await;
        let inflight = self.in_flight.lock().await;
        let paused = self.paused.lock().await;
        let mut jobs: Vec<Job> = Vec::with_capacity(inflight.len() + pending.len());
        jobs.extend(inflight.values().cloned());
        jobs.extend(pending.iter().cloned());
        let paused_list: Vec<PausedTask> = paused.values().cloned().collect();
        drop(paused);
        drop(inflight);
        drop(pending);
        let json = serde_json::json!({
            "jobs": jobs,
            "paused": paused_list,
        });
        let tmp = path.with_extension("json.tmp");
        let result = std::fs::write(&tmp, serde_json::to_vec(&json).unwrap_or_default())
            .and_then(|_| std::fs::rename(&tmp, path));
        if let Err(e) = result {
            log::error!("could not persist download queue: {e}");
        }
    }

    /// Restore pending & paused tasks from disk, preserving IDs & timestamps.
    pub async fn restore(&self, path: &std::path::Path) {
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return;
        };
        let mut max_id = 0u64;
        let mut seen_urls = HashSet::new();
        let mut restored_pending = VecDeque::new();
        let mut restored_paused = BTreeMap::new();
        let fallback_now = now_ms();

        if let Some(paused) = v.get("paused").and_then(|p| p.as_array()) {
            for entry in paused {
                let Ok(mut task) = serde_json::from_value::<PausedTask>(entry.clone()) else {
                    continue;
                };
                if task.url.is_empty() || !seen_urls.insert(task.url.clone()) {
                    continue;
                }
                if task.id == 0 {
                    max_id += 1;
                    task.id = max_id;
                } else {
                    max_id = max_id.max(task.id);
                }
                restored_paused.insert(task.id, task);
            }
        }

        if let Some(jobs) = v.get("jobs").and_then(|j| j.as_array()) {
            for entry in jobs {
                if let Ok(mut job) = serde_json::from_value::<Job>(entry.clone()) {
                    if job.url.is_empty() || !seen_urls.insert(job.url.clone()) {
                        continue;
                    }
                    if job.id == 0 {
                        max_id += 1;
                        job.id = max_id;
                    } else {
                        max_id = max_id.max(job.id);
                    }
                    restored_pending.push_back(job);
                } else if let Some(url) = entry.get("url").and_then(|u| u.as_str()) {
                    if url.is_empty() || !seen_urls.insert(url.to_string()) {
                        continue;
                    }
                    let id = entry
                        .get("id")
                        .and_then(|x| x.as_u64())
                        .filter(|&x| x > 0)
                        .unwrap_or_else(|| {
                            max_id += 1;
                            max_id
                        });
                    max_id = max_id.max(id);
                    restored_pending.push_back(Job {
                        id,
                        url: url.to_string(),
                        created_at: fallback_now,
                        name: None,
                        current: 0,
                        total: 0,
                        percent: 0.0,
                        part_path: None,
                    });
                }
            }
        }

        if !restored_pending.is_empty() || !restored_paused.is_empty() {
            *self.pending.lock().await = restored_pending;
            *self.paused.lock().await = restored_paused;
            let next = max_id.saturating_add(1);
            self.next_id
                .fetch_max(next, std::sync::atomic::Ordering::SeqCst);
            log::info!("restored download queue state");
        }
    }

    pub async fn enqueue(&self, urls: Vec<String>) -> Vec<u64> {
        let mut ids = Vec::with_capacity(urls.len());
        {
            let mut pending = self.pending.lock().await;
            let inflight = self.in_flight.lock().await;
            let mut paused = self.paused.lock().await;
            let mut seen: HashSet<String> = pending
                .iter()
                .chain(inflight.values())
                .map(|job| job.url.clone())
                .collect();
            let now = now_ms();
            for url in urls {
                if !seen.insert(url.clone()) {
                    continue;
                }
                if let Some(paused_id) = paused
                    .iter()
                    .find(|(_, task)| task.url == url)
                    .map(|(id, _)| *id)
                {
                    if let Some(task) = paused.remove(&paused_id) {
                        pending.push_back(Job {
                            id: task.id,
                            url: task.url,
                            created_at: task.created_at,
                            name: task.name,
                            current: task.current,
                            total: task.total,
                            percent: task.percent,
                            part_path: task.part_path,
                        });
                        ids.push(task.id);
                        continue;
                    }
                }
                let id = self
                    .next_id
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                pending.push_back(Job {
                    id,
                    url,
                    created_at: now,
                    name: None,
                    current: 0,
                    total: 0,
                    percent: 0.0,
                    part_path: None,
                });
                ids.push(id);
            }
        }
        self.persist(&self.path).await;
        self.wake.notify_waiters();
        ids
    }
}

pub struct Inner {
    pub bus: crate::events::EventBus,
    pub config_path: PathBuf,
    pub session_path: PathBuf,
    pub queue_path: PathBuf,
    pub default_downloads_dir: PathBuf,
    pub library: Mutex<crate::library::LibraryStore>,
    pub file_locks: std::sync::Mutex<std::collections::HashMap<String, std::sync::Weak<Mutex<()>>>>,
    pub config: RwLock<Config>,
    /// Connected Telegram client (present once API is configured and connect succeeded).
    pub client: RwLock<Option<Client>>,
    /// The sqlite session shared with the sender pool (for peer-cache lookups).
    pub session: RwLock<Option<Arc<SqliteSession>>>,
    /// Handle used to quit the pool runner on logout.
    pub pool_handle: Mutex<Option<SenderPoolFatHandle>>,
    /// Raw updates channel, consumed once by the watcher task.
    pub updates_rx: Mutex<Option<mpsc::UnboundedReceiver<grammers_session::updates::UpdatesLike>>>,
    /// Pending login token between "send code" and "verify code".
    pub login_token: Mutex<Option<LoginToken>>,
    /// Pending 2FA token between "code ok" and "password ok".
    pub password_token: Mutex<Option<PasswordToken>>,
    pub auth_lock: Mutex<()>,
    pub connect_lock: Mutex<()>,
    pub catchup_lock: Mutex<()>,
    /// Ensures exactly one watcher task and one sweeper task.
    pub watcher_running: AtomicBool,
    pub sweeper_running: AtomicBool,
    pub window_close_pending: AtomicBool,
    pub queue: QueueState,
}

impl Inner {
    pub fn file_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.file_locks.lock().unwrap();
        if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
            return lock;
        }
        if locks.len() > 256 {
            locks.retain(|_, lock| lock.strong_count() > 0);
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(key.to_string(), Arc::downgrade(&lock));
        lock
    }
    /// Effective downloads root: user-configured, else the default.
    pub async fn downloads_root(&self) -> PathBuf {
        let custom = self
            .config
            .read()
            .await
            .downloads_dir
            .clone()
            .filter(|p| !p.as_os_str().is_empty());
        custom.unwrap_or_else(|| self.default_downloads_dir.clone())
    }
}

pub type SharedState = Arc<Inner>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_jobs_are_deduplicated_and_all_inflight_jobs_are_journalled() {
        let root = std::env::temp_dir().join(format!(
            "tg-grab-queue-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let queue = QueueState::new(root.join("queue.json"));
        assert_eq!(
            queue
                .enqueue(vec!["A".into(), "A".into(), "B".into()])
                .await
                .len(),
            2
        );
        let jobs: Vec<Job> = queue.pending.lock().await.drain(..).collect();
        for job in jobs {
            queue.in_flight.lock().await.insert(job.id, job);
        }
        assert!(queue.enqueue(vec!["A".into()]).await.is_empty());
        queue.persist(&queue.path).await;
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&queue.path).unwrap()).unwrap();
        assert_eq!(persisted["jobs"].as_array().unwrap().len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
