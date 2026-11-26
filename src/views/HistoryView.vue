<script setup lang="ts">
import { computed, onUnmounted, ref, watch, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "../lib/backend";
import { errorText } from "../lib/errors";
import { NButton, NDatePicker, NInput, NSelect, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import type {
  HistoryPage,
  VideoInfo,
  FavoriteDraft,
  MediaDetails,
} from "../types";
import AppIcon from "../components/AppIcon.vue";
import ChannelPicker from "../components/ChannelPicker.vue";
import FavoriteDialog from "../components/FavoriteDialog.vue";
import MediaPreview from "../components/MediaPreview.vue";
import {
  findFavorite,
  formatSize,
  formatTime,
  mediaIcon,
  localPathFor,
} from "../lib/library";

const { t, locale } = useI18n();
const store = useAppStore();
const message = useMessage();
const link = ref("");
const linkInput = ref<{ focus: () => void } | null>(null);
const favorite = ref<FavoriteDraft | null>(null);
const preview = ref<MediaDetails | null>(null);
const channelUrl = ref("");
const search = ref("");
const browsing = ref(false);
const downloading = ref(false);
const chatTitle = ref("");
const chatRawId = ref(0);
const browsedRoot = ref("");
const videos = ref<VideoInfo[]>([]);
const exhausted = ref(false);
const nextOffset = ref(0);
const nextDate = ref<string | null>(null);
const queryDate = ref<string | null>(null);
const selected = ref<Set<string>>(new Set());
const dateFilter = ref<number | null>(null);
const thumbs = ref<Record<string, string>>({});
const kindFilter = ref("all");
const thumbsLoading = ref(false);
const KIND_FILTERS = computed(() =>
  ["all", "video", "photo", "audio", "document"].map((key) => ({
    key,
    label: t(`media.${key}`),
  })),
);
const visibleVideos = computed(() => videos.value);
const kindOptions = computed(() =>
  KIND_FILTERS.value.map((item) => ({ label: item.label, value: item.key })),
);
watch([kindFilter, search], () => {
  selected.value = new Set();
});
type Query = {
  link: string;
  date: string | null;
  query: string;
  kind: string;
  dayStart?: number;
  dayEnd?: number;
};
let currentQuery: Query | null = null;
let browseSeq = 0;
let thumbSeq = 0;
onUnmounted(() => {
  browseSeq++;
  thumbSeq++;
});


type DayEntry =
  | {
      kind: "group";
      key: string;
      groupId: string;
      caption: string;
      time: string;
      totalSize: number;
      videos: VideoInfo[];
    }
  | {
      kind: "single";
      key: string;
      video: VideoInfo;
    };

function postTimeLabel(dateStr?: string | null): string {
  if (!dateStr) return "";
  const ms = Date.parse(dateStr);
  if (Number.isNaN(ms)) return "";
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}` || formatTime(ms);
}

const grouped = computed(() => {
  const days = new Map<string, { key: string; raw: VideoInfo[] }>();
  for (const video of visibleVideos.value) {
    const date = video.date ? new Date(video.date) : null;
    const key =
      date && !isNaN(date.getTime())
        ? date.toLocaleDateString(locale.value, {
            year: "numeric",
            month: "2-digit",
            day: "2-digit",
          })
        : "—";
    if (!days.has(key)) days.set(key, { key, raw: [] });
    days.get(key)!.raw.push(video);
  }
  return [...days.values()].map((day) => {
    const byGroup = new Map<string, VideoInfo[]>();
    for (const v of day.raw) {
      if (v.groupedId) {
        const list = byGroup.get(v.groupedId);
        if (list) list.push(v);
        else byGroup.set(v.groupedId, [v]);
      }
    }
    const emittedGroups = new Set<string>();
    const entries: DayEntry[] = [];
    const orderedVideos: VideoInfo[] = [];
    for (const v of day.raw) {
      const gid = v.groupedId;
      if (gid && (byGroup.get(gid)?.length ?? 0) >= 2) {
        if (emittedGroups.has(gid)) continue;
        emittedGroups.add(gid);
        const members = [...byGroup.get(gid)!].sort((a, b) => a.msgId - b.msgId);
        const caption =
          members.map((item) => item.text?.trim() ?? "").find(Boolean) ?? "";
        const totalSize = members.reduce((sum, item) => sum + (item.size ?? 0), 0);
        const time = postTimeLabel(members[0]?.date);
        entries.push({
          kind: "group",
          key: `g:${gid}`,
          groupId: gid,
          caption,
          time,
          totalSize,
          videos: members,
        });
        orderedVideos.push(...members);
      } else {
        entries.push({
          kind: "single",
          key: `s:${v.msgId}`,
          video: v,
        });
        orderedVideos.push(v);
      }
    }
    return {
      key: day.key,
      videos: orderedVideos,
      entries,
    };
  });
});

const flatDisplayVideos = computed(() =>
  grouped.value.flatMap((day) => day.videos),
);
const taskStates = computed(() => {
  const states = new Map<string, "queued" | "paused" | "downloading" | "success">();
  for (const result of store.results) {
    if (
      result.status === "success" &&
      store.files.some((file) => file.name === (result.path ?? result.name))
    ) {
      states.set(result.url, "success");
    }
  }
  for (const paused of store.paused ?? []) states.set(paused.url, "paused");
  for (const pending of store.pending) states.set(pending.url, "queued");
  for (const active of store.activeTasks) states.set(active.url, "downloading");
  return states;
});
function videoState(video: VideoInfo) {
  return (
    taskStates.value.get(video.url) ??
    (localPathFor(details(video), store.files, store.settings)
      ? "success"
      : null)
  );
}
function selectable(video: VideoInfo) {
  return !videoState(video);
}
function fmtDuration(seconds: number): string {
  if (!seconds) return "";
  const hours = Math.floor(seconds / 3600),
    minutes = Math.floor((seconds % 3600) / 60);
  return (
    (hours ? `${hours}:${String(minutes).padStart(2, "0")}` : String(minutes)) +
    `:${String(seconds % 60).padStart(2, "0")}`
  );
}
function pickedDay(): string | null {
  if (dateFilter.value === null) return null;
  const date = new Date(dateFilter.value);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}
function freezeQuery(): Query {
  const date = pickedDay();
  if (dateFilter.value === null)
    return {
      link: link.value.trim(),
      date,
      query: search.value.trim(),
      kind: kindFilter.value,
    };
  const chosen = new Date(dateFilter.value);
  const year = chosen.getFullYear(),
    month = chosen.getMonth(),
    day = chosen.getDate();
  return {
    link: link.value.trim(),
    date,
    query: search.value.trim(),
    kind: kindFilter.value,
    dayStart: new Date(year, month, day).getTime() / 1000,
    dayEnd: new Date(year, month, day + 1).getTime() / 1000,
  };
}
function requestPage(query: Query, offsetId: number) {
  const command =
    query.query || query.kind !== "all" || query.date
      ? "history_search"
      : "history_page";
  return invoke<HistoryPage>(command, { ...query, offsetId });
}
function chooseChannel(value: string) {
  link.value = value;
  void browse();
}
function applyCursor(page: HistoryPage) {
  nextOffset.value = page.nextOffset;
  nextDate.value = page.nextDate;
  exhausted.value = page.exhausted;
}
async function browse() {
  if (!link.value.trim() || !store.authorized) return;
  const sequence = ++browseSeq;
  thumbSeq++;
  const query = freezeQuery();
  const requestedRoot = store.settings?.effective_downloads_dir ?? "";
  currentQuery = null;
  queryDate.value = query.date;
  videos.value = [];
  selected.value = new Set();
  lastClickedUrl.value = null;
  chatTitle.value = "";
  browsing.value = true;
  try {
    const page = await requestPage(query, 0);
    if (sequence !== browseSeq) return;
    currentQuery = query;
    chatTitle.value = page.chat.title;
    chatRawId.value = page.chat.rawId;
    browsedRoot.value = requestedRoot;
    channelUrl.value = page.chat.username
      ? `https://t.me/${page.chat.username}`
      : `https://t.me/c/${page.chat.rawId}`;
    videos.value = page.videos;
    applyCursor(page);

    void loadThumbs();
  } catch (error) {
    if (sequence === browseSeq) message.error(errorText(error, t));
  } finally {
    if (sequence === browseSeq) browsing.value = false;
  }
}
// Follow the server cursor, including pages containing only non-video messages.
async function loadPrevDay() {
  if (browsing.value || exhausted.value || !currentQuery) return;
  const sequence = ++browseSeq;
  const query: Query = { ...currentQuery, date: nextDate.value };
  if (currentQuery.query || currentQuery.kind !== "all" || currentQuery.date) {
    query.date = currentQuery.date;
  }
  if (!query.date) {
    delete query.dayStart;
    delete query.dayEnd;
  }
  const offset = nextOffset.value;
  browsing.value = true;
  try {
    const page = await requestPage(query, offset);
    if (sequence !== browseSeq) return;
    const seen = new Set(videos.value.map((video) => video.msgId));
    videos.value.push(...page.videos.filter((video) => !seen.has(video.msgId)));
    currentQuery = query;
    applyCursor(page);
    void loadThumbs();
  } catch (error) {
    if (sequence === browseSeq) message.error(errorText(error, t));
  } finally {
    if (sequence === browseSeq) browsing.value = false;
  }
}
async function loadThumbs() {
  if (!currentQuery) return;
  const sequence = ++thumbSeq,
    generation = browseSeq;
  const query = currentQuery,
    rawId = chatRawId.value;
  const need = videos.value
    .slice(-300)
    .filter(
      (video) => video.hasThumb && !thumbs.value[`${rawId}_${video.msgId}`],
    )
    .map((video) => video.msgId);
  if (!need.length) {
    thumbsLoading.value = false;
    return;
  }
  thumbsLoading.value = true;
  try {
    for (let index = 0; index < need.length; index += 50) {
      const list = await invoke<{ msgId: number; data: string }[]>(
        "history_thumbs",
        {
          link: query.link,
          msgIds: need.slice(index, index + 50),
        },
      );
      if (sequence !== thumbSeq || generation !== browseSeq) return;
      const next = { ...thumbs.value };
      for (const item of list)
        next[`${rawId}_${item.msgId}`] = `data:image/jpeg;base64,${item.data}`;
      for (const key of Object.keys(next).slice(
        0,
        Math.max(0, Object.keys(next).length - 300),
      ))
        delete next[key];
      thumbs.value = next;
    }
  } catch {
    /* Individual thumbnails can be retried on the next page. */
  } finally {
    if (sequence === thumbSeq) thumbsLoading.value = false;
  }
}
function thumbKey(video: VideoInfo) {
  return `${chatRawId.value}_${video.msgId}`;
}
const lastClickedUrl = ref<string | null>(null);
function isSelected(video: VideoInfo) {
  return selectable(video) && selected.value.has(video.url);
}
function selectableIn(items: VideoInfo[]) {
  return items.filter(selectable);
}
function isGroupAllSelected(items: VideoInfo[]) {
  const list = selectableIn(items);
  return list.length > 0 && list.every(isSelected);
}
function isGroupPartiallySelected(items: VideoInfo[]) {
  const list = selectableIn(items);
  const count = list.filter(isSelected).length;
  return count > 0 && count < list.length;
}
function toggleGroupSelect(items: VideoInfo[]) {
  const list = selectableIn(items);
  if (!list.length) return;
  const selectAll = !list.every(isSelected);
  const next = new Set(selected.value);
  for (const video of list) {
    if (selectAll) next.add(video.url);
    else next.delete(video.url);
  }
  selected.value = next;
  lastClickedUrl.value = list[list.length - 1]?.url ?? null;
}
function toggleSelect(video: VideoInfo, event?: Event) {
  if (!selectable(video)) return;
  const shift = Boolean((event as MouseEvent | undefined)?.shiftKey);
  const targetChecked = !isSelected(video);
  const next = new Set(selected.value);
  if (shift && lastClickedUrl.value) {
    const list = flatDisplayVideos.value;
    const from = list.findIndex((item) => item.url === lastClickedUrl.value);
    const to = list.findIndex((item) => item.url === video.url);
    if (from >= 0 && to >= 0) {
      const start = Math.min(from, to);
      const end = Math.max(from, to);
      for (const item of list.slice(start, end + 1)) {
        if (!selectable(item)) continue;
        if (targetChecked) next.add(item.url);
        else next.delete(item.url);
      }
      selected.value = next;
      lastClickedUrl.value = video.url;
      return;
    }
  }
  if (targetChecked) next.add(video.url);
  else next.delete(video.url);
  selected.value = next;
  lastClickedUrl.value = video.url;
}
const selectableCount = computed(
  () => visibleVideos.value.filter(selectable).length,
);
const selectedCount = computed(() => videos.value.filter(isSelected).length);
const pageAllSelected = computed(
  () =>
    selectableCount.value > 0 &&
    visibleVideos.value.filter(selectable).every(isSelected),
);
function toggleSelectAll(event: Event) {
  const checked = (event.target as HTMLInputElement).checked;
  selected.value = new Set(
    checked
      ? visibleVideos.value.filter(selectable).map((video) => video.url)
      : [],
  );
}
async function downloadSelected() {
  if (downloading.value) return;
  const urls = [
    ...new Set(
      visibleVideos.value.filter(isSelected).map((video) => video.url),
    ),
  ];
  if (!urls.length) return;
  const generation = browseSeq;
  downloading.value = true;
  try {
    const ids = await invoke<number[]>("download_urls", { urls });
    message.success(t("history.added", { n: ids.length }));
    if (generation === browseSeq)
      selected.value = new Set(
        [...selected.value].filter((url) => !urls.includes(url)),
      );
    await store.refreshQueue();
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    downloading.value = false;
  }
}

