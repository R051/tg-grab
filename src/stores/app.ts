import { defineStore } from "pinia";
import { invoke, on, onReconnect } from "../lib/backend";
import type {
  ActiveTask,
  AppSettings,
  DownloadedFile,
  QueueSnapshot,
  Status,
  TaskResult,
  WatchEntry,
} from "../types";

let initPromise: Promise<void> | null = null;
let queueSeq = 0;

export const useAppStore = defineStore("app", {
  state: () => ({
    status: null as Status | null,
    samples: [] as { ts: number; current: number }[],
    speed: 0,
    settings: null as AppSettings | null,
    pending: [] as { id: number; url: string }[],
    active: null as ActiveTask | null,
    results: [] as TaskResult[],
    files: [] as DownloadedFile[],
    watched: [] as WatchEntry[],
  }),
  getters: {
    authorized: (s) => s.status?.authorized ?? false,
    configured: (s) => s.status?.configured ?? false,
    user: (s) => s.status?.user ?? null,
  },
  actions: {
    async refreshStatus() {
      this.status = await invoke<Status>("get_status");
    },
    async refreshSettings() {
      this.settings = await invoke<AppSettings>("get_settings");
    },
    async refreshQueue() {
      // Single-flight with stale-response discard: parallel calls never
      // overwrite newer state with an older snapshot.
      const seq = ++queueSeq;
      const q = await invoke<QueueSnapshot>("get_queue");
      if (seq !== queueSeq) return;
      this.pending = q.pending;
      this.active = q.active;
      this.results = q.results;
    },
    async refreshFiles() {
      this.files = await invoke<DownloadedFile[]>("list_files");
    },
    async deleteFile(name: string) {
      await invoke("delete_file", { name });
      await this.refreshFiles();
    },
    async refreshWatched() {
      this.watched = await invoke<WatchEntry[]>("watch_list");
    },
    async init() {
      if (initPromise) return initPromise;
      initPromise = (async () => {
        await Promise.all([this.refreshStatus(), this.refreshSettings(), this.refreshWatched()]);
        await this.refreshQueue().catch(() => undefined);
        await this.refreshFiles().catch(() => undefined);
      })();
      try {
        await initPromise;
      } catch (e) {
        // Allow a retry on the next call (e.g. server came back).
        initPromise = null;
        throw e;
      }

      on("queue:progress", (p) => {
        const a = p as ActiveTask;
        const now = Date.now();
        // Sliding 3-second window: stable speed, no flicker to zero.
        const keep = this.samples.filter((s) => now - s.ts < 3000);
        keep.push({ ts: now, current: a.current });
        this.samples = keep;
        const first = keep[0];
        const span = (now - first.ts) / 1000;
        if (span >= 0.8) {
          this.speed = Math.max(0, (a.current - first.current) / span);
        }
        this.active = a;
      });
      on("queue:result", (p) => {
        const r = p as TaskResult;
        this.results.push(r);
        if (this.active?.id === r.id) {
          this.active = null;
          this.samples = [];
          this.speed = 0;
        }
        void this.refreshFiles();
      });
      on("queue:pending", () => {
        void this.refreshQueue();
      });
      on("queue:changed", () => {
        void this.refreshQueue();
      });
      on("watch:changed", () => {
        void this.refreshWatched();
        void this.refreshSettings();
      });
      on("auth:changed", () => {
        void this.refreshStatus();
      });
      // WebSocket reconnected: resync everything that may have changed.
      onReconnect(() => {
        void this.refreshQueue().catch(() => undefined);
        void this.refreshStatus().catch(() => undefined);
        void this.refreshFiles().catch(() => undefined);
      });
    },
  },
});
