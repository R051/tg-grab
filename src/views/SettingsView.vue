<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke, isDesktop } from "../lib/backend";
import { errorText } from "../lib/errors";
import type { SignInOutcome, MediaKind, AppSettings } from "../types";
import {
  NCard,
  NInput,
  NButton,
  NFormItem,
  NSelect,
  NInputNumber,
  NText,
  NCheckboxGroup,
  NCheckbox,
  NSwitch,
  NCollapse,
  NCollapseItem,
  useMessage,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import { setLocale } from "../i18n";
import { exportCollections, formatSize } from "../lib/library";

const { t } = useI18n();
const store = useAppStore();
const desktop = computed(() => isDesktop);
const section = computed({
  get: () => store.settingsSection,
  set: (value: string) => {
    store.settingsSection = value;
  },
});
const sectionKeys = ["account", "storage", "rules", "general", "data"] as const;
const message = useMessage();
const msgOpts = { duration: 6000, keepAliveOnHover: true };

// API credentials
const apiId = ref("");
const apiHash = ref("");
const savingApi = ref(false);

// Proxy
const proxyUrl = ref("");
const savingProxy = ref(false);

// Login wizard
const phone = ref("");
const code = ref("");
const password = ref("");
const step = ref<"phone" | "code" | "password">("phone");
const pwHint = ref<string | null>(null);
const loggingIn = ref(false);

// Preferences
const concurrency = ref(1);
const lang = ref("zh-CN");
const clearingThumbs = ref(false);
const downloadsDir = ref("");
const mediaKinds = ref<MediaKind[]>(["video"]);
const organizeByChannel = ref(false);
const classifyByType = ref(false);
const closeAction = ref<"ask" | "tray" | "exit">("ask");
const choosingDirectory = ref(false);
const checkingConnection = ref(false);
const exporting = ref(false);
const preferenceError = ref("");
const savingPrefs = ref(false);

const closeOptions = computed(() =>
  ["ask", "tray", "exit"].map((value) => ({
    value,
    label: t(`windowUi.${value}`),
  })),
);

const settingsLoaded = computed(() => store.settings !== null);

const langOptions = [
  { label: "中文", value: "zh-CN" },
  { label: "English", value: "en" },
];

// Form starts blank; sync once the store's async settings arrive — but
// never overwrite fields the user has already touched.
type DirtyField =
  | "apiId"
  | "apiHash"
  | "proxyUrl"
  | "concurrency"
  | "lang"
  | "downloadsDir"
  | "mediaKinds"
  | "classifyByType"
  | "organizeByChannel"
  | "closeAction";
const dirty = reactive<Record<DirtyField, boolean>>({
  apiId: false,
  apiHash: false,
  proxyUrl: false,
  concurrency: false,
  lang: false,
  downloadsDir: false,
  mediaKinds: false,
  classifyByType: false,
  organizeByChannel: false,
  closeAction: false,
});
const prefKeys: DirtyField[] = [
  "concurrency",
  "lang",
  "downloadsDir",
  "mediaKinds",
  "classifyByType",
  "organizeByChannel",
  "closeAction",
];
const prefsDirty = computed(() => prefKeys.some((key) => dirty[key]));
const pathPreview = computed(() =>
  [
    downloadsDir.value.trim() ||
      store.settings?.default_downloads_dir ||
      store.settings?.effective_downloads_dir ||
      "…",
    ...(organizeByChannel.value ? [t("media.channel"), "YYYY-MM-DD"] : []),
    ...(classifyByType.value ? [t("library.type")] : []),
  ].join(" / "),
);
const storageSummary = computed(() =>
  t("files.summary", {
    n: store.files.length,
    size: formatSize(store.files.reduce((sum, file) => sum + file.size, 0)),
  }),
);
function syncSettings(s: AppSettings | null) {
  if (!s) return;
  if (!dirty.apiId) apiId.value = s.api_id?.toString() ?? "";
  if (!dirty.apiHash) apiHash.value = s.api_hash ?? "";
  if (!dirty.proxyUrl) proxyUrl.value = s.proxy_url ?? "";
  if (!dirty.concurrency) concurrency.value = s.concurrency;
  if (!dirty.lang) lang.value = s.lang;
  if (!dirty.downloadsDir) downloadsDir.value = s.downloads_dir ?? "";
  if (!dirty.mediaKinds) mediaKinds.value = s.media_kinds?.slice() ?? ["video"];
  if (!dirty.organizeByChannel)
    organizeByChannel.value = s.organize_by_channel ?? false;
  if (!dirty.classifyByType) classifyByType.value = s.classify_by_type ?? false;
  if (!dirty.closeAction) closeAction.value = s.close_action ?? "ask";
}
watch(() => store.settings, syncSettings, { immediate: true });
function discardPrefs() {
  for (const key of prefKeys) dirty[key] = false;
  preferenceError.value = "";
  syncSettings(store.settings);
}
async function chooseDirectory() {
  if (choosingDirectory.value) return;
  choosingDirectory.value = true;
  try {
    const path = await invoke<string | null>("choose_download_directory");
    if (path) {
      downloadsDir.value = path;
      markTouched("downloadsDir");
    }
  } catch (error) {
    message.error(errText(error));
  } finally {
    choosingDirectory.value = false;
  }
}
async function testConnection() {
  if (checkingConnection.value) return;
  checkingConnection.value = true;
  try {
    await store.refreshStatus();
    if (store.status?.connected) message.success(t("settings.connectionOk"));
    else message.error(store.status?.error ?? t("settings.connectionFailed"));
  } catch (error) {
    message.error(errText(error));
  } finally {
    checkingConnection.value = false;
  }
}
async function exportBackup() {
  if (exporting.value) return;
  exporting.value = true;
  try {
    if (await exportCollections()) message.success(t("library.exported"));
  } catch (error) {
    message.error(errText(error));
  } finally {
    exporting.value = false;
  }
}
function markTouched(field: DirtyField) {
  dirty[field] = true;
}

watch(
  () => store.status?.authorized,
  (authorized) => {
    if (authorized) {
      step.value = "phone";
      code.value = "";
      password.value = "";
      pwHint.value = null;
    }
  },
);

const loggedIn = computed(() => store.authorized);
const apiConfigured = computed(() => store.status?.configured ?? false);
const apiHashSaved = computed(() => store.settings?.api_hash_set ?? false);
const connectionError = computed(() => store.status?.error ?? "");
const effectiveRoot = computed(
  () => store.settings?.effective_downloads_dir ?? "",
);
const canTestConnection = computed(
  () => apiConfigured.value && !dirty.proxyUrl,
);
function showCollections() {
  store.currentView = "library";
}
const KNOWN_LEAKED_IDS = ["2040"];
const usingLeakedId = computed(() => {
  const id = store.settings?.api_id;
  return id != null && KNOWN_LEAKED_IDS.includes(String(id));
});
const user = computed(() => store.user ?? "");

async function saveApi() {
  if (savingApi.value || !store.settings) return;
  if (
    !apiId.value.trim() ||
    (!apiHash.value.trim() && !store.settings?.api_hash_set)
  ) {
    message.error(t("settings.api.invalid"), msgOpts);
    return;
  }
  savingApi.value = true;
  const submitted = {
    apiId: apiId.value.trim(),
    apiHash: apiHash.value.trim(),
  };
  try {
    await invoke("save_api_config", submitted);
    if (apiId.value.trim() === submitted.apiId) dirty.apiId = false;
    if (apiHash.value.trim() === submitted.apiHash) dirty.apiHash = false;
    message.success(t("settings.api.saved"));
    await Promise.all([store.refreshSettings(), store.refreshStatus()]);
  } catch (e) {
    message.error(errText(e), msgOpts);
  } finally {
    savingApi.value = false;
  }
}

async function saveProxy() {
  if (savingProxy.value || !store.settings) return;
  savingProxy.value = true;
  const submitted = proxyUrl.value.trim();
  try {
    await invoke("save_proxy", { proxyUrl: submitted });
    if (proxyUrl.value.trim() === submitted) dirty.proxyUrl = false;
    await Promise.all([store.refreshSettings(), store.refreshStatus()]);
    message.success(t("settings.proxy.saved"));
  } catch (e) {
    message.error(errText(e), msgOpts);
  } finally {
    savingProxy.value = false;
  }
}

async function resendCode() {
  await sendCode();
}

async function sendCode() {
  if (loggingIn.value || !phone.value.trim()) return;
  loggingIn.value = true;
  try {
    await invoke("auth_send_code", { phone: phone.value.trim() });
    step.value = "code";
    code.value = "";
    password.value = "";
    pwHint.value = null;
    message.success(t("settings.login.codeSent"));
  } catch (e) {
    message.error(errText(e), msgOpts);
  } finally {
    loggingIn.value = false;
  }
}

async function verifyCode() {
  if (loggingIn.value || !code.value.trim()) return;
  loggingIn.value = true;
  try {
    const r = await invoke<SignInOutcome>("auth_sign_in", {
      code: code.value.trim(),
    });
    if (r.status === "ok") {
      message.success(t("settings.login.loginSuccess"));
      step.value = "phone";
      code.value = "";
      await store.refreshStatus();
    } else if (r.status === "need_password") {
      // need_password
      pwHint.value = r.hint ?? null;
      step.value = "password";
    } else {
      throw new Error("INVALID_RESPONSE");
    }
  } catch (e) {
    message.error(errText(e), msgOpts);
  } finally {
    loggingIn.value = false;
  }
}

async function submitPassword() {
  if (loggingIn.value || !password.value) return;
  loggingIn.value = true;
  try {
    await invoke("auth_check_password", { password: password.value });
    message.success(t("settings.login.loginSuccess"));
    step.value = "phone";
    password.value = "";
    await store.refreshStatus();
  } catch (e) {
    message.error(errText(e), msgOpts);
  } finally {
    loggingIn.value = false;
  }
}

async function logout() {
  if (!window.confirm(t("settings.login.logoutConfirm"))) return;
  try {
    await invoke("logout");
    await store.refreshStatus();
  } catch (e) {
    message.error(errText(e), msgOpts);
  }
}

async function clearThumbs() {
  if (clearingThumbs.value) return;
  if (!window.confirm(t("settings.prefs.clearThumbsConfirm"))) return;
  clearingThumbs.value = true;
  try {
    const r = await invoke<{ freed: number }>("clear_thumbs");
    message.success(
      t("settings.prefs.thumbsCleared", {
        mb: ((r.freed ?? 0) / 1048576).toFixed(1),
      }),
    );
  } catch (e) {
    message.error(errText(e));
  } finally {
    clearingThumbs.value = false;
  }
}

async function savePrefs() {
  if (savingPrefs.value || !store.settings || !concurrency.value) return;
  preferenceError.value = "";
  if (!mediaKinds.value.length) {
    preferenceError.value = t("settings.prefs.needKind");
    return;
  }
  savingPrefs.value = true;
  const submitted = {
    concurrency: concurrency.value,
    lang: lang.value,
    downloadsDir: downloadsDir.value.trim(),
    mediaKinds: [...mediaKinds.value],
    classifyByType: classifyByType.value,
    organizeByChannel: organizeByChannel.value,
    closeAction: closeAction.value,
  };
  try {
    await invoke("save_settings", submitted);
    for (const key of prefKeys) dirty[key] = false;
    setLocale(submitted.lang as "zh-CN" | "en");
    await store.refreshSettings();
    message.success(t("settings.prefs.saved"));
  } catch (error) {
    preferenceError.value = errText(error);
    message.error(preferenceError.value);
  } finally {
    savingPrefs.value = false;
  }
}

function errText(error: unknown): string {
  return errorText(error, t);
}
</script>

<template>
  <div class="settings">
    <header class="view-heading">
      <div>
        <p class="eyebrow">{{ t("settings.prefs.title") }}</p>
        <h2>{{ t("app.tabs.settings") }}</h2>
        <p class="view-description">{{ t("prefsUi.accountHint") }}</p>
      </div>
      <span class="pill" :class="loggedIn ? 'success' : 'queued'">{{
        loggedIn ? t("app.signedIn") : t("app.notSignedIn")
      }}</span>
    </header>
    <div class="settings-sections" role="tablist">
      <button
        v-for="key in sectionKeys"
        :key="key"
        role="tab"
        :aria-selected="section === key"
        :class="{ on: section === key }"
        @click="section = key"
      >
        {{ t(`simple.settings.${key}`) }}
      </button>
    </div>
    <div class="settings-body">
      <section v-show="section === 'account'" class="settings-section">
        <n-card :title="t('prefsUi.account')" class="card" size="small">
          <div v-if="usingLeakedId" class="field-error">
            {{ t("settings.api.leakDesc") }}
          </div>
          <n-collapse
            :default-expanded-names="apiConfigured ? [] : ['credentials']"
            ><n-collapse-item
              name="credentials"
              :title="t('settings.api.title')"
              ><p class="desc">
                {{ t("settings.api.desc") }}
                <a
                  href="https://my.telegram.org/apps"
                  target="_blank"
                  rel="noreferrer"
                  >my.telegram.org/apps</a
                >
              </p>
              <div class="settings-grid">
                <n-form-item :label="t('settings.api.apiId')"
                  ><n-input
                    v-model:value="apiId"
                    placeholder="12345678"
                    @update:value="markTouched('apiId')" /></n-form-item
                ><n-form-item :label="t('settings.api.apiHash')"
                  ><n-input
                    v-model:value="apiHash"
                    type="password"
                    show-password-on="click"
                    :placeholder="
                      apiHashSaved ? t('prefsUi.credentialsSaved') : 'API Hash'
                    "
                    @update:value="markTouched('apiHash')"
                /></n-form-item>
              </div>
              <n-button
                type="primary"
                size="small"
                :loading="savingApi"
                :disabled="!settingsLoaded"
                @click="saveApi"
                >{{ t("settings.api.save") }}</n-button
              ></n-collapse-item
            ></n-collapse
          >
          <div class="login-area">
            <div v-if="loggedIn" class="settings-row">
              <n-text type="success">{{
                t("settings.login.loggedAs", { user })
              }}</n-text
              ><span class="collection-spacer" /><n-button
                size="small"
                @click="logout"
                >{{ t("settings.login.logout") }}</n-button
              >
            </div>
            <n-text v-else-if="!apiConfigured" depth="3">{{
              t("settings.login.needApiFirst")
            }}</n-text
            ><template v-else
              ><n-form-item
                v-if="step === 'phone'"
                :label="t('settings.login.phone')"
                ><n-input
                  v-model:value="phone"
                  placeholder="+8613800138000"
                  :disabled="loggingIn"
                  @keyup.enter="sendCode" /></n-form-item
              ><n-form-item
                v-else-if="step === 'code'"
                :label="t('settings.login.code')"
                ><n-input
                  v-model:value="code"
                  :disabled="loggingIn"
                  @keyup.enter="verifyCode" /></n-form-item
              ><n-form-item v-else :label="t('settings.login.password')"
                ><div style="width: 100%">
                  <n-input
                    v-model:value="password"
                    type="password"
                    show-password-on="click"
                    :disabled="loggingIn"
                    @keyup.enter="submitPassword"
                  />
                  <p v-if="pwHint" class="desc">
                    {{ t("settings.login.passwordHint", { hint: pwHint }) }}
                  </p>
                </div></n-form-item
              >
              <div class="settings-row">
                <n-button
                  type="primary"
                  :loading="loggingIn"
                  @click="
                    step === 'phone'
                      ? sendCode()
                      : step === 'code'
                        ? verifyCode()
                        : submitPassword()
                  "
                  >{{
                    step === "phone"
                      ? t("settings.login.sendCode")
                      : step === "code"
                        ? t("settings.login.verify")
                        : t("settings.login.submitPassword")
                  }}</n-button
                ><n-button
                  v-if="step === 'code'"
                  :disabled="loggingIn"
                  @click="resendCode"
                  >{{ t("settings.login.resend") }}</n-button
                ><n-button
                  v-if="step !== 'phone'"
                  :disabled="loggingIn"
                  @click="
                    step = 'phone';
                    code = '';
                    password = '';
                    pwHint = null;
                  "
                  >{{ t("settings.login.changePhone") }}</n-button
                >
              </div></template
            >
          </div>
          <n-collapse
            ><n-collapse-item name="proxy" :title="t('settings.proxy.title')"
              ><p class="desc">{{ t("settings.proxy.desc") }}</p>
              <n-form-item :label="t('settings.proxy.title')"
                ><n-input
                  v-model:value="proxyUrl"
                  placeholder="socks5://127.0.0.1:7890"
                  @update:value="markTouched('proxyUrl')"
              /></n-form-item>
              <div class="settings-row">
                <n-button
                  :loading="savingProxy"
                  :disabled="!settingsLoaded"
                  @click="saveProxy"
                  >{{ t("settings.proxy.save") }}</n-button
                ><n-button
                  :loading="checkingConnection"
                  :disabled="!canTestConnection"
                  @click="testConnection"
                  >{{ t("settings.testConnection") }}</n-button
                >
              </div>
              <p v-if="connectionError" class="field-error">
                {{ connectionError }}
              </p></n-collapse-item
            ></n-collapse
          >
        </n-card>
      </section>
      <section v-show="section === 'storage'" class="settings-section">
        <n-card :title="t('prefsUi.storage')" class="card" size="small"
          ><p class="desc">{{ t("prefsUi.storageHint") }}</p>
          <n-form-item :label="t('settings.prefs.downloadsDir')"
            ><div style="width: 100%">
              <div class="settings-row">
                <n-input
                  v-model:value="downloadsDir"
                  :disabled="savingPrefs"
                  :placeholder="
                    effectiveRoot || t('settings.prefs.downloadsDirHint')
                  "
                  @update:value="markTouched('downloadsDir')"
                /><n-button
                  v-if="desktop"
                  :loading="choosingDirectory"
                  :disabled="savingPrefs"
                  @click="chooseDirectory"
                  >{{ t("prefsUi.chooseFolder") }}</n-button
                >
              </div>
              <p v-if="!desktop" class="desc">
                {{ t("prefsUi.serverDirectory") }}
              </p>
            </div></n-form-item
          >
          <div class="settings-grid">
            <n-form-item :label="t('prefsUi.organize')"
              ><n-switch
                v-model:value="organizeByChannel"
                :disabled="savingPrefs"
                @update:value="markTouched('organizeByChannel')" /></n-form-item
            ><n-form-item :label="t('settings.prefs.classifyByType')"
              ><n-switch
                v-model:value="classifyByType"
                :disabled="savingPrefs"
                @update:value="markTouched('classifyByType')"
            /></n-form-item>
          </div>
          <div class="path-preview">
            <span>{{ t("prefsUi.directoryPreview") }}</span
            ><code>{{ pathPreview }}</code>
          </div>
          <p class="desc">{{ storageSummary }}</p></n-card
        >
      </section>
      <section v-show="section === 'rules'" class="settings-section">
        <n-card :title="t('prefsUi.auto')" class="card" size="small"
          ><p class="desc">{{ t("prefsUi.autoHint") }}</p>
          <n-form-item :label="t('settings.prefs.mediaKinds')"
            ><n-checkbox-group
              v-model:value="mediaKinds"
              :disabled="savingPrefs"
              @update:value="markTouched('mediaKinds')"
              ><div class="settings-row">
                <n-checkbox
                  v-for="kind in ['video', 'photo', 'audio', 'document']"
                  :key="kind"
                  :value="kind"
                  >{{ t(`media.${kind}`) }}</n-checkbox
                >
              </div></n-checkbox-group
            ></n-form-item
          >
          <p v-if="!mediaKinds.length" class="field-error">
            {{ t("settings.prefs.needKind") }}
          </p>
          <n-form-item :label="t('settings.prefs.concurrency')"
            ><n-input-number
              v-model:value="concurrency"
              :min="1"
              :max="4"
              :disabled="savingPrefs"
              style="width: 140px"
              @update:value="markTouched('concurrency')" /></n-form-item
        ></n-card>
      </section>
      <section v-show="section === 'general'" class="settings-section">
        <n-card :title="t('prefsUi.appearance')" class="card" size="small"
          ><div class="settings-grid">
            <n-form-item :label="t('settings.prefs.lang')"
              ><n-select
                v-model:value="lang"
                :options="langOptions"
                :disabled="savingPrefs"
                @update:value="markTouched('lang')" /></n-form-item
            ><n-form-item v-if="desktop" :label="t('windowUi.preference')"
              ><n-select
                v-model:value="closeAction"
                :options="closeOptions"
                :disabled="savingPrefs"
                @update:value="markTouched('closeAction')"
            /></n-form-item>
          </div>
          <p v-if="desktop" class="desc">
            {{ t("windowUi.preferenceHint") }}
          </p></n-card
        >
      </section>
      <section v-show="section === 'data'" class="settings-section">
        <n-card :title="t('prefsUi.backup')" class="card" size="small"
          ><p class="desc">{{ t("prefsUi.backupHint") }}</p>
          <div class="settings-row">
            <n-button :loading="exporting" @click="exportBackup">{{
              t("library.export")
            }}</n-button
            ><n-button @click="showCollections()">{{
              t("settings.manageCollections")
            }}</n-button>
          </div></n-card
        >
        <n-card :title="t('prefsUi.cache')" class="card" size="small"
          ><p class="desc">{{ t("prefsUi.cacheHint") }}</p>
          <n-button :loading="clearingThumbs" @click="clearThumbs">{{
            t("settings.prefs.clearThumbs")
          }}</n-button></n-card
        >
      </section>
    </div>
    <div
      v-if="['storage', 'rules', 'general'].includes(section) || prefsDirty"
      class="preferences-save panel"
    >
      <div>
        <strong>{{
          prefsDirty ? t("prefsUi.unsaved") : t("settings.prefs.saved")
        }}</strong>
        <p v-if="preferenceError" class="field-error" role="alert">
          {{ preferenceError }}
        </p>
      </div>
      <span class="collection-spacer" /><n-button
        :disabled="savingPrefs || !prefsDirty"
        @click="discardPrefs"
        >{{ t("prefsUi.discard") }}</n-button
      ><n-button
        type="primary"
        :loading="savingPrefs"
        :disabled="!settingsLoaded || !mediaKinds.length || !prefsDirty"
        @click="savePrefs"
        >{{ t("common.save") }}</n-button
      >
    </div>
  </div>
</template>
<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 1000px;
  width: 100%;
  min-height: 0;
  height: 100%;
}
.settings-sections {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border-soft);
  padding-bottom: 10px;
}
.settings-sections button {
  font: inherit;
  color: var(--text-dim);
  background: none;
  border: 0;
  border-radius: 6px;
  padding: 8px 14px;
  cursor: pointer;
}
.settings-sections button.on {
  background: var(--accent-soft);
  color: var(--accent-bright);
}
.settings-body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.settings-section {
  height: 100%;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.settings-section > .card {
  flex-shrink: 0;
}
.settings-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px 22px;
}
.settings-row {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
}
.settings-row > .n-input {
  flex: 1;
  min-width: 180px;
}
.desc {
  font-size: 12px;
  color: var(--text-dim);
  line-height: 1.8;
  margin: 0 0 14px;
}
.login-area {
  padding: 20px 0;
  border-top: 1px solid var(--border-soft);
  border-bottom: 1px solid var(--border-soft);
  margin: 18px 0;
}
.path-preview {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 12px 14px;
  border: 1px solid var(--border-soft);
  background: var(--bg-inset);
  border-radius: 8px;
  margin-bottom: 12px;
}
.path-preview span {
  font-size: 11px;
  color: var(--text-faint);
}
.path-preview code {
  font-size: 12px;
  color: var(--accent-bright);
  overflow-wrap: anywhere;
}
.field-error {
  color: var(--red);
  font-size: 12px;
  line-height: 1.7;
  margin: 8px 0;
}
.preferences-save {
  position: static;
  flex-shrink: 0;
  z-index: 3;
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 14px 18px;
  background: var(--surface-2);
  border-color: var(--border);
  box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.2);
}
.preferences-save strong {
  font-size: 12px;
  font-weight: 500;
}
a {
  color: var(--accent-bright);
}
@media (max-width: 760px) {
  .settings-grid {
    grid-template-columns: 1fr;
  }
  .preferences-save {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    bottom: 8px;
  }
  .preferences-save > div:first-child {
    grid-column: 1 / -1;
  }
  .preferences-save .collection-spacer {
    display: none;
  }
  .preferences-save > .n-button {
    width: 100%;
    min-height: 44px;
  }
  .settings-row > .n-input {
    min-width: 0;
  }
}
</style>