function details(video: VideoInfo): MediaDetails {
  return {
    kind: video.kind ?? "video",
    title: video.name,
    url: video.url,
    localPath: video.localPath,
    localRoot: browsedRoot.value,
    sourceTitle: chatTitle.value,
    channelId: chatRawId.value,
    msgId: video.msgId,
    size: video.size,
    date: video.date,
    thumbnail: thumbs.value[thumbKey(video)],
  };
}
function channelFavorite(): FavoriteDraft {
  return {
    kind: "channel",
    title: chatTitle.value,
    url: channelUrl.value,
    channelId: chatRawId.value,
  };
}
function stopLoading() {
  browseSeq++;
  thumbSeq++;
  browsing.value = false;
  thumbsLoading.value = false;
}
async function closeBrowse() {
  stopLoading();
  currentQuery = null;
  videos.value = [];
  selected.value = new Set();
  lastClickedUrl.value = null;
  thumbs.value = {};
  chatTitle.value = "";
  chatRawId.value = 0;
  nextOffset.value = 0;
  nextDate.value = null;
  queryDate.value = null;
  dateFilter.value = null;
  kindFilter.value = "all";
  search.value = "";
  link.value = "";
  preview.value = null;
  await nextTick();
  linkInput.value?.focus();
}
watch(
  () => store.channelRequest,
  (request) => {
    if (request) {
      link.value = request.link;
      dateFilter.value = null;
      if (store.authorized) void browse();
    }
  },
  { immediate: true },
);
</script>

