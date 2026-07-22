<script setup lang="ts">
import { onMounted, ref } from "vue";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useI18n } from "vue-i18n";
import ProjectList from "./components/ProjectList.vue";
import PoolList from "./components/PoolList.vue";
import UsageList from "./components/UsageList.vue";
import { setLocale } from "./i18n";

const tab = ref<"projects" | "pools" | "usage">("projects");
const autostart = ref(false);
const { locale } = useI18n();

onMounted(async () => {
  autostart.value = await isEnabled();

  // macOS nutzt die native Ampel (Overlay); sonst eigene Fensterknöpfe im Header
  // (Linux ist dekorationslos, siehe lib.rs).
  const isMac = /Mac|Macintosh/.test(navigator.userAgent);
  document.documentElement.dataset.platform = isMac ? "mac" : "other";
  if (!isMac) {
    const win = getCurrentWebviewWindow();
    document
      .getElementById("win-min")
      ?.addEventListener("click", () => win.minimize());
    document
      .getElementById("win-max")
      ?.addEventListener("click", () => win.toggleMaximize());
    document
      .getElementById("win-close")
      ?.addEventListener("click", () => win.close());
    for (const g of document.querySelectorAll<HTMLElement>(".grip")) {
      g.addEventListener("mousedown", (e) => {
        e.preventDefault();
        win.startResizeDragging(g.dataset.dir as never);
      });
    }
  }
});

async function toggleAutostart() {
  if (autostart.value) {
    await disable();
  } else {
    await enable();
  }
  autostart.value = await isEnabled();
}
</script>

<template>
  <header data-tauri-drag-region>
    <h1 data-tauri-drag-region>aICentral</h1>
    <nav>
      <button :class="{ active: tab === 'projects' }" @click="tab = 'projects'">
        {{ $t("app.projects") }}
      </button>
      <button :class="{ active: tab === 'pools' }" @click="tab = 'pools'">
        {{ $t("app.pools") }}
      </button>
      <button :class="{ active: tab === 'usage' }" @click="tab = 'usage'">
        {{ $t("app.usage") }}
      </button>
    </nav>
    <div class="header-right">
      <div class="lang">
        <button :class="{ active: locale === 'de' }" @click="setLocale('de')">DE</button>
        <button :class="{ active: locale === 'en' }" @click="setLocale('en')">EN</button>
      </div>
      <label class="autostart">
        <input type="checkbox" :checked="autostart" @change="toggleAutostart" />
        {{ $t("app.autostart") }}
      </label>
    </div>
    <div id="winbtns">
      <button class="winbtn" id="win-min" aria-label="Minimieren">
        <svg width="12" height="12" viewBox="0 0 12 12"><line x1="2" y1="6.5" x2="10" y2="6.5" /></svg>
      </button>
      <button class="winbtn" id="win-max" aria-label="Maximieren">
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><rect x="2.5" y="2.5" width="7" height="7" rx="1" /></svg>
      </button>
      <button class="winbtn close" id="win-close" aria-label="Schließen">
        <svg width="12" height="12" viewBox="0 0 12 12"><line x1="2.7" y1="2.7" x2="9.3" y2="9.3" /><line x1="9.3" y1="2.7" x2="2.7" y2="9.3" /></svg>
      </button>
    </div>
  </header>
  <main>
    <ProjectList v-if="tab === 'projects'" />
    <PoolList v-else-if="tab === 'pools'" />
    <UsageList v-else />
  </main>
  <div id="resize-grips" aria-hidden="true">
    <div class="grip n" data-dir="North"></div>
    <div class="grip s" data-dir="South"></div>
    <div class="grip w" data-dir="West"></div>
    <div class="grip e" data-dir="East"></div>
    <div class="grip nw" data-dir="NorthWest"></div>
    <div class="grip ne" data-dir="NorthEast"></div>
    <div class="grip sw" data-dir="SouthWest"></div>
    <div class="grip se" data-dir="SouthEast"></div>
  </div>
</template>

<style>
/* macOS: Platz links für die Ampel, eigene Fensterknöpfe aus. */
:root[data-platform="mac"] header {
  padding-left: 80px;
}
:root[data-platform="mac"] #winbtns {
  display: none;
}
/* Linux/Windows: Fensterknöpfe rechtsbündig bis zum Rand. */
:root[data-platform="other"] header {
  padding-right: 0;
}
#winbtns {
  display: flex;
  align-self: stretch;
  margin: -0.85rem 0 -0.85rem 0.5rem;
}
.winbtn {
  width: 46px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 0;
  background: transparent;
  color: var(--subtext);
  cursor: default;
  transition: background 0.12s ease;
}
.winbtn svg {
  stroke: currentColor;
  stroke-width: 1.2;
  opacity: 0.85;
}
.winbtn:hover {
  background: var(--surface0);
  color: var(--text);
}
.winbtn.close:hover {
  background: #f38ba8;
  color: var(--crust);
}
/* Resize-Zonen für das dekorationslose Fenster (nur Linux/Windows). */
#resize-grips {
  display: none;
}
:root[data-platform="other"] #resize-grips {
  display: block;
}
.grip {
  position: fixed;
  z-index: 100;
}
.grip.n {
  top: 0;
  left: 0;
  right: 0;
  height: 4px;
  cursor: ns-resize;
}
.grip.s {
  bottom: 0;
  left: 0;
  right: 0;
  height: 4px;
  cursor: ns-resize;
}
.grip.w {
  top: 0;
  bottom: 0;
  left: 0;
  width: 4px;
  cursor: ew-resize;
}
.grip.e {
  top: 0;
  bottom: 0;
  right: 0;
  width: 4px;
  cursor: ew-resize;
}
.grip.nw {
  top: 0;
  left: 0;
  width: 9px;
  height: 9px;
  cursor: nwse-resize;
  z-index: 101;
}
.grip.ne {
  top: 0;
  right: 0;
  width: 9px;
  height: 9px;
  cursor: nesw-resize;
  z-index: 101;
}
.grip.sw {
  bottom: 0;
  left: 0;
  width: 9px;
  height: 9px;
  cursor: nesw-resize;
  z-index: 101;
}
.grip.se {
  bottom: 0;
  right: 0;
  width: 9px;
  height: 9px;
  cursor: nwse-resize;
  z-index: 101;
}
</style>
