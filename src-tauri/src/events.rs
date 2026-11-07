//! Event bus: decouples core logic from any UI shell (Tauri window / web WS).
//!
//! Core modules emit named events with JSON payloads; each frontend shell
//! subscribes (Tauri re-emits as window events, web forwards over WebSocket).

use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus(Arc<broadcast::Sender<(String, Value)>>);

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self(Arc::new(tx))
    }

    /// Emit an event to all subscribers. Safe to call from anywhere;
    /// no-subscriber and slow-consumer situations are both non-fatal.
    pub fn emit(&self, event: &str, payload: Value) {
        let _ = self.0.send((event.to_string(), payload));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
        self.0.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
