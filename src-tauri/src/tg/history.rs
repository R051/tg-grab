//! Channel history browsing: paginated video listing for selective downloads.

use crate::tg::client::resolve_chat;
use crate::tg::links::ChatLink;
use crate::tg::links::{make_msg_link, parse_chat_link, sanitize_filename};
use chrono::Local;
use grammers_client::Client;
use crate::tg::media_meta;
use grammers_client::media::Downloadable;

use grammers_client::message::Message;
use serde::Serialize;

/// One video entry in a history page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub msg_id: i32,
    pub kind: String,
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
    /// Keep this date while paging within it; null continues into older history.
    pub next_date: Option<String>,
    pub exhausted: bool,
}

const PAGE_VIDEOS: usize = 120;
const MAX_SCAN: usize = 1200;

/// Browse one page of a channel's video history.
///
/// `date` (local "YYYY-MM-DD") jumps straight to that day via the server's
/// max_date bound and collects every video posted within the day.
pub async fn browse(
    st: &crate::state::Inner,
    client: &Client,
    link: &str,
    offset_id: i32,
    date: Option<String>,
    day_start: Option<i64>,
    day_end: Option<i64>,
) -> Result<HistoryPage, String> {
    let chat_link: ChatLink = parse_chat_link(link)?;
    let rc = resolve_chat(st, client, &chat_link).await?;
    let identity = &rc.identity;

    // The browser supplies its local-day UTC boundaries (including DST).
    let (day_start, day_end) = match &date {
        Some(day) => {
            let d = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
                .map_err(|_| "INVALID_DATE".to_string())?;
            match (day_start, day_end) {
                (Some(start), Some(end))
                    if start >= 0
                        && end <= i32::MAX as i64
                        && end > start
                        && end - start <= 172800 =>
                {
                    (Some(start), Some(end))
                }
                (None, None) => {
                    use chrono::TimeZone;
                    let start = Local
                        .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
                        .earliest()
                        .ok_or("INVALID_DATE")?;
                    let end = Local
                        .from_local_datetime(
                            &d.succ_opt()
                                .ok_or("INVALID_DATE")?
                                .and_hms_opt(0, 0, 0)
                                .unwrap(),
                        )
                        .earliest()
                        .ok_or("INVALID_DATE")?;
                    (Some(start.timestamp()), Some(end.timestamp()))
                }
                _ => return Err("INVALID_DATE".into()),
            }
        }
        None => (None, None),
    };

    let mut videos: Vec<VideoInfo> = Vec::new();
    let mut scanned = 0usize;
    let mut last_id = offset_id;
    let mut finished = false;
    let mut next_date = date;

    let mut messages = client
        .iter_messages(rc.peer)
        .offset_id(offset_id)
        .max_date(day_end.map(|e| e as i32).unwrap_or(0));
    loop {
        let Some(msg) = messages.next().await.map_err(|e| e.to_string())? else {
            finished = true;
            next_date = None;
            break;
        };
        scanned += 1;
        // Newest → oldest: stop once we leave the requested day.
        if let Some(start) = day_start {
            if msg.date().timestamp() < start {
                // Do not consume the first message of the previous day.
                last_id = msg.id().saturating_add(1);
                next_date = None;
                break;
            }
        }
        last_id = msg.id();
        if let Some(info) = video_entry(
            st,
            &msg,
            identity.username.as_deref(),
            identity.raw_id,
            identity.title.as_deref().unwrap_or(""),
        ) {
            videos.push(info);
            if videos.len() >= PAGE_VIDEOS {
                // Page full — more content may exist below (unless the
                // requested day was scanned to its start, handled above).
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
        next_date,
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
    channel_title: &str,
) -> Option<VideoInfo> {
    let media = msg.media()?;
    let meta = media_meta(&media)?;
    let msg_id = msg.id();
    let name = sanitize_filename(
        &meta
            .name
            .unwrap_or_else(|| format!("tg_video_{msg_id}.mp4")),
    );

    // The file may sit at the downloads root (legacy) or inside the
    // organized <channel>/<date>/ subdirectories — check both.
    let day = msg
        .date()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();
    let chan = sanitize_filename(if channel_title.is_empty() {
        username.unwrap_or("未知频道")
    } else {
        channel_title
    });
    let candidates = [
        st.default_downloads_dir.join(&name),
        st.default_downloads_dir.join(&chan).join(&day).join(&name),
    ];
    let mut local: u64 = 0;
    for c in &candidates {
        if let Ok(m) = std::fs::metadata(c) {
            local = m.len();
            break;
        }
    }
    let downloaded = meta.size > 0 && local == meta.size;

    // A thumbnail counts as available if cached on disk or offered by the server.
    let has_thumb = thumb_path(st, raw_id, msg_id).exists() || best_thumb(&media).is_some();

    Some(VideoInfo {
        msg_id,
        kind: meta.kind.as_str().to_string(),
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
    st.default_downloads_dir
        .join("thumbs")
        .join(format!("{raw_id}_{msg_id}.jpg"))
}
