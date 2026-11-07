<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "../lib/backend";
import { errorText } from "../lib/errors";
import { NButton, NDatePicker, NInput, NText, useMessage } from "naive-ui";
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
const chatRawId = ref(0);
const videos = ref<VideoInfo[]>([]);
const exhausted = ref(false);
const nextOffset = ref(0);
const nextDate = ref<string | null>(null);
const queryDate = ref<string | null>(null);
const selected = ref<Set<string>>(new Set());
const recents = ref<WatchEntry[]>([]);
const dateFilter = ref<number | null>(null);
const thumbs = ref<Record<string, string>>({});
const kindFilter = ref("all");
const thumbsLoading = ref(false);
const KIND_FILTERS = [
  { key: "all", label: "全部", icon: "📋" },
  { key: "video", label: "视频", icon: "🎬" },
  { key: "photo", label: "图片", icon: "🖼" },
  { key: "audio", label: "音频", icon: "🎵" },
  { key: "document", label: "文件", icon: "📄" },
] as const;
type Query = { link: string; date: string | null; dayStart?: number; dayEnd?: number };
let currentQuery: Query | null = null;
let browseSeq = 0;
let thumbSeq = 0;
onUnmounted(() => { browseSeq++; thumbSeq++; });
void refreshRecents();

const grouped = computed(() => {
  const groups = new Map<string, { key: string; videos: VideoInfo[] }>();
  for (const video of videos.value) {
    const date = video.date ? new Date(video.date) : null;
    const key = date && !isNaN(date.getTime()) ? date.toLocaleDateString(locale.value,
      { year: "numeric", month: "2-digit", day: "2-digit" }) : "—";
    if (!groups.has(key)) groups.set(key, { key, videos: [] });
    groups.get(key)!.videos.push(video);
  }
  return [...groups.values()];
});
const taskStates = computed(() => {
  const states = new Map<string, "queued" | "downloading" | "success">();
  for (const result of store.results) {
    if (result.status === "success" && store.files.some((file) => file.name === (result.path ?? result.name))) {
      states.set(result.url, "success");
    }
  }
  for (const pending of store.pending) states.set(pending.url, "queued");
  for (const active of store.activeTasks) states.set(active.url, "downloading");
  return states;
});
function videoState(video: VideoInfo) { return taskStates.value.get(video.url) ?? (video.downloaded ? "success" : null); }
function selectable(video: VideoInfo) { return !videoState(video); }
function fmtDuration(seconds: number): string {
  if (!seconds) return "";
  const hours = Math.floor(seconds / 3600), minutes = Math.floor((seconds % 3600) / 60);
  return (hours ? `${hours}:${String(minutes).padStart(2, "0")}` : String(minutes)) + `:${String(seconds % 60).padStart(2, "0")}`;
}
function pickedDay(): string | null {
  if (dateFilter.value === null) return null;
  const date = new Date(dateFilter.value);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}
