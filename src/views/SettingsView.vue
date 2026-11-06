<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
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

const { t } = useI18n();
const store = useAppStore();
const message = useMessage();

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

const downloadsDir = computed(() => store.settings?.downloads_dir ?? "");

const langOptions = [
  { label: "中文", value: "zh-CN" },
  { label: "English", value: "en" },
];

onMounted(() => {
  const s = store.settings;
  if (s) {
    apiId.value = s.api_id?.toString() ?? "";
    apiHash.value = s.api_hash ?? "";
    proxyUrl.value = s.proxy_url ?? "";
    concurrency.value = s.concurrency;
    lang.value = s.lang;
  }
});

watch(
  () => store.status?.authorized,
  (authorized) => {
    if (authorized) step.value = "phone"; // reset wizard for next time
  },
);

const loggedIn = computed(() => store.authorized);
const user = computed(() => store.user ?? "");

async function saveApi() {
  if (!apiId.value.trim() || !apiHash.value.trim()) {
    message.error(t("settings.api.invalid"));
    return;
  }
  savingApi.value = true;
  try {
    await invoke("save_api_config", {
      apiId: apiId.value.trim(),
      apiHash: apiHash.value.trim(),
    });
    message.success(t("settings.api.saved"));
    await store.refreshStatus();
  } catch (e) {
    message.error(String(e));
  } finally {
    savingApi.value = false;
  }
}

async function saveProxy() {
  savingProxy.value = true;
  try {
    await invoke("save_proxy", { proxyUrl: proxyUrl.value.trim() });
    message.success(t("settings.proxy.saved"));
  } catch (e) {
    message.error(String(e));
  } finally {
    savingProxy.value = false;
  }
}

async function sendCode() {
  if (!phone.value.trim()) return;
  loggingIn.value = true;
  try {
    await invoke("auth_send_code", { phone: phone.value.trim() });
    step.value = "code";
    message.success(t("settings.login.codeSent"));
  } catch (e) {
    message.error(errText(e));
  } finally {
    loggingIn.value = false;
  }
}

async function verifyCode() {
  if (!code.value.trim()) return;
  loggingIn.value = true;
  try {
    const r = await invoke<{ status: string; user?: string; hint?: string | null }>(
      "auth_sign_in",
      { code: code.value.trim() },
    );
    if (r.status === "Ok") {
      message.success(t("settings.login.loginSuccess"));
      step.value = "phone";
      await store.refreshStatus();
    } else {
      pwHint.value = r.hint ?? null;
      step.value = "password";
    }
  } catch (e) {
    message.error(errText(e));
  } finally {
    loggingIn.value = false;
  }
}

async function submitPassword() {
  if (!password.value) return;
  loggingIn.value = true;
  try {
    await invoke("auth_check_password", { password: password.value });
    message.success(t("settings.login.loginSuccess"));
    step.value = "phone";
    password.value = "";
    await store.refreshStatus();
  } catch (e) {
    message.error(errText(e));
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
    message.error(String(e));
  }
}

async function savePrefs() {
  try {
    await invoke("save_settings", {
      concurrency: concurrency.value,
      lang: lang.value,
    });
    message.success(t("settings.prefs.saved"));
    await store.refreshSettings();
  } catch (e) {
    message.error(String(e));
  }
}

/** Map backend error codes to localized messages. */
function errText(e: unknown): string {
  const raw = String(e);
  const code = raw.includes(":") ? raw.split(":")[0].trim() : raw.trim();
  for (const key of [
    "INVALID_LINK",
    "INVITE_LINK",
    "NOT_LOGGED_IN",
    "API_NOT_CONFIGURED",
    "INVALID_API_ID",
    "SESSION_OPEN",
    "SEND_CODE",
    "SIGN_IN",
    "PASSWORD",
  ]) {
    if (code.startsWith(key)) {
      const msg = t(`download.errors.${key}`);
      if (msg !== `download.errors.${key}`) return msg;
      return raw;
    }
  }
  return raw;
}
</script>

<template>
  <div class="settings">
    <!-- API credentials -->
    <n-card :title="t('settings.api.title')" size="small" class="card">
      <p class="desc">{{ t("settings.api.desc") }}</p>
      <n-form label-placement="top" size="small">
        <n-form-item :label="t('settings.api.apiId')">
          <n-input v-model:value="apiId" placeholder="e.g. 12345678" />
        </n-form-item>
        <n-form-item :label="t('settings.api.apiHash')">
          <n-input v-model:value="apiHash" placeholder="0123456789abcdef0123456789abcdef" show-password-on="click" />
        </n-form-item>
      </n-form>
      <n-button type="primary" size="small" :loading="savingApi" @click="saveApi">
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
            v-else
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
      <n-input v-model:value="proxyUrl" placeholder="socks5://127.0.0.1:7890" size="small" />
      <n-button type="primary" size="small" style="margin-top: 8px" :loading="savingProxy" @click="saveProxy">
        {{ t("settings.proxy.save") }}
      </n-button>
    </n-card>

    <!-- Preferences -->
    <n-card :title="t('settings.prefs.title')" size="small" class="card">
      <n-form label-placement="top" size="small">
        <n-form-item :label="t('settings.prefs.concurrency')">
          <n-input-number v-model:value="concurrency" :min="1" :max="4" style="width: 120px" />
        </n-form-item>
        <n-form-item :label="t('settings.prefs.lang')">
          <n-select v-model:value="lang" :options="langOptions" style="width: 160px" />
        </n-form-item>
        <n-form-item v-if="downloadsDir" :label="t('settings.prefs.downloadsDir')">
          <n-text depth="3">{{ downloadsDir }}</n-text>
        </n-form-item>
      </n-form>
      <n-button type="primary" size="small" @click="savePrefs">{{ t("settings.api.save") }}</n-button>
    </n-card>
  </div>
</template>

<style scoped>
.settings { display: flex; flex-direction: column; gap: 12px; }
.desc { font-size: 12px; color: #8b949e; margin-bottom: 10px; }
.row { display: flex; gap: 8px; }
.hint { display: block; font-size: 12px; margin-top: 4px; }
</style>
