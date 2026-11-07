//! Watcher: real-time new-video detection + periodic catch-up.

use crate::state::SharedState;
use crate::tg::client::resolve_chat;
use crate::tg::links::{make_msg_link, ChatLink};
use crate::tg::queue::link_for_entry;
use grammers_client::client::UpdatesConfiguration;
use grammers_client::update::Update;
use grammers_client::Client;
use grammers_session::types::PeerId;
use std::time::Duration;

/// Spawn the always-running watcher task. It waits for authorization,
/// then consumes the update stream; on disconnect it waits and retries.
pub fn spawn(st: SharedState) {
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

        // One-shot catch-up for messages missed while the app was closed.
        if let Err(e) = catchup_watched(&st, &client).await {
            log::warn!("catchup failed: {e}");
        }
        // Periodic belt-and-braces sweep (long connections can drop updates).
        spawn_sweeper(st.clone());

        // Take the raw updates channel (recreated on every reconnect).
        let rx = st.updates_rx.lock().await.take();
        let Some(rx) = rx else {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        };

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

    // Video only.
    let Some(media) = msg.media() else {
        return Ok(());
    };
    if crate::tg::video_meta(&media).is_none() {
        return Ok(());
    }

    let msg_id = msg.id();
    let url = link_for_entry(&entry, msg_id);
    st.queue.enqueue(vec![url]).await;
    st.bus.emit("queue:pending", serde_json::Value::Bool(true));

    // Advance the watch point and persist.
    let mut cfg = st.config.write().await;
    if let Some(idx) = cfg.watched.iter().position(|w| w.raw_id == entry.raw_id) {
        cfg.watched[idx].last_msg_id = cfg.watched[idx].last_msg_id.max(msg_id);
        let snapshot = cfg.clone();
        drop(cfg);
        if let Err(e) = snapshot.save(&st.config_path) {
            log::warn!("config save failed: {e}");
        }
    }
    Ok(())
}

/// Enqueue videos newer than `last_msg_id` for every enabled watch entry.
pub async fn catchup_watched(st: &SharedState, client: &Client) -> Result<(), String> {
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

    let mut all_urls: Vec<String> = Vec::new();
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
        let mut messages = client
            .iter_messages(rc.peer)
            .offset_id(entry.last_msg_id.saturating_add(1));
        loop {
            let Some(msg) = messages.next().await.map_err(|e| e.to_string())? else {
                break;
            };
            // offset_id is an exclusive upper bound; ids only decrease from here.
            if msg.id() <= entry.last_msg_id {
                break;
            }
            if let Some(media) = msg.media() {
                if crate::tg::video_meta(&media).is_some() {
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
            all_urls.extend(urls);
        }

        let mut cfg = st.config.write().await;
        if let Some(idx) = cfg.watched.iter().position(|w| w.raw_id == entry.raw_id) {
            cfg.watched[idx].last_msg_id = max_id;
        }
        let snapshot = cfg.clone();
        drop(cfg);
        let _ = snapshot.save(&st.config_path);
    }

    if !all_urls.is_empty() {
        st.queue.enqueue(all_urls).await;
        st.bus.emit("queue:pending", serde_json::Value::Bool(true));
        st.bus.emit("watch:changed", serde_json::Value::Bool(true));
    }
    Ok(())
}