function freezeQuery(): Query {
  const date = pickedDay();
  if (dateFilter.value === null) return { link: link.value.trim(), date };
  const chosen = new Date(dateFilter.value);
  const year = chosen.getFullYear(), month = chosen.getMonth(), day = chosen.getDate();
  return { link: link.value.trim(), date,
    dayStart: new Date(year, month, day).getTime() / 1000,
    dayEnd: new Date(year, month, day + 1).getTime() / 1000 };
}
async function refreshRecents() { recents.value = await invoke<WatchEntry[]>("history_recent").catch(() => []); }
function browseRecent(entry: WatchEntry) {
  link.value = entry.username ? `https://t.me/${entry.username}` : `https://t.me/c/${entry.raw_id}`;
  dateFilter.value = null;
  void browse();
}
async function removeRecent(entry: WatchEntry) {
  try { await invoke("history_recent_remove", { rawId: entry.raw_id }); await refreshRecents(); }
  catch (error) { message.error(errorText(error, t)); }
}
function applyCursor(page: HistoryPage) {
  nextOffset.value = page.nextOffset;
  nextDate.value = page.nextDate;
  exhausted.value = page.exhausted;
}
async function browse() {
  if (!link.value.trim()) return;
  const sequence = ++browseSeq;
  thumbSeq++;
  const query = freezeQuery();
  currentQuery = null;
  queryDate.value = query.date;
  videos.value = []; selected.value = new Set(); chatTitle.value = "";
  browsing.value = true;
  try {
    const page = await invoke<HistoryPage>("history_page", { ...query, offsetId: 0 });
    if (sequence !== browseSeq) return;
    currentQuery = query;
    chatTitle.value = page.chat.title;
    chatRawId.value = page.chat.rawId;
    videos.value = page.videos;
    applyCursor(page);
    void refreshRecents(); void loadThumbs();
  } catch (error) { if (sequence === browseSeq) message.error(errorText(error, t)); }
  finally { if (sequence === browseSeq) browsing.value = false; }
}
// Follow the server cursor, including pages containing only non-video messages.
async function loadPrevDay() {
  if (browsing.value || exhausted.value || !currentQuery) return;
  const sequence = ++browseSeq;
  const query: Query = { ...currentQuery, date: nextDate.value };
  if (!query.date) { delete query.dayStart; delete query.dayEnd; }
  const offset = nextOffset.value;
  browsing.value = true;
  try {
    const page = await invoke<HistoryPage>("history_page", { ...query, offsetId: offset });
    if (sequence !== browseSeq) return;
    const seen = new Set(videos.value.map((video) => video.msgId));
    videos.value.push(...page.videos.filter((video) => !seen.has(video.msgId)));
    currentQuery = query;
    applyCursor(page);
    void loadThumbs();
  } catch (error) { if (sequence === browseSeq) message.error(errorText(error, t)); }
  finally { if (sequence === browseSeq) browsing.value = false; }
}
async function loadThumbs() {
  if (!currentQuery) return;
  const sequence = ++thumbSeq, generation = browseSeq;
  const query = currentQuery, rawId = chatRawId.value;
  const need = videos.value.slice(-300).filter((video) => video.hasThumb && !thumbs.value[`${rawId}_${video.msgId}`]).map((video) => video.msgId);
  if (!need.length) { thumbsLoading.value = false; return; }
  thumbsLoading.value = true;
  try {
    for (let index = 0; index < need.length; index += 50) {
      const list = await invoke<{ msgId: number; data: string }[]>("history_thumbs", {
        link: query.link, msgIds: need.slice(index, index + 50),
      });
      if (sequence !== thumbSeq || generation !== browseSeq) return;
      const next = { ...thumbs.value };
      for (const item of list) next[`${rawId}_${item.msgId}`] = `data:image/jpeg;base64,${item.data}`;
      for (const key of Object.keys(next).slice(0, Math.max(0, Object.keys(next).length - 300))) delete next[key];
      thumbs.value = next;
    }
  } catch { /* Individual thumbnails can be retried on the next page. */ }
  finally { if (sequence === thumbSeq) thumbsLoading.value = false; }
}
function thumbKey(video: VideoInfo) { return `${chatRawId.value}_${video.msgId}`; }
function isSelected(video: VideoInfo) { return selectable(video) && selected.value.has(video.url); }
function toggleSelect(video: VideoInfo) {
  if (!selectable(video)) return;
  const next = new Set(selected.value);
  if (next.has(video.url)) next.delete(video.url); else next.add(video.url);
  selected.value = next;
}
const selectableCount = computed(() => videos.value.filter(selectable).length);
const selectedCount = computed(() => videos.value.filter(isSelected).length);
const pageAllSelected = computed(() => selectableCount.value > 0 && videos.value.filter(selectable).every(isSelected));
function toggleSelectAll(event: Event) {
  const checked = (event.target as HTMLInputElement).checked;
  selected.value = new Set(checked ? videos.value.filter(selectable).map((video) => video.url) : []);
}
async function downloadSelected() {
  if (downloading.value) return;
  const urls = [...new Set(videos.value.filter(isSelected).map((video) => video.url))];
  if (!urls.length) return;
  const generation = browseSeq;
  downloading.value = true;
  try {
    const ids = await invoke<number[]>("download_urls", { urls });
    message.success(t("history.added", { n: ids.length }));
    if (generation === browseSeq) selected.value = new Set([...selected.value].filter((url) => !urls.includes(url)));
    await store.refreshQueue();
  } catch (error) { message.error(errorText(error, t)); }
  finally { downloading.value = false; }
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
        @keyup.enter="browse()"
      />
      <n-date-picker
        v-model:value="dateFilter"
        type="date"
        size="small"
        clearable
        :placeholder="t('history.pickDate')"
        style="width: 150px"
      />
      <n-button type="primary" size="small" :loading="browsing" @click="browse()">
        {{ t("history.browse") }}
      </n-button>
    </div>
    <div v-if="videos.length" class="kind-chips">
      <button
        v-for="kf in KIND_FILTERS"
        :key="kf.key"
        class="chip-filter"
        :class="{ on: kindFilter === kf.key }"
        @click="kindFilter = kf.key"
      >
        {{ kf.icon }} {{ kf.label }}
      </button>
    </div>
    <div v-if="recents.length" class="recents">
      <span class="recents-label">{{ t("history.recent") }}</span>
      <span v-for="r in recents" :key="r.raw_id" class="recent-chip">
        <button class="chip-btn" :title="r.title" @click="browseRecent(r)">{{ r.title }}</button>
        <button class="chip-x" @click.stop="removeRecent(r)">✕</button>
      </span>
    </div>
  </div>

  <div v-if="chatTitle" class="panel">
    <div class="panel-title">
      <span>{{ chatTitle }}</span>
      <span class="count">{{ videos.length }}</span>
      <n-text v-if="queryDate" depth="3" style="font-size: 11.5px; margin-left: auto">
        {{ t("history.filteredDay", { day: pickedDay() ?? "" }) }}
      </n-text>
    </div>
    <div v-if="!videos.length" class="empty">{{ exhausted ? t("history.empty") : t("history.noVideosOnPage") }}</div>
    <div class="grid">
      <template v-for="g in grouped" :key="g.key">
        <div class="day-divider">{{ g.key }}</div>
        <div
          v-for="v in g.videos"
          :key="v.msgId"
          class="vcard"
          :class="{ downloaded: videoState(v) === 'success', selected: isSelected(v) }"
          @click="toggleSelect(v)" role="button" :tabindex="selectable(v) ? 0 : -1" :aria-label="v.name" :aria-disabled="!selectable(v)" @keydown.enter.prevent="toggleSelect(v)" @keydown.space.prevent="toggleSelect(v)"
        >
          <div class="thumb-wrap">
            <img
              v-if="thumbs[thumbKey(v)]"
              class="thumb"
              :src="thumbs[thumbKey(v)]"
              alt=""
              loading="lazy"
            />
            <span v-else class="thumb thumb-empty">{{ v.hasThumb ? "…" : "🎬" }}</span>
            <input
              type="checkbox"
              class="check"
              :checked="isSelected(v)"
              :disabled="!selectable(v)"
              @click.stop
              @change="toggleSelect(v)"
            />
            <span v-if="v.duration" class="duration">{{ fmtDuration(v.duration) }}</span>
            <span v-if="videoState(v) === 'success'" class="done-badge">✅</span>
          </div>
          <div class="vname" :title="v.name">{{ v.name }}</div>
          <span v-if="videoState(v) && videoState(v) !== 'success'" class="pill" :class="videoState(v)">{{ t(`download.status.${videoState(v)}`) }}</span>
        </div>
      </template>
    </div>
    <div class="row actions">
      <n-button
        type="primary"
        size="small"
        :disabled="!selectedCount"
        :loading="downloading"
        @click="downloadSelected"
      >
        {{ t("history.downloadSelected", { n: selectedCount }) }}
      </n-button>
      <n-button v-if="!exhausted" quaternary size="small" :loading="browsing" @click="loadPrevDay">
        {{ t("history.loadMore") }}
      </n-button>
      <label class="selall">
        <input
          type="checkbox"
          :checked="pageAllSelected"
          :disabled="selectableCount === 0"
          @change="toggleSelectAll"
        />
        {{ t("history.selectAll") }}
      </label>
    </div>
  </div>


</template>

<style scoped>
.row { display: flex; align-items: center; gap: 10px; }
.actions { margin-top: 14px; }
.selall { margin-left: auto; display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); cursor: pointer; }
.count { font-size: 12px; font-weight: 400; }

