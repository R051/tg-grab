import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./style.css";
import { i18n } from "./i18n";
import { applyTheme } from "./theme";

applyTheme();
createApp(App).use(createPinia()).use(i18n).mount("#app");
