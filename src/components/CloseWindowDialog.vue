<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { useI18n } from "vue-i18n";
import { NModal, NButton, NCheckbox } from "naive-ui";
import { invoke, on, isDesktop } from "../lib/backend";
import { errorText } from "../lib/errors";
import { useAppStore } from "../stores/app";
import AppIcon from "./AppIcon.vue";
const { t } = useI18n();
const store = useAppStore();
const show = ref(false),
  remember = ref(false),
  busy = ref(false),
  error = ref("");
const counts = computed(() => ({
  active: store.activeTasks.length,
  pending: store.pending.length,
}));
let cleanup: (() => void) | undefined;
let disposed = false;
function requestClose() {
  if (show.value) return;
  remember.value = false;
  error.value = "";
  show.value = true;
}
onMounted(async () => {
  if (!isDesktop) return;
  try {
    const unlisten = await on("desktop:close-requested", requestClose);
    if (disposed) {
      unlisten();
      return;
    }
    cleanup = unlisten;
    if (await invoke<boolean>("window_close_pending")) requestClose();
  } catch (cause) {
    error.value = errorText(cause, t);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  cleanup?.();
});
async function choose(action: "tray" | "exit" | "cancel") {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await invoke("window_action", {
      action,
      remember: remember.value && action !== "cancel",
    });
    show.value = false;
    if (remember.value) await store.refreshSettings();
  } catch (cause) {
    error.value = errorText(cause, t);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <n-modal
    :show="show"
    preset="card"
    class="close-window-dialog"
    :title="t('windowUi.title')"
    :closable="false"
    :mask-closable="false"
    :close-on-esc="!busy"
    @update:show="
      (value) => {
        if (!value) choose('cancel');
      }
    "
  >
    <p class="close-description">{{ t("windowUi.description") }}</p>
    <p v-if="counts.active || counts.pending" class="close-queue">
      {{ t("windowUi.active", counts) }}
    </p>
    <div class="close-options">
      <button class="close-option" :disabled="busy" @click="choose('tray')">
        <span class="close-option-icon"
          ><AppIcon name="window" :size="22" /></span
        ><span
          ><strong>{{ t("windowUi.tray") }}</strong
          ><small>{{ t("windowUi.trayHint") }}</small></span
        ><AppIcon name="arrow" />
      </button>
      <button class="close-option" :disabled="busy" @click="choose('exit')">
        <span class="close-option-icon"
          ><AppIcon name="power" :size="22" /></span
        ><span
          ><strong>{{ t("windowUi.exit") }}</strong
          ><small>{{ t("windowUi.exitHint") }}</small></span
        ><AppIcon name="arrow" />
      </button>
    </div>
    <n-checkbox v-model:checked="remember" :disabled="busy">{{
      t("windowUi.remember")
    }}</n-checkbox>
    <p class="close-description close-hint">{{ t("windowUi.rememberHint") }}</p>
    <p v-if="error" class="field-error" role="alert">{{ error }}</p>
    <div class="close-footer">
      <n-button :loading="busy" @click="choose('cancel')">{{
        t("windowUi.cancel")
      }}</n-button>
    </div>
  </n-modal>
</template>
<style>
.close-window-dialog {
  width: min(480px, calc(100vw - 32px));
}
.close-description {
  font-size: 13px;
  color: var(--text-dim);
  line-height: 1.7;
}
.close-queue {
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--accent-soft);
  font-size: 12px;
  color: var(--accent-bright);
}
.close-options {
  display: grid;
  gap: 10px;
  margin: 22px 0;
}
.close-option {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--surface-2);
  text-align: left;
  cursor: pointer;
  color: var(--text);
  font: inherit;
}
.close-option:hover {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.close-option:disabled {
  opacity: 0.5;
  cursor: wait;
}
.close-option > span:nth-child(2) {
  flex: 1;
}
.close-option strong {
  display: block;
  font-size: 14px;
  font-weight: 600;
}
.close-option small {
  display: block;
  font-size: 12px;
  color: var(--text-dim);
  line-height: 1.6;
  margin-top: 4px;
}
.close-option-icon {
  color: var(--accent-bright);
  display: flex;
}
.close-hint {
  font-size: 11px;
  margin: 6px 0 0 24px;
}
.close-footer {
  display: flex;
  justify-content: flex-end;
  margin-top: 20px;
}
.field-error {
  color: var(--red);
  font-size: 12px;
  line-height: 1.7;
  margin-top: 12px;
}
</style>
