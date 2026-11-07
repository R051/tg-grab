//! Channel history browsing: paginated video listing for selective downloads.

use crate::tg::client::resolve_chat;
use crate::tg::links::{make_msg_link, parse_chat_link, sanitize_filename};
use crate::tg::links::ChatLink;
use grammers_client::media::Downloadable;
use grammers_client::message::Message;
use grammers_client::Client;
use serde::Serialize;

/// One video entry in a history page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub msg_id: i32,
    pub name: String,
    pub size: u64,
    /// Seconds, rounded; 0 when unknown.
    pub duration: u64,
    /// ISO-8601 UTC timestamp string (frontend localizes).
    pub date: Option<String>,
    /// The local file exists and matches the source size.
    pub downloaded: bool,
    /// A server thumbnail is available for this video.
    pub has_thumb: bool,
    /// Canonical t.me link (queue display + download pipeline input).
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInfo {
    pub raw_id: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub chat: ChatInfo,
    pub videos: Vec<VideoInfo>,
    /// Pass as `offset_id` to fetch the next (older) page.
    pub next_offset: i32,
    pub exhausted: bool,
}

const PAGE_VIDEOS: usize = 20;
const MAX_SCAN: usize = 300;

/// Browse one page of a channel's video history.
pub async fn browse(
    st: &crate::state::Inner,
    client: &Client,
    link: &str,
    offset_id: i32,
) -> Result<HistoryPage, String> {
    let chat_link: ChatLink = parse_chat_link(link)?;
    let rc = resolve_chat(st, client, &chat_link).await?;
    let identity = &rc.identity;

    let mut videos: Vec<VideoInfo> = Vec::new();
    let mut scanned = 0usize;
    let mut last_id = offset_id;
    let mut finished = true;

    let mut messages = client.iter_messages(rc.peer).offset_id(offset_id);
    loop {
        let Some(msg) = messages.next().await.map_err(|e| e.to_string())? else {
            break;
        };
        scanned += 1;
        last_id = msg.id();
        if let Some(info) =
            video_entry(st, &msg, identity.username.as_deref(), identity.raw_id)
        {
            videos.push(info);
            if videos.len() >= PAGE_VIDEOS {
                finished = false;
                break;
            }
        }
        if scanned >= MAX_SCAN {
            finished = false;
            break;
        }
    }

    Ok(HistoryPage {
        chat: ChatInfo {
            raw_id: identity.raw_id,
            title: identity.title.clone().unwrap_or_else(|| "…".into()),
        },
        videos,
        next_offset: last_id,
        exhausted: finished,
    })
}

/// Build a [`VideoInfo`] when the message carries a video, including local
/// completeness (size match) so the UI can mark already-downloaded items.
fn video_entry(
    st: &crate::state::Inner,
    msg: &Message,
    username: Option<&str>,
    raw_id: i64,
) -> Option<VideoInfo> {
    let media = msg.media()?;
    let meta = crate::tg::video_meta(&media)?;
    let msg_id = msg.id();
    let name = sanitize_filename(
        &meta.name.unwrap_or_else(|| format!("tg_video_{msg_id}.mp4")),
    );
    let path = st.downloads_dir.join(&name);
    let local = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let downloaded = meta.size > 0 && local == meta.size;

    // A thumbnail counts as available if cached on disk or offered by the server.
    let has_thumb = thumb_path(st, raw_id, msg_id).exists() || best_thumb(&media).is_some();

    Some(VideoInfo {
        msg_id,
        name,
        size: meta.size,
        duration: meta.duration_secs.map(|d| d as u64).unwrap_or(0),
        date: Some(msg.date().to_rfc3339()),
        downloaded,
        has_thumb,
        url: make_msg_link(username, raw_id, msg_id),
    })
}

/// Best-effort thumbnail fetch for one history entry.
/// Returns JPEG/PNG bytes: disk cache first, else picked from the document
/// (server-stored sizes are downloaded, inline sizes are taken as-is).
pub async fn fetch_thumb(
    st: &crate::state::Inner,
    client: &Client,
    media: &grammers_client::media::Media,
    raw_id: i64,
    msg_id: i32,
) -> Option<Vec<u8>> {
    let path = thumb_path(st, raw_id, msg_id);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Some(bytes);
    }
    let thumb = best_thumb(media)?;
    let bytes: Vec<u8> = match thumb {
        ThumbSource::Inline(data) => data,
        ThumbSource::Remote(ps) => {
            let mut it = client.iter_download(&ps);
            let mut buf = Vec::new();
            while let Some(chunk) = it.next().await.ok()? {
                buf.extend_from_slice(&chunk);
            }
            buf
        }
    };
    if bytes.is_empty() {
        return None;
    }
    // Cache for next time (best effort).
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&path, &bytes).ok();
    Some(bytes)
}

enum ThumbSource {
    Inline(Vec<u8>),
    Remote(grammers_client::media::PhotoSize),
}

/// Pick the largest usable thumbnail of the document's media.
fn best_thumb(media: &grammers_client::media::Media) -> Option<ThumbSource> {
    use grammers_client::media::Media;
    let doc = match media {
        Media::Document(d) => d,
        _ => return None,
    };
    let mut sizes = doc.thumbs();
    sizes.sort_by_key(|s| std::cmp::Reverse(s.size()));
    for s in sizes {
        if s.size() == 0 {
            continue;
        }
        if s.to_data().is_some() {
            return Some(ThumbSource::Inline(s.to_data()?));
        }
        if s.to_raw_input_location().is_some() {
            return Some(ThumbSource::Remote(s));
        }
    }
    None
}

fn thumb_path(st: &crate::state::Inner, raw_id: i64, msg_id: i32) -> std::path::PathBuf {
    st.downloads_dir
        .join("thumbs")
        .join(format!("{raw_id}_{msg_id}.jpg"))
}
