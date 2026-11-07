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

const FILTERS = computed(
  () =>
    [
      { key: "all", label: t("download.filters.all") },
      { key: "active", label: t("download.filters.active") },
      { key: "done", label: t("download.filters.done") },
      { key: "failed", label: t("download.filters.failed") },
    ] as const,
);
function errText(error: unknown): string {
  return errorText(error, t);
}
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
    rows.push({
      kind: "active",
      id: active.id,
      url: active.url,
      name: active.name,
    });
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
  {
    num: store.results.filter((r) => r.status === "success").length,
    label: t("download.status.success"),
  },
  { num: store.files.length, label: t("download.filesTitle") },
]);

const queueIndices = computed(
  () => new Map(store.pending.map((task, index) => [task.id, index])),
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
    if (url) window.open(url, "_blank");
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

import { fileUrl, isDesktop } from "../lib/backend";
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
            <span class="pill" :class="statusMeta(row).pill">{{
              statusMeta(row).label
            }}</span>
            <span v-if="row.size">· {{ fmtSize(row.size) }}</span>
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
        </div>
        <template v-if="row.kind === 'queued'">
          <button
            v-if="rowIndexQueue(row.id) > 0"
            class="ghost-btn row-action"
            :title="t('download.toTop')"
            :disabled="busy.has(row.id)"
            @click="moveTask(row, true)"
          >
            ⤒
          </button>
          <button
            v-if="rowIndexQueue(row.id) > 1"
            class="ghost-btn row-action"
            :title="t('download.moveUp')"
            :disabled="busy.has(row.id)"
            @click="moveTask(row, false)"
          >
            ↑
          </button>
          <button
            class="ghost-btn row-action danger"
            :disabled="busy.has(row.id)"
            :title="t('download.cancel')"
            @click="cancelTask(row)"
          >
            ✕
          </button>
        </template>
        <button
          v-if="
            (row.kind === 'success' || row.kind === 'cached') &&
            (row.path || row.name)
          "
          class="ghost-btn row-action"
          @click="openFile(row.path || row.name!)"
        >
          ⤓
        </button>
        <button
          v-else-if="row.kind === 'failed'"
          class="ghost-btn row-action"
          :disabled="busy.has(row.id)"
          :title="t('download.retry')"
          @click="retryTask(row)"
        >
          ⟳
        </button>
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
  background: rgba(96, 165, 250, 0.05);
}

.filters {
  margin-left: auto;
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
