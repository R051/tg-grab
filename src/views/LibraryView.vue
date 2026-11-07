<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  NButton,
  NInput,
  NSelect,
  NModal,
  NFormItem,
  useMessage,
  useDialog,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import { invoke } from "../lib/backend";
import { errorText } from "../lib/errors";
import { exportCollections, localPathFor, mediaIcon } from "../lib/library";
import type {
  Favorite,
  FavoriteDraft,
  MediaDetails,
  FavoriteKind,
  CollectionFolder,
} from "../types";
import AppIcon from "../components/AppIcon.vue";
import FavoriteDialog from "../components/FavoriteDialog.vue";
import MediaPreview from "../components/MediaPreview.vue";
const store = useAppStore();
const { t } = useI18n();
const message = useMessage();
const dialog = useDialog();
const search = ref("");
const folder = ref("all");
const kind = ref("all");
const editing = ref<FavoriteDraft | null>(null);
const preview = ref<MediaDetails | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);
const importing = ref(false);
const exporting = ref(false);
const folderModal = ref(false);
const folderName = ref("");
const folderId = ref<string | null>(null);
const savingFolder = ref(false);
const adding = ref(false);
const newKind = ref<FavoriteKind>("channel");
const newUrl = ref("");
const newTitle = ref("");
const kinds = computed(() => [
  { label: t("media.all"), value: "all" },
  ...["channel", "video", "photo", "audio", "document", "file"].map(
    (value) => ({ label: t(`media.${value}`), value }),
  ),
]);
const itemKinds = computed(() =>
  kinds.value.filter(
    (option) => option.value !== "all" && option.value !== "file",
  ),
);
const items = computed(() => {
  const query = search.value.trim().toLowerCase();
  return store.library.items.filter(
    (item) =>
      (folder.value === "all" ||
        (folder.value === "unfiled"
          ? !item.folderId
          : item.folderId === folder.value)) &&
      (kind.value === "all" || item.kind === kind.value) &&
      (!query ||
        [item.title, item.notes, item.sourceTitle, item.url].some((value) =>
          value?.toLowerCase().includes(query),
        )),
  );
});
function open(item: Favorite) {
  if (item.kind === "channel") store.openChannel(item.url);
  else preview.value = { ...item };
}
function status(item: Favorite) {
  return item.kind === "channel"
    ? t("library.channelEntry")
    : localPathFor(item, store.files, store.settings)
      ? t("library.available")
      : item.localPath
        ? t("library.missing")
        : t("library.linkOnly");
}
function editFolder(existing?: CollectionFolder) {
  folderId.value = existing?.id ?? null;
  folderName.value = existing?.name ?? "";
  folderModal.value = true;
}
async function saveFolder() {
  if (savingFolder.value || !folderName.value.trim()) return;
  savingFolder.value = true;
  try {
    const saved = await invoke<CollectionFolder>("folder_save", {
      id: folderId.value,
      name: folderName.value.trim(),
    });
    await store.refreshLibrary();
    folder.value = saved.id;
    folderModal.value = false;
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    savingFolder.value = false;
  }
}
function deleteFolder(existing: CollectionFolder) {
  dialog.warning({
    title: t("library.deleteFolder"),
    content: t("library.deleteFolderHint"),
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        await invoke("folder_remove", { id: existing.id });
        await store.refreshLibrary();
        if (folder.value === existing.id) folder.value = "unfiled";
      } catch (error) {
        message.error(errorText(error, t));
        return false;
      }
    },
  });
}
async function exportData() {
  if (exporting.value) return;
  exporting.value = true;
  try {
    if (await exportCollections()) message.success(t("library.exported"));
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    exporting.value = false;
  }
}
async function importData(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  importing.value = true;
  try {
    if (file.size > 5 * 1024 * 1024) throw Error("LIBRARY_TOO_LARGE");
    const data = JSON.parse(await file.text());
    const result = await invoke<{ added: number; skipped: number }>(
      "library_import",
      { data },
    );
    await store.refreshLibrary();
    message.success(t("library.imported", result));
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    importing.value = false;
    input.value = "";
  }
}
function add() {
  if (!newUrl.value.trim()) return;
  editing.value = {
    kind: newKind.value,
    title: newTitle.value.trim() || newUrl.value.trim(),
    url: newUrl.value.trim(),
    folderId:
      folder.value !== "all" && folder.value !== "unfiled"
        ? folder.value
        : null,
  };
  adding.value = false;
  newTitle.value = "";
  newUrl.value = "";
}
</script>
<template>
  <header class="view-heading">
    <div>
      <p class="eyebrow">{{ t("library.eyebrow") }}</p>
      <h2>{{ t("library.heading") }}</h2>
      <p class="view-description">{{ t("library.description") }}</p>
    </div>
    <div class="view-tools">
      <input
        ref="fileInput"
        type="file"
        accept="application/json,.json"
        hidden
        @change="importData"
      /><n-button :loading="importing" @click="fileInput?.click()">{{
        t("library.import")
      }}</n-button
      ><n-button :loading="exporting" @click="exportData">{{
        t("library.export")
      }}</n-button
      ><n-button type="primary" @click="adding = true">{{
        t("library.add")
      }}</n-button>
    </div>
  </header>
  <div class="library-layout">
    <aside class="collection-sidebar panel">
      <div class="folder-heading">
        <span>{{ t("library.folders") }}</span
        ><button
          class="icon-button"
          :title="t('library.newFolder')"
          @click="editFolder()"
        >
          <AppIcon name="plus" />
        </button>
      </div>
      <button
        class="folder-link"
        :class="{ selected: folder === 'all' }"
        @click="folder = 'all'"
      >
        <AppIcon name="star" /><span>{{ t("library.all") }}</span
        ><small>{{ store.library.items.length }}</small>
      </button>
      <button
        class="folder-link"
        :class="{ selected: folder === 'unfiled' }"
        @click="folder = 'unfiled'"
      >
        <AppIcon name="folder" /><span>{{ t("library.unfiled") }}</span
        ><small>{{
          store.library.items.filter((item) => !item.folderId).length
        }}</small>
      </button>
      <div
        v-for="entry in store.library.folders"
        :key="entry.id"
        class="folder-entry"
      >
        <button
          class="folder-link"
          :class="{ selected: folder === entry.id }"
          @click="folder = entry.id"
        >
          <AppIcon name="folder" /><span>{{ entry.name }}</span
          ><small>{{
            store.library.items.filter((item) => item.folderId === entry.id)
              .length
          }}</small></button
        ><button
          class="icon-button"
          :title="t('library.renameFolder')"
          @click="editFolder(entry)"
        >
          <AppIcon name="edit" :size="14" /></button
        ><button
          class="icon-button danger"
          :title="t('library.deleteFolder')"
          @click="deleteFolder(entry)"
        >
          <AppIcon name="close" :size="14" />
        </button>
      </div>
    </aside>
    <section class="collection-content">
      <div class="library-toolbar">
        <n-input
          v-model:value="search"
          clearable
          :placeholder="t('library.search')"
          :aria-label="t('library.search')"
        /><n-select
          v-model:value="kind"
          :options="kinds"
          style="width: 160px"
        />
      </div>
      <div class="saved-scroll">
        <div v-if="!items.length" class="panel empty">
          <AppIcon name="star" :size="34" /><strong>{{
            store.library.items.length
              ? t("library.noResults")
              : t("library.empty")
          }}</strong>
          <p>{{ t("library.emptyHint") }}</p>
          <n-button @click="store.currentView = 'history'">{{
            t("library.browseChannels")
          }}</n-button>
        </div>
        <div v-else class="saved-grid">
          <article
            v-for="item in items"
            :key="item.id"
            class="saved-card panel"
          >
            <div class="saved-card-top">
              <span class="saved-kind"
                ><AppIcon :name="mediaIcon(item.kind)" />{{
                  t(`media.${item.kind}`)
                }}</span
              ><button
                class="icon-button saved-star"
                :title="t('library.edit')"
                @click="editing = { ...item }"
              >
                <AppIcon name="star" />
              </button>
            </div>
            <button class="saved-card-main" @click="open(item)">
              <h3>{{ item.title }}</h3>
              <p v-if="item.sourceTitle" class="saved-source">
                {{ item.sourceTitle }}
              </p>
              <p v-if="item.notes" class="saved-notes">{{ item.notes }}</p>
            </button>
            <div class="saved-card-bottom">
              <span
                class="pill"
                :class="
                  localPathFor(item, store.files, store.settings)
                    ? 'success'
                    : 'queued'
                "
                >{{ status(item) }}</span
              ><span v-if="item.folderId" class="saved-folder">{{
                store.library.folders.find(
                  (entry) => entry.id === item.folderId,
                )?.name
              }}</span>
            </div>
          </article>
        </div>
      </div>
    </section>
  </div>
  <n-modal
    v-model:show="folderModal"
    preset="card"
    class="collection-dialog"
    :title="folderId ? t('library.renameFolder') : t('library.newFolder')"
    ><n-form-item :label="t('library.folderName')"
      ><n-input
        v-model:value="folderName"
        :maxlength="80"
        @keyup.enter="saveFolder"
    /></n-form-item>
    <div class="collection-actions">
      <span class="collection-spacer" /><n-button
        @click="folderModal = false"
        >{{ t("common.cancel") }}</n-button
      ><n-button
        type="primary"
        :loading="savingFolder"
        :disabled="!folderName.trim()"
        @click="saveFolder"
        >{{ t("common.save") }}</n-button
      >
    </div></n-modal
  >
  <n-modal
    v-model:show="adding"
    preset="card"
    class="collection-dialog"
    :title="t('library.add')"
    ><n-form-item :label="t('library.type')"
      ><n-select v-model:value="newKind" :options="itemKinds" /></n-form-item
    ><n-form-item :label="t('library.sourceLink')"
      ><n-input
        v-model:value="newUrl"
        placeholder="https://t.me/..." /></n-form-item
    ><n-form-item :label="t('library.optionalTitle')"
      ><n-input v-model:value="newTitle"
    /></n-form-item>
    <div class="collection-actions">
      <span class="collection-spacer" /><n-button @click="adding = false">{{
        t("common.cancel")
      }}</n-button
      ><n-button type="primary" :disabled="!newUrl.trim()" @click="add">{{
        t("common.continue")
      }}</n-button>
    </div></n-modal
  >
  <FavoriteDialog :item="editing" @close="editing = null" /><MediaPreview
    :item="preview"
    @close="preview = null"
  />
