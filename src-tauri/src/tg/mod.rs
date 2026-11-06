//! Telegram integration: client, auth, links, download queue, watcher, history.

pub mod auth;
pub mod client;
pub mod history;
pub mod links;
pub mod queue;
pub mod watcher;

use grammers_client::media::Media;

/// Video detection + metadata shared by the queue and history browser.
pub struct VideoMeta {
    pub name: Option<String>,
    pub size: u64,
    pub duration_secs: Option<f64>,
}

/// Return video metadata when the media is a video document (mime `video/*`).
pub fn video_meta(media: &Media) -> Option<VideoMeta> {
    match media {
        Media::Document(doc) => {
            let mime = doc.mime_type().unwrap_or("");
            if !mime.starts_with("video/") {
                return None;
            }
            Some(VideoMeta {
                name: doc.name().map(str::to_owned),
                size: doc.size().unwrap_or(0) as u64,
                duration_secs: doc.duration(),
            })
        }
        _ => None,
    }
}
