//! Local collection metadata. Export DTOs deliberately exclude account configuration.
use crate::state::SharedState;
use crate::tg::links::{ChatLink, canonical_msg_link, parse_chat_link};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteDraft {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub local_root: Option<String>,
    #[serde(default)]
    pub source_title: String,
    #[serde(default)]
    pub channel_id: Option<i64>,
    #[serde(default)]
    pub msg_id: Option<i32>,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    pub id: String,
    #[serde(flatten)]
    pub item: FavoriteDraft,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub source_id: String,
    pub source_url: String,
    pub relative_path: String,
    pub root: PathBuf,
    pub size: u64,
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryData {
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub items: Vec<Favorite>,
    #[serde(default)]
    pub downloads: BTreeMap<String, DownloadRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryExport {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub folders: Vec<Folder>,
    pub items: Vec<Favorite>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub folders: Vec<Folder>,
    pub items: Vec<Favorite>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: usize,
    pub skipped: usize,
    pub folders_added: usize,
}

pub struct LibraryStore {
    path: PathBuf,
    pub data: LibraryData,
    load_error: Option<String>,
}
impl LibraryStore {
    pub fn load(path: PathBuf) -> Self {
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(data) => Self {
                    path,
                    data,
                    load_error: None,
                },
                Err(error) => Self {
                    path,
                    data: LibraryData::default(),
                    load_error: Some(error.to_string()),
                },
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self {
                path,
                data: LibraryData::default(),
                load_error: None,
            },
            Err(error) => Self {
                path,
                data: LibraryData::default(),
                load_error: Some(error.to_string()),
            },
        }
    }
    fn check(&self) -> Result<(), String> {
        match &self.load_error {
            Some(error) => Err(format!("LIBRARY_LOAD_FAILED: {error}")),
            None => Ok(()),
        }
    }
    pub fn commit(&mut self, data: LibraryData) -> Result<(), String> {
        use std::io::Write;
        self.check()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(temporary, &self.path).map_err(|e| e.to_string())?;
        self.data = data;
        Ok(())
    }
}

pub fn relative_path(input: &str) -> Result<String, String> {
    let input = input.replace('\\', "/");
    if input.is_empty() || input.len() > 2048 || input.contains(':') || input.contains('\0') {
        return Err("INVALID_NAME".into());
    }
    if Path::new(&input)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("INVALID_NAME".into());
    }
    Ok(input)
}

/// Stable source identity plus a collision suffix; never replace an unrelated file.
pub fn download_target(
    base: &Path,
    original: &str,
    source: &str,
    fallback_extension: &str,
) -> (String, PathBuf) {
    let original = crate::tg::links::sanitize_filename(original);
    let path = Path::new(&original);
    let raw_stem = path
        .file_stem()
        .and_then(|part| part.to_str())
        .unwrap_or("media");
    let mut units = 0;
    let stem: String = raw_stem
        .chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= 160
        })
        .collect();
    let extension = path
        .extension()
        .and_then(|part| part.to_str())
        .filter(|extension| extension.len() <= 12)
        .map(|extension| format!(".{extension}"))
        .unwrap_or_else(|| fallback_extension.to_string());
    let mut name = format!("{stem}_{source}{extension}");
    let mut suffix = 2;
    while base.join(&name).exists() {
        name = format!("{stem}_{source}_{suffix}{extension}");
        suffix += 1;
    }
    let target = base.join(&name);
    (name, target)
}

fn normalize(mut item: FavoriteDraft) -> Result<(String, FavoriteDraft), String> {
    if !["channel", "video", "photo", "audio", "document", "file"].contains(&item.kind.as_str()) {
        return Err("INVALID_LIBRARY_ITEM".into());
    }
    item.title = item.title.trim().to_string();
    if item.title.is_empty()
        || item.title.chars().count() > 500
        || item.notes.len() > 16000
        || item.source_title.len() > 2000
    {
        return Err("INVALID_LIBRARY_ITEM".into());
    }
    if let Some(path) = &item.local_path {
        item.local_path = Some(relative_path(path)?);
    }
    let id = if item.kind == "channel" {
        item.local_path = None;
        let chat = parse_chat_link(&item.url)?;
        match chat {
            ChatLink::ChannelRaw(id) => {
                item.channel_id = Some(id);
                item.url = format!("https://t.me/c/{id}");
            }
            ChatLink::Username(name) => {
                item.url = format!("https://t.me/{}", name.to_ascii_lowercase());
            }
        }
        item.channel_id
            .filter(|id| *id > 0)
            .map(|id| format!("channel:{id}"))
            .unwrap_or_else(|| format!("channel:{}", item.url))
    } else if !item.url.is_empty() {
        item.url = canonical_msg_link(&item.url)?;
        let (chat, message) = crate::tg::links::parse_msg_link(&item.url)?;
        if let ChatLink::ChannelRaw(id) = chat {
            item.channel_id = Some(id);
        }
        item.msg_id = Some(message);
        item.channel_id
            .filter(|id| *id > 0)
            .map(|id| format!("media:{id}:{message}"))
            .unwrap_or_else(|| format!("media:{}", item.url))
    } else {
        let path = item.local_path.as_ref().ok_or("INVALID_LIBRARY_ITEM")?;
        format!(
            "file:{}:{path}",
            item.local_root.as_deref().unwrap_or_default()
        )
    };
    Ok((id, item))
}

