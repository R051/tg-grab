<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  invoke,
  setUnauthorizedHandler,
  setToken,
  isDesktop,
} from "./lib/backend";
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
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
import LibraryView from "./views/LibraryView.vue";
import FilesView from "./views/FilesView.vue";
import CloseWindowDialog from "./components/CloseWindowDialog.vue";

import type { GlobalThemeOverrides } from "naive-ui";

const { t, locale } = useI18n();
const store = useAppStore();
const tab = computed({
  get: () => store.currentView,
  set: (value: string) => {
    store.currentView = value;
  },
});
const channelTab = computed({
  get: () => store.channelMode,
  set: (value: string) => {
    store.channelMode = value;
  },
});
watch(
  () => store.channelRequest,
  () => {
    channelTab.value = "browse";
  },
);

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
    bodyColor: "#101114",
    cardColor: "#17191e",
    inputColor: "#0d0f12",
    actionColor: "#17191e",
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
  {
    key: "history",
    label: t("app.tabs.history"),
    icon: "channel",
    badge: null,
    disabled: false,
  },
  {
    key: "library",
    label: t("app.tabs.library"),
    icon: "star",
    badge: store.library.items.length || null,
    disabled: false,
  },
  {
    key: "download",
    label: t("app.tabs.download"),
    icon: "download",
    badge: store.pending.length + store.activeTasks.length || null,
    disabled: false,
  },
  {
    key: "files",
    label: t("app.tabs.files"),
    icon: "folder",
    badge: null,
    disabled: false,
  },
  {
    key: "settings",
    label: t("app.tabs.settings"),
    icon: "settings",
    badge: null,
    disabled: false,
  },
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
  try {
    await invoke("save_settings", { lang: next });
    await store.refreshSettings();
  } catch (error) {
    setLocale(previous);
    languageError.value =
      error instanceof Error ? error.message : String(error);
  } finally {
    languageSaving.value = false;
  }
}

watch(tab, (v) => {
  if (v === "files") void store.refreshFiles().catch(() => undefined);
});

const pageTitle = computed(
  () =>
    navs.value.find((item) => item.key === tab.value)?.label ??
    t("app.tabs.history"),
);
const connectionLabel = computed(() =>
  store.status?.connected
    ? t("workspace.connected")
    : store.configured
      ? t("workspace.offline")
      : t("workspace.local"),
);

function retryInit() {
  void store.init().catch(() => undefined);
}
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
      <n-dialog-provider>
        <CloseWindowDialog />
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
            <n-button
              type="primary"
              style="margin-top: 10px"
              @click="submitWebToken"
              >确定</n-button
            >
          </div>
        </div>
        <div v-else class="shell">
          <!-- Sidebar -->
          <aside class="sidebar">
            <div class="brand">
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                class="logo"
              >
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
              <button
                class="user-chip account-entry"
                :title="t('simple.account')"
                @click="store.openAccount()"
              >
                <span class="avatar">{{
                  store.authorized ? (store.user ?? "?")[0] : "?"
                }}</span>
                <span class="account-label"
                  ><span class="name">{{
                    store.authorized ? store.user : t("simple.account")
                  }}</span
                  ><small>{{
                    store.authorized ? t("app.signedIn") : t("app.notSignedIn")
                  }}</small></span
                >
              </button>
              <button
                class="ghost-btn"
                :disabled="languageSaving"
                style="text-align: center"
                @click="toggleLang"
              >
                {{ locale === "zh-CN" ? "English" : "中文" }}
              </button>
            </div>
          </aside>

          <!-- Content -->
          <main class="content">
            <div class="workspace-bar">
              <div class="workspace-path">
                <strong>{{ pageTitle }}</strong>
              </div>
              <span
                class="workspace-connection"
                :class="{ online: store.status?.connected }"
                ><i />{{ connectionLabel }}</span
              >
            </div>
            <div
              v-if="store.initState === 'error'"
              class="connection-banner"
              role="alert"
            >
              {{ t("common.initializationFailed") }}: {{ store.initError }}
              <n-button size="small" @click="retryInit">{{
                t("common.retry")
              }}</n-button>
            </div>
            <div
              v-else-if="store.initState === 'loading'"
              class="connection-banner"
              role="status"
            >
              {{ t("common.loading") }}
            </div>
            <div
              v-else-if="!isDesktop && store.connection !== 'connected'"
              class="connection-banner"
              role="status"
            >
              {{ t("common.reconnecting") }}
            </div>
            <div v-if="languageError" class="connection-banner" role="alert">
              {{ languageError }}
            </div>
            <div v-show="tab === 'download'" class="page">
              <DownloadView />
            </div>
            <div v-show="tab === 'history'" class="page">
              <header class="view-heading">
                <div>
                  <p class="eyebrow">{{ t("app.tabs.history") }}</p>
                  <h2>
                    {{
                      channelTab === "auto"
                        ? t("history.autoTab")
                        : t("history.title")
                    }}
                  </h2>
                  <p class="view-description">
                    {{
                      channelTab === "auto"
                        ? t("workspace.watchHint")
                        : t("workspace.historyHint")
                    }}
                  </p>
                </div>
              </header>
              <div class="channel-tabs">
                <button
                  :class="{ on: channelTab === 'browse' }"
                  @click="channelTab = 'browse'"
                >
                  {{ t("history.browseTab") }}</button
                ><button
                  :class="{ on: channelTab === 'auto' }"
                  @click="channelTab = 'auto'"
                >
                  {{ t("history.autoTab") }}
                  <span v-if="store.watched.length">{{
                    store.watched.length
                  }}</span>
                </button>
              </div>
              <div v-show="channelTab === 'browse'" class="channel-pane">
                <HistoryView />
              </div>
              <div v-show="channelTab === 'auto'" class="channel-pane">
                <WatchView />
              </div>
            </div>
            <div v-show="tab === 'library'" class="page"><LibraryView /></div>
            <div v-show="tab === 'files'" class="page"><FilesView /></div>
            <div v-show="tab === 'settings'" class="page">
              <SettingsView />
            </div>
          </main>
        </div>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
.connection-banner {
  padding: 10px 24px;
  color: var(--yellow);
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}
.shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}
.token-gate {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
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
.token-card h2 {
  color: var(--text);
  margin-bottom: 6px;
}
.token-card p {
  font-size: 12.5px;
  color: var(--text-dim);
  margin-bottom: 14px;
}
</style>
