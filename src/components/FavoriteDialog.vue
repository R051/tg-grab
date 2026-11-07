<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  NModal,
  NInput,
  NSelect,
  NButton,
  NFormItem,
  useMessage,
  useDialog,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import { invoke } from "../lib/backend";
import { errorText } from "../lib/errors";
import { findFavorite } from "../lib/library";
import type { FavoriteDraft, CollectionFolder } from "../types";
const props = defineProps<{ item: FavoriteDraft | null }>();
const emit = defineEmits<{ close: []; saved: [] }>();
const { t } = useI18n();
const store = useAppStore();
const message = useMessage();
const dialog = useDialog();
const title = ref("");
const notes = ref("");
const folderId = ref<string | null>(null);
const newFolder = ref("");
const saving = ref(false);
const creating = ref(false);
const existing = computed(() =>
  props.item ? findFavorite(store.library.items, props.item) : undefined,
);
const folders = computed(() => [
  { label: t("library.unfiled"), value: "" },
  ...store.library.folders.map((folder) => ({
    label: folder.name,
    value: folder.id,
  })),
]);
watch(
  () => props.item,
  (item) => {
    if (!item) return;
    const saved = findFavorite(store.library.items, item);
    title.value = saved?.title ?? item.title;
    notes.value = saved?.notes ?? item.notes ?? "";
    folderId.value = saved?.folderId ?? item.folderId ?? "";
    newFolder.value = "";
  },
);
async function createFolder() {
  if (creating.value || !newFolder.value.trim()) return;
  creating.value = true;
  try {
    const folder = await invoke<CollectionFolder>("folder_save", {
      name: newFolder.value.trim(),
    });
    await store.refreshLibrary();
    folderId.value = folder.id;
    newFolder.value = "";
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    creating.value = false;
  }
}
async function save() {
  if (saving.value || !props.item || !title.value.trim()) return;
  saving.value = true;
  try {
    await invoke("favorite_save", {
      item: {
        ...props.item,
        title: title.value.trim(),
        folderId: folderId.value || null,
        notes: notes.value,
      },
    });
    await store.refreshLibrary();
    message.success(t("library.saved"));
    emit("saved");
    emit("close");
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    saving.value = false;
  }
}
function remove() {
  const item = existing.value;
  if (!item) return;
  dialog.warning({
    title: t("library.remove"),
    content: t("library.removeHint"),
    positiveText: t("library.remove"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        await invoke("favorite_remove", { id: item.id });
        await store.refreshLibrary();
        emit("close");
      } catch (error) {
        message.error(errorText(error, t));
        return false;
      }
    },
  });
}
</script>
<template>
  <n-modal
    :show="!!item"
    preset="card"
    class="collection-dialog"
    :title="existing ? t('library.edit') : t('library.save')"
    :mask-closable="!saving"
    :close-on-esc="!saving"
    @update:show="
      (show) => {
        if (!show) emit('close');
      }
    "
  >
    <p class="collection-hint">{{ t("library.localHint") }}</p>
    <n-form-item :label="t('library.title')"
      ><n-input v-model:value="title" :maxlength="500" @keyup.enter="save"
    /></n-form-item>
    <n-form-item :label="t('library.folder')"
      ><n-select v-model:value="folderId" :options="folders"
    /></n-form-item>
    <div class="collection-inline">
      <n-input
        v-model:value="newFolder"
        :placeholder="t('library.newFolder')"
        :maxlength="80"
        @keyup.enter="createFolder"
      /><n-button
        :loading="creating"
        :disabled="!newFolder.trim()"
        @click="createFolder"
        >{{ t("common.create") }}</n-button
      >
    </div>
    <n-form-item :label="t('library.notes')"
      ><n-input
        v-model:value="notes"
        type="textarea"
        :rows="3"
        :maxlength="8000"
    /></n-form-item>
    <div class="collection-actions">
      <n-button v-if="existing" type="error" quaternary @click="remove">{{
        t("library.remove")
      }}</n-button
      ><span class="collection-spacer" /><n-button
        :disabled="saving"
        @click="emit('close')"
        >{{ t("common.cancel") }}</n-button
      ><n-button
        type="primary"
        :loading="saving"
        :disabled="!title.trim()"
        @click="save"
        >{{ t("common.save") }}</n-button
      >
    </div>
  </n-modal>
</template>
<style>
.collection-dialog {
  width: min(480px, calc(100vw - 32px));
}
.collection-hint {
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.7;
  margin-bottom: 18px;
}
.collection-inline,
.collection-actions {
  display: flex;
  gap: 10px;
  align-items: center;
}
.collection-inline {
  margin: 0 0 20px;
  padding: 10px;
  background: var(--surface-2);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
}
.collection-inline > .n-input {
  flex: 1;
  min-width: 0;
}
.collection-inline > .n-button {
  flex-shrink: 0;
}
.collection-spacer {
  flex: 1;
}
@media (max-width: 480px) {
  .collection-actions {
    flex-wrap: wrap;
  }
  .collection-actions > .n-button {
    min-height: 44px;
  }
  .collection-inline > .n-button {
    min-height: 44px;
  }
}
</style>