fn same_favorite(existing: &Favorite, id: &str, item: &FavoriteDraft) -> bool {
    existing.id == id
        || (!item.url.is_empty() && existing.item.url == item.url)
        || (item.kind == "channel"
            && existing.item.kind == "channel"
            && item.channel_id.is_some()
            && existing.item.channel_id == item.channel_id)
        || (item.msg_id.is_some()
            && item.channel_id.is_some()
            && existing.item.msg_id == item.msg_id
            && existing.item.channel_id == item.channel_id)
        || (item.local_path.is_some()
            && existing.item.local_path == item.local_path
            && existing.item.local_root == item.local_root)
}

fn changed(st: &SharedState) {
    st.bus
        .emit("library:changed", serde_json::Value::Bool(true));
}

pub async fn get(st: &SharedState) -> Result<LibraryView, String> {
    let store = st.library.lock().await;
    store.check()?;
    Ok(LibraryView {
        folders: store.data.folders.clone(),
        items: store.data.items.clone(),
    })
}

pub async fn save_favorite(st: &SharedState, mut item: FavoriteDraft) -> Result<Favorite, String> {
    if item.local_path.is_some() && item.local_root.is_none() {
        item.local_root = Some(st.downloads_root().await.to_string_lossy().to_string());
    }
    let (mut id, mut item) = normalize(item)?;
    let mut store = st.library.lock().await;
    let mut data = store.data.clone();
    if let Some(existing) = data
        .items
        .iter()
        .find(|existing| same_favorite(existing, &id, &item))
    {
        id = existing.id.clone();
        item.channel_id = item.channel_id.or(existing.item.channel_id);
    }
    if item
        .folder_id
        .as_ref()
        .is_some_and(|id| !data.folders.iter().any(|folder| &folder.id == id))
    {
        return Err("NOT_FOUND".into());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let favorite = Favorite {
        created_at: data
            .items
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        id: id.clone(),
        item,
        updated_at: now,
    };
    if let Some(existing) = data.items.iter_mut().find(|entry| entry.id == id) {
        *existing = favorite.clone();
    } else {
        if data.items.len() >= 10000 {
            return Err("LIBRARY_TOO_LARGE".into());
        }
        data.items.insert(0, favorite.clone());
    }
    store.commit(data)?;
    drop(store);
    changed(st);
    Ok(favorite)
}

pub async fn remove_favorite(st: &SharedState, id: String) -> Result<(), String> {
    let mut store = st.library.lock().await;
    let mut data = store.data.clone();
    data.items.retain(|item| item.id != id);
    store.commit(data)?;
    drop(store);
    changed(st);
    Ok(())
}

pub async fn save_folder(
    st: &SharedState,
    id: Option<String>,
    name: String,
) -> Result<Folder, String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("INVALID_FOLDER_NAME".into());
    }
    let mut store = st.library.lock().await;
    let mut data = store.data.clone();
    if data
        .folders
        .iter()
        .any(|folder| folder.name.eq_ignore_ascii_case(&name) && Some(&folder.id) != id.as_ref())
    {
        return Err("FOLDER_EXISTS".into());
    }
    let folder = if let Some(id) = id {
        let folder = data
            .folders
            .iter_mut()
            .find(|folder| folder.id == id)
            .ok_or("NOT_FOUND")?;
        folder.name = name;
        folder.clone()
    } else {
        if data.folders.len() >= 200 {
            return Err("LIBRARY_TOO_LARGE".into());
        }
        let folder = Folder {
            id: format!(
                "folder_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ),
            name,
        };
        data.folders.push(folder.clone());
        folder
    };
    store.commit(data)?;
    drop(store);
    changed(st);
    Ok(folder)
}