/* video card grid — fixed-size cards, thumbs crop to fit */
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  gap: 7px;
  align-items: start;
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
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
  transition: all 0.12s;
}
.vcard:hover { border-color: #3d444d; }
.vcard.selected { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
.vcard.downloaded { opacity: 0.55; cursor: default; }
.thumb-wrap { position: relative; aspect-ratio: 16 / 9; background: var(--surface-2); }
.thumb { width: 100%; height: 100%; object-fit: cover; display: block; }
.thumb-empty {
  display: flex; align-items: center; justify-content: center;
  font-size: 18px; color: var(--text-faint);
}
.check {
  position: absolute; top: 4px; left: 4px; z-index: 2;
  width: 13px; height: 13px; cursor: pointer; accent-color: var(--accent);
}
.duration {
  position: absolute; right: 4px; bottom: 4px;
  background: rgba(0, 0, 0, 0.75); color: #fff;
  font-size: 9px; font-weight: 600;
  padding: 0 4px; border-radius: 3px;
}
.done-badge {
  position: absolute; left: 20px; bottom: 4px;
  background: rgba(0, 0, 0, 0.75);
  font-size: 9px;
  padding: 0 4px; border-radius: 3px;
}
.vname {
  font-size: 10.5px; color: var(--text);
  padding: 5px 6px 6px;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

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
