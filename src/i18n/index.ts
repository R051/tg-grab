import { createI18n } from "vue-i18n";
import zhCN from "./zh-CN";
import en from "./en";

export type Locale = "zh-CN" | "en";

export const i18n = createI18n({
  legacy: false,
  locale: (localStorage.getItem("tg-grab-lang") as Locale) || "zh-CN",
  fallbackLocale: "en",
  messages: { "zh-CN": zhCN, en },
});

export function setLocale(locale: Locale) {
  i18n.global.locale.value = locale;
  localStorage.setItem("tg-grab-lang", locale);
}
