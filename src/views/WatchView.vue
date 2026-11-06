<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import {
  NCard,
  NButton,
  NInput,
  NCheckbox,
  NEmpty,
  NTag,
  useMessage,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import type { WatchEntry } from "../types";

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

const link = ref("");
const backfill = ref(false);
const adding = ref(false);
const busy = ref(false);

const ERR_KEYS = [
  "ALREADY_WATCHED",
  "CHAT_NOT_FOUND",
  "NOT_A_CHANNEL",
] as const;

function errText(e: unknown): string {
  const raw = String(e);
  const code = raw.includes(":") ? raw.split(":")[0].trim() : raw.trim();
  for (const k of ERR_KEYS) {
    if (code.startsWith(k)) return t(`watch.errors.${k}`);
  }
  return raw;
}

async function addWatch() {
  if (!link.value.trim()) return;
  adding.value = true;
  try {
    const entry = await invoke<WatchEntry>("watch_add", {
      link: link.value.trim(),
      backfill: backfill.value,
    });
    message.success(t("watch.added", { title: entry.title }));
    link.value = "";
    await store.refreshWatched();
  } catch (e) {
    message.error(errText(e));
  } finally {
    adding.value = false;
  }
}

async function toggle(entry: WatchEntry) {
  try {
    await invoke("watch_toggle", { rawId: entry.raw_id });
    await store.refreshWatched();
  } catch (e) {
    message.error(String(e));
  }
}

async function remove(entry: WatchEntry) {
  if (!window.confirm(t("watch.deleteConfirm"))) return;
  try {
    await invoke("watch_delete", { rawId: entry.raw_id });
    await store.refreshWatched();
  } catch (e) {
    message.error(String(e));
  }
}

async function catchupNow() {
  busy.value = true;
  try {
    await invoke("watch_catchup");
    message.success(t("watch.catchupDone"));
  } catch (e) {
    message.error(errText(e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="watch">
    <n-card :title="t('watch.title')" size="small" class="card">
      <p class="desc">{{ t("watch.desc") }}</p>
      <n-input
        v-model:value="link"
        :placeholder="t('watch.input')"
        size="small"
        @keyup.enter="addWatch"
      />
      <div class="row">
        <n-button type="primary" size="small" :loading="adding" @click="addWatch">
          {{ t("watch.add") }}
        </n-button>
        <n-checkbox v-model:checked="backfill" size="small">
          {{ t("watch.backfill") }}
        </n-checkbox>
        <n-button quaternary size="small" :loading="busy" style="margin-left: auto" @click="catchupNow">
          {{ t("watch.catchup") }}
        </n-button>
      </div>
    </n-card>

    <n-card size="small" class="card">
      <n-empty v-if="!store.watched.length" :description="t('watch.empty')" />
      <div v-else class="list">
        <div v-for="w in store.watched" :key="w.raw_id" class="item">
          <span class="icon">{{ w.enabled ? "👁" : "⏸" }}</span>
          <div class="body">
            <div class="title">{{ w.title }}</div>
            <div class="meta">
              <n-tag size="tiny" :type="w.enabled ? 'success' : 'default'">
                {{ w.enabled ? t("watch.watching") : t("watch.paused") }}
              </n-tag>
              <span class="track">{{ t("watch.track", { n: w.last_msg_id }) }}</span>
            </div>
          </div>
          <div class="actions">
            <n-button quaternary size="tiny" @click="toggle(w)">
              {{ w.enabled ? t("watch.pause") : t("watch.resume") }}
            </n-button>
            <n-button quaternary size="tiny" type="error" @click="remove(w)">
              {{ t("watch.delete") }}
            </n-button>
          </div>
        </div>
      </div>
    </n-card>
  </div>
</template>

<style scoped>
.watch { display: flex; flex-direction: column; gap: 12px; }
.desc { font-size: 12px; color: #8b949e; margin-bottom: 10px; }
.row { display: flex; align-items: center; gap: 10px; margin-top: 10px; }
.list { max-height: 420px; overflow-y: auto; }
.item {
  display: flex; align-items: center; gap: 10px;
  padding: 8px 4px; border-bottom: 1px solid #21262d; font-size: 13px;
}
.item:last-child { border-bottom: 0; }
.body { flex: 1; min-width: 0; }
.title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.meta { display: flex; align-items: center; gap: 8px; margin-top: 2px; }
.track { color: #8b949e; font-size: 11px; }
.actions { display: flex; gap: 2px; flex-shrink: 0; }
</style>
