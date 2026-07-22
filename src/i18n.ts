/// vue-i18n-Wrapper für die Vue-Fenster (index.html). Die Tabellen liegen in
/// messages.ts und werden von der Terminal-/Panel-Schicht ohne Vue genutzt.

import { createI18n } from "vue-i18n";

import { messages, storedLocale, type Locale } from "./messages";

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: storedLocale(),
  fallbackLocale: "en",
  messages,
});

export function setLocale(lang: Locale) {
  i18n.global.locale.value = lang;
  localStorage.setItem("lang", lang);
}
