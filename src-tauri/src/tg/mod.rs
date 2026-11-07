//! Telegram integration: client, auth, links, download queue, watcher, history.

pub mod auth;
pub mod client;
pub mod directory;
pub mod history;
pub mod links;
pub mod queue;
pub mod watcher;

use grammers_client::media::Media;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Photo,
    Audio,
    Document,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Video => "video",
            MediaKind::Photo => "photo",
            MediaKind::Audio => "audio",
            MediaKind::Document => "document",
        }
    }

    pub fn folder(&self) -> &'static str {
        match self {
            MediaKind::Video => "video",
            MediaKind::Photo => "photo",
            MediaKind::Audio => "audio",
            MediaKind::Document => "docs",
        }
    }

    pub fn parse(s: &str) -> Option<MediaKind> {
        match s {
            "video" => Some(MediaKind::Video),
            "photo" => Some(MediaKind::Photo),
            "audio" => Some(MediaKind::Audio),
            "document" => Some(MediaKind::Document),
            _ => None,
        }
    }
}

/// Classified media metadata shared by the queue, watcher and history.
pub struct MediaMeta {
    pub kind: MediaKind,
    pub name: Option<String>,
    pub size: u64,
    pub duration_secs: Option<f64>,
    pub ext: String,
}

/// Classify a message media attachment (None = not a downloadable kind).
pub fn media_meta(media: &Media) -> Option<MediaMeta> {
    match media {
        Media::Document(doc) => {
            let mime = doc.mime_type().unwrap_or("").to_lowercase();
            let (kind, default_ext) = if mime.starts_with("video/") {
                (MediaKind::Video, ".mp4")
            } else if mime.starts_with("audio/") {
                (MediaKind::Audio, ".mp3")
            } else if mime.starts_with("image/") {
                (MediaKind::Photo, ".jpg")
            } else {
                (MediaKind::Document, ".bin")
            };
            Some(MediaMeta {
                kind,
                name: doc.name().map(str::to_owned),
                size: doc.size().unwrap_or(0) as u64,
                duration_secs: doc.duration(),
                ext: default_ext.to_string(),
            })
        }
        Media::Photo(photo) => Some(MediaMeta {
            kind: MediaKind::Photo,
            name: None,
            size: photo.size().unwrap_or(0) as u64,
            duration_secs: None,
            ext: ".jpg".to_string(),
        }),
        _ => None,
    }
}

/// Default filename when the media carries none.
pub fn default_name(kind: &MediaKind, msg_id: i32) -> String {
    let ext = match kind {
        MediaKind::Video => ".mp4",
        MediaKind::Photo => ".jpg",
        MediaKind::Audio => ".mp3",
        MediaKind::Document => ".bin",
    };
    let prefix = match kind {
        MediaKind::Video => "tg_video",
        MediaKind::Photo => "tg_photo",
        MediaKind::Audio => "tg_audio",
        MediaKind::Document => "tg_file",
    };
    format!("{prefix}_{msg_id}{ext}")
}
