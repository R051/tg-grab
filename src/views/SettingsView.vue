<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "../lib/backend";
import { errorText } from "../lib/errors";
import type { SignInOutcome } from "../types";
import {
  NCard,
  NInput,
  NButton,
  NForm,
  NFormItem,
  NSelect,
  NInputNumber,
  NText,
  useMessage,
} from "naive-ui";
import { useAppStore } from "../stores/app";
import { setLocale } from "../i18n";

const { t } = useI18n();
const store = useAppStore();
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
const kindVideo = ref(true);
const kindPhoto = ref(false);
const kindAudio = ref(false);
const kindDoc = ref(false);
const classifyByType = ref(false);
const savingPrefs = ref(false);

const settingsLoaded = computed(() => store.settings !== null);

const langOptions = [
  { label: "中文", value: "zh-CN" },
  { label: "English", value: "en" },
];

// Form starts blank; sync once the store's async settings arrive — but
// never overwrite fields the user has already touched.
const dirty: Record<string, boolean> = { apiId: false, apiHash: false, proxyUrl: false, concurrency: false, lang: false, downloadsDir: false, classifyByType: false };
watch(
  () => store.settings,
  (s) => {
    if (!s) return;
    if (!dirty.apiId) apiId.value = s.api_id?.toString() ?? "";
    if (!dirty.apiHash) apiHash.value = s.api_hash ?? "";
    if (!dirty.proxyUrl) proxyUrl.value = s.proxy_url ?? "";
    if (!dirty.concurrency) concurrency.value = s.concurrency;
    if (!dirty.lang) lang.value = s.lang;
  },
  { immediate: true, deep: false },
);
function markTouched(field: string) {
  dirty[field] = true;
}

watch(
  () => store.status?.authorized,
  (authorized) => {
    if (authorized) { step.value = "phone"; code.value = ""; password.value = ""; pwHint.value = null; }
  },
);

const loggedIn = computed(() => store.authorized);
const apiConfigured = computed(() => store.status?.configured ?? false);
const KNOWN_LEAKED_IDS = ["2040"];
const usingLeakedId = computed(() => {
  const id = store.settings?.api_id;
  return id != null && KNOWN_LEAKED_IDS.includes(String(id));
});
const user = computed(() => store.user ?? "");

async function saveApi() {
  if (savingApi.value || !store.settings) return;
  if (!apiId.value.trim() || !apiHash.value.trim()) {
    message.error(t("settings.api.invalid"), msgOpts);
    return;
  }
  savingApi.value = true;
  const submitted = { apiId: apiId.value.trim(), apiHash: apiHash.value.trim() };
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
    code.value = ""; password.value = ""; pwHint.value = null;
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
    const r = await invoke<SignInOutcome>(
      "auth_sign_in",
      { code: code.value.trim() },
    );
    if (r.status === "ok") {
      message.success(t("settings.login.loginSuccess"));
      step.value = "phone";
      code.value = "";
      await store.refreshStatus();
    } else if (r.status === "need_password") {
      // need_password
      pwHint.value = r.hint ?? null;
      step.value = "password";
    } else { throw new Error("INVALID_RESPONSE"); }
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
    message.success(t("settings.prefs.thumbsCleared", { mb: ((r.freed ?? 0) / 1048576).toFixed(1) }));
  } catch (e) {
    message.error(errText(e));
  } finally {
    clearingThumbs.value = false;
  }
}

async function savePrefs() {
  if (savingPrefs.value || !store.settings || !concurrency.value) return;
  savingPrefs.value = true;
  const submitted = { concurrency: concurrency.value, lang: lang.value };
  try {
    await invoke("save_settings", submitted);
    if (concurrency.value === submitted.concurrency) dirty.concurrency = false;
    if (lang.value === submitted.lang) dirty.lang = false;
    setLocale(submitted.lang as "zh-CN" | "en");
    message.success(t("settings.prefs.saved"));
    await store.refreshSettings();
  } catch (e) {
    message.error(errText(e), msgOpts);
  } finally { savingPrefs.value = false; }
}

function errText(error: unknown): string { return errorText(error, t); }
</script>