pub async fn remove_folder(st: &SharedState, id: String) -> Result<(), String> {
    let mut store = st.library.lock().await;
    let mut data = store.data.clone();
    data.folders.retain(|folder| folder.id != id);
    for item in &mut data.items {
        if item.item.folder_id.as_ref() == Some(&id) {
            item.item.folder_id = None;
        }
    }
    store.commit(data)?;
    drop(store);
    changed(st);
    Ok(())
}

pub async fn export(st: &SharedState) -> Result<LibraryExport, String> {
    let view = get(st).await?;
    Ok(LibraryExport {
        format: "tg-grab-library".into(),
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        folders: view.folders,
        items: view.items,
    })
}

pub async fn import(st: &SharedState, input: LibraryExport) -> Result<ImportSummary, String> {
    if input.format != "tg-grab-library"
        || input.version != 1
        || input.items.len() > 10000
        || input.folders.len() > 200
    {
        return Err("INVALID_LIBRARY_BACKUP".into());
    }
    let mut store = st.library.lock().await;
    let mut data = store.data.clone();
    let mut summary = ImportSummary {
        added: 0,
        skipped: 0,
        folders_added: 0,
    };
    let mut folder_ids = BTreeMap::new();
    for folder in input.folders {
        let name = folder.name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err("INVALID_FOLDER_NAME".into());
        }
        let id = if let Some(existing) = data
            .folders
            .iter()
            .find(|existing| existing.name.eq_ignore_ascii_case(name))
        {
            existing.id.clone()
        } else {
            let id = format!(
                "folder_{}_{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                data.folders.len()
            );
            data.folders.push(Folder {
                id: id.clone(),
                name: name.into(),
            });
            summary.folders_added += 1;
            id
        };
        folder_ids.insert(folder.id, id);
    }
    for item in input.items {
        let (id, mut draft) = normalize(item.item)?;
        draft.folder_id = draft.folder_id.and_then(|id| folder_ids.get(&id).cloned());
        if data
            .items
            .iter()
            .any(|existing| same_favorite(existing, &id, &draft))
        {
            summary.skipped += 1;
            continue;
        }
        let now = chrono::Utc::now().to_rfc3339();
        data.items.push(Favorite {
            id,
            item: draft,
            created_at: now.clone(),
            updated_at: now,
        });
        summary.added += 1;
    }
    if data.items.len() > 10000 || data.folders.len() > 200 {
        return Err("LIBRARY_TOO_LARGE".into());
    }
    store.commit(data)?;
    drop(store);
    changed(st);
    Ok(summary)
}

