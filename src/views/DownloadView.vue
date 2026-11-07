<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke, fileUrl, isDesktop } from "../lib/backend";
import { NButton, NInput, NProgress, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";

import AppIcon from "../components/AppIcon.vue";

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

const input = ref("");
const submitting = ref(false);

const stats = computed(() => [
  { num: store.pending.length + (store.active ? 1 : 0), label: t("download.status.downloading") },
  { num: store.results.filter((r) => r.status === "success").length, label: t("download.status.success") },
  { num: store.results.filter((r) => r.status === "failed").length, label: t("download.status.failed") },
  { num: store.files.length, label: t("download.filesTitle") },
]);

function fmtSize(bytes: number | undefined | null): string {
  const b = bytes ?? 0;
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(2)} GB`;
}

const ERR_KEYS = [
  "INVALID_LINK",
  "INVITE_LINK",
  "NOT_LOGGED_IN",
  "MESSAGE_NOT_FOUND",
  "NOT_A_VIDEO",
  "RATE_LIMITED",
  "CHANNEL_UNAVAILABLE",
  "API_NOT_CONFIGURED",
] as const;

function errText(e: unknown): string {
  const raw = String(e);
  const code = raw.includes(":") ? raw.split(":")[0].trim() : raw.trim();
  for (const k of ERR_KEYS) {
    if (code.startsWith(k)) return t(`download.errors.${k}`);
  }
  return raw;
}

interface Row {
  kind: "active" | "queued" | "success" | "cached" | "failed";
  id: number;
  url: string;
  name?: string;
  size?: number;
  cached?: boolean;
  error?: string;
  percent?: number;
  current?: number;
  total?: number;
}

/** One row per task — status transitions in place (Gopeed-style). */
const taskRows = computed<Row[]>(() => {
  const rows: Row[] = [];
  const seen = new Set<number>();

  if (store.active) {
    seen.add(store.active.id);
    rows.push({
      kind: "active",
      id: store.active.id,
      url: store.active.url,
      name: store.active.name,
      percent: store.active.percent,
      current: store.active.current,
      total: store.active.total,
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
      size: r.size,
      error: r.error,
    });
  }
  return rows;
});

function rowTitle(row: Row): string {
  if (row.name) return row.name;
  const m = row.url.match(/t\.me\/(?:c\/)?[^/]+\/\d+/);
  return m ? m[0] : row.url;
}

function rowIndexQueue(id: number): number {
  return store.pending.findIndex((p) => p.id === id);
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

async function openFile(name: string) {
  if (!isDesktop) {
    const url = fileUrl(name);
    if (url) window.open(url, "_blank");
    return;
  }
  try {
    await invoke("reveal_file", { name });
  } catch {
    /* no-op */
  }
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

async function moveTask(row: Row, toFront: boolean) {
  try {
    await invoke("queue_move", { id: row.id, toFront });
    await store.refreshQueue();
  } catch (e) {
    message.error(String(e));
  }
}

async function retryTask(row: Row) {
  try {
    await invoke<number[]>("download_urls", { urls: [row.url] });
    message.success(t("download.queued", { n: 1 }));
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  }
}

async function startDownload() {
  const urls = input.value
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  if (!urls.length) return;
  submitting.value = true;
  try {
    const ids = await invoke<number[]>("download_urls", { urls });
    message.success(t("download.queued", { n: ids.length }));
    input.value = "";
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally {
    submitting.value = false;
  }
}
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
      <n-button type="primary" size="large" :loading="submitting" @click="startDownload">
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

  <!-- Unified task list -->
  <div class="panel">
    <div class="panel-title">
      {{ t("download.queueTitle") }}
      <span class="count">{{ taskRows.length }}</span>
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
            <span v-if="row.kind === 'active' && row.current !== undefined">
              {{ fmtSize(row.current) }} / {{ fmtSize(row.total) }}
            </span>
            <span v-else-if="row.size">· {{ fmtSize(row.size) }}</span>
          </div>
          <n-progress
            v-if="row.kind === 'active'"
            type="line"
            :percentage="Number((row.percent ?? 0).toFixed(1))"
            :height="5"
            border-radius="3"
            :show-indicator="false"
            style="margin-top: 5px"
          />
        </div>
        <template v-if="row.kind === 'queued'">
          <button
            v-if="rowIndexQueue(row.id) > 0"
            class="ghost-btn row-action"
            :title="t('download.toTop')"
            @click="moveTask(row, true)"
          >
            ⤒
          </button>
          <button
            v-if="rowIndexQueue(row.id) > 1"
            class="ghost-btn row-action"
            :title="t('download.moveUp')"
            @click="moveTask(row, false)"
          >
            ↑
          </button>
        </template>
        <button
          v-if="(row.kind === 'success' || row.kind === 'cached') && row.name"
          class="ghost-btn row-action"
          @click="openFile(row.name)"
        >
          ⤓
        </button>
        <button
          v-else-if="row.kind === 'failed'"
          class="ghost-btn row-action"
          @click="retryTask(row)"
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
.row-item.k-active { background: rgba(88, 166, 255, 0.05); }
</style>