<template>
  <section class="panel browse-controls">
    <ChannelPicker
      v-model:value="link"
      @selected="chooseChannel"
      @clear="closeBrowse"
    />
    <div class="history-query">
      <n-input
        v-model:value="search"
        clearable
        :placeholder="t('simple.keyword')"
        :aria-label="t('simple.keyword')"
        @keyup.enter="browse"
      />
      <n-select
        v-model:value="kindFilter"
        :options="kindOptions"
        :aria-label="t('library.type')"
      />
      <n-date-picker
        v-model:value="dateFilter"
        type="date"
        clearable
        :placeholder="t('history.pickDate')"
        :aria-label="t('history.pickDate')"
      />
      <n-button
        type="primary"
        :loading="browsing"
        :disabled="!link.trim() || !store.authorized"
        @click="browse"
        >{{ t("simple.search") }}</n-button
      >
    </div>
  </section>
  <section class="panel browse-results">
    <div class="results-heading">
      <strong>{{ chatTitle || t("history.title") }}</strong
      ><span class="muted">{{ t("simple.count", { n: videos.length }) }}</span
      ><span class="collection-spacer" />
      <template v-if="chatTitle"
        ><button
          class="icon-button"
          :title="t('history.saveChannel')"
          @click="favorite = channelFavorite()"
        >
          <AppIcon name="star" /></button
        ><n-button size="small" @click="closeBrowse">{{
          t("history.close")
        }}</n-button></template
      >
      <n-button v-if="browsing" size="small" @click="stopLoading">{{
        t("history.stop")
      }}</n-button>
    </div>
    <div class="results-scroll">
      <div v-if="!chatTitle && !browsing" class="empty">
        <AppIcon name="channel" :size="30" />
        <p>
          {{
            store.authorized ? t("simple.selectChannel") : t("simple.loginHint")
          }}
        </p>
      </div>
      <div v-else-if="!videos.length && !browsing" class="empty">
        {{ exhausted ? t("simple.searchEmpty") : t("history.noVideosOnPage") }}
      </div>
      <div class="grid">
        <template v-for="g in grouped" :key="g.key"
          ><div class="day-divider">
            <span>{{ g.key }}</span>
            <label
              v-if="selectableIn(g.videos).length > 1"
              class="day-select"
            >
              <input
                type="checkbox"
                :checked="isGroupAllSelected(g.videos)"
                :indeterminate.prop="isGroupPartiallySelected(g.videos)"
                @change="toggleGroupSelect(g.videos)"
              />
              <span>{{ t("history.selectDay") }}</span>
            </label>
          </div>
          <template v-for="entry in g.entries" :key="entry.key">
            <section
              v-if="entry.kind === 'group'"
              class="media-group"
              :class="{ 'is-selected': isGroupAllSelected(entry.videos) }"
            >
              <header class="media-group-header">
                <div class="media-group-top">
                  <label class="group-check">
                    <input
                      type="checkbox"
                      :checked="isGroupAllSelected(entry.videos)"
                      :indeterminate.prop="isGroupPartiallySelected(entry.videos)"
                      :disabled="!selectableIn(entry.videos).length"
                      @change="toggleGroupSelect(entry.videos)"
                    />
                    <span>{{ t("history.selectGroup") }}</span>
                  </label>
                  <span class="media-group-meta">
                    {{ t("history.groupLabel", { n: entry.videos.length }) }}
                    <template v-if="entry.totalSize">
                      · {{ formatSize(entry.totalSize) }}
                    </template>
                    <template v-if="entry.time"> · {{ entry.time }}</template>
                  </span>
                </div>
                <p
                  v-if="entry.caption"
                  class="media-group-caption"
                  :title="entry.caption"
                >
                  {{ entry.caption }}
                </p>
              </header>
              <div class="media-group-grid">
                <article
                  v-for="v in entry.videos"
                  :key="v.msgId"
                  class="vcard"
                  :class="{
                    downloaded: videoState(v) === 'success',
                    selected: isSelected(v),
                  }"
                >
                  <button
                    class="vcard-preview"
                    :aria-label="t('history.preview', { name: v.name })"
                    @click="preview = details(v)"
                  >
                    <span class="thumb-wrap"
                      ><img
                        v-if="thumbs[thumbKey(v)]"
                        class="thumb"
                        :src="thumbs[thumbKey(v)]"
                        alt=""
                        loading="lazy"
                      /><span v-else class="thumb thumb-empty"
                        ><AppIcon :name="mediaIcon(v.kind)" :size="24" /></span
                      ><span v-if="v.duration" class="duration">{{
                        fmtDuration(v.duration)
                      }}</span></span
                    ><span class="vname" :title="v.name">{{ v.name }}</span
                    ><span
                      v-if="v.text && v.text.trim() !== entry.caption"
                      class="caption"
                      :title="v.text"
                      >{{ v.text }}</span
                    >
                  </button>
                  <input
                    type="checkbox"
                    class="check"
                    :aria-label="t('history.selectItem', { name: v.name })"
                    :checked="isSelected(v)"
                    :disabled="!selectable(v)"
                    @click.prevent="toggleSelect(v, $event)"
                  />
                  <button
                    class="card-save"
                    :class="{
                      saved: findFavorite(store.library.items, details(v)),
                    }"
                    :title="t('library.save')"
                    @click="favorite = details(v)"
                  >
                    <AppIcon name="star" :size="14" />
                  </button>
                  <span
                    v-if="videoState(v)"
                    class="pill"
                    :class="videoState(v)"
                    >{{ t(`download.status.${videoState(v)}`) }}</span
                  >
                </article>
              </div>
            </section>
            <article
              v-else
              class="vcard"
              :class="{
                downloaded: videoState(entry.video) === 'success',
                selected: isSelected(entry.video),
              }"
            >
              <button
                class="vcard-preview"
                :aria-label="t('history.preview', { name: entry.video.name })"
                @click="preview = details(entry.video)"
              >
                <span class="thumb-wrap"
                  ><img
                    v-if="thumbs[thumbKey(entry.video)]"
                    class="thumb"
                    :src="thumbs[thumbKey(entry.video)]"
                    alt=""
                    loading="lazy"
                  /><span v-else class="thumb thumb-empty"
                    ><AppIcon
                      :name="mediaIcon(entry.video.kind)"
                      :size="24" /></span
                  ><span v-if="entry.video.duration" class="duration">{{
                    fmtDuration(entry.video.duration)
                  }}</span></span
                ><span class="vname" :title="entry.video.name">{{
                  entry.video.name
                }}</span
                ><span
                  v-if="entry.video.text"
                  class="caption"
                  :title="entry.video.text"
                  >{{ entry.video.text }}</span
                >
              </button>
              <input
                type="checkbox"
                class="check"
                :aria-label="t('history.selectItem', { name: entry.video.name })"
                :checked="isSelected(entry.video)"
                :disabled="!selectable(entry.video)"
                @click.prevent="toggleSelect(entry.video, $event)"
              />
              <button
                class="card-save"
                :class="{
                  saved: findFavorite(
                    store.library.items,
                    details(entry.video),
                  ),
                }"
                :title="t('library.save')"
                @click="favorite = details(entry.video)"
              >
                <AppIcon name="star" :size="14" />
              </button>
              <span
                v-if="videoState(entry.video)"
                class="pill"
                :class="videoState(entry.video)"
                >{{ t(`download.status.${videoState(entry.video)}`) }}</span
              >
            </article>
          </template>
        </template>
      </div>
    </div>
    <div v-if="chatTitle" class="results-footer">
      <label class="selall"
        ><input
          type="checkbox"
          :checked="pageAllSelected"
          :disabled="!selectableCount"
          @change="toggleSelectAll"
        />{{ t("history.selectAll") }}</label
      ><span class="muted shift-hint">{{ t("history.shiftHint") }}</span
      ><span class="collection-spacer" /><n-button
        v-if="!exhausted"
        :loading="browsing"
        @click="loadPrevDay"
        >{{ t("history.loadMore") }}</n-button
      ><n-button
        type="primary"
        :disabled="!selectedCount"
        :loading="downloading"
        @click="downloadSelected"
        >{{ t("history.downloadSelected", { n: selectedCount }) }}</n-button
      >
    </div>
  </section>
  <FavoriteDialog :item="favorite" @close="favorite = null" /><MediaPreview
    :item="preview"
    @close="preview = null"
  />
