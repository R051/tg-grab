<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { NConfigProvider, NMessageProvider, darkTheme, zhCN, dateZhCN, enUS, dateEnUS } from "naive-ui";
import { useAppStore } from "./stores/app";
import { setLocale } from "./i18n";
import AppIcon from "./components/AppIcon.vue";
import DownloadView from "./views/DownloadView.vue";
import WatchView from "./views/WatchView.vue";
import HistoryView from "./views/HistoryView.vue";
import SettingsView from "./views/SettingsView.vue";

import type { GlobalThemeOverrides } from "naive-ui";

const { t, locale } = useI18n();
const store = useAppStore();
const tab = ref("download");

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#10b981",
    primaryColorHover: "#34d399",
    primaryColorPressed: "#059669",
    primaryColorSuppl: "#34d399",
    bodyColor: "#0a0a0c",
    cardColor: "#111114",
    inputColor: "#0c0d10",
    actionColor: "#111114",
    borderColor: "rgba(255, 255, 255, 0.09)",
    dividerColor: "rgba(255, 255, 255, 0.06)",
    borderRadius: "10px",
    fontWeight: "500",
  },
  Button: {
    colorPrimary: "#10b981",
    colorHoverPrimary: "#34d399",
    colorPressedPrimary: "#059669",
    borderPrimary: "1px solid rgba(16, 185, 129, 0.4)",
  },
  Card: { borderRadius: "14px" },
};

const navs = computed(() => [
  { key: "download", label: t("app.tabs.download"), icon: "download", badge: store.pending.length || null, disabled: !store.configured },
  { key: "watch", label: t("app.tabs.watch"), icon: "watch", badge: store.watched.length || null, disabled: !store.configured },
  { key: "history", label: t("app.tabs.history"), icon: "history", badge: null, disabled: !store.configured },
  { key: "settings", label: t("app.tabs.settings"), icon: "settings", badge: null, disabled: false },
]);

function go(key: string, disabled?: boolean) {
  if (disabled) return;
  tab.value = key;
}

function toggleLang() {
  const next = locale.value === "zh-CN" ? "en" : "zh-CN";
  setLocale(next);
  void invoke("save_settings", { lang: next }).catch(() => undefined);
}

watch(tab, (v) => {
  if (v === "download") void store.refreshFiles();
});

watch(
  () => store.configured,
  (ok) => {
    if (!ok) tab.value = "settings";
  },
  { immediate: true },
);

onMounted(() => {
  void store.init();
});
</script>

<template>
  <n-config-provider
    :theme="darkTheme"
    :theme-overrides="themeOverrides"
    :locale="locale === 'zh-CN' ? zhCN : enUS"
    :date-locale="locale === 'zh-CN' ? dateZhCN : dateEnUS"
    style="height: 100vh"
  >
    <n-message-provider>
      <div class="shell">
        <!-- Sidebar -->
        <aside class="sidebar">
          <div class="brand">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="logo">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="7 10 12 15 17 10" />
              <line x1="12" y1="15" x2="12" y2="3" />
            </svg>
            <div>
              <h1>tg-grab</h1>
              <p class="sub">{{ t("app.tagline") }}</p>
            </div>
          </div>

          <nav class="nav">
            <button
              v-for="n in navs"
              :key="n.key"
              class="nav-item"
              :class="{ active: tab === n.key, disabled: n.disabled }"
              @click="go(n.key, n.disabled)"
            >
              <AppIcon :name="n.icon" />
              <span class="label">{{ n.label }}</span>
              <span v-if="n.badge" class="nav-badge">{{ n.badge }}</span>
            </button>
          </nav>

          <div class="sidebar-footer">
            <div v-if="store.authorized" class="user-chip">
              <div class="avatar">{{ (store.user ?? "?")[0] }}</div>
              <div style="min-width: 0">
                <div class="name">{{ store.user }}</div>
                <div class="status">{{ t("app.signedIn") }}</div>
              </div>
            </div>
            <div v-else class="user-chip">
              <div class="avatar" style="background: #30363d">?</div>
              <div class="name" style="color: var(--text-dim)">{{ t("app.notSignedIn") }}</div>
            </div>
            <button class="ghost-btn" style="text-align: center" @click="toggleLang">
              {{ locale === "zh-CN" ? "English" : "中文" }}
            </button>
          </div>
        </aside>

        <!-- Content -->
        <main class="content">
          <div v-show="tab === 'download'" class="page">
            <DownloadView />
          </div>
          <div v-show="tab === 'watch'" class="page">
            <WatchView />
          </div>
          <div v-show="tab === 'history'" class="page">
            <HistoryView />
          </div>
          <div v-show="tab === 'settings'" class="page">
            <SettingsView />
          </div>
        </main>
      </div>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
.shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}
</style>
