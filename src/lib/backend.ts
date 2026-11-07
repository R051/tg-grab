// Unified backend transport: Tauri IPC on desktop, HTTP+WebSocket on web.

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const isDesktop = isTauri;

// ── token (web mode only) ──
export function getToken(): string {
  return localStorage.getItem("tg-grab-token") ?? "";
}
export function setToken(t: string) {
  localStorage.setItem("tg-grab-token", t);
}

let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(fn: () => void) {
  onUnauthorized = fn;
}

// ── invoke ──
async function webInvoke(name: string, args?: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`/api/invoke/${name}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${getToken()}`,
    },
    body: JSON.stringify(args ?? {}),
  });
  if (res.status === 401) {
    onUnauthorized?.();
    throw new Error("UNAUTHORIZED");
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw (data as { error?: string })?.error ?? res.statusText;
  return data;
}

export async function invoke<T = unknown>(
  name: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke(name, args);
  }
  return webInvoke(name, args) as Promise<T>;
}

// ── events ──
type Handler = (payload: unknown) => void;
const webSubs: { event: string; handler: Handler }[] = [];
let ws: WebSocket | null = null;


function ensureWs() {
  if (isTauri || ws) return;
  connect();
  function connect() {
    ws = new WebSocket(`ws://${location.host}/ws?token=${encodeURIComponent(getToken())}`);
    ws.onmessage = (m) => {
      try {
        const d = JSON.parse(m.data as string);
        for (const s of webSubs) {
          if (s.event === d.event) s.handler(d.payload);
        }
      } catch {
        /* ignore malformed frames */
      }
    };
    ws.onclose = () => {
      ws = null;
      window.setTimeout(() => {
        connect();
        onReconnectFn?.();
      }, 3000);
    };
    ws.onerror = () => ws?.close();
  }
}

let onReconnectFn: (() => void) | null = null;
export function onReconnect(fn: () => void) {
  onReconnectFn = fn;
}

export function on(event: string, handler: Handler): void {
  if (isTauri) {
    void import("@tauri-apps/api/event").then(({ listen }) => {
      void listen(event, (e) => handler(e.payload));
    });
    return;
  }
  webSubs.push({ event, handler });
  ensureWs();
}

// ── files ──
/** Web: direct URL to a downloaded file. Desktop: null (use reveal_file). */
export function fileUrl(name: string): string | null {
  if (isTauri) return null;
  return `/files/${encodeURIComponent(name)}?token=${encodeURIComponent(getToken())}`;
}