<template>
  <div class="settings">
    <!-- API credentials -->
    <n-card size="small" class="card leak-banner" v-if="usingLeakedId">
      <div class="leak-row">
        <span class="leak-icon">⚠️</span>
        <div>
          <div class="leak-title">{{ t("settings.api.leakTitle") }}</div>
          <div class="leak-desc">
            {{ t("settings.api.leakDesc") }}
            <a href="https://my.telegram.org/apps" target="_blank" rel="noreferrer">my.telegram.org/apps</a>
          </div>
        </div>
      </div>
    </n-card>

    <!-- API credentials -->
    <n-card :title="t('settings.api.title')" size="small" class="card">
      <p class="desc">{{ t("settings.api.desc") }}</p>
      <n-form label-placement="top" size="small">
        <n-form-item :label="t('settings.api.apiId')">
          <n-input v-model:value="apiId" placeholder="e.g. 12345678" @update:value="markTouched('apiId')" />
        </n-form-item>
        <n-form-item :label="t('settings.api.apiHash')">
          <n-input v-model:value="apiHash" placeholder="0123456789abcdef0123456789abcdef" show-password-on="click" @update:value="markTouched('apiHash')" />
        </n-form-item>
      </n-form>
      <n-button type="primary" size="small" :loading="savingApi" :disabled="!settingsLoaded" @click="saveApi">
        {{ t("settings.api.save") }}
      </n-button>
    </n-card>

    <!-- Login -->
    <n-card :title="t('settings.login.title')" size="small" class="card">
      <template v-if="loggedIn">
        <n-text type="success">{{ t("settings.login.loggedAs", { user }) }}</n-text>
        <n-button size="small" style="margin-left: 12px" @click="logout">
          {{ t("settings.login.logout") }}
        </n-button>
      </template>
      <template v-else-if="!apiConfigured">
        <n-text depth="3">{{ t("settings.login.needApiFirst") }}</n-text>
      </template>
      <template v-else>
        <n-form label-placement="top" size="small">
          <n-form-item v-show="step === 'phone'" :label="t('settings.login.phone')">
            <n-input v-model:value="phone" placeholder="+8613800138000" @keyup.enter="sendCode" />
          </n-form-item>
          <n-form-item v-show="step === 'code'" :label="t('settings.login.code')">
            <n-input v-model:value="code" @keyup.enter="verifyCode" />
          </n-form-item>
          <n-form-item v-show="step === 'password'" :label="t('settings.login.password')">
            <n-input
              v-model:value="password"
              type="password"
              show-password-on="click"
              @keyup.enter="submitPassword"
            />
            <n-text v-if="pwHint" depth="3" class="hint">
              {{ t("settings.login.passwordHint", { hint: pwHint }) }}
            </n-text>
          </n-form-item>
        </n-form>
        <div class="row">
          <n-button
            v-if="step === 'phone'"
            type="primary"
            size="small"
            :loading="loggingIn"
            @click="sendCode"
          >
            {{ t("settings.login.sendCode") }}
          </n-button>
          <n-button
            v-else-if="step === 'code'"
            type="primary"
            size="small"
            :loading="loggingIn"
            @click="verifyCode"
          >
            {{ t("settings.login.verify") }}
          </n-button>
          <n-button
            v-if="step === 'code'"
            quaternary
            size="small"
            :disabled="loggingIn"
            @click="resendCode"
          >
            {{ t("settings.login.resend") }}
          </n-button>
          <n-button
            v-if="step === 'code'"
            quaternary
            size="small"
            :disabled="loggingIn"
            @click="step = 'phone'; code = ''; password = ''; pwHint = null"
          >
            {{ t("settings.login.changePhone") }}
          </n-button>
          <n-button
            v-if="step === 'password'"
            type="primary"
            size="small"
            :loading="loggingIn"
            @click="submitPassword"
          >
            {{ t("settings.login.submitPassword") }}
          </n-button>
        </div>
      </template>
    </n-card>

    <!-- Proxy -->
    <n-card :title="t('settings.proxy.title')" size="small" class="card">
      <p class="desc">{{ t("settings.proxy.desc") }}</p>
      <n-input v-model:value="proxyUrl" placeholder="socks5://127.0.0.1:7890" size="small"  @update:value="markTouched('proxyUrl')" />
      <n-button type="primary" size="small" style="margin-top: 8px" :loading="savingProxy" :disabled="!settingsLoaded" @click="saveProxy">
        {{ t("settings.proxy.save") }}
      </n-button>
    </n-card>

    <!-- Preferences -->
    <n-card :title="t('settings.prefs.title')" size="small" class="card">
      <n-form label-placement="top" size="small">
        <n-form-item :label="t('settings.prefs.concurrency')">
          <n-input-number v-model:value="concurrency" :min="1" :max="4" style="width: 120px"  @update:value="markTouched('concurrency')" />
        </n-form-item>
        <n-form-item :label="t('settings.prefs.lang')">
          <n-select v-model:value="lang" :options="langOptions" style="width: 160px"  @update:value="markTouched('lang')" />
        </n-form-item>
        <n-form-item :label="t('settings.prefs.downloadsDir')">
          <n-input
            v-model:value="downloadsDir"
            :placeholder="t('settings.prefs.downloadsDirHint')"
            size="small"
            @update:value="markTouched('downloadsDir')"
          />
        </n-form-item>
        <n-form-item :label="t('settings.prefs.mediaKinds')">
          <n-checkbox v-model:checked="kindVideo" size="small" style="margin-right: 12px">{{ t("settings.prefs.kindVideo") }}</n-checkbox>
          <n-checkbox v-model:checked="kindPhoto" size="small" style="margin-right: 12px">{{ t("settings.prefs.kindPhoto") }}</n-checkbox>
          <n-checkbox v-model:checked="kindAudio" size="small" style="margin-right: 12px">{{ t("settings.prefs.kindAudio") }}</n-checkbox>
          <n-checkbox v-model:checked="kindDoc" size="small">{{ t("settings.prefs.kindDoc") }}</n-checkbox>
        </n-form-item>
        <n-form-item :label="t('settings.prefs.classifyByType')">
          <n-switch v-model:value="classifyByType" size="small" @update:value="markTouched('classifyByType')" />
        </n-form-item>
      </n-form>
      <div class="row" style="gap: 10px">
        <n-button type="primary" size="small" :loading="savingPrefs" :disabled="!settingsLoaded" @click="savePrefs">{{ t("settings.api.save") }}</n-button>
        <n-button size="small" :loading="clearingThumbs" @click="clearThumbs">
          {{ t("settings.prefs.clearThumbs") }}
        </n-button>
      </div>
    </n-card>
  </div>
</template>

<style scoped>
.settings { display: flex; flex-direction: column; gap: 12px; }
.desc { font-size: 12px; color: #8b949e; margin-bottom: 10px; }
.row { display: flex; gap: 8px; }
.hint { display: block; font-size: 12px; margin-top: 4px; }
</style>
