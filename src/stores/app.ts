import { defineStore } from "pinia";
import { invoke, on, onReconnect, onConnection } from "../lib/backend";
import type { ConnectionState } from "../lib/backend";
import { setLocale } from "../i18n";
import type { ActiveTask, AppSettings, DownloadedFile, QueueSnapshot, Status, TaskResult, WatchEntry } from "../types";

type Runtime = {
  init: Promise<void> | null; queue: Promise<void> | null; files: Promise<void> | null;
  queueDirty: boolean; filesDirty: boolean; revision: number; generation: number;
  subscribed: boolean; cleanup: (() => void)[];
  statusSeq: number; settingsSeq: number; watchedSeq: number;
};
const runtimes = new WeakMap<object, Runtime>();
function runtime(store: object): Runtime {
  let value = runtimes.get(store);
  if (!value) {
    value = { init: null, queue: null, files: null, queueDirty: false, filesDirty: false,
      revision: 0, generation: 0, subscribed: false, cleanup: [], statusSeq: 0, settingsSeq: 0, watchedSeq: 0 };
    runtimes.set(store, value);
  }
  return value;
}
export const useAppStore = defineStore("app", {
  state: () => ({
    status: null as Status | null, settings: null as AppSettings | null,
    pending: [] as { id: number; url: string }[], activeTasks: [] as ActiveTask[],
    results: [] as TaskResult[], files: [] as DownloadedFile[], watched: [] as WatchEntry[],
    samplesByTask: {} as Record<number, { ts: number; current: number }[]>,
    speeds: {} as Record<number, number>,
    initState: "idle" as "idle" | "loading" | "ready" | "error", initError: "",
    connection: "disconnected" as ConnectionState,
  }),
  getters: {
    authorized: (state) => state.status?.authorized ?? false,
    configured: (state) => state.status?.configured ?? false,
    user: (state) => state.status?.user ?? null,
    active: (state): ActiveTask | null => state.activeTasks[0] ?? null,
    speed: (state) => Object.values(state.speeds).reduce((sum, speed) => sum + speed, 0),
  },
  actions: {
    async refreshStatus() {
      const rt = runtime(this), sequence = ++rt.statusSeq;
      const status = await invoke<Status>("get_status");
      if (sequence === rt.statusSeq) this.status = status;
    },
    async refreshSettings() {
      const rt = runtime(this), sequence = ++rt.settingsSeq;
      const settings = await invoke<AppSettings>("get_settings");
      if (sequence !== rt.settingsSeq) return;
      this.settings = settings;
      if (this.settings.lang === "zh-CN" || this.settings.lang === "en") setLocale(this.settings.lang);
    },
    async refreshWatched() {
      const rt = runtime(this), sequence = ++rt.watchedSeq;
      const watched = await invoke<WatchEntry[]>("watch_list");
      if (sequence === rt.watchedSeq) this.watched = watched;
    },
    async refreshQueue() {
      const rt = runtime(this);
      rt.queueDirty = true;
      if (rt.queue) return rt.queue;
      const generation = rt.generation;
      rt.queue = (async () => {
        while (rt.queueDirty && generation === rt.generation) {
          rt.queueDirty = false;
          const revision = rt.revision;
          const snapshot = await invoke<QueueSnapshot>("get_queue");
          if (generation !== rt.generation) return;
          if (revision !== rt.revision) { rt.queueDirty = true; continue; }
          const terminal = new Set(snapshot.results.map((result) => result.id));
          const tasks = (snapshot.activeTasks ?? (snapshot.active ? [snapshot.active] : [])).filter((task) => !terminal.has(task.id));
          const latest = new Map(this.activeTasks.map((task) => [task.id, task]));
          this.activeTasks = tasks.map((task) => {
            const progress = latest.get(task.id);
            return progress && progress.current > task.current ? progress : task;
          });
          const busy = new Set(this.activeTasks.map((task) => task.id));
          this.pending = snapshot.pending.filter((task) => !busy.has(task.id) && !terminal.has(task.id));
          this.results = [...new Map(snapshot.results.map((result) => [result.id, result])).values()];
          for (const id of Object.keys(this.samplesByTask).map(Number)) {
            if (!busy.has(id)) { delete this.samplesByTask[id]; delete this.speeds[id]; }
          }
        }
      })();
      try { await rt.queue; } finally { rt.queue = null; }
    },
    async refreshFiles() {
      const rt = runtime(this);
      rt.filesDirty = true;
      if (rt.files) return rt.files;
      rt.files = (async () => {
        while (rt.filesDirty) {
          rt.filesDirty = false;
          this.files = await invoke<DownloadedFile[]>("list_files");
        }
      })();
      try { await rt.files; } finally { rt.files = null; }
    },
    async deleteFile(name: string) { await invoke("delete_file", { name }); await this.refreshFiles(); },
    progress(task: ActiveTask) {
      if (this.results.some((result) => result.id === task.id)) return;
      const now = Date.now();
      const previous = this.samplesByTask[task.id] ?? [];
      const reset = previous.length && task.current < previous[previous.length - 1].current;
      const samples = reset ? [] : previous.filter((sample) => now - sample.ts <= 3000);
      samples.push({ ts: now, current: task.current });
      this.samplesByTask[task.id] = samples;
      const elapsed = (now - samples[0].ts) / 1000;
      this.speeds[task.id] = elapsed >= 0.8 ? Math.max(0, (task.current - samples[0].current) / elapsed) : 0;
      this.pending = this.pending.filter((pending) => pending.id !== task.id);
      const index = this.activeTasks.findIndex((active) => active.id === task.id);
      if (index < 0) this.activeTasks.push(task); else this.activeTasks[index] = task;
    },
    async subscribe() {
      const rt = runtime(this);
      if (rt.subscribed) return;
      const refresh = () => { rt.revision++; void this.refreshQueue().catch(() => undefined); };
      try {
        rt.cleanup.push(onReconnect(() => {
          this.samplesByTask = {}; this.speeds = {};
          void this.resync().catch(() => undefined);
        }));
        rt.cleanup.push(onConnection((state) => { this.connection = state; }));
        rt.cleanup.push(await on("queue:progress", (payload) => this.progress(payload as ActiveTask)));
        rt.cleanup.push(await on("queue:result", (payload) => {
          const result = payload as TaskResult;
          rt.revision++;
          this.results = [...this.results.filter((old) => old.id !== result.id), result].slice(-200);
          this.activeTasks = this.activeTasks.filter((task) => task.id !== result.id);
          this.pending = this.pending.filter((task) => task.id !== result.id);
          delete this.samplesByTask[result.id]; delete this.speeds[result.id];
          void this.refreshFiles().catch(() => undefined);
        }));
        rt.cleanup.push(await on("queue:pending", refresh));
        rt.cleanup.push(await on("queue:changed", refresh));
        rt.cleanup.push(await on("watch:changed", () => {
          void Promise.all([this.refreshWatched(), this.refreshSettings()]).catch(() => undefined);
        }));
        rt.cleanup.push(await on("auth:changed", () => { void this.refreshStatus().catch(() => undefined); }));
        rt.subscribed = true;
      } catch (error) {
        rt.cleanup.splice(0).forEach((cleanup) => cleanup());
        throw error;
      }
    },
    async resync() {
      await Promise.all([this.refreshStatus(), this.refreshSettings(), this.refreshWatched(), this.refreshQueue(), this.refreshFiles()]);
    },
    async init() {
      const rt = runtime(this);
      if (rt.init) return rt.init;
      if (this.initState === "ready") return;
      this.initState = "loading"; this.initError = "";
      rt.init = (async () => {
        await this.subscribe();
        await this.resync();
        this.initState = "ready";
      })();
      try { await rt.init; } catch (error) {
        this.initState = "error";
        this.initError = error instanceof Error ? error.message : String(error);
        throw error;
      } finally { rt.init = null; }
    },
    dispose() {
      const rt = runtime(this);
      rt.generation++;
      rt.cleanup.splice(0).forEach((cleanup) => cleanup());
      rt.subscribed = false;
      this.initState = "idle";
    },
  },
});
