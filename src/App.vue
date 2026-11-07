<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke, setUnauthorizedHandler, setToken, isDesktop } from "./lib/backend";
import {
  NConfigProvider,
  NMessageProvider,
  NButton,
  NInput,
  darkTheme,
  zhCN,
  dateZhCN,
  enUS,
  dateEnUS,
} from "naive-ui";
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

const webNeedToken = ref(false);
const webToken = ref("");
const languageSaving = ref(false);
const languageError = ref("");
if (!isDesktop) {
  setUnauthorizedHandler(() => {
    webNeedToken.value = true;
  });
}

function submitWebToken() {
  if (!webToken.value.trim()) return;
  setToken(webToken.value.trim());
  webNeedToken.value = false;
  location.reload();
}

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

async function toggleLang() {
  if (languageSaving.value) return;
  const previous = locale.value as "zh-CN" | "en";
  const next = locale.value === "zh-CN" ? "en" : "zh-CN";
  languageSaving.value = true;
  languageError.value = "";
  setLocale(next);
  try { await invoke("save_settings", { lang: next }); await store.refreshSettings(); }
  catch (error) { setLocale(previous); languageError.value = error instanceof Error ? error.message : String(error); }
  finally { languageSaving.value = false; }
}

watch(tab, (v) => {
  if (v === "download") void store.refreshFiles().catch(() => undefined);
});

watch(
  () => store.status?.configured,
  (ok) => {
    if (ok === false) tab.value = "settings";
  },
  { immediate: true },
);

function retryInit() { void store.init().catch(() => undefined); }
onMounted(retryInit);
onBeforeUnmount(() => store.dispose());
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
      <div v-if="webNeedToken" class="token-gate">
        <div class="token-card">
          <h2>tg-grab</h2>
          <p>请输入访问令牌（服务端启动时打印）</p>
          <n-input
            v-model:value="webToken"
            type="password"
            show-password-on="click"
            placeholder="access token"
            @keyup.enter="submitWebToken"
          />
          <n-button type="primary" style="margin-top: 10px" @click="submitWebToken">确定</n-button>
        </div>
      </div>
      <div v-else class="shell">
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
              :disabled="n.disabled"
              :aria-label="n.label"
              :aria-current="tab === n.key ? 'page' : undefined"
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
            <button class="ghost-btn" :disabled="languageSaving" style="text-align: center" @click="toggleLang">
              {{ locale === "zh-CN" ? "English" : "中文" }}
            </button>
          </div>
        </aside>

        <!-- Content -->
        <main class="content">
          <div v-if="store.initState === 'error'" class="connection-banner" role="alert">
            {{ t('common.initializationFailed') }}: {{ store.initError }}
            <n-button size="small" @click="retryInit">{{ t('common.retry') }}</n-button>
          </div>
          <div v-else-if="store.initState === 'loading'" class="connection-banner" role="status">{{ t('common.loading') }}</div>
          <div v-else-if="!isDesktop && store.connection !== 'connected'" class="connection-banner" role="status">{{ t('common.reconnecting') }}</div>
          <div v-if="languageError" class="connection-banner" role="alert">{{ languageError }}</div>
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
.connection-banner { padding: 10px 24px; color: var(--yellow); display: flex; align-items: center; gap: 12px; flex-shrink: 0; }
.shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}
.token-gate {
  height: 100vh;
  display: flex; align-items: center; justify-content: center;
  background: var(--bg);
}
.token-card {
  background: var(--surface);
  border: 1px solid var(--border-soft);
  border-radius: 14px;
  padding: 28px 26px;
  width: 340px;
  text-align: center;
}
.token-card h2 { color: var(--text); margin-bottom: 6px; }
.token-card p { font-size: 12.5px; color: var(--text-dim); margin-bottom: 14px; }
</style>
