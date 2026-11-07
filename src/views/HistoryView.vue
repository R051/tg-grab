<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { NButton, NDatePicker, NInput, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import type { HistoryPage, VideoInfo, WatchEntry } from "../types";
import AppIcon from "../components/AppIcon.vue";

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
const recents = ref<WatchEntry[]>([]);
const dateFilter = ref<number | null>(null); // ms timestamp of picked day
const thumbs = ref<Record<number, string>>({});
const thumbsLoading = ref(false);

interface DayGroup {
  key: string;
  label: string;
  videos: VideoInfo[];
}

const grouped = computed<DayGroup[]>(() => {
  const groups: DayGroup[] = [];
  const index = new Map<string, DayGroup>();
  for (const v of videos.value) {
    let day = "";
    if (v.date) {
      try {
        day = new Date(v.date).toLocaleDateString(locale.value, {
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
        });
      } catch {
        day = "";
      }
    }
    const key = day || "—";
    if (!index.has(key)) {
      const g: DayGroup = { key, label: key, videos: [] };
      index.set(key, g);
      groups.push(g);
    }
    index.get(key)!.videos.push(v);
  }
  return groups;
});

function fmtSize(bytes: number): string {
  if (!bytes) return "0 B";
  if (bytes < 1048576) return (bytes / 1024).toFixed(0) + " KB";
  if (bytes < 1073741824) return (bytes / 1048576).toFixed(1) + " MB";
  return (bytes / 1073741824).toFixed(2) + " GB";
}

function fmtDuration(sec: number): string {
  if (!sec) return "";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  return h > 0
    ? h + ":" + String(m).padStart(2, "0") + ":" + String(s).padStart(2, "0")
    : m + ":" + String(s).padStart(2, "0");
}

function pickedDay(): string | null {
  if (!dateFilter.value) return null;
  const d = new Date(dateFilter.value);
  const p = (n: number) => String(n).padStart(2, "0");
  return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate());
}

async function refreshRecents() {
  recents.value = await invoke<WatchEntry[]>("history_recent").catch(() => []);
}

function browseRecent(entry: WatchEntry) {
  link.value = entry.username ? "https://t.me/" + entry.username : "https://t.me/c/" + entry.raw_id;
  dateFilter.value = null;
  void browse(false);
}

async function removeRecent(entry: WatchEntry) {
  await invoke("history_recent_remove", { rawId: entry.raw_id }).catch(() => undefined);
  await refreshRecents();
}

