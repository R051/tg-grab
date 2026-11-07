<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "../lib/backend";
import {
  NButton,
  NInput,
  NCheckbox,
  useMessage,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import AppIcon from "../components/AppIcon.vue";
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
  <div class="panel">
    <div class="panel-title">
      <AppIcon name="watch" :size="14" />
      {{ t("watch.title") }}
    </div>
    <p class="desc">{{ t("watch.desc") }}</p>
    <div class="row">
      <n-input
        v-model:value="link"
        :placeholder="t('watch.input')"
        size="small"
        @keyup.enter="addWatch"
      />
      <n-button type="primary" size="small" :loading="adding" @click="addWatch">
        {{ t("watch.add") }}
      </n-button>
    </div>
    <div class="row" style="margin-top: 10px">
      <n-checkbox v-model:checked="backfill" size="small">
        {{ t("watch.backfill") }}
      </n-checkbox>
      <n-button quaternary size="small" :loading="busy" style="margin-left: auto" @click="catchupNow">
        {{ t("watch.catchup") }}
      </n-button>
    </div>
  </div>

  <div class="panel">
    <div class="panel-title">
      {{ t("watch.title") }}
      <span class="count">{{ store.watched.length }}</span>
    </div>
    <div v-if="!store.watched.length" class="empty">
      <span class="empty-icon">📡</span>
      {{ t("watch.empty") }}
    </div>
    <div v-else class="rows">
      <div v-for="w in store.watched" :key="w.raw_id" class="row-item">
        <span class="row-icon">{{ w.enabled ? "👁" : "⏸" }}</span>
        <div class="row-body">
          <div class="row-title">{{ w.title }}</div>
          <div class="row-meta">
            <span class="pill" :class="w.enabled ? 'success' : 'queued'">
              {{ w.enabled ? t("watch.watching") : t("watch.paused") }}
            </span>
            <span>{{ t("watch.track", { n: w.last_msg_id }) }}</span>
          </div>
        </div>
        <div class="actions">
          <button class="ghost-btn" @click="toggle(w)">
            {{ w.enabled ? t("watch.pause") : t("watch.resume") }}
          </button>
          <button class="ghost-btn danger" @click="remove(w)">{{ t("watch.delete") }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.desc { font-size: 12px; color: var(--text-dim); margin: -4px 0 10px; }
.row { display: flex; align-items: center; gap: 10px; }
.actions { display: flex; gap: 6px; flex-shrink: 0; }
.ghost-btn.danger:hover { border-color: var(--red); color: var(--red); }
</style>
