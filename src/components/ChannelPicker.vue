<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { NSelect, NInput, NButton } from "naive-ui";
import { useAppStore } from "../stores/app";
import { invoke } from "../lib/backend";
import { errorText } from "../lib/errors";
import type { ChannelDirectory } from "../types";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{ value: string }>();
const emit = defineEmits<{
  "update:value": [value: string];
  selected: [value: string];
  clear: [];
}>();
const store = useAppStore();
const { t } = useI18n();
const directory = ref<ChannelDirectory>({
  channels: [],
  folders: [],
  foldersError: null,
  truncated: false,
});
const folder = ref(0),
  manual = ref(false),
  loading = ref(false),
  error = ref("");
let sequence = 0;
const folders = computed(() => [
  { label: t("simple.allFolders"), value: 0 },
  { label: t("simple.archived"), value: -1 },
  ...directory.value.folders.map((item) => ({
    label: item.name,
    value: item.id,
  })),
]);
const channels = computed(() =>
  directory.value.channels
    .filter(
      (item) => folder.value === 0 || item.folderIds.includes(folder.value),
    )
    .map((item) => ({ label: item.title, value: item.url })),
);
async function refresh() {
  if (!store.authorized) return;
  const current = ++sequence;
  loading.value = true;
  error.value = "";
  try {
    const result = await invoke<ChannelDirectory>("joined_channels");
    if (current === sequence) directory.value = result;
  } catch (cause) {
    if (current === sequence) error.value = errorText(cause, t);
  } finally {
    if (current === sequence) loading.value = false;
  }
}
function select(value: string | null) {
  emit("update:value", value ?? "");
  if (value) emit("selected", value);
  else emit("clear");
}
function changeFolder(value: number) {
  folder.value = value;
  emit("update:value", "");
  emit("clear");
}
watch(
  () => [store.authorized, store.user] as const,
  () => {
    sequence++;
    directory.value = {
      channels: [],
      folders: [],
      foldersError: null,
      truncated: false,
    };
    folder.value = 0;
    loading.value = false;
    error.value = "";
    if (store.authorized) void refresh();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  sequence++;
});
</script>
<template>
  <div class="channel-picker">
    <n-select
      v-if="!manual"
      :value="folder"
      :options="folders"
      :disabled="!store.authorized"
      :aria-label="t('simple.folder')"
      @update:value="changeFolder"
    />
    <n-select
      v-if="!manual"
      :value="value || null"
      :options="channels"
      filterable
      clearable
      :loading="loading"
      :disabled="!store.authorized"
      :placeholder="t('simple.channel')"
      :aria-label="t('simple.channel')"
      @update:value="select"
    />
    <n-input
      v-else
      :value="value"
      :disabled="!store.authorized"
      :placeholder="t('history.input')"
      :aria-label="t('history.input')"
      @update:value="(value) => emit('update:value', value)"
      @keyup.enter="emit('selected', value)"
    />
    <n-button
      v-if="!manual"
      :loading="loading"
      :disabled="!store.authorized"
      :title="t('simple.refresh')"
      :aria-label="t('simple.refresh')"
      @click="refresh"
      ><AppIcon name="history"
    /></n-button>
    <n-button @click="manual = !manual">{{
      manual ? t("simple.choose") : t("simple.manual")
    }}</n-button>
  </div>
  <p v-if="error" class="inline-note error" role="alert">{{ error }}</p>
  <p v-else-if="directory.foldersError" class="inline-note">
    {{ t("simple.folderError") }}
  </p>
  <p v-else-if="directory.truncated" class="inline-note">
    {{ t("simple.truncated") }}
  </p>
</template>
<style scoped>
.channel-picker {
  display: grid;
  grid-template-columns: 150px minmax(0, 1fr) 36px 88px;
  gap: 10px;
  align-items: center;
}
.channel-picker:has(> .n-input) {
  grid-template-columns: minmax(0, 1fr) 88px;
}
.inline-note {
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-dim);
}
.error {
  color: var(--red);
}
@media (max-width: 760px) {
  .channel-picker {
    grid-template-columns: 120px minmax(0, 1fr) 36px;
  }
  .channel-picker > .n-button:last-child {
    grid-column: 1/-1;
  }
  .channel-picker:has(> .n-input) {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
