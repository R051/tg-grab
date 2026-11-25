<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { errorText } from "../lib/errors";
import { fileUrl, invoke, isDesktop } from "../lib/backend";
import { formatTime } from "../lib/library";
import { NButton, NInput, NProgress, NSelect, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import AppIcon from "../components/AppIcon.vue";

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

const input = ref("");
const submitting = ref(false);
const filter = ref<"all" | "active" | "paused" | "done" | "failed">("all");
const sortOrder = ref<"timeDesc" | "timeAsc" | "queue">("timeDesc");

const FILTERS = computed(
  () =>
    [
      { key: "all", label: t("download.filters.all") },
      { key: "active", label: t("download.filters.active") },
      { key: "paused", label: t("download.filters.paused") },
      { key: "done", label: t("download.filters.done") },
      { key: "failed", label: t("download.filters.failed") },
    ] as const,
);

const SORT_OPTIONS = computed(() => [
  { label: t("download.sortNewest"), value: "timeDesc" },
  { label: t("download.sortOldest"), value: "timeAsc" },
  { label: t("download.sortQueue"), value: "queue" },
]);

function errText(error: unknown): string {
  return errorText(error, t);
}
const busy = ref<Set<number>>(new Set());

interface Row {
  kind: "active" | "paused" | "queued" | "success" | "cached" | "failed";
  id: number;
  url: string;
  name?: string;
  path?: string;
  size?: number;
  current?: number;
  total?: number;
  percent?: number;
  cached?: boolean;
  error?: string;
  timestamp: number;
}

/** One row per task — status transitions in place. */
const taskRowsRaw = computed<Row[]>(() => {
  const rows: Row[] = [];
  const seen = new Set<number>();

  for (const active of store.activeTasks ?? []) {
    seen.add(active.id);
    rows.push({
      kind: "active",
      id: active.id,
      url: active.url,
      name: active.name,
      current: active.current,
      total: active.total,
      percent: active.percent,
      timestamp: active.startedAt ?? active.createdAt ?? 0,
    });
  }
  for (const paused of store.paused ?? []) {
    if (seen.has(paused.id)) continue;
    seen.add(paused.id);
    rows.push({
      kind: "paused",
      id: paused.id,
      url: paused.url,
      name: paused.name,
      current: paused.current,
      total: paused.total,
      percent: paused.percent,
      timestamp: paused.pausedAt ?? paused.createdAt ?? 0,
    });
  }
  for (const p of store.pending ?? []) {
    if (seen.has(p.id)) continue;
    seen.add(p.id);
    rows.push({
      kind: "queued",
      id: p.id,
      url: p.url,
      name: p.name,
      current: p.current,
      total: p.total,
      percent: p.percent,
      timestamp: p.createdAt ?? 0,
    });
  }
  for (const r of [...(store.results ?? [])].reverse()) {
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
      timestamp: r.completedAt ?? r.createdAt ?? 0,
    });
  }
  return rows;
});

const taskRows = computed<Row[]>(() => {
  const f = filter.value;
  const filtered =
    f === "all"
      ? taskRowsRaw.value
      : taskRowsRaw.value.filter((r) => {
          if (f === "active") return r.kind === "active" || r.kind === "queued";
          if (f === "paused") return r.kind === "paused";
          if (f === "done") return r.kind === "success" || r.kind === "cached";
          return r.kind === "failed";
        });

  if (sortOrder.value === "queue") return filtered;
  return [...filtered].sort((a, b) =>
    sortOrder.value === "timeAsc"
      ? a.timestamp - b.timestamp || a.id - b.id
      : b.timestamp - a.timestamp || b.id - a.id,
  );
});

const stats = computed(() => [
  { num: store.activeTasks.length, label: t("download.status.downloading") },
  {
    num: store.pending.length + (store.paused?.length ?? 0),
    label: t("download.status.queued"),
  },
  {
    num: store.results.filter((r) => r.status === "success").length,
    label: t("download.status.success"),
  },
  { num: store.files.length, label: t("download.filesTitle") },
]);

const queueIndices = computed(
  () => new Map((store.pending ?? []).map((task, index) => [task.id, index])),
);

