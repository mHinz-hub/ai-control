import "./popup-window.css";
import { createApp } from "vue";
import Popup from "./components/Popup.vue";
import { i18n } from "./i18n";

createApp(Popup).use(i18n).mount("#app");
