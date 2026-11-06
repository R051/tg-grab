import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ActiveTask,
  AppSettings,
  DownloadedFile,
  QueueSnapshot,
  Status,
  TaskResult,
  WatchEntry,
} from "../types";

let initialized = false;

export const useAppStore = defineStore("app", {
  state: () => ({
    status: null as Status | null,
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
      const q = await invoke<QueueSnapshot>("get_queue");
      this.pending = q.pending;
      this.active = q.active;
      this.results = q.results;
    },
    async refreshFiles() {
      this.files = await invoke<DownloadedFile[]>("list_files");
    },
    async refreshWatched() {
      this.watched = await invoke<WatchEntry[]>("watch_list");
    },
    async init() {
      if (initialized) return;
      initialized = true;
      await Promise.all([this.refreshStatus(), this.refreshSettings(), this.refreshWatched()]);
      await this.refreshQueue().catch(() => undefined);
      await this.refreshFiles().catch(() => undefined);

      await listen<ActiveTask>("queue:progress", (e) => {
        this.active = e.payload;
      });
      await listen<TaskResult>("queue:result", (e) => {
        this.results.push(e.payload);
        if (this.active?.id === e.payload.id) this.active = null;
        void this.refreshFiles();
      });
      await listen("queue:pending", () => {
        void this.refreshQueue();
      });
      await listen("watch:changed", () => {
        void this.refreshWatched();
        void this.refreshSettings();
      });
      await listen("auth:changed", () => {
        void this.refreshStatus();
      });
    },
  },
});
