//! Shared application state managed by Tauri.

use crate::config::Config;
use crate::tg::queue::TaskResult;
use grammers_client::client::{LoginToken, PasswordToken};
use grammers_client::Client;
use grammers_mtsender::SenderPoolFatHandle;
use grammers_session::storages::SqliteSession;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

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
    pub next_id: std::sync::atomic::AtomicU64,
    pub pending: Mutex<VecDeque<Job>>,
    pub results: Mutex<Vec<TaskResult>>,
    pub active: Mutex<Option<ActiveTask>>,
    /// Where the pending-journal lives (set at startup).
    pub path: std::path::PathBuf,
}

impl QueueState {
    /// Write the pending jobs to disk so a restart can restore them.
    pub async fn persist(&self, path: &std::path::Path) {
        let pending = self.pending.lock().await;
        let jobs: Vec<&Job> = pending.iter().collect();
        let json = serde_json::json!({ "jobs": jobs
            .iter()
            .map(|j| serde_json::json!({"id": j.id, "url": j.url}))
            .collect::<Vec<_>>() });
        drop(pending);
        let _ = std::fs::write(path, serde_json::to_vec(&json).unwrap_or_default());
    }

    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            next_id: std::sync::atomic::AtomicU64::new(1),
            pending: Mutex::new(VecDeque::new()),
            results: Mutex::new(Vec::new()),
            active: Mutex::new(None),
            path,
        }
    }

    pub async fn enqueue(&self, urls: Vec<String>) -> Vec<u64> {
        let mut ids = Vec::with_capacity(urls.len());
        let mut pending = self.pending.lock().await;
        for url in urls {
            let id = self
                .next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            pending.push_back(Job { id, url });
            ids.push(id);
        }
        drop(pending);
        self.persist(&self.path).await;
        ids
    }
}

pub struct Inner {
    pub config_path: PathBuf,
    pub session_path: PathBuf,
    pub queue_path: PathBuf,
    pub downloads_dir: PathBuf,
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
    pub queue: QueueState,
}

pub type SharedState = Arc<Inner>;
