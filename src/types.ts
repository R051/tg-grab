// Types mirroring the Rust command payloads.

export interface Status {
  configured: boolean;
  connected: boolean;
  authorized: boolean;
  user: string | null;
  error: string | null;
}

export type SignInOutcome =
  | { status: "Ok"; user: string }
  | { status: "NeedPassword"; hint: string | null };

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
  size?: number;
  cached?: boolean;
  error?: string;
}

export interface QueueSnapshot {
  pending: { id: number; url: string }[];
  active: ActiveTask | null;
  results: TaskResult[];
}

/** Rust Config struct (snake_case on the wire). */
export interface AppSettings {
  api_id: number | null;
  api_hash: string | null;
  proxy_url: string | null;
  concurrency: number;
  watched: WatchEntry[];
  lang: string;
  downloads_dir: string | null;
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
  msgId: number;
  name: string;
  size: number;
  duration: number;
  date: string | null;
  downloaded: boolean;
  hasThumb: boolean;
  url: string;
}

export interface HistoryPage {
  chat: { rawId: number; title: string };
  videos: VideoInfo[];
  nextOffset: number;
  exhausted: boolean;
}

export interface DownloadedFile {
  name: string;
  size: number;
}