</template>
<style scoped>
.browse-controls {
  flex-shrink: 0;
  padding: 14px;
}
.history-query {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 120px 150px 112px;
  gap: 10px;
  margin-top: 10px;
  align-items: center;
}
.browse-results {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 14px;
  overflow: hidden;
}
.results-heading {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 34px;
  padding-bottom: 10px;
  flex-shrink: 0;
}
.results-heading strong {
  font-size: 14px;
}
.muted {
  font-size: 12px;
  color: var(--text-dim);
}
.results-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  scrollbar-gutter: stable;
}
.results-footer {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
  flex-shrink: 0;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 12px;
  align-content: start;
}
.day-divider {
  grid-column: 1/-1;
  padding: 8px 0 4px;
  color: var(--text-dim);
  font-size: 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.day-select,
.group-check {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  user-select: none;
}
.day-select input,
.group-check input {
  width: 15px;
  height: 15px;
  accent-color: var(--accent);
  cursor: pointer;
}
.day-select {
  color: var(--text-dim);
  font-size: 11.5px;
}
.day-select:hover {
  color: var(--text);
}
.media-group {
  grid-column: 1/-1;
  border: 1px solid var(--border-soft);
  border-radius: 11px;
  background: color-mix(in srgb, var(--bg-inset) 65%, var(--surface));
  padding: 10px;
  transition: border-color 0.15s ease, background 0.15s ease;
}
.media-group.is-selected {
  border-color: var(--accent-border);
  background: color-mix(in srgb, var(--accent-soft) 55%, var(--bg-inset));
}
.media-group-header {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-bottom: 10px;
}
.media-group-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
}
.group-check {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}
.group-check input:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}
.media-group-meta {
  font-size: 11.5px;
  color: var(--text-dim);
  font-variant-numeric: tabular-nums;
}
.media-group-caption {
  font-size: 12px;
  color: var(--text-dim);
  line-height: 1.5;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.media-group-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
}
.vcard {
  position: relative;
  padding: 8px;
  border: 1px solid var(--border-soft);
  border-radius: 9px;
  min-width: 0;
}
.vcard.selected {
  border-color: var(--accent);
}
.vcard-preview {
  display: block;
  width: 100%;
  border: 0;
  background: none;
  color: var(--text);
  text-align: left;
  cursor: pointer;
}
.thumb-wrap {
  display: block;
  position: relative;
}
.thumb {
  width: 100%;
  height: 92px;
  border-radius: 5px;
  object-fit: cover;
}
.thumb-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-inset);
  color: var(--text-dim);
}
.duration {
  position: absolute;
  bottom: 5px;
  right: 5px;
  background: var(--bg);
  border-radius: 3px;
  padding: 2px 4px;
  font-size: 10px;
}
.vname,
.caption {
  display: block;
  font-size: 12px;
  margin-top: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.caption {
  color: var(--text-dim);
  font-size: 11px;
}
.check {
  position: absolute;
  top: 14px;
  left: 14px;
  width: 16px;
  height: 16px;
  accent-color: var(--accent);
}
.card-save {
  position: absolute;
  right: 12px;
  top: 12px;
  width: 25px;
  height: 25px;
  background: var(--surface);
  border: 0;
  border-radius: 5px;
  color: var(--text-dim);
  cursor: pointer;
}
.card-save.saved {
  color: var(--accent-bright);
}
.selall {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
}
.vcard > .pill {
  margin-top: 6px;
}
@media (max-width: 760px) {
  .history-query {
    grid-template-columns: minmax(0, 1fr) 110px;
  }
  .results-footer {
    flex-wrap: wrap;
  }
  .shift-hint {
    display: none;
  }
}
</style>
