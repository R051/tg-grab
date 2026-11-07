import { invoke, isDesktop, fileUrl } from "./backend";
import type {
  AppSettings,
  DownloadedFile,
  Favorite,
  FavoriteDraft,
} from "../types";

export function findFavorite(
  items: Favorite[],
  item: FavoriteDraft,
): Favorite | undefined {
  return items.find(
    (saved) =>
      (item.url && saved.url === item.url) ||
      (item.kind === "channel" &&
        item.channelId &&
        saved.kind === "channel" &&
        saved.channelId === item.channelId) ||
      (item.localPath &&
        saved.localPath === item.localPath &&
        (!item.localRoot || saved.localRoot === item.localRoot)),
  );
}
export function localPathFor(
  item: FavoriteDraft,
  files: DownloadedFile[],
  settings: AppSettings | null,
): string | null {
  const source = item.url && files.find((file) => file.sourceUrl === item.url);
  if (source) return source.name;
  if (!item.localPath) return null;
  const normalize = (path: string) => {
    const value = path.split("\\").join("/").replace(/\/$/, "");
    return /^[a-z]:/i.test(value) ? value.toLowerCase() : value;
  };
  if (
    item.localRoot &&
    settings &&
    normalize(item.localRoot) !== normalize(settings.effective_downloads_dir)
  )
    return null;
  return files.some((file) => file.name === item.localPath)
    ? item.localPath
    : null;
}
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
  return `${(bytes / 1073741824).toFixed(2)} GB`;
}
export function mediaIcon(kind: string): string {
  return (
    (
      {
        channel: "channel",
        video: "video",
        photo: "image",
        audio: "audio",
      } as Record<string, string>
    )[kind] ?? "file"
  );
}
export async function previewUrl(name: string): Promise<string> {
  if (!isDesktop) return fileUrl(name)!;
  const path = await invoke<string>("media_preview_path", { name });
  const { convertFileSrc } = await import("@tauri-apps/api/core");
  return convertFileSrc(path);
}
export async function openSource(url: string): Promise<void> {
  if (!/^https:\/\/t\.me\//i.test(url)) return;
  if (isDesktop) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else window.open(url, "_blank", "noopener,noreferrer");
}
export async function exportCollections(): Promise<boolean> {
  if (isDesktop) return invoke<boolean>("library_export_file");
  const data = await invoke("library_export");
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }),
  );
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "tg-grab-collections.json";
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  return true;
}
