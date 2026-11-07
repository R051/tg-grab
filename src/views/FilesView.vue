<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { NInput, NSelect, NButton, useMessage, useDialog } from "naive-ui";
import { useAppStore } from "../stores/app";
import { errorText } from "../lib/errors";
import { formatSize, mediaIcon, findFavorite } from "../lib/library";
import type { DownloadedFile, FavoriteDraft, MediaDetails } from "../types";
import AppIcon from "../components/AppIcon.vue";
import FavoriteDialog from "../components/FavoriteDialog.vue";
import MediaPreview from "../components/MediaPreview.vue";
const { t } = useI18n();
const store = useAppStore();
const message = useMessage();
const dialog = useDialog();
const search = ref("");
const kind = ref("all");
const sort = ref("name");
const refreshing = ref(false);
const preview = ref<MediaDetails | null>(null);
const favorite = ref<FavoriteDraft | null>(null);
const kinds = computed(() => [
  { label: t("media.all"), value: "all" },
  ...["video", "photo", "audio", "document"].map((value) => ({
    label: t(`media.${value}`),
    value,
  })),
]);
const sorts = computed(() => [
  { label: t("files.byName"), value: "name" },
  { label: t("files.bySize"), value: "size" },
]);
const files = computed(() =>
  store.files
    .filter(
      (file) =>
        (kind.value === "all" || file.kind === kind.value) &&
        file.name.toLowerCase().includes(search.value.trim().toLowerCase()),
    )
    .slice()
    .sort((a, b) =>
      sort.value === "size" ? b.size - a.size : a.name.localeCompare(b.name),
    ),
);
const totalSize = computed(() =>
  store.files.reduce((sum, file) => sum + file.size, 0),
);
function details(file: DownloadedFile): MediaDetails {
  return {
    kind: file.kind,
    title: file.name.split("/").pop() ?? file.name,
    url: file.sourceUrl ?? "",
    localPath: file.name,
    localRoot: store.settings?.effective_downloads_dir,
    size: file.size,
  };
}
function remove(file: DownloadedFile) {
  dialog.warning({
    title: t("files.deleteTitle"),
    content: t("files.deleteHint", { name: file.name }),
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        await store.deleteFile(file.name);
        message.success(t("download.deleted"));
      } catch (error) {
        message.error(errorText(error, t));
        return false;
      }
    },
  });
}
async function refresh() {
  if (refreshing.value) return;
  refreshing.value = true;
  try {
    await store.refreshFiles();
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    refreshing.value = false;
  }
}
</script>
<template>
  <header class="view-heading">
    <div>
      <p class="eyebrow">{{ t("files.eyebrow") }}</p>
      <h2>{{ t("files.heading") }}</h2>
      <p class="view-description">
        {{
          t("files.summary", {
            n: store.files.length,
            size: formatSize(totalSize),
          })
        }}
      </p>
    </div>
    <n-button :loading="refreshing" @click="refresh">{{
      t("common.refresh")
    }}</n-button>
  </header>
  <div class="panel">
    <p class="storage-location">
      <AppIcon name="folder" /><span
        :title="store.settings?.effective_downloads_dir"
        >{{ store.settings?.effective_downloads_dir }}</span
      >
    </p>
    <div class="files-toolbar">
      <n-input
        v-model:value="search"
        clearable
        :placeholder="t('files.search')"
        :aria-label="t('files.search')"
      /><n-select
        v-model:value="kind"
        :options="kinds"
        style="width: 140px"
      /><n-select v-model:value="sort" :options="sorts" style="width: 150px" />
    </div>
  </div>
  <section class="panel fill-panel">
    <div v-if="!files.length" class="empty">
      <AppIcon name="folder" :size="34" /><strong>{{
        store.files.length ? t("library.noResults") : t("files.empty")
      }}</strong>
      <p>{{ t("files.emptyHint") }}</p>
      <n-button @click="store.currentView = 'history'">{{
        t("library.browseChannels")
      }}</n-button>
    </div>
    <div v-else class="rows scroll-area">
      <div v-for="file in files" :key="file.name" class="row-item file-entry">
        <button class="file-main" @click="preview = details(file)">
          <span class="file-kind"
            ><AppIcon :name="mediaIcon(file.kind)" :size="22" /></span
          ><span class="row-body"
            ><span class="file-name">{{ file.name.split("/").pop() }}</span
            ><span class="file-directory"
              >{{ file.name }} · {{ formatSize(file.size) }}</span
            ></span
          ></button
        ><button
          class="icon-button"
          :class="{
            'is-saved': findFavorite(store.library.items, details(file)),
          }"
          :title="t('library.save')"
          @click="favorite = details(file)"
        >
          <AppIcon name="star" /></button
        ><button
          class="icon-button danger"
          :title="t('common.delete')"
          @click="remove(file)"
        >
          <AppIcon name="trash" />
        </button>
      </div>
    </div>
  </section>
  <FavoriteDialog :item="favorite" @close="favorite = null" /><MediaPreview
    :item="preview"
    @close="preview = null"
  />
</template>
<style scoped>
.storage-location {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 12px;
  color: var(--text-dim);
  overflow-wrap: anywhere;
  margin-bottom: 16px;
}
.storage-location span {
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.storage-location svg {
  flex-shrink: 0;
}
.files-toolbar {
  display: flex;
  gap: 12px;
}
.file-entry {
  padding: 6px 8px;
}
.file-main {
  display: flex;
  align-items: center;
  gap: 14px;
  flex: 1;
  min-width: 0;
  cursor: pointer;
  border: 0;
  background: none;
  color: var(--text);
  font: inherit;
  text-align: left;
  padding: 6px 0;
}
.file-kind {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 36px;
  height: 36px;
  background: var(--surface-2);
  border-radius: 8px;
  color: var(--accent-bright);
}
.file-name {
  display: block;
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.file-directory {
  display: block;
  font-size: 11px;
  color: var(--text-dim);
  margin-top: 5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.is-saved {
  color: var(--accent-bright);
}
@media (max-width: 760px) {
  .files-toolbar {
    flex-wrap: wrap;
  }
  .files-toolbar > .n-input {
    flex: 1 1 100%;
  }
  .files-toolbar > .n-select {
    flex: 1 1 40%;
    min-width: 0;
  }
}
</style>