</template>
<style scoped>
.library-layout {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 20px;
  align-items: stretch;
  flex: 1;
  min-height: 0;
}
.collection-sidebar {
  padding: 12px;
  overflow: auto;
  min-height: 0;
}
.folder-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--text-dim);
  padding: 4px 8px 12px;
  font-size: 12px;
}
.folder-link {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 10px 8px;
  background: none;
  border: 0;
  color: var(--text-dim);
  text-align: left;
  border-radius: 8px;
  cursor: pointer;
  min-width: 0;
  font: inherit;
}
.folder-link.selected {
  background: var(--accent-soft);
  color: var(--accent-bright);
}
.folder-link span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
.folder-link small {
  color: var(--text-faint);
}
.folder-entry {
  display: flex;
  align-items: center;
}
.folder-entry .folder-link {
  flex: 1;
}
.folder-entry .icon-button {
  padding: 4px;
  flex-shrink: 0;
}
.collection-content {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.saved-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.library-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-bottom: 16px;
}
.library-toolbar > .n-input {
  flex: 1 1 200px;
}
.library-toolbar > .n-select {
  flex: 0 1 160px;
}
.saved-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 12px;
}
.saved-card {
  padding: 16px;
  display: flex;
  flex-direction: column;
  height: 180px;
  min-height: 180px;
}
.saved-card:focus-within {
  border-color: var(--accent);
}
.saved-card-top,
.saved-card-bottom {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: space-between;
}
.saved-kind {
  display: flex;
  gap: 7px;
  align-items: center;
  color: var(--text-dim);
  font-size: 11px;
}
.saved-star {
  color: var(--accent-bright);
}
.saved-card-main {
  background: none;
  border: 0;
  color: var(--text);
  text-align: left;
  padding: 16px 0;
  cursor: pointer;
  flex: 1;
  font: inherit;
  min-height: 0;
  overflow: hidden;
}
.saved-card-main h3 {
  font-size: 14px;
  font-weight: 600;
  line-height: 1.6;
  overflow-wrap: anywhere;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.saved-source,
.saved-notes {
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.7;
  margin-top: 6px;
}
.saved-notes {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.saved-folder {
  font-size: 11px;
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 110px;
}
@media (max-width: 900px) {
  .library-toolbar > .n-select {
    flex: 1 1 140px;
  }
  .library-layout {
    grid-template-columns: 1fr;
  }
  .collection-sidebar {
    position: static;
    max-height: 130px;
    overflow-y: auto;
  }
  .folder-heading {
    position: sticky;
    top: -12px;
    z-index: 1;
    background: var(--surface);
  }
}
</style>
