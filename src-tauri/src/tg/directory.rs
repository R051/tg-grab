//! Read-only channel picker and Telegram folder membership.
use crate::state::SharedState;
use crate::tg::client::{PeerIdentity, ensure_client};
use grammers_client::{peer::Peer, tl};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinedChannel {
    pub raw_id: i64,
    pub title: String,
    pub username: Option<String>,
    pub url: String,
    pub folder_ids: Vec<i32>,
}
#[derive(Debug, Serialize)]
pub struct TelegramFolder {
    pub id: i32,
    pub name: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDirectory {
    pub channels: Vec<JoinedChannel>,
    pub folders: Vec<TelegramFolder>,
    pub folders_error: Option<String>,
    pub truncated: bool,
}

fn includes(peers: &[tl::enums::InputPeer], id: i64) -> bool {
    peers
        .iter()
        .any(|peer| matches!(peer,tl::enums::InputPeer::Channel(channel) if channel.channel_id==id))
}
fn membership(
    filter: &tl::enums::DialogFilter,
    id: i64,
    group: bool,
    muted: bool,
    unread: bool,
    archived: bool,
) -> bool {
    match filter {
        tl::enums::DialogFilter::Default => false,
        tl::enums::DialogFilter::Chatlist(folder) => {
            includes(&folder.pinned_peers, id) || includes(&folder.include_peers, id)
        }
        tl::enums::DialogFilter::Filter(folder) => {
            if includes(&folder.exclude_peers, id) {
                return false;
            }
            if includes(&folder.pinned_peers, id) || includes(&folder.include_peers, id) {
                return true;
            }
            (if group {
                folder.groups
            } else {
                folder.broadcasts
            }) && !(folder.exclude_muted && muted)
                && !(folder.exclude_read && !unread)
                && !(folder.exclude_archived && archived)
        }
    }
}
fn folder_info(filter: &tl::enums::DialogFilter) -> Option<TelegramFolder> {
    let (id, title) = match filter {
        tl::enums::DialogFilter::Filter(f) => (f.id, &f.title),
        tl::enums::DialogFilter::Chatlist(f) => (f.id, &f.title),
        _ => return None,
    };
    let tl::enums::TextWithEntities::Entities(title) = title;
    Some(TelegramFolder {
        id,
        name: title.text.clone(),
    })
}
pub async fn list(st: &SharedState) -> Result<ChannelDirectory, String> {
    let client = ensure_client(st).await?;
    if !client.is_authorized().await.map_err(|e| e.to_string())? {
        return Err("NOT_LOGGED_IN".into());
    }
    let (filters, mut folders_error) = match client
        .invoke(&tl::functions::messages::GetDialogFilters {})
        .await
    {
        Ok(tl::enums::messages::DialogFilters::Filters(value)) => (value.filters, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let folders = filters.iter().filter_map(folder_info).collect();
    let mut channels = Vec::new();
    let mut dialogs = client.iter_dialogs();
    let mut scanned = 0;
    let now = chrono::Utc::now().timestamp();
    let mut group_muted = false;
    let mut broadcast_muted = false;
    if filters.iter().any(
        |filter| matches!(filter,tl::enums::DialogFilter::Filter(folder) if folder.exclude_muted),
    ) {
        for (peer, is_group) in [
            (tl::enums::InputNotifyPeer::InputNotifyChats, true),
            (tl::enums::InputNotifyPeer::InputNotifyBroadcasts, false),
        ] {
            match client
                .invoke(&tl::functions::account::GetNotifySettings { peer })
                .await
            {
                Ok(tl::enums::PeerNotifySettings::Settings(value)) => {
                    let muted = value.mute_until.is_some_and(|until| i64::from(until) > now);
                    if is_group {
                        group_muted = muted;
                    } else {
                        broadcast_muted = muted;
                    }
                }
                Err(error) => {
                    folders_error = Some(error.to_string());
                }
            }
        }
    }
    while let Some(dialog) = dialogs.next().await.map_err(|e| e.to_string())? {
        scanned += 1;
        if scanned > 5000 {
            break;
        }
        let group = match &dialog.peer {
            Peer::Channel(channel) => !channel.raw.broadcast,
            Peer::Group(group) => match &group.raw {
                tl::enums::Chat::Channel(_) => true,
                _ => continue,
            },
            _ => continue,
        };
        let identity = PeerIdentity::from_peer(&dialog.peer);
        let tl::enums::Dialog::Dialog(raw) = &dialog.raw else {
            continue;
        };
        let tl::enums::PeerNotifySettings::Settings(notify) = &raw.notify_settings;
        let muted = notify
            .mute_until
            .map(|until| i64::from(until) > now)
            .unwrap_or(if group { group_muted } else { broadcast_muted });
        let archived = raw.folder_id == Some(1);
        let unread = raw.unread_count > 0 || raw.unread_mark;
        let mut folder_ids: Vec<i32> = filters
            .iter()
            .filter(|filter| membership(filter, identity.raw_id, group, muted, unread, archived))
            .filter_map(|filter| folder_info(filter).map(|folder| folder.id))
            .collect();
        if archived {
            folder_ids.push(-1);
        }
        let url = identity
            .username
            .as_ref()
            .map(|name| format!("https://t.me/{name}"))
            .unwrap_or_else(|| format!("https://t.me/c/{}", identity.raw_id));
        channels.push(JoinedChannel {
            raw_id: identity.raw_id,
            title: identity.title.unwrap_or_default(),
            username: identity.username,
            url,
            folder_ids,
        });
        if scanned >= 5000 {
            break;
        }
    }
    channels.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(ChannelDirectory {
        channels,
        folders,
        folders_error,
        truncated: scanned >= 5000,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn telegram_folders_apply_inclusions_categories_and_exclusions() {
        let mut folder = tl::types::DialogFilter {
            contacts: false,
            non_contacts: false,
            groups: false,
            broadcasts: true,
            bots: false,
            exclude_muted: true,
            exclude_read: false,
            exclude_archived: true,
            title_noanimate: false,
            id: 2,
            title: tl::types::TextWithEntities {
                text: "Channels".into(),
                entities: vec![],
            }
            .into(),
            emoticon: None,
            color: None,
            pinned_peers: vec![],
            include_peers: vec![],
            exclude_peers: vec![],
        };
        assert!(membership(
            &folder.clone().into(),
            42,
            false,
            false,
            true,
            false
        ));
        assert!(!membership(
            &folder.clone().into(),
            42,
            true,
            false,
            true,
            false
        ));
        assert!(!membership(
            &folder.clone().into(),
            42,
            false,
            true,
            true,
            false
        ));
        folder.include_peers.push(
            tl::types::InputPeerChannel {
                channel_id: 42,
                access_hash: 0,
            }
            .into(),
        );
        assert!(membership(
            &folder.clone().into(),
            42,
            false,
            true,
            true,
            true
        ));
        folder.exclude_peers = folder.include_peers.clone();
        assert!(!membership(&folder.into(), 42, false, false, true, false));
    }
}