pub async fn record_download(st: &SharedState, record: DownloadRecord) -> Result<(), String> {
    let mut store = st.library.lock().await;
    let mut data = store.data.clone();
    for favorite in &mut data.items {
        if favorite.item.url == record.source_url
            || (favorite
                .item
                .channel_id
                .zip(favorite.item.msg_id)
                .is_some_and(|(channel, message)| {
                    format!("{channel}_{message}") == record.source_id
                }))
        {
            favorite.item.local_path = Some(record.relative_path.clone());
            favorite.item.local_root = Some(record.root.to_string_lossy().to_string());
        }
    }
    data.downloads.insert(record.source_id.clone(), record);
    store.commit(data)?;
    drop(store);
    changed(st);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    async fn fixture() -> (PathBuf, SharedState) {
        let root = std::env::temp_dir().join(format!(
            "tg-grab-library-test-{}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let state = crate::init_core(root.clone()).await;
        (root, state)
    }
    fn item() -> FavoriteDraft {
        FavoriteDraft {
            kind: "video".into(),
            title: "Saved clip".into(),
            url: "https://t.me/example/42".into(),
            channel_id: Some(123),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn favorites_persist_deduplicate_and_survive_folder_removal() {
        let (root, state) = fixture().await;
        let folder = save_folder(&state, None, "Research".into()).await.unwrap();
        let mut draft = item();
        draft.folder_id = Some(folder.id.clone());
        draft.notes = "Keep this".into();
        let saved = save_favorite(&state, draft.clone()).await.unwrap();
        draft.title = "Updated title".into();
        draft.channel_id = None;
        assert_eq!(save_favorite(&state, draft).await.unwrap().id, saved.id);
        assert_eq!(get(&state).await.unwrap().items.len(), 1);
        remove_folder(&state, folder.id).await.unwrap();
        let stored = LibraryStore::load(root.join("library.json"));
        assert!(stored.data.items[0].item.folder_id.is_none());
        assert_eq!(stored.data.items[0].item.title, "Updated title");
        assert_eq!(stored.data.items[0].item.notes, "Keep this");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn backups_merge_without_overwriting_and_invalid_imports_are_atomic() {
        let (root, state) = fixture().await;
        save_favorite(&state, item()).await.unwrap();
        let backup = export(&state).await.unwrap();
        let mut repeated = backup.clone();
        repeated.items[0].item.notes = "Overwrite existing".into();
        let summary = import(&state, repeated).await.unwrap();
        assert_eq!(summary.skipped, 1);
        assert_eq!(get(&state).await.unwrap().items[0].item.notes, "");
        let before = std::fs::read(root.join("library.json")).unwrap();
        let mut invalid = backup.clone();
        invalid.folders.push(Folder {
            id: "bad".into(),
            name: "Partial import".into(),
        });
        invalid.items[0].item.local_path = Some("../secret".into());
        assert!(import(&state, invalid).await.is_err());
        assert_eq!(std::fs::read(root.join("library.json")).unwrap(), before);
        let json = serde_json::to_value(backup).unwrap();
        for field in ["downloads", "api_hash", "server_token", "session"] {
            assert!(json.get(field).is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn removing_favorites_never_deletes_files_and_missing_files_can_be_restored() {
        let (root, state) = fixture().await;
        let media = state.downloads_root().await.join("clip.mp4");
        std::fs::write(&media, b"data").unwrap();
        let mut draft = item();
        draft.local_path = Some("clip.mp4".into());
        let saved = save_favorite(&state, draft).await.unwrap();
        remove_favorite(&state, saved.id).await.unwrap();
        assert!(media.exists());
        record_download(
            &state,
            DownloadRecord {
                source_id: "123_42".into(),
                source_url: item().url,
                relative_path: "clip.mp4".into(),
                root: state.downloads_root().await,
                size: 4,
                kind: "video".into(),
            },
        )
        .await
        .unwrap();
        let files = crate::api::list_files(&state).await.unwrap();
        assert_eq!(
            files[0].source_url.as_deref(),
            Some("https://t.me/example/42")
        );
        std::fs::remove_file(&media).unwrap();
        assert!(crate::api::list_files(&state).await.unwrap().is_empty());
        assert!(
            state
                .library
                .lock()
                .await
                .data
                .downloads
                .contains_key("123_42")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn local_favorites_keep_their_root_and_alias_backups_do_not_duplicate() {
        let (root, state) = fixture().await;
        let mut first = FavoriteDraft {
            kind: "file".into(),
            title: "First folder".into(),
            local_path: Some("same.txt".into()),
            local_root: Some(root.join("first").to_string_lossy().to_string()),
            ..Default::default()
        };
        let saved = save_favorite(&state, first.clone()).await.unwrap();
        let mut second = first.clone();
        second.local_root = Some(root.join("second").to_string_lossy().to_string());
        assert_ne!(save_favorite(&state, second).await.unwrap().id, saved.id);
        first.notes = "Edited after changing download location".into();
        let updated = save_favorite(&state, first.clone()).await.unwrap();
        assert_eq!(updated.item.local_root, first.local_root);
        let mut channel = item();
        channel.channel_id = None;
        save_favorite(&state, channel.clone()).await.unwrap();
        channel.channel_id = Some(123);
        save_favorite(&state, channel).await.unwrap();
        let backup = export(&state).await.unwrap();
        let count = backup.items.len();
        let imported = import(&state, backup).await.unwrap();
        assert_eq!(imported.added, 0);
        assert_eq!(imported.skipped, count);
        assert_eq!(get(&state).await.unwrap().items.len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filename_collisions_never_remove_or_replace_existing_files() {
        let root = std::env::temp_dir().join(format!(
            "tg-grab-path-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (_, first) = download_target(&root, "same.mp4", "111_42", ".mp4");
        std::fs::write(&first, b"original").unwrap();
        let (_, repeated) = download_target(&root, "same.mp4", "111_42", ".mp4");
        let (_, other) = download_target(&root, "same.mp4", "222_42", ".mp4");
        assert_ne!(first, repeated);
        assert_ne!(first, other);
        assert_eq!(std::fs::read(first).unwrap(), b"original");
        std::fs::remove_dir_all(root).unwrap();
        for path in ["../a", "C:/Windows/a", "/etc/passwd", "..\\a"] {
            assert!(relative_path(path).is_err());
        }
    }

    #[test]
    fn corrupted_library_is_not_silently_overwritten() {
        let path = std::env::temp_dir().join(format!(
            "tg-grab-corrupt-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, b"broken-json").unwrap();
        let mut store = LibraryStore::load(path.clone());
        assert!(store.commit(LibraryData::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken-json");
        std::fs::remove_file(path).unwrap();
    }
}
