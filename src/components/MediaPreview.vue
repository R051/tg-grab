<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { NModal, NButton, NSpin, useMessage } from "naive-ui";
import { useAppStore } from "../stores/app";
import { invoke, isDesktop, fileUrl } from "../lib/backend";
import { errorText } from "../lib/errors";
import {
  localPathFor,
  previewUrl,
  openSource,
  formatSize,
  mediaIcon,
  findFavorite,
} from "../lib/library";
import type { MediaDetails, FavoriteDraft } from "../types";
import AppIcon from "./AppIcon.vue";
import FavoriteDialog from "./FavoriteDialog.vue";
const props = defineProps<{ item: MediaDetails | null }>();
const emit = defineEmits<{ close: [] }>();
const store = useAppStore();
const { t } = useI18n();
const message = useMessage();
const mediaUrl = ref("");
const loading = ref(false);
const failed = ref(false);
const submitting = ref(false);
const favorite = ref<FavoriteDraft | null>(null);
let sequence = 0;
const path = computed(() =>
  props.item ? localPathFor(props.item, store.files, store.settings) : null,
);
const saved = computed(() =>
  props.item ? findFavorite(store.library.items, props.item) : undefined,
);
const busy = computed(
  () =>
    props.item &&
    [...store.pending, ...store.activeTasks].some(
      (task) => task.url === props.item!.url,
    ),
);
watch([() => props.item, path], async () => {
  const current = ++sequence;
  mediaUrl.value = "";
  failed.value = false;
  if (
    !props.item ||
    !path.value ||
    !["video", "audio", "photo"].includes(props.item.kind)
  ) {
    loading.value = false;
    return;
  }
  loading.value = true;
  try {
    const url = await previewUrl(path.value);
    if (current === sequence) mediaUrl.value = url;
  } catch {
    if (current === sequence) failed.value = true;
  } finally {
    if (current === sequence) loading.value = false;
  }
});
async function download() {
  if (!props.item?.url || submitting.value || busy.value) return;
  submitting.value = true;
  try {
    await invoke("download_urls", { urls: [props.item.url] });
    await store.refreshQueue();
    message.success(t("download.queued", { n: 1 }));
  } catch (error) {
    message.error(errorText(error, t));
  } finally {
    submitting.value = false;
  }
}
async function openLocal() {
  if (!path.value) return;
  try {
    if (isDesktop) await invoke("reveal_file", { name: path.value });
    else window.open(fileUrl(path.value)!, "_blank", "noopener,noreferrer");
  } catch (error) {
    message.error(errorText(error, t));
  }
}
async function showSource(url: string) {
  try {
    await openSource(url);
  } catch (error) {
    message.error(errorText(error, t));
  }
}
</script>
<template>
  <n-modal
    :show="!!item"
    preset="card"
    class="media-dialog"
    :title="item?.title"
    @update:show="
      (show) => {
        if (!show) emit('close');
      }
    "
  >
    <template v-if="item">
      <div class="media-stage">
        <n-spin v-if="loading" /><template v-else-if="mediaUrl && !failed"
          ><video
            v-if="item.kind === 'video'"
            :src="mediaUrl"
            controls
            preload="metadata"
            @error="failed = true" /><audio
            v-else-if="item.kind === 'audio'"
            :src="mediaUrl"
            controls
            preload="metadata"
            @error="failed = true" /><img
            v-else
            :src="mediaUrl"
            :alt="item.title"
            @error="failed = true" /></template
        ><img
          v-else-if="item.thumbnail"
          :src="item.thumbnail"
          :alt="item.title"
        />
        <div v-else class="media-placeholder">
          <AppIcon :name="mediaIcon(item.kind)" :size="44" />
          <p>
            {{
              failed
                ? t("library.previewFailed")
                : path
                  ? t("library.documentHint")
                  : t("library.downloadToPreview")
            }}
          </p>
        </div>
      </div>
      <div class="media-meta">
        <span>{{ t(`media.${item.kind}`) }}</span
        ><span v-if="item.size">{{ formatSize(item.size) }}</span
        ><span v-if="item.date">{{ new Date(item.date).toLocaleString() }}</span
        ><span class="pill" :class="path ? 'success' : 'queued'">{{
          path
            ? t("library.available")
            : item.localPath
              ? t("library.missing")
              : t("library.linkOnly")
        }}</span>
      </div>
      <p v-if="item.sourceTitle" class="collection-hint">
        {{ item.sourceTitle }}
      </p>
      <div class="collection-actions media-actions">
        <n-button
          @click="favorite = { ...item, localPath: path ?? item.localPath }"
          >{{ saved ? t("library.edit") : t("library.save") }}</n-button
        ><n-button v-if="item.url" @click="showSource(item.url)">{{
          t("library.openSource")
        }}</n-button
        ><span class="collection-spacer" /><n-button
          v-if="path"
          type="primary"
          @click="openLocal"
          >{{
            isDesktop ? t("library.reveal") : t("library.downloadDevice")
          }}</n-button
        ><n-button
          v-else-if="item.url"
          type="primary"
          :loading="submitting"
          :disabled="!store.authorized || !!busy"
          @click="download"
          >{{
            busy ? t("download.status.queued") : t("download.start")
          }}</n-button
        >
      </div>
    </template>
  </n-modal>
  <FavoriteDialog :item="favorite" @close="favorite = null" />
</template>
<style scoped>
.media-stage {
  min-height: 240px;
  max-height: 55vh;
  background: var(--bg-inset);
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.media-stage video,
.media-stage img {
  max-width: 100%;
  max-height: 55vh;
  object-fit: contain;
}
.media-stage audio {
  width: 90%;
}
.media-placeholder {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-direction: column;
  color: var(--text-dim);
  padding: 36px;
  font-size: 13px;
  line-height: 1.7;
  text-align: center;
}
.media-meta {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  align-items: center;
  margin: 16px 0;
  color: var(--text-dim);
  font-size: 12px;
}
.media-actions {
  flex-wrap: wrap;
}
</style>
<style>
.media-dialog {
  width: min(860px, calc(100vw - 32px));
}
</style>
