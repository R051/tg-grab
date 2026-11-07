const downloadCodes = new Set(["INVALID_CODE", "INVALID_PASSWORD", "INVALID_LINK", "INVITE_LINK", "NOT_LOGGED_IN", "MESSAGE_NOT_FOUND", "NOT_A_VIDEO", "RATE_LIMITED", "CANCELLED", "CHANNEL_UNAVAILABLE", "API_NOT_CONFIGURED", "INVALID_API_ID", "NO_PENDING_CODE", "NO_PENDING_PASSWORD"]);
const watchCodes = new Set(["ALREADY_WATCHED", "CHAT_NOT_FOUND", "NOT_A_CHANNEL"]);
const commonCodes = new Set(["REQUEST_TIMEOUT", "UNAUTHORIZED", "INVALID_RESPONSE"]);
export function errorText(error: unknown, t: (key: string) => string): string {
  const raw = error instanceof Error ? error.message : String(error);
  const code = raw.split(":", 1)[0].trim();
  const prefix = downloadCodes.has(code) ? "download.errors" : watchCodes.has(code) ? "watch.errors" : commonCodes.has(code) ? "common.errors" : null;
  return prefix ? t(`${prefix}.${code}`) : raw;
}
