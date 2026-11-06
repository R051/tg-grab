<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import {
  NConfigProvider,
  NMessageProvider,
  NTabs,
  NTabPane,
  NTag,
  NButton,
  darkTheme,
  zhCN,
  dateZhCN,
  enUS,
  dateEnUS,
  type GlobalThemeOverrides,
} from "naive-ui";
import { useAppStore } from "./stores/app";
import { setLocale } from "./i18n";
import DownloadView from "./views/DownloadView.vue";
import WatchView from "./views/WatchView.vue";
import HistoryView from "./views/HistoryView.vue";
import SettingsView from "./views/SettingsView.vue";

const { t, locale } = useI18n();
const store = useAppStore();
const tab = ref("download");

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#2ea043",
    primaryColorHover: "#3fb950",
    primaryColorPressed: "#238636",
    bodyColor: "#0d1117",
    cardColor: "#161b22",
    modalColor: "#161b22",
    popoverColor: "#161b22",
  },
};

const naiveLocale = computed(() => (locale.value === "zh-CN" ? zhCN : enUS));
const naiveDateLocale = computed(() => (locale.value === "zh-CN" ? dateZhCN : dateEnUS));

function toggleLang() {
  const next = locale.value === "zh-CN" ? "en" : "zh-CN";
  setLocale(next);
  void invoke("save_settings", { lang: next }).catch(() => undefined);
}

onMounted(() => {
  void store.init();
});
</script>

<template>
  <n-config-provider
    :theme="darkTheme"
    :theme-overrides="themeOverrides"
    :locale="naiveLocale"
    :date-locale="naiveDateLocale"
    style="height: 100vh"
  >
    <n-message-provider>
      <div class="app">
        <header class="header">
          <div class="brand">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="logo">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="7 10 12 15 17 10" />
              <line x1="12" y1="15" x2="12" y2="3" />
            </svg>
            <div>
              <h1>tg-grab</h1>
              <p class="tagline">{{ t("app.tagline") }}</p>
            </div>
          </div>
          <div class="header-right">
            <n-button quaternary size="small" @click="toggleLang">
              {{ locale === "zh-CN" ? "EN" : "中文" }}
            </n-button>
            <n-tag v-if="store.authorized" round size="small" type="success">
              {{ store.user ?? "?" }}
            </n-tag>
            <n-tag v-else-if="store.configured" round size="small" type="warning">
              {{ t("settings.login.title") }}
            </n-tag>
          </div>
        </header>

        <div v-if="!store.configured" class="setup-hint">
          <p>{{ t("app.notConfigured") }}</p>
          <n-button type="primary" @click="tab = 'settings'">{{ t("app.goSettings") }}</n-button>
        </div>

        <n-tabs v-else v-model:value="tab" type="segment" animated size="large" class="tabs">
          <n-tab-pane name="download" :tab="t('app.tabs.download')">
            <DownloadView />
          </n-tab-pane>
          <n-tab-pane name="watch" :tab="t('app.tabs.watch')">
            <WatchView />
          </n-tab-pane>
          <n-tab-pane name="history" :tab="t('app.tabs.history')">
            <HistoryView />
          </n-tab-pane>
          <n-tab-pane name="settings" :tab="t('app.tabs.settings')">
            <SettingsView />
          </n-tab-pane>
        </n-tabs>
      </div>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body, #app { height: 100%; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: #0d1117;
  color: #c9d1d9;
}
.app {
  max-width: 880px;
  margin: 0 auto;
  padding: 20px 16px 40px;
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}
.brand { display: flex; align-items: center; gap: 10px; }
.logo { width: 28px; height: 28px; color: #2ea043; }
.brand h1 { font-size: 20px; color: #f0f6fc; font-weight: 700; }
.tagline { font-size: 12px; color: #8b949e; }
.header-right { display: flex; align-items: center; gap: 8px; }
.tabs { margin-top: 4px; }
.setup-hint {
  margin-top: 60px;
  text-align: center;
  color: #8b949e;
  display: flex;
  flex-direction: column;
  gap: 16px;
  align-items: center;
}
</style>
