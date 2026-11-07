// One command/event contract over Tauri IPC and HTTP/WebSocket.
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const isDesktop = isTauri;
export type ConnectionState = "connecting" | "connected" | "disconnected";
export function getToken(): string { return localStorage.getItem("tg-grab-token") ?? ""; }
export function setToken(token: string) { localStorage.setItem("tg-grab-token", token); }
let unauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(fn: () => void) { unauthorized = fn; }

async function webInvoke(name: string, args?: Record<string, unknown>): Promise<unknown> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), 30000);
  try {
    const response = await fetch(`/api/invoke/${name}`, {
      method: "POST", signal: controller.signal,
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify(args ?? {}),
    });
    if (response.status === 401) { unauthorized?.(); throw new Error("UNAUTHORIZED"); }
    const data: unknown = await response.json();
    if (!response.ok) throw new Error((data as { error?: string })?.error ?? response.statusText);
    return data;
  } catch (error) {
    if (controller.signal.aborted) throw new Error("REQUEST_TIMEOUT");
    throw error;
  } finally { window.clearTimeout(timeout); }
}
export async function invoke<T = unknown>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const api = await import("@tauri-apps/api/core");
    return api.invoke<T>(name, args);
  }
  return webInvoke(name, args) as Promise<T>;
}

type Handler = (payload: unknown) => void;
const subscriptions = new Set<{ event: string; handler: Handler }>();
const reconnectHandlers = new Set<() => void>();
const connectionHandlers = new Set<(state: ConnectionState) => void>();
let socket: WebSocket | null = null;
let retryTimer: number | undefined;
let retryCount = 0;
let connectionState: ConnectionState = isTauri ? "connected" : "disconnected";
function notifyConnection(state: ConnectionState) {
  connectionState = state;
  for (const handler of connectionHandlers) handler(state);
}
function ensureWs() {
  if (isTauri || socket || retryTimer !== undefined || !subscriptions.size) return;
  notifyConnection("connecting");
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  const current = new WebSocket(`${scheme}//${location.host}/ws?token=${encodeURIComponent(getToken())}`);
  socket = current;
  current.onopen = () => {
    if (socket !== current) return;
    retryCount = 0;
    notifyConnection("connected");
    // Also resync the first connection: events may have changed during startup.
    for (const handler of reconnectHandlers) handler();
  };
  current.onmessage = (message) => {
    let frame: { event: string; payload: unknown };
    try { frame = JSON.parse(String(message.data)); } catch { return; }
    for (const item of subscriptions) {
      if (item.event !== frame.event) continue;
      try { item.handler(frame.payload); } catch (error) { console.error(error); }
    }
  };
  current.onclose = () => {
    if (socket !== current) return;
    socket = null;
    notifyConnection("disconnected");
    if (!subscriptions.size) return;
    const delay = Math.min(30000, 1000 * 2 ** retryCount++);
    retryTimer = window.setTimeout(() => { retryTimer = undefined; ensureWs(); }, delay);
  };
  current.onerror = () => current.close();
}
export function onReconnect(handler: () => void): () => void {
  reconnectHandlers.add(handler);
  return () => { reconnectHandlers.delete(handler); };
}
export function onConnection(handler: (state: ConnectionState) => void): () => void {
  connectionHandlers.add(handler);
  handler(connectionState);
  return () => { connectionHandlers.delete(handler); };
}
export async function on(event: string, handler: Handler): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen(event, (message) => handler(message.payload));
  }
  const item = { event, handler };
  subscriptions.add(item);
  ensureWs();
  return () => {
    subscriptions.delete(item);
    if (subscriptions.size) return;
    window.clearTimeout(retryTimer);
    retryTimer = undefined;
    const previous = socket;
    socket = null;
    previous?.close();
  };
}
export function fileUrl(name: string): string | null {
  if (isTauri) return null;
  const path = name.split("\\").join("/").split("/").map(encodeURIComponent).join("/");
  return `/files/${path}?token=${encodeURIComponent(getToken())}`;
}
