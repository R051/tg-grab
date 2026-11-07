const downloadCodes = new Set(["INVALID_CODE", "INVALID_PASSWORD", "INVALID_LINK", "INVITE_LINK", "NOT_LOGGED_IN", "MESSAGE_NOT_FOUND", "NOT_A_VIDEO", "UNSUPPORTED_MEDIA", "RATE_LIMITED", "CANCELLED", "CHANNEL_UNAVAILABLE", "API_NOT_CONFIGURED", "INVALID_API_ID", "NO_PENDING_CODE", "NO_PENDING_PASSWORD"]);
const watchCodes = new Set(["ALREADY_WATCHED", "CHAT_NOT_FOUND", "NOT_A_CHANNEL"]);
const commonCodes = new Set(["INVALID_CLOSE_ACTION","INVALID_DATE","INVALID_SEARCH","INVALID_LIBRARY_ITEM","INVALID_LIBRARY_BACKUP","INVALID_FOLDER_NAME","FOLDER_EXISTS","LIBRARY_TOO_LARGE","LIBRARY_LOAD_FAILED","INVALID_DOWNLOAD_DIR","DOWNLOADS_ACTIVE","INVALID_MEDIA_KINDS","LIBRARY_WRITE_FAILED","REQUEST_TIMEOUT", "UNAUTHORIZED", "INVALID_RESPONSE"]);
export function errorText(error: unknown, t: (key: string) => string): string {
  const raw = error instanceof Error ? error.message : String(error);
  const code = raw.split(":", 1)[0].trim();
  const prefix = downloadCodes.has(code) ? "download.errors" : watchCodes.has(code) ? "watch.errors" : commonCodes.has(code) ? "common.errors" : null;
  return prefix ? t(`${prefix}.${code}`) : raw;
}
