<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { errorText } from "../lib/errors";
import { invoke } from "../lib/backend";
import { NButton, NInput, NProgress, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import AppIcon from "../components/AppIcon.vue";

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

const input = ref("");
const submitting = ref(false);
const filter = ref<"all" | "active" | "done" | "failed">("all");

const FILTERS = computed(() => [
  { key: "all", label: t("download.filters.all") },
  { key: "active", label: t("download.filters.active") },
  { key: "done", label: t("download.filters.done") },
  { key: "failed", label: t("download.filters.failed") },
] as const);
function errText(error: unknown): string { return errorText(error, t); }
const busy = ref<Set<number>>(new Set());

interface Row {
  kind: "active" | "queued" | "success" | "cached" | "failed";
  id: number;
  url: string;
  name?: string;
  path?: string;
  size?: number;
  cached?: boolean;
  error?: string;
}

/** One row per task — status transitions in place. */
const taskRowsRaw = computed<Row[]>(() => {
  const rows: Row[] = [];
  const seen = new Set<number>();

  for (const active of store.activeTasks) {
    seen.add(active.id);
    rows.push({ kind: "active", id: active.id, url: active.url, name: active.name });
  }
  for (const p of store.pending) {
    if (seen.has(p.id)) continue;
    seen.add(p.id);
    rows.push({ kind: "queued", id: p.id, url: p.url });
  }
  for (const r of [...store.results].reverse()) {
    if (seen.has(r.id)) continue;
    seen.add(r.id);
    rows.push({
      kind: r.status === "failed" ? "failed" : r.cached ? "cached" : "success",
      id: r.id,
      url: r.url,
      name: r.name,
      path: r.path,
      size: r.size,
      error: r.error,
    });
  }
  return rows;
});

const taskRows = computed<Row[]>(() => {
  const f = filter.value;
  if (f === "all") return taskRowsRaw.value;
  return taskRowsRaw.value.filter((r) => {
    if (f === "active") return r.kind === "active" || r.kind === "queued";
    if (f === "done") return r.kind === "success" || r.kind === "cached";
    return r.kind === "failed";
  });
});

const stats = computed(() => [
  { num: store.activeTasks.length, label: t("download.status.downloading") },
  { num: store.pending.length, label: t("download.status.queued") },
  { num: store.results.filter((r) => r.status === "success").length, label: t("download.status.success") },
  { num: store.files.length, label: t("download.filesTitle") },
]);

const queueIndices = computed(() => new Map(store.pending.map((task, index) => [task.id, index])));

function fmtSize(bytes: number | undefined | null): string {
  const b = bytes ?? 0;
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(2)} GB`;
}

function fmtEta(row: { id: number; total?: number; current?: number }): string {
  const sp = store.speeds[row.id] ?? 0;
  if (sp <= 0 || !row.total) return "—";
  const remain = row.total - (row.current ?? 0);
  if (remain <= 0) return "—";
  const sec = Math.round(remain / sp);
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s2 = sec % 60;
  if (h > 0) return t("download.etaHours", { h, m });
  if (m > 0) return t("download.etaMinutes", { m, s: String(s2).padStart(2, "0") });
  return t("download.etaSeconds", { s: s2 });
}

function statusMeta(row: Row): { pill: string; label: string; icon: string } {
  switch (row.kind) {
    case "active":
      return { pill: "downloading", label: t("download.status.downloading"), icon: "🔄" };
    case "queued":
      return { pill: "queued", label: t("download.status.queued"), icon: "⏳" };
    case "cached":
      return { pill: "cached", label: t("download.status.cached"), icon: "📦" };
    case "success":
      return { pill: "success", label: t("download.status.success"), icon: "✅" };
    default:
      return { pill: "failed", label: errText(row.error ?? "FAILED"), icon: "❌" };
  }
}

function rowTitle(row: Row): string {
  if (row.name) return row.name;
  const m = row.url.match(/t\.me\/(?:c\/)?[^/]+\/\d+/);
  return m ? m[0] : row.url;
}

function rowIndexQueue(id: number): number {
  return queueIndices.value.get(id) ?? -1;
}

async function openFile(name: string) {
  if (!isDesktop) {
    const url = fileUrl(name);
    if (url) window.open(url, "_blank");
    return;
  }
  try {
    await invoke("reveal_file", { name });
  } catch (error) { message.error(errText(error)); }
}

async function removeFile(name: string) {
  if (!window.confirm(t("download.deleteConfirm", { name }))) return;
  try {
    await store.deleteFile(name);
    message.success(t("download.deleted"));
  } catch (e) {
    message.error(String(e));
  }
}

async function retryTask(row: Row) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke<number[]>("download_urls", { urls: [row.url] });
    message.success(t("download.queued", { n: 1 }));
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally { busy.value.delete(row.id); }
}

async function cancelTask(row: Row) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke("queue_cancel", { id: row.id });
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally { busy.value.delete(row.id); }
}

async function moveTask(row: Row, toFront: boolean) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke("queue_move", { id: row.id, toFront });
    await store.refreshQueue();
  } catch (e) {
    message.error(String(e));
  } finally { busy.value.delete(row.id); }
}

async function startDownload() {
  if (submitting.value) return;
  const submittedInput = input.value;
  const urls = [...new Set(input.value.split("\n").map((s) => s.trim()).filter(Boolean))];
  if (!urls.length) return;
  submitting.value = true;
  try {
    const ids = await invoke<number[]>("download_urls", { urls });
    message.success(t("download.queued", { n: ids.length }));
    if (input.value === submittedInput) input.value = "";
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally {
    submitting.value = false;
  }
}

import { fileUrl, isDesktop } from "../lib/backend";
</script>

<template>
  <!-- Hero input -->
  <div class="panel">
    <div class="panel-title">
      <AppIcon name="download" :size="14" />
      {{ t("download.title") }}
    </div>
    <n-input
      v-model:value="input"
      type="textarea"
      :rows="4"
      :placeholder="t('download.placeholder')"
      class="hero-input"
    />
    <div class="hero-actions">
      <n-button type="primary" size="large" :loading="submitting" :disabled="!store.authorized || !input.trim()" @click="startDownload">
        <template #icon>
          <AppIcon name="download" :size="16" />
        </template>
        {{ t("download.start") }}
      </n-button>
      <button v-if="input" class="ghost-btn" @click="input = ''">{{ t("download.clear") }}</button>
    </div>
  </div>

  <!-- Stats -->
  <div class="stats">
    <div v-for="s in stats" :key="s.label" class="stat">
      <div class="num">{{ s.num }}</div>
      <div class="label">{{ s.label }}</div>
    </div>
  </div>

  <!-- Monitor: active download -->
  <div v-for="active in store.activeTasks" :key="active.id" class="panel monitor">
    <div class="monitor-head">
      <span class="row-icon">🔄</span>
      <div class="m-name" :title="active.name || active.url">
        {{ active.name || active.url }}
      </div>
      <span class="pill downloading">{{ (active.percent ?? 0).toFixed(1) }}%</span>
      <button class="ghost-btn danger" @click="cancelTask({ id: active.id, url: active.url, kind: 'active' })">
        ✕ {{ t("download.cancel") }}
      </button>
    </div>
    <n-progress
      type="line"
      :percentage="Number((active.percent ?? 0).toFixed(1))"
      :height="10"
      border-radius="5"
      :show-indicator="false"
      style="margin: 10px 0 8px"
    />
    <div class="monitor-grid">
      <div class="m-stat">
        <div class="m-val">{{ fmtSize(store.speeds[active.id] ?? 0) }}/s</div>
        <div class="m-label">{{ t("download.speed") }}</div>
      </div>
      <div class="m-stat">
        <div class="m-val">{{ fmtSize(active.current) }}</div>
        <div class="m-label">{{ t("download.doneBytes") }}</div>
      </div>
      <div class="m-stat">
        <div class="m-val">{{ fmtSize(active.total) }}</div>
        <div class="m-label">{{ t("download.totalBytes") }}</div>
      </div>
      <div class="m-stat">
        <div class="m-val">{{ fmtEta(active) }}</div>
        <div class="m-label">{{ t("download.eta") }}</div>
      </div>
    </div>
  </div>

  <!-- Unified task list -->
  <div class="panel">
    <div class="panel-title">
      {{ t("download.queueTitle") }}
      <span class="count">{{ taskRows.length }}</span>
      <div class="filters">
        <button
          v-for="f in FILTERS"
          :key="f.key"
          class="chip-filter"
          :class="{ on: filter === f.key }"
          @click="filter = f.key"
        >
          {{ f.label }}
        </button>
      </div>
    </div>
    <div v-if="!taskRows.length" class="empty">
      <span class="empty-icon">🗂</span>
      {{ t("download.emptyQueue") }}
    </div>
    <div v-else class="rows">
      <div v-for="row in taskRows" :key="row.id" class="row-item" :class="'k-' + row.kind">
        <span class="row-icon">{{ statusMeta(row).icon }}</span>
        <div class="row-body">
          <div class="row-title" :title="row.url">{{ rowTitle(row) }}</div>
          <div class="row-meta">
            <span class="pill" :class="statusMeta(row).pill">{{ statusMeta(row).label }}</span>
            <span v-if="row.size">· {{ fmtSize(row.size) }}</span>
            <span v-if="row.kind === 'failed' && row.error">· {{ errText(row.error) }}</span>
          </div>
        </div>
        <template v-if="row.kind === 'queued'">
          <button
            v-if="rowIndexQueue(row.id) > 0"
            class="ghost-btn row-action"
            :title="t('download.toTop')"
            :disabled="busy.has(row.id)" @click="moveTask(row, true)"
          >
            ⤒
          </button>
          <button
            v-if="rowIndexQueue(row.id) > 1"
            class="ghost-btn row-action"
            :title="t('download.moveUp')"
            :disabled="busy.has(row.id)" @click="moveTask(row, false)"
          >
            ↑
          </button>
          <button class="ghost-btn row-action danger" :disabled="busy.has(row.id)" :title="t('download.cancel')" @click="cancelTask(row)">✕</button>
        </template>
        <button
          v-if="(row.kind === 'success' || row.kind === 'cached') && (row.path || row.name)"
          class="ghost-btn row-action"
          @click="openFile(row.path || row.name!)"
        >
          ⤓
        </button>
        <button
          v-else-if="row.kind === 'failed'"
          class="ghost-btn row-action"
          :disabled="busy.has(row.id)" :title="t('download.retry')" @click="retryTask(row)"
        >
          ⟳
        </button>
      </div>
    </div>
  </div>

  <!-- Files -->
  <div class="panel">
    <div class="panel-title">
      {{ t("download.filesTitle") }}
      <span class="count">{{ store.files.length }}</span>
    </div>
    <div v-if="!store.files.length" class="empty">
      <span class="empty-icon">📁</span>
      {{ t("download.noFiles") }}
    </div>
    <div v-else class="rows">
      <div v-for="f in store.files" :key="f.name" class="row-item">
        <span class="row-icon">🎬</span>
        <div class="row-body">
          <div class="row-title" :title="f.name">{{ f.name }}</div>
          <div class="row-meta">{{ fmtSize(f.size) }}</div>
        </div>
        <button class="ghost-btn row-action" @click="openFile(f.name)">⤓ {{ t("download.open") }}</button>
        <button class="ghost-btn row-action danger" @click="removeFile(f.name)">
          {{ t("download.delete") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hero-input :deep(textarea) { font-size: 13.5px; }
.hero-actions { display: flex; align-items: center; gap: 10px; margin-top: 12px; }
.ghost-btn.danger:hover { border-color: var(--red); color: var(--red); }
.row-item.k-active { background: rgba(96, 165, 250, 0.05); }

/* monitor card */
.monitor { border-color: rgba(16, 185, 129, 0.3); }
.monitor-head { display: flex; align-items: center; gap: 10px; }
.m-name {
  flex: 1; min-width: 0;
  color: var(--text); font-size: 13px; font-weight: 500;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.monitor-grid {
  display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px;
  margin-top: 4px;
}
.m-stat {
  background: var(--bg-inset);
  border: 1px solid var(--border-soft);
  border-radius: 8px;
  padding: 8px 10px;
}
.m-val { font-size: 14px; font-weight: 700; color: var(--text); font-variant-numeric: tabular-nums; }
.m-label { font-size: 10.5px; color: var(--text-faint); margin-top: 2px; }

.filters { margin-left: auto; display: flex; gap: 4px; }
.chip-filter {
  background: none; border: 0; color: var(--text-faint);
  font-size: 11px; font-weight: 500; cursor: pointer;
  padding: 2px 8px; border-radius: 999px; font-family: inherit;
  transition: all 0.12s;
}
.chip-filter:hover { color: var(--text-dim); }
.chip-filter.on { background: var(--accent-soft); color: var(--accent-bright); }
</style>
