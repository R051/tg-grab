//! Shared application state managed by Tauri (or the web server shell).

use crate::config::Config;
use crate::tg::queue::TaskResult;
use grammers_client::Client;
use grammers_client::client::{LoginToken, PasswordToken};
use grammers_mtsender::SenderPoolFatHandle;
use grammers_session::storages::SqliteSession;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use tokio::sync::{Mutex, RwLock, mpsc};

/// One job waiting in (or being processed by) the download queue.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: u64,
    pub url: String,
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
}

pub struct QueueState {
    pub next_id: AtomicU64,
    pub pending: Mutex<VecDeque<Job>>,
    /// The job currently being downloaded (journalled so a restart can
    /// re-queue it instead of losing it).
    pub in_flight: Mutex<BTreeMap<u64, Job>>,
    /// Jobs the user asked to cancel (checked by the writer loop).
    pub cancel_flags: std::sync::Mutex<std::collections::HashSet<u64>>,
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
            cancel_flags: std::sync::Mutex::new(std::collections::HashSet::new()),
            path,
            active: Mutex::new(BTreeMap::new()),
            results: Mutex::new(Vec::new()),
            persist_lock: Mutex::new(()),
            no_auth_until: AtomicU64::new(0),
            archive_path,
        }
    }

    /// Write the pending + in-flight jobs to disk so a restart can restore them.
    pub async fn persist(&self, path: &std::path::Path) {
        let _writer = self.persist_lock.lock().await;
        let pending = self.pending.lock().await;
        let inflight = self.in_flight.lock().await;
        let mut list: Vec<serde_json::Value> = Vec::new();
        for j in inflight.values() {
            list.push(serde_json::json!({"id": j.id, "url": j.url}));
        }
        for j in pending.iter() {
            list.push(serde_json::json!({"id": j.id, "url": j.url}));
        }
        drop(inflight);
        drop(pending);
        let json = serde_json::json!({ "jobs": list });
        let tmp = path.with_extension("json.tmp");
        let result = std::fs::write(&tmp, serde_json::to_vec(&json).unwrap_or_default())
            .and_then(|_| std::fs::rename(&tmp, path));
        if let Err(e) = result {
            log::error!("could not persist download queue: {e}");
        }
    }

    pub async fn enqueue(&self, urls: Vec<String>) -> Vec<u64> {
        let mut ids = Vec::with_capacity(urls.len());
        {
            let mut pending = self.pending.lock().await;
            let inflight = self.in_flight.lock().await;
            let mut seen: HashSet<String> = pending
                .iter()
                .chain(inflight.values())
                .map(|job| job.url.clone())
                .collect();
            for url in urls {
                if !seen.insert(url.clone()) {
                    continue;
                }
                let id = self
                    .next_id
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                pending.push_back(Job { id, url });
                ids.push(id);
            }
        }
        self.persist(&self.path).await;
        ids
    }
}

pub struct Inner {
    pub bus: crate::events::EventBus,
    pub config_path: PathBuf,
    pub session_path: PathBuf,
    pub queue_path: PathBuf,
    pub default_downloads_dir: PathBuf,
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
    pub queue: QueueState,
}

impl Inner {
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
