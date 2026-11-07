//! Watcher: real-time new-video detection + periodic catch-up.

use crate::state::SharedState;
use crate::tg::client::resolve_chat;
use crate::tg::links::{ChatLink, make_msg_link};
use crate::tg::media_meta;
use crate::tg::queue::link_for_entry;

use grammers_client::Client;
use grammers_client::client::UpdatesConfiguration;
use grammers_client::update::Update;
use grammers_session::types::PeerId;
use std::time::Duration;

/// Spawn the always-running watcher task. It waits for authorization,
/// then consumes the update stream; on disconnect it waits and retries.
pub fn spawn(st: SharedState) {
    // Only one watcher task for the lifetime of the process.
    if st
        .watcher_running
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return;
    }
    tokio::spawn(async move {
        watcher_loop(st).await;
    });
}

async fn watcher_loop(st: SharedState) {
    loop {
        // Wait until a connected, authorized client exists.
        let client = loop {
            let c = st.client.read().await.clone();
            if let Some(c) = c {
                if c.is_authorized().await.unwrap_or(false) {
                    break c;
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        };

        // Take the raw updates channel (recreated on every reconnect).
        let rx = st.updates_rx.lock().await.take();
        let Some(rx) = rx else {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        };
        if let Err(e) = catchup_watched(&st, &client).await {
            log::warn!("catchup failed: {e}");
        }
        spawn_sweeper(st.clone());

        let mut stream = client
            .stream_updates(
                rx,
                UpdatesConfiguration {
                    catch_up: true,
                    ..Default::default()
                },
            )
            .await;

        log::info!("watcher: update stream started");
        loop {
            match stream.next().await {
                Ok(update) => {
                    if let Err(e) = handle_update(&st, &client, update).await {
                        log::warn!("update handling failed: {e}");
                    }
                }
                Err(e) => {
                    log::warn!("update stream error: {e}");
                    break;
                }
            }
        }
        // Stream died (logout or disconnect) — wait and re-enter the outer loop.
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn spawn_sweeper(st: SharedState) {
    if st
        .sweeper_running
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(600)).await;
        loop {
            let client = st.client.read().await.clone();
            if let Some(client) = client {
                if client.is_authorized().await.unwrap_or(false) {
                    if let Err(e) = catchup_watched(&st, &client).await {
                        log::warn!("periodic sweep failed: {e}");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

/// Enqueue downloads for videos posted in watched chats.
async fn handle_update(st: &SharedState, _client: &Client, update: Update) -> Result<(), String> {
    let Update::NewMessage(msg) = update else {
        return Ok(());
    };

    let msg_peer = msg.peer_id();
    let entry = {
        let cfg = st.config.read().await;
        cfg.watched
            .iter()
            .find(|w| w.enabled && msg_peer == PeerId::channel(w.raw_id))
            .cloned()
    };
    let Some(entry) = entry else {
        return Ok(());
    };
    if msg.outgoing() {
        return Ok(());
    }

    // Respect configured media kinds.
    let Some(media) = msg.media() else {
        return Ok(());
    };
    let Some(meta) = media_meta(&media) else {
        return Ok(());
    };
    {
        let cfg = st.config.read().await;
        let kind_str = meta.kind.as_str().to_string();
        if !cfg.media_kinds.iter().any(|k| k == &kind_str) {
            return Ok(());
        }
    }

    let msg_id = msg.id();
    if msg_id <= entry.last_msg_id {
        return Ok(());
    }
    let url = link_for_entry(&entry, msg_id);
    st.queue.enqueue(vec![url]).await;
    st.bus.emit("queue:pending", serde_json::Value::Bool(true));

    // Advance the watch point and persist.
    let mut cfg = st.config.write().await;
    if let Some(idx) = cfg.watched.iter().position(|w| w.raw_id == entry.raw_id) {
        cfg.watched[idx].last_msg_id = cfg.watched[idx].last_msg_id.max(msg_id);
        if let Err(e) = cfg.save(&st.config_path) {
            log::warn!("config save failed: {e}");
        }
    }
    Ok(())
}

/// Enqueue videos newer than `last_msg_id` for every enabled watch entry.
pub async fn catchup_watched(st: &SharedState, client: &Client) -> Result<(), String> {
    let _catchup = st.catchup_lock.lock().await;
    let entries: Vec<crate::config::WatchEntry> = {
        let cfg = st.config.read().await;
        cfg.watched
            .iter()
            .filter(|w| w.enabled && w.last_msg_id > 0)
            .cloned()
            .collect()
    };
    if entries.is_empty() {
        return Ok(());
    }

    let mut added = false;
    for entry in entries {
        let link = ChatLink::ChannelRaw(entry.raw_id);
        let rc = match resolve_chat(st, client, &link).await {
            Ok(rc) => rc,
            Err(e) => {
                log::warn!("catchup resolve {}: {e}", entry.title);
                continue;
            }
        };

        let mut urls: Vec<String> = Vec::new();
        let mut max_id = entry.last_msg_id;
        // Start at the newest message and stop at the saved checkpoint.
        let mut messages = client.iter_messages(rc.peer);
        loop {
            let Some(msg) = messages.next().await.map_err(|e| e.to_string())? else {
                break;
            };
            if msg.id() <= entry.last_msg_id {
                break;
            }
            if let Some(media) = msg.media() {
                let allowed = {
                    let cfg = st.config.read().await;
                    media_meta(&media)
                        .is_some_and(|m| cfg.media_kinds.iter().any(|k| k == m.kind.as_str()))
                };
                if allowed {
                    urls.push(make_msg_link(
                        entry.username.as_deref(),
                        entry.raw_id,
                        msg.id(),
                    ));
                }
            }
            max_id = max_id.max(msg.id());
        }

        if !urls.is_empty() {
            log::info!("catchup {}: {} new videos", entry.title, urls.len());
            urls.reverse();
            st.queue.enqueue(urls).await;
            added = true;
        }

        let mut cfg = st.config.write().await;
        if let Some(idx) = cfg.watched.iter().position(|w| w.raw_id == entry.raw_id) {
            cfg.watched[idx].last_msg_id = cfg.watched[idx].last_msg_id.max(max_id);
        }
        cfg.save(&st.config_path)?;
    }

    if added {
        st.bus.emit("queue:pending", serde_json::Value::Bool(true));
        st.bus.emit("watch:changed", serde_json::Value::Bool(true));
    }
    Ok(())
}
