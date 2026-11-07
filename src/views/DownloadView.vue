<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { NButton, NInput, NProgress, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import type { TaskResult } from "../types";
import AppIcon from "../components/AppIcon.vue";

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

const input = ref("");
const submitting = ref(false);

const stats = computed(() => [
  { num: store.pending.length, label: t("download.status.queued") },
  { num: store.active ? 1 : 0, label: t("download.status.downloading") },
  { num: store.results.filter((r) => r.status === "success").length, label: t("download.status.success") },
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

function statusMeta(r: TaskResult): { pill: string; label: string; icon: string } {
  if (r.status === "success") {
    return r.cached
      ? { pill: "cached", label: t("download.status.cached"), icon: "📦" }
      : { pill: "success", label: t("download.status.success"), icon: "✅" };
  }
  return { pill: "failed", label: errText(r.error ?? "FAILED"), icon: "❌" };
}

function shortUrl(url: string): string {
  const m = url.match(/t\.me\/(?:c\/)?[^/]+\/\d+/);
  return m ? m[0] : url;
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

async function openFile(name: string) {
  try {
    await invoke("reveal_file", { name });
  } catch {
    /* no-op */
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

  <!-- Active download -->
  <div v-if="store.active" class="panel active-card">
    <div class="row-item" style="border: 0; padding: 0">
      <span class="row-icon">🔄</span>
      <div class="row-body">
        <div class="row-title">{{ store.active.name || shortUrl(store.active.url) }}</div>
        <n-progress
          type="line"
          :percentage="Number(store.active.percent.toFixed(1))"
          :height="7"
          border-radius="4"
          :show-indicator="false"
          style="margin-top: 6px"
        />
        <div class="row-meta">
          <span>{{ fmtSize(store.active.current) }} / {{ fmtSize(store.active.total) }}</span>
          <span class="pill downloading">{{ t("download.status.downloading") }} {{ store.active.percent.toFixed(1) }}%</span>
        </div>
      </div>
    </div>
  </div>

  <!-- Tasks -->
  <div class="panel">
    <div class="panel-title">
      {{ t("download.queueTitle") }}
      <span class="count">{{ store.pending.length + store.results.length }}</span>
    </div>
    <div v-if="!store.pending.length && !store.results.length" class="empty">
      <span class="empty-icon">🗂</span>
      {{ t("download.emptyQueue") }}
    </div>
    <div v-else class="rows">
      <div v-for="j in store.pending" :key="`p${j.id}`" class="row-item">
        <span class="row-icon">⏳</span>
        <div class="row-body">
          <div class="row-title">{{ shortUrl(j.url) }}</div>
          <div class="row-meta"><span class="pill queued">{{ t("download.status.queued") }}</span></div>
        </div>
      </div>
      <div v-for="r in [...store.results].reverse()" :key="`r${r.id}`" class="row-item">
        <span class="row-icon">{{ statusMeta(r).icon }}</span>
        <div class="row-body">
          <div class="row-title">{{ r.name || shortUrl(r.url) }}</div>
          <div class="row-meta">
            <span class="pill" :class="statusMeta(r).pill">{{ statusMeta(r).label }}</span>
            <span v-if="r.size">· {{ fmtSize(r.size) }}</span>
          </div>
        </div>
        <button
          v-if="r.status === 'success' && r.name"
          class="ghost-btn row-action"
          @click="openFile(r.name!)"
        >
          ⤓ {{ t("download.open") }}
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
.active-card { border-color: rgba(88, 166, 255, 0.35); background: linear-gradient(180deg, rgba(88, 166, 255, 0.05), var(--surface)); }
</style>
