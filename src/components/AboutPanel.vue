<script setup lang="ts">
import { NCard, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { author, homepage, bugs, license, version } from "../../package.json";
import { isDesktop } from "../lib/backend";
import AppIcon from "./AppIcon.vue";

const { t } = useI18n();
const message = useMessage();
const links = [
  { key: "releases", url: `${homepage}/releases` },
  { key: "feedback", url: bugs.url },
  { key: "changelog", url: `${homepage}/blob/main/CHANGELOG.md` },
];

async function openLink(event: MouseEvent, url: string) {
  if (!isDesktop) return;
  event.preventDefault();
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } catch {
    message.error(t("simple.about.openFailed"));
  }
}
</script>

<template>
  <n-card class="card about-card" size="small">
    <header class="about-brand">
      <img src="/brand/logo.svg" alt="" width="56" height="56" />
      <h3>tg-grab</h3>
      <p>{{ t("simple.about.description") }}</p>
      <span class="about-version">
        v{{ version }} · {{ t(isDesktop ? "simple.about.desktop" : "simple.about.web") }}
      </span>
    </header>
    <dl class="about-details">
      <div>
        <dt>{{ t("simple.about.author") }}</dt>
        <dd>
          <a :href="author.url" target="_blank" rel="noopener noreferrer"
            @click="openLink($event, author.url)">{{ author.name }}</a>
        </dd>
      </div>
      <div>
        <dt>{{ t("simple.about.repository") }}</dt>
        <dd>
          <a :href="homepage" target="_blank" rel="noopener noreferrer"
            @click="openLink($event, homepage)">{{ homepage }}</a>
        </dd>
      </div>
      <div>
        <dt>{{ t("simple.about.license") }}</dt>
        <dd>
          <a :href="`${homepage}/blob/main/LICENSE`" target="_blank" rel="noopener noreferrer"
            @click="openLink($event, `${homepage}/blob/main/LICENSE`)">{{ license }}</a>
        </dd>
      </div>
    </dl>
    <nav class="about-links" :aria-label="t('simple.about.links')">
      <a v-for="link in links" :key="link.key" :href="link.url" target="_blank"
        rel="noopener noreferrer" @click="openLink($event, link.url)">
        <span>{{ t(`simple.about.${link.key}`) }}</span>
        <AppIcon name="arrow" :size="15" />
      </a>
    </nav>
  </n-card>
</template>

<style scoped>
.about-brand {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 8px 0 16px;
  text-align: center;
}
.about-brand h3 { color: var(--text); font-size: 20px; font-weight: 650; }
.about-brand p { color: var(--text-dim); font-size: 13px; }
.about-version { color: var(--text-faint); font-size: 12px; font-variant-numeric: tabular-nums; }
.about-details > div {
  display: grid;
  grid-template-columns: 90px minmax(0, 1fr);
  gap: 16px;
  padding: 12px 0;
  border-top: 1px solid var(--border-soft);
  line-height: 1.6;
}
.about-details dt { color: var(--text-dim); }
.about-details dd { margin: 0; overflow-wrap: anywhere; }
a { color: var(--accent-bright); text-decoration: none; }
a:hover { text-decoration: underline; }
a:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; border-radius: 4px; }
.about-links { display: flex; flex-wrap: wrap; gap: 10px; padding-top: 20px; }
.about-links a {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--text);
  font-size: 12.5px;
}
.about-links a:hover { background: var(--surface-2); border-color: var(--accent-border); text-decoration: none; }
.about-links svg { color: var(--text-dim); transform: rotate(-45deg); }
</style>
