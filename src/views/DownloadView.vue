<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import {
  NCard,
  NButton,
  NInput,
  NProgress,
  NTag,
  NEmpty,
  useMessage,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import type { TaskResult } from "../types";

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

const input = ref("");
const submitting = ref(false);

function fmtSize(bytes: number | undefined | null): string {
  const b = bytes ?? 0;
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(2)} GB`;
}

const ERR_KEYS = [
  "INVALID_LINK",
  "INVITE_LINK",
  "NOT_LOGGED_IN",
  "MESSAGE_NOT_FOUND",
  "NOT_A_VIDEO",
  "RATE_LIMITED",
  "CHANNEL_UNAVAILABLE",
  "API_NOT_CONFIGURED",
] as const;

function errText(e: unknown): string {
  const raw = String(e);
  const code = raw.includes(":") ? raw.split(":")[0].trim() : raw.trim();
  for (const k of ERR_KEYS) {
    if (code.startsWith(k)) return t(`download.errors.${k}`);
  }
  return raw;
}

function statusMeta(r: TaskResult): { icon: string; label: string; type: "success" | "error" | "warning" | "default" } {
  if (r.status === "success") {
    return r.cached
      ? { icon: "📦", label: t("download.status.cached"), type: "warning" }
      : { icon: "✅", label: t("download.status.success"), type: "success" };
  }
  return { icon: "❌", label: errText(r.error ?? "FAILED"), type: "error" };
}

function shortUrl(url: string): string {
  const m = url.match(/t\.me\/(?:c\/)?[^/]+\/\d+/);
  return m ? m[0] : url;
}

async function openFile(name: string) {
  try {
    await invoke("reveal_file", { name });
  } catch {
    /* no-op */
  }
}

async function startDownload() {
  const urls = input.value
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  if (!urls.length) return;
  submitting.value = true;
  try {
    const ids = await invoke<number[]>("download_urls", { urls });
    message.success(t("download.queued", { n: ids.length }));
    input.value = "";
    await store.refreshQueue();
  } catch (e) {
    message.error(errText(e));
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="download">
    <!-- Active progress -->
    <n-card v-if="store.active" size="small" class="card">
      <div class="active-name">{{ store.active.name || shortUrl(store.active.url) }}</div>
      <n-progress
        type="line"
        :percentage="Number(store.active.percent.toFixed(1))"
        :height="8"
        border-radius="4"
        indicator-placement="inside"
        processing
      />
      <div class="active-meta">
        <span>{{ fmtSize(store.active.current) }} / {{ fmtSize(store.active.total) }}</span>
        <span>{{ t("download.active") }}</span>
      </div>
    </n-card>

    <!-- Input -->
    <n-card :title="t('download.title')" size="small" class="card">
      <n-input
        v-model:value="input"
        type="textarea"
        :rows="4"
        :placeholder="t('download.placeholder')"
      />
      <div class="row">
        <n-button type="primary" :loading="submitting" @click="startDownload">
          {{ t("download.start") }}
        </n-button>
        <n-button quaternary @click="input = ''">{{ t("download.clear") }}</n-button>
      </div>
    </n-card>

    <!-- Task list -->
    <n-card :title="`${t('download.queueTitle')} (${store.pending.length + store.results.length})`" size="small" class="card">
      <n-empty v-if="!store.pending.length && !store.results.length" :description="t('download.emptyQueue')" />
      <div v-else class="list">
        <div v-for="j in store.pending" :key="j.id" class="item">
          <span class="icon">⏳</span>
          <span class="body" :title="j.url">{{ shortUrl(j.url) }}</span>
          <n-tag size="tiny">{{ t("download.status.queued") }}</n-tag>
        </div>
        <div v-for="r in [...store.results].reverse()" :key="r.id" class="item">
          <span class="icon">{{ statusMeta(r).icon }}</span>
          <span class="body" :title="r.url">
            {{ r.name || shortUrl(r.url) }}
            <span class="meta">{{ statusMeta(r).label }}{{ r.size ? ` · ${fmtSize(r.size)}` : "" }}</span>
          </span>
          <a
            v-if="r.status === 'success' && r.name"
            class="dl"
            href="#"
            @click.prevent="openFile(r.name!)"
          >
            ⤓
          </a>
        </div>
      </div>
    </n-card>

    <!-- Files -->
    <n-card :title="`${t('download.filesTitle')} (${store.files.length})`" size="small" class="card">
      <n-empty v-if="!store.files.length" description="—" />
      <div v-else class="list">
        <div v-for="f in store.files" :key="f.name" class="item">
          <span class="icon">💾</span>
          <span class="body">{{ f.name }}<span class="meta">{{ fmtSize(f.size) }}</span></span>
          <a class="dl" href="#" @click.prevent="openFile(f.name)">⤓</a>
        </div>
      </div>
    </n-card>
  </div>
</template>


<style scoped>
.download { display: flex; flex-direction: column; gap: 12px; }
.card { border-radius: 8px; }
.row { display: flex; gap: 8px; margin-top: 10px; }
.active-name { font-size: 13px; margin-bottom: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.active-meta { display: flex; justify-content: space-between; font-size: 12px; color: #8b949e; margin-top: 4px; }
.list { max-height: 320px; overflow-y: auto; }
.item {
  display: flex; align-items: center; gap: 10px;
  padding: 8px 4px; border-bottom: 1px solid #21262d; font-size: 13px;
}
.item:last-child { border-bottom: 0; }
.icon { flex-shrink: 0; }
.body { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.meta { color: #8b949e; font-size: 11px; margin-left: 6px; }
.dl { color: #58a6ff; text-decoration: none; flex-shrink: 0; }
</style>
