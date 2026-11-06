<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { NButton, NInput, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import AppIcon from "../components/AppIcon.vue";
import type { HistoryPage, VideoInfo } from "../types";

const { t, locale } = useI18n();
const store = useAppStore();
const message = useMessage();

const link = ref("");
const browsing = ref(false);
const downloading = ref(false);
const chatTitle = ref("");
const videos = ref<VideoInfo[]>([]);
const nextOffset = ref(0);
const exhausted = ref(true);
const selected = ref<Set<string>>(new Set());

function fmtSize(bytes: number): string {
  if (!bytes) return "0 B";
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
  return `${(bytes / 1073741824).toFixed(2)} GB`;
}

function fmtDuration(sec: number): string {
  if (!sec) return "";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}

function fmtDate(iso: string | null): string {
  if (!iso) return "";
  try {
    return new Date(iso).toLocaleString(locale.value, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

function isSelected(v: VideoInfo): boolean {
  return selected.value.has(v.url);
}

function toggleSelect(v: VideoInfo) {
  const next = new Set(selected.value);
  if (next.has(v.url)) next.delete(v.url);
  else next.add(v.url);
  selected.value = next;
}

function selectableCount(): number {
  return videos.value.filter((v) => !v.downloaded).length;
}

function pageAllSelected(): boolean {
  const n = selectableCount();
  return n > 0 && videos.value.filter((v) => !v.downloaded).every((v) => isSelected(v));
}

function toggleSelectAll(ev: Event) {
  const checked = (ev.target as HTMLInputElement).checked;
  const next = new Set(selected.value);
  for (const v of videos.value) {
    if (v.downloaded) continue;
    if (checked) next.add(v.url);
    else next.delete(v.url);
  }
  selected.value = next;
}

async function browse(more: boolean) {
  if (!link.value.trim()) return;
  browsing.value = true;
  try {
    const page = await invoke<HistoryPage>("history_page", {
      link: link.value.trim(),
      offsetId: more ? nextOffset.value : 0,
    });
    chatTitle.value = page.chat.title;
    videos.value = more ? [...videos.value, ...page.videos] : page.videos;
    nextOffset.value = page.nextOffset;
    exhausted.value = page.exhausted;
    selected.value = new Set();
    if (page.videos.length || more) {
      message.info(t("history.loaded", { chat: page.chat.title, n: videos.value.length }));
    } else {
      message.warning(t("history.empty"));
    }
  } catch (e) {
    message.error(String(e));
  } finally {
    browsing.value = false;
  }
}

async function downloadSelected() {
  const urls = videos.value.filter((v) => isSelected(v)).map((v) => v.url);
  if (!urls.length) return;
  downloading.value = true;
  try {
    await invoke<number[]>("download_urls", { urls });
    message.success(t("history.added", { n: urls.length }));
    selected.value = new Set();
    await store.refreshQueue();
  } catch (e) {
    message.error(String(e));
  } finally {
    downloading.value = false;
  }
}
</script>

<template>
  <div class="panel">
    <div class="panel-title">
      <AppIcon name="history" :size="14" />
      {{ t("history.title") }}
    </div>
    <div class="row">
      <n-input
        v-model:value="link"
        :placeholder="t('history.input')"
        size="small"
        @keyup.enter="browse(false)"
      />
      <n-button type="primary" size="small" :loading="browsing" @click="browse(false)">
        {{ t("history.browse") }}
      </n-button>
    </div>
  </div>

  <div v-if="videos.length" class="panel">
    <div class="panel-title">
      <span v-if="chatTitle" class="chat-name">{{ chatTitle }}</span>
      <span class="count">{{ videos.length }}</span>
    </div>
    <div class="rows">
      <div
        v-for="v in videos"
        :key="v.msgId"
        class="row-item"
        :class="{ downloaded: v.downloaded }"
        @click="!v.downloaded && toggleSelect(v)"
      >
        <input
          type="checkbox"
          class="check"
          :checked="isSelected(v)"
          :disabled="v.downloaded"
          @click.stop
          @change="toggleSelect(v)"
        />
        <span class="row-icon">🎬</span>
        <div class="row-body">
          <div class="row-title" :title="v.name">{{ v.name }}</div>
          <div class="row-meta">
            <span>{{ fmtSize(v.size) }}</span>
            <template v-if="v.duration"> · <span>{{ fmtDuration(v.duration) }}</span></template>
            <template v-if="fmtDate(v.date)"> · <span>{{ fmtDate(v.date) }}</span></template>
            <span v-if="v.downloaded" class="pill success">✅ {{ t("history.downloaded") }}</span>
          </div>
        </div>
      </div>
    </div>
    <div class="row actions">
      <n-button
        type="primary"
        size="small"
        :disabled="!selected.size"
        :loading="downloading"
        @click="downloadSelected"
      >
        {{ t("history.downloadSelected", { n: selected.size }) }}
      </n-button>
      <n-button v-if="!exhausted" quaternary size="small" :loading="browsing" @click="browse(true)">
        {{ t("history.loadMore") }}
      </n-button>
      <label class="selall">
        <input
          type="checkbox"
          :checked="pageAllSelected()"
          :disabled="selectableCount() === 0"
          @change="toggleSelectAll"
        />
        {{ t("history.selectAll") }}
      </label>
    </div>
  </div>

  <div v-else-if="chatTitle" class="panel">
    <div class="empty">
      <span class="empty-icon">🎞</span>
      {{ t("history.empty") }}
    </div>
  </div>
</template>

<style scoped>
.row { display: flex; align-items: center; gap: 10px; }
.actions { margin-top: 12px; }
.chat-name { color: var(--text); }
.selall { margin-left: auto; display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); cursor: pointer; }
.rows { max-height: 420px; overflow-y: auto; }
.row-item { cursor: pointer; }
.row-item.downloaded { opacity: 0.55; cursor: default; }
.check { flex-shrink: 0; cursor: pointer; accent-color: var(--accent); }
</style>
