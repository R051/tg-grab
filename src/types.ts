// Types mirroring the Rust command payloads.

export interface Status {
  configured: boolean;
  connected: boolean;
  authorized: boolean;
  user: string | null;
  error: string | null;
}

export type SignInOutcome =
  | { status: "ok"; user: string }
  | { status: "need_password"; hint: string | null };

export interface ActiveTask {
  id: number;
  url: string;
  name: string;
  current: number;
  total: number;
  percent: number;
}

export interface TaskResult {
  id: number;
  url: string;
  status: "success" | "failed";
  name?: string;
  path?: string;
  size?: number;
  cached?: boolean;
  error?: string;
}

export interface QueueSnapshot {
  pending: { id: number; url: string }[];
  active: ActiveTask | null;
  activeTasks: ActiveTask[];
  results: TaskResult[];
}

/** Rust Config struct (snake_case on the wire). */
export interface AppSettings {
  api_id: number | null;
  api_hash: string | null;
  api_hash_set?: boolean;
  proxy_url: string | null;
  concurrency: number;
  watched: WatchEntry[];
  lang: string;
  downloads_dir: string | null;
  effective_downloads_dir: string;
  default_downloads_dir?: string;
  media_kinds: MediaKind[];
  classify_by_type: boolean;
  organize_by_channel: boolean;
  close_to_tray: boolean;
  close_action?: "ask" | "tray" | "exit";
}

export interface WatchEntry {
  chat_id: number;
  raw_id: number;
  access_hash: number;
  title: string;
  username: string | null;
  enabled: boolean;
  last_msg_id: number;
}

export interface VideoInfo {
  text?: string;
  kind: MediaKind;
  msgId: number;
  name: string;
  size: number;
  duration: number;
  date: string | null;
  downloaded: boolean;
  localPath?: string | null;
  hasThumb: boolean;
  url: string;
}
export interface ChannelDirectory { channels:{rawId:number;title:string;username:string|null;url:string;folderIds:number[]}[];folders:{id:number;name:string}[];foldersError:string|null;truncated:boolean }

export interface HistoryPage {
  chat: { rawId: number; title: string; username?: string | null };
  videos: VideoInfo[];
  nextOffset: number;
  nextDate: string | null;
  exhausted: boolean;
}

export interface DownloadedFile {
  name: string;
  size: number;
  kind: MediaKind;
  sourceUrl?: string | null;
}

export type MediaKind = "video" | "photo" | "audio" | "document";
export type FavoriteKind = MediaKind | "channel" | "file";
export interface FavoriteDraft {
  kind: FavoriteKind; title: string; url: string; localPath?: string | null; localRoot?: string | null;
  sourceTitle?: string; channelId?: number | null; msgId?: number | null;
  folderId?: string | null; notes?: string;
}
export interface Favorite extends FavoriteDraft { id: string; createdAt: string; updatedAt: string }
export interface CollectionFolder { id: string; name: string }
export interface LibraryData { folders: CollectionFolder[]; items: Favorite[] }
export interface LibraryBackup extends LibraryData { format: "tg-grab-library"; version: 1; exportedAt: string }
export interface MediaDetails extends FavoriteDraft { size?: number; date?: string | null; thumbnail?: string }
