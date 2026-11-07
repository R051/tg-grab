//! Channel history browsing: paginated video listing for selective downloads.

use crate::tg::client::resolve_chat;
use crate::tg::links::ChatLink;
use crate::tg::links::{make_msg_link, parse_chat_link, sanitize_filename};
use crate::tg::media_meta;
use chrono::Local;
use grammers_client::Client;
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
    pub local_path: Option<String>,
    /// A server thumbnail is available for this video.
    pub has_thumb: bool,
    /// Canonical t.me link (queue display + download pipeline input).
    pub url: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInfo {
    pub raw_id: i64,
    pub title: String,
    pub username: Option<String>,
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

pub fn validate_search(
    query: &str,
    kind: &str,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<(), String> {
    if query.chars().count() > 256 {
        return Err("INVALID_SEARCH".into());
    }
    if kind != "all" && crate::tg::MediaKind::parse(kind).is_none() {
        return Err("INVALID_MEDIA_KINDS".into());
    }
    if start.is_some_and(|v| v < 0 || v > i32::MAX as i64)
        || end.is_some_and(|v| v < 0 || v > i32::MAX as i64)
        || start.zip(end).is_some_and(|(a, b)| b <= a)
    {
        return Err("INVALID_DATE".into());
    }
    Ok(())
}

pub async fn search(
    st: &crate::state::Inner,
    client: &Client,
    link: &str,
    query: &str,
    kind: &str,
    offset: i32,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<HistoryPage, String> {
    validate_search(query, kind, start, end)?;
    let rc = resolve_chat(st, client, &parse_chat_link(link)?).await?;
    use grammers_client::tl::enums::MessagesFilter;
    let filter = match kind {
        "video" => MessagesFilter::InputMessagesFilterVideo,
        "photo" => MessagesFilter::InputMessagesFilterPhotos,
        "audio" => MessagesFilter::InputMessagesFilterEmpty,
        "document" => MessagesFilter::InputMessagesFilterDocument,
        _ => MessagesFilter::InputMessagesFilterEmpty,
    };
    let mut searched = client
        .search_messages(rc.peer)
        .query(query.trim())
        .filter(filter)
        .offset_id(offset);
    if let Some(value) = start {
        let date = chrono::DateTime::from_timestamp(value.saturating_sub(1), 0)
            .ok_or("INVALID_DATE")?
            .fixed_offset();
        searched = searched.min_date(&date);
    }
    if let Some(value) = end {
        let date = chrono::DateTime::from_timestamp(value, 0)
            .ok_or("INVALID_DATE")?
            .fixed_offset();
        searched = searched.max_date(&date);
    }
    let mut history = client
        .iter_messages(rc.peer)
        .offset_id(offset)
        .max_date(end.unwrap_or(0) as i32);
    let use_search = !query.trim().is_empty() || matches!(kind, "video" | "photo" | "document");
    let root = st.downloads_root().await;
    let records = st.library.lock().await.data.downloads.clone();
    let mut videos = Vec::new();
    let mut last = offset;
    let mut exhausted = false;
    for _ in 0..MAX_SCAN {
        let next = if use_search {
            searched.next().await
        } else {
            history.next().await
        }
        .map_err(|e| e.to_string())?;
        let Some(msg) = next else {
            exhausted = true;
            break;
        };
        last = msg.id();
        if start.is_some_and(|bound| msg.date().timestamp() < bound) {
            exhausted = true;
            break;
        }
        if end.is_some_and(|bound| msg.date().timestamp() >= bound) {
            continue;
        }
        if let Some(info) = video_entry(
            st,
            &msg,
            rc.identity.username.as_deref(),
            rc.identity.raw_id,
            "",
            &root,
            &records,
        ) {
            if kind == "all" || info.kind == kind {
                videos.push(info);
            }
            if videos.len() >= 60 {
                break;
            }
        }
    }
    Ok(HistoryPage {
        chat: ChatInfo {
            raw_id: rc.identity.raw_id,
            title: rc.identity.title.unwrap_or_default(),
            username: rc.identity.username,
        },
        videos,
        next_offset: last,
        next_date: None,
        exhausted,
    })
}

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

    let root = st.downloads_root().await;
    let records = st.library.lock().await.data.downloads.clone();
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
            &root,
            &records,
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
            username: identity.username.clone(),
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
    _channel_title: &str,
    root: &std::path::Path,
    records: &std::collections::BTreeMap<String, crate::library::DownloadRecord>,
) -> Option<VideoInfo> {
    let media = msg.media()?;
    let meta = media_meta(&media)?;
    let msg_id = msg.id();
    let name = sanitize_filename(
        &meta
            .name
            .unwrap_or_else(|| crate::tg::default_name(&meta.kind, msg_id)),
    );

    let key = format!("{raw_id}_{msg_id}");
    let local_path = records
        .get(&key)
        .filter(|record| record.root == root && record.size == meta.size)
        .filter(|record| {
            std::fs::metadata(root.join(&record.relative_path))
                .map(|metadata| metadata.is_file() && metadata.len() == meta.size)
                .unwrap_or(false)
        })
        .map(|record| record.relative_path.clone());
    let downloaded = local_path.is_some();

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
        local_path,
        has_thumb,
        url: make_msg_link(username, raw_id, msg_id),
        text: msg.text().chars().take(320).collect(),
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