function fmtSize(bytes: number | undefined | null): string {
  const b = bytes ?? 0;
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(2)} GB`;
}

function statusMeta(row: Row): { pill: string; label: string; icon: string } {
  switch (row.kind) {
    case "active":
      return {
        pill: "downloading",
        label: t("download.status.downloading"),
        icon: "download",
      };
    case "paused":
      return {
        pill: "paused",
        label: t("download.status.paused"),
        icon: "pause",
      };
    case "queued":
      return {
        pill: "queued",
        label: t("download.status.queued"),
        icon: "history",
      };
    case "cached":
      return {
        pill: "cached",
        label: t("download.status.cached"),
        icon: "file",
      };
    case "success":
      return {
        pill: "success",
        label: t("download.status.success"),
        icon: "download",
      };
    default:
      return {
        pill: "failed",
        label: errText(row.error ?? "FAILED"),
        icon: "close",
      };
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
    if (url) window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  try {
    await invoke("reveal_file", { name });
  } catch (error) {
    message.error(errText(error));
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
  } finally {
    busy.value.delete(row.id);
  }
}

async function pauseTask(row: Row) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke("queue_pause", { id: row.id });
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally {
    busy.value.delete(row.id);
  }
}

async function resumeTask(row: Row) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke("queue_resume", { id: row.id });
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally {
    busy.value.delete(row.id);
  }
}

async function cancelTask(row: Row) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke("queue_cancel", { id: row.id });
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally {
    busy.value.delete(row.id);
  }
}

async function moveTask(row: Row, toFront: boolean) {
  if (busy.value.has(row.id)) return;
  busy.value.add(row.id);
  try {
    await invoke("queue_move", { id: row.id, toFront });
    await store.refreshQueue();
  } catch (e) {
    message.error(String(e));
  } finally {
    busy.value.delete(row.id);
  }
}

async function startDownload() {
  if (submitting.value) return;
  const submittedInput = input.value;
  const urls = [
    ...new Set(
      input.value
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean),
    ),
  ];
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
</script>

<template>
  <header class="view-heading">
    <div>
      <p class="eyebrow">{{ t("app.tabs.download") }}</p>
      <h2>{{ t("app.tabs.download") }}</h2>
      <p class="view-description">{{ t("workspace.downloadHint") }}</p>
    </div>
  </header>
  <!-- Hero input -->
  <div class="panel">
    <div class="panel-title">
      <AppIcon name="download" :size="14" />
      {{ t("download.title") }}
    </div>
    <div class="download-entry">
      <n-input
        v-model:value="input"
        type="textarea"
        :rows="2"
        :placeholder="t('download.placeholder')"
        class="hero-input"
      />
      <div class="hero-actions">
        <n-button
          type="primary"
          size="large"
          :loading="submitting"
          :disabled="!store.authorized || !input.trim()"
          @click="startDownload"
        >
          <template #icon>
            <AppIcon name="download" :size="16" />
          </template>
          {{ t("download.start") }}
        </n-button>
        <button v-if="input" class="ghost-btn" @click="input = ''">
          {{ t("download.clear") }}
        </button>
      </div>
    </div>
  </div>

  <div
    v-if="store.pending.length && !store.authorized"
    class="queue-note"
  >
    {{ t("download.waitLogin") }}
  </div>
  <!-- Stats -->
  <div class="stats">
    <div v-for="s in stats" :key="s.label" class="stat">
      <div class="num">{{ s.num }}</div>
      <div class="label">{{ s.label }}</div>
    </div>
  </div>

  <!-- Unified task list -->
  <div class="panel fill-panel">
    <div class="panel-title queue-header">
      <span>{{ t("download.queueTitle") }}</span>
      <span class="count">{{ taskRows.length }}</span>
      <div class="queue-controls">
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
        <n-select
          v-model:value="sortOrder"
          :options="SORT_OPTIONS"
          size="tiny"
          class="queue-sort"
        />
      </div>
    </div>
    <div v-if="!taskRows.length" class="empty">
      <span class="empty-icon"><AppIcon name="download" :size="24" /></span>
      {{ t("download.emptyQueue") }}
    </div>
    <div v-else class="rows scroll-area">
      <div
        v-for="row in taskRows"
        :key="row.id"
        class="row-item"
        :class="'k-' + row.kind"
      >
        <span class="row-icon"
          ><AppIcon :name="statusMeta(row).icon" :size="17"
        /></span>
        <div class="row-body">
          <div class="row-title" :title="row.url">{{ rowTitle(row) }}</div>
          <div class="row-meta">
            <span v-if="row.kind === 'active'"
              >{{
                (
                  store.activeTasks.find((task) => task.id === row.id)
                    ?.percent ?? 0
                ).toFixed(1)
              }}% · {{ fmtSize(store.speeds[row.id] ?? 0) }}/s</span
            >
            <span v-else-if="row.kind === 'paused' && (row.total || row.percent)"
              >{{ (row.percent ?? 0).toFixed(1) }}% ·
              {{ fmtSize(row.current) }} / {{ fmtSize(row.total) }}</span
            >
            <span class="pill" :class="statusMeta(row).pill">{{
              statusMeta(row).label
            }}</span>
            <span v-if="row.size">· {{ fmtSize(row.size) }}</span>
            <span v-if="formatTime(row.timestamp)">· {{ formatTime(row.timestamp) }}</span>
            <span v-if="row.kind === 'failed' && row.error"
              >· {{ errText(row.error) }}</span
            >
          </div>
          <n-progress
            v-if="row.kind === 'active'"
            type="line"
            :percentage="
              Number(
                (
                  store.activeTasks.find((task) => task.id === row.id)
                    ?.percent ?? 0
                ).toFixed(1),
              )
            "
            :height="4"
            :show-indicator="false"
            style="margin-top: 6px"
          />
          <n-progress
            v-else-if="row.kind === 'paused' && row.total"
            type="line"
            status="warning"
            :percentage="Number((row.percent ?? 0).toFixed(1))"
            :height="4"
            :show-indicator="false"
            style="margin-top: 6px"
          />
        </div>
        <div class="row-actions">
          <template v-if="row.kind === 'active'">
            <button
              class="ghost-btn row-action"
              :disabled="busy.has(row.id)"
              :title="t('download.pause')"
              @click="pauseTask(row)"
            >
              <AppIcon name="pause" :size="13" />
              <span>{{ t("download.pause") }}</span>
            </button>
            <button
              class="ghost-btn row-action danger"
              :disabled="busy.has(row.id)"
              :title="t('download.cancel')"
              @click="cancelTask(row)"
            >
              <AppIcon name="close" :size="13" />
              <span>{{ t("common.cancel") }}</span>
            </button>
          </template>
          <template v-else-if="row.kind === 'paused'">
            <button
              class="ghost-btn row-action"
              :disabled="busy.has(row.id)"
              :title="t('download.resume')"
              @click="resumeTask(row)"
            >
              <AppIcon name="play" :size="13" />
              <span>{{ t("download.resume") }}</span>
            </button>
            <button
              class="ghost-btn row-action danger"
              :disabled="busy.has(row.id)"
              :title="t('download.cancel')"
              @click="cancelTask(row)"
            >
              <AppIcon name="close" :size="13" />
              <span>{{ t("common.cancel") }}</span>
            </button>
          </template>
          <template v-else-if="row.kind === 'queued'">
            <button
              v-if="rowIndexQueue(row.id) > 0"
              class="ghost-btn row-action icon-only"
              :title="t('download.toTop')"
              :disabled="busy.has(row.id)"
              @click="moveTask(row, true)"
            >
              <AppIcon name="toTop" :size="13" />
            </button>
            <button
              v-if="rowIndexQueue(row.id) > 1"
              class="ghost-btn row-action icon-only"
              :title="t('download.moveUp')"
              :disabled="busy.has(row.id)"
              @click="moveTask(row, false)"
            >
              <AppIcon name="arrowUp" :size="13" />
            </button>
            <button
              class="ghost-btn row-action"
              :disabled="busy.has(row.id)"
              :title="t('download.pause')"
              @click="pauseTask(row)"
            >
              <AppIcon name="pause" :size="13" />
              <span>{{ t("download.pause") }}</span>
            </button>
            <button
              class="ghost-btn row-action danger"
              :disabled="busy.has(row.id)"
              :title="t('download.cancel')"
              @click="cancelTask(row)"
            >
              <AppIcon name="close" :size="13" />
              <span>{{ t("common.cancel") }}</span>
            </button>
          </template>
          <button
            v-else-if="
              (row.kind === 'success' || row.kind === 'cached') &&
              (row.path || row.name)
            "
            class="ghost-btn row-action"
            :title="isDesktop ? t('library.reveal') : t('library.downloadDevice')"
            @click="openFile(row.path || row.name!)"
          >
            <AppIcon :name="isDesktop ? 'folderOpen' : 'download'" :size="14" />
            <span>{{
              isDesktop ? t("download.openFolder") : t("library.downloadDevice")
            }}</span>
          </button>
          <template v-else-if="row.kind === 'failed'">
            <button
              class="ghost-btn row-action"
              :disabled="busy.has(row.id)"
              :title="t('download.retry')"
              @click="retryTask(row)"
            >
              <AppIcon name="refresh" :size="13" />
              <span>{{ t("download.retry") }}</span>
            </button>
            <button
              class="ghost-btn row-action danger icon-only"
              :disabled="busy.has(row.id)"
              :title="t('download.cancel')"
              @click="cancelTask(row)"
            >
              <AppIcon name="close" :size="13" />
            </button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hero-input :deep(textarea) {
  font-size: 13.5px;
}
.download-entry {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 140px;
  gap: 12px;
  align-items: stretch;
}
.hero-actions {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
}
.queue-note {
  margin: 0;
  font-size: 12px;
  color: var(--text-dim);
}
.ghost-btn.danger:hover {
  border-color: var(--red);
  color: var(--red);
}
.row-item.k-active {
  background: var(--accent-soft);
}
.row-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.row-action {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.row-action.icon-only {
  padding: 4px 8px;
}
.queue-header {
  flex-wrap: wrap;
}
.queue-controls {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.queue-sort {
  width: 122px;
}
.filters {
  display: flex;
  gap: 4px;
}
.chip-filter {
  background: none;
  border: 0;
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  padding: 2px 8px;
  border-radius: 999px;
  font-family: inherit;
  transition: all 0.12s;
}
.chip-filter:hover {
  color: var(--text-dim);
}
.chip-filter.on {
  background: var(--accent-soft);
  color: var(--accent-bright);
}
</style>