async function browse(more: boolean) {
  if (!link.value.trim()) return;
  browsing.value = true;
  try {
    // Date filter only applies to the initial jump; "load more" continues
    // older-than-that pagination from where the day scan stopped.
    const page = await invoke<HistoryPage>("history_page", {
      link: link.value.trim(),
      offsetId: more ? nextOffset.value : 0,
      date: more ? null : pickedDay(),
    });
    chatTitle.value = page.chat.title;
    videos.value = more ? [...videos.value, ...page.videos] : page.videos;
    nextOffset.value = page.nextOffset;
    exhausted.value = page.exhausted;
    if (!more) {
      selected.value = new Set();
      thumbs.value = {};
      void refreshRecents();
    }
    void loadThumbs();
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

async function loadThumbs() {
  if (thumbsLoading.value) return;
  const need = videos.value
    .filter((v) => v.hasThumb && !thumbs.value[v.msgId])
    .map((v) => v.msgId);
  if (!need.length || !link.value.trim()) return;
  thumbsLoading.value = true;
  try {
    const list = await invoke<{ msgId: number; data: string }[]>("history_thumbs", {
      link: link.value.trim(),
      msgIds: need,
    });
    const next = { ...thumbs.value };
    for (const item of list) {
      next[item.msgId] = "data:image/jpeg;base64," + item.data;
    }
    thumbs.value = next;
  } catch {
    /* thumbs are decorative — ignore failures */
  } finally {
    thumbsLoading.value = false;
  }
}

function onGridScroll(ev: Event) {
  const el = ev.target as HTMLElement;
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
    if (!exhausted.value && !browsing.value) void browse(true);
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
  const all = videos.value.filter((v) => !v.downloaded);
  return all.length > 0 && all.every((v) => isSelected(v));
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
      <n-date-picker
        v-model:value="dateFilter"
        type="date"
        size="small"
        clearable
        :placeholder="t('history.pickDate')"
        style="width: 150px"
      />
      <n-button type="primary" size="small" :loading="browsing" @click="browse(false)">
        {{ t("history.browse") }}
      </n-button>
    </div>
    <div v-if="recents.length" class="recents">
      <span class="recents-label">{{ t("history.recent") }}</span>
      <span v-for="r in recents" :key="r.raw_id" class="recent-chip">
        <button class="chip-btn" :title="r.title" @click="browseRecent(r)">{{ r.title }}</button>
        <button class="chip-x" @click.stop="removeRecent(r)">✕</button>
      </span>
    </div>
  </div>

  <div v-if="videos.length" class="panel">
    <div class="panel-title">
      <span>{{ chatTitle }}</span>
      <span class="count">{{ videos.length }}</span>
      <n-text v-if="dateFilter" depth="3" style="font-size: 11.5px; margin-left: auto">
        {{ t("history.filteredDay", { day: pickedDay() ?? "" }) }}
      </n-text>
    </div>
    <div class="grid" @scroll="onGridScroll">
      <template v-for="g in grouped" :key="g.key">
        <div class="day-divider">{{ g.label }}</div>
        <div
          v-for="v in g.videos"
          :key="v.msgId"
          class="vcard"
          :class="{ downloaded: v.downloaded, selected: isSelected(v) }"
          @click="!v.downloaded && toggleSelect(v)"
        >
          <div class="thumb-wrap">
            <img
              v-if="thumbs[v.msgId]"
              class="thumb"
              :src="thumbs[v.msgId]"
              alt=""
              loading="lazy"
            />
            <span v-else class="thumb thumb-empty">{{ v.hasThumb ? "…" : "🎬" }}</span>
            <input
              type="checkbox"
              class="check"
              :checked="isSelected(v)"
              :disabled="v.downloaded"
              @click.stop
              @change="toggleSelect(v)"
            />
            <span v-if="v.duration" class="duration">{{ fmtDuration(v.duration) }}</span>
            <span v-if="v.downloaded" class="done-badge">✅ {{ t("history.downloaded") }}</span>
          </div>
          <div class="vname" :title="v.name">{{ v.name }}</div>
          <div class="vmeta">{{ fmtSize(v.size) }}</div>
        </div>
      </template>
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
.actions { margin-top: 14px; }
.selall { margin-left: auto; display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); cursor: pointer; }
.count { font-size: 12px; font-weight: 400; }

/* video card grid */
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 9px;
  max-height: 620px;
  overflow-y: auto;
  padding: 2px;
}
.day-divider {
  grid-column: 1 / -1;
  position: sticky; top: 0; z-index: 1;
  background: var(--surface);
  padding: 6px 2px 4px;
  font-size: 11.5px; font-weight: 600; color: var(--accent-bright);
  border-bottom: 1px solid var(--border-soft);
}
.vcard {
  background: var(--bg-inset);
  border: 1px solid var(--border-soft);
  border-radius: 10px;
  overflow: hidden;
  cursor: pointer;
  transition: all 0.12s;
  display: flex; flex-direction: column;
}
.vcard:hover { border-color: #3d444d; transform: translateY(-1px); }
.vcard.selected { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
.vcard.downloaded { opacity: 0.55; cursor: default; }
.thumb-wrap { position: relative; aspect-ratio: 16 / 9; background: var(--surface-2); }
.thumb { width: 100%; height: 100%; object-fit: cover; display: block; }
.thumb-empty {
  display: flex; align-items: center; justify-content: center;
  font-size: 22px; color: var(--text-faint);
}
.check {
  position: absolute; top: 6px; left: 6px; z-index: 2;
  width: 16px; height: 16px; cursor: pointer; accent-color: var(--accent);
}
.duration {
  position: absolute; right: 6px; bottom: 6px;
  background: rgba(0, 0, 0, 0.75); color: #fff;
  font-size: 10.5px; font-weight: 600;
  padding: 1px 6px; border-radius: 4px;
}
.done-badge {
  position: absolute; left: 6px; bottom: 6px;
  background: rgba(0, 0, 0, 0.75); color: var(--accent-bright);
  font-size: 10.5px; font-weight: 600;
  padding: 1px 6px; border-radius: 4px;
}
.vname {
  font-size: 11.5px; color: var(--text);
  padding: 6px 7px 0;
  display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical;
  overflow: hidden; line-height: 1.35;
  word-break: break-all; white-space: nowrap; text-overflow: ellipsis;
  -webkit-line-clamp: unset;
}
.vmeta { font-size: 10.5px; color: var(--text-dim); padding: 2px 7px 7px; }

.recents { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-top: 10px; }
.recents-label { font-size: 11.5px; color: var(--text-faint); }
.recent-chip { display: inline-flex; align-items: center; background: var(--surface-2); border-radius: 999px; overflow: hidden; }
.chip-btn {
  background: none; border: 0; color: var(--text-dim); font-size: 12px;
  padding: 4px 6px 4px 10px; cursor: pointer; font-family: inherit;
  max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.chip-btn:hover { color: var(--accent-bright); }
.chip-x {
  background: none; border: 0; color: var(--text-faint); font-size: 10px;
  padding: 4px 8px 4px 2px; cursor: pointer;
}
.chip-x:hover { color: var(--red); }
</style>
