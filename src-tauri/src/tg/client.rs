//! Client connection management: session, sender pool, peer resolution.

use crate::config::Config;
use crate::state::Inner;
use grammers_client::peer::Peer;
use grammers_client::{Client, tl};
use grammers_mtsender::{ConnectionParams, SenderPool};
use grammers_session::Session;
use grammers_session::storages::SqliteSession;
use grammers_session::types::{PeerAuth, PeerId, PeerInfo, PeerRef};
use std::sync::Arc;

/// Raw identity of a channel/supergroup extracted from a resolved [`Peer`].
pub struct PeerIdentity {
    pub raw_id: i64,
    pub access_hash: i64,
    pub title: Option<String>,
    pub username: Option<String>,
}

impl PeerIdentity {
    pub fn from_peer(peer: &Peer) -> PeerIdentity {
        match peer {
            Peer::Channel(ch) => PeerIdentity {
                raw_id: ch.raw.id,
                access_hash: ch.raw.access_hash.unwrap_or(0),
                title: Some(ch.title().to_string()),
                username: ch.username().map(str::to_owned),
            },
            Peer::Group(g) => {
                // Megagroups are channels on the wire.
                match &g.raw {
                    tl::enums::Chat::Channel(ch) => PeerIdentity {
                        raw_id: ch.id,
                        access_hash: ch.access_hash.unwrap_or(0),
                        title: Some(ch.title.clone()),
                        username: ch.username.clone(),
                    },
                    tl::enums::Chat::Chat(ch) => PeerIdentity {
                        raw_id: ch.id,
                        access_hash: 0,
                        title: Some(ch.title.clone()),
                        username: None,
                    },
                    tl::enums::Chat::Forbidden(f) => PeerIdentity {
                        raw_id: f.id,
                        access_hash: 0,
                        title: Some(f.title.clone()),
                        username: None,
                    },
                    tl::enums::Chat::Empty(_) => PeerIdentity {
                        raw_id: 0,
                        access_hash: 0,
                        title: None,
                        username: None,
                    },
                    tl::enums::Chat::ChannelForbidden(f) => PeerIdentity {
                        raw_id: f.id,
                        access_hash: 0,
                        title: Some(f.title.clone()),
                        username: None,
                    },
                }
            }
            Peer::User(_) => PeerIdentity {
                raw_id: 0,
                access_hash: 0,
                title: None,
                username: None,
            },
        }
    }
}

fn build_params(cfg: &Config) -> ConnectionParams {
    let mut params = ConnectionParams {
        device_model: "tg-grab".into(),
        system_version: std::env::consts::OS.into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        ..Default::default()
    };
    if let Some(url) = cfg
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        params.proxy_url = Some(url.to_owned());
    }
    params
}

/// Return the connected client, connecting a new pool on first use.
pub async fn ensure_client(st: &Inner) -> Result<Client, String> {
    let _connect = st.connect_lock.lock().await;
    if let Some(c) = st.client.read().await.clone() {
        return Ok(c);
    }
    let cfg = st.config.read().await.clone();
    let api_id = cfg.api_id.ok_or("API_NOT_CONFIGURED")?;

    let session = Arc::new(
        SqliteSession::open(&st.session_path)
            .await
            .map_err(|e| format!("SESSION_OPEN: {e}"))?,
    );

    let SenderPool {
        runner,
        handle,
        updates,
        ..
    } = SenderPool::with_configuration(session.clone(), api_id, build_params(&cfg));

    tokio::spawn(async move {
        runner.run().await;
    });

    let client = Client::new(handle.clone());
    *st.client.write().await = Some(client.clone());
    *st.session.write().await = Some(session);
    *st.pool_handle.lock().await = Some(handle);
    *st.updates_rx.lock().await = Some(updates);
    Ok(client)
}

/// Drop the current client/pool (logout or credential change).
pub async fn reset_client(st: &Inner) {
    let _connect = st.connect_lock.lock().await;
    if let Some(handle) = st.pool_handle.lock().await.take() {
        handle.quit();
    }
    *st.client.write().await = None;
    *st.session.write().await = None;
    *st.updates_rx.lock().await = None;
}

/// A resolved chat: session peer reference + raw identity.
pub struct ResolvedChat {
    pub peer: PeerRef,
    pub identity: PeerIdentity,
}

/// Resolve a parsed chat link to a [`ResolvedChat`].
///
/// * usernames go through the server lookup;
/// * raw channel ids use the access hash remembered in our config first,
///   then the session peer cache, then a hash-less attempt.
pub async fn resolve_chat(
    st: &Inner,
    client: &Client,
    link: &crate::tg::links::ChatLink,
) -> Result<ResolvedChat, String> {
    use crate::tg::links::ChatLink;
    match link {
        ChatLink::Username(u) => {
            let peer = client
                .resolve_username(u)
                .await
                .map_err(|e| format!("RESOLVE: {e}"))?
                .ok_or("CHAT_NOT_FOUND".to_string())?;
            let identity = PeerIdentity::from_peer(&peer);
            let peer = peer.to_ref().await.ok_or("PEER_REF".to_string())?;
            Ok(ResolvedChat { peer, identity })
        }
        ChatLink::ChannelRaw(raw_id) => {
            let id = PeerId::channel(*raw_id);
            // access hash remembered in the watched list?
            let cfg = st.config.read().await;
            let known = cfg
                .watched
                .iter()
                .find(|w| w.raw_id == *raw_id)
                .map(|w| w.access_hash);
            drop(cfg);
            let auth = match known {
                Some(hash) if hash != 0 => PeerAuth::from_hash(hash),
                _ => match session_peer_auth(st, id).await {
                    Some(auth) => auth,
                    None => PeerAuth::default(),
                },
            };
            let peer = client
                .resolve_peer(PeerRef { id, auth })
                .await
                .map_err(|e| format!("RESOLVE: {e}"))?;
            let identity = PeerIdentity::from_peer(&peer);
            let peer = peer.to_ref().await.ok_or("PEER_REF".to_string())?;
            Ok(ResolvedChat { peer, identity })
        }
    }
}

/// Look up a peer's access hash in the session cache.
async fn session_peer_auth(st: &Inner, id: PeerId) -> Option<PeerAuth> {
    let session = st.session.read().await.clone()?;
    match session.peer(id).await? {
        PeerInfo::Channel { auth, .. } => auth,
        PeerInfo::Chat { .. } => None,
        PeerInfo::User { auth, .. } => auth,
    }
}

/// Display name of the logged-in account.
pub async fn logged_in_name(client: &Client) -> Option<String> {
    let me = client.get_me().await.ok()?;
    Some(
        [me.first_name().unwrap_or(""), me.last_name().unwrap_or("")]
            .join(" ")
            .trim()
            .to_string(),
    )
}
