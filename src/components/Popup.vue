<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { currentMonitor } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";

interface Project {
  id: string;
  name: string;
  running: boolean;
  terminal: { theme: string | null; icon: string | null; title: string | null };
}

const WIDTH = 300;
// Sicherheitsabstand für Panel + Rand; die Extension klemmt zusätzlich exakt.
const MARGIN = 64;

const projects = ref<Project[]>([]);
const icons = ref<Record<string, string>>({});
const wrapRef = ref<HTMLElement>();
const maxH = ref(10000);
const win = getCurrentWebviewWindow();

async function refresh() {
  try {
    projects.value = await invoke<Project[]>("list_projects");
    for (const p of projects.value) {
      if (p.terminal.icon && !(p.id in icons.value)) {
        icons.value[p.id] =
          (await invoke<string | null>("project_icon", { project: p.id })) ?? "";
      }
    }
  } catch (e) {
    console.error(e);
  }
}

async function pick(p: Project) {
  await invoke("start_or_focus_cmd", { project: p.id });
  await win.hide();
}
async function openMain() {
  await invoke("open_main_window");
  await win.hide();
}
async function quit() {
  await invoke("quit_app");
}

// Fenster nur so hoch wie der Inhalt: bei jeder Layout-Änderung nachziehen.
let ro: ResizeObserver | undefined;
let timer: number;
onMounted(async () => {
  const mon = await currentMonitor();
  if (mon) maxH.value = Math.floor(mon.size.height / mon.scaleFactor) - MARGIN;
  refresh();
  timer = window.setInterval(refresh, 2000);
  if (wrapRef.value) {
    ro = new ResizeObserver(() => {
      const h = Math.min(Math.ceil(wrapRef.value!.getBoundingClientRect().height), maxH.value);
      if (h > 0) win.setSize(new LogicalSize(WIDTH, h));
    });
    ro.observe(wrapRef.value);
  }
});
onUnmounted(() => {
  window.clearInterval(timer);
  ro?.disconnect();
});
</script>

<template>
  <div ref="wrapRef" class="wrap" :style="{ maxHeight: maxH + 'px' }">
    <ul class="list">
      <li v-for="p in projects" :key="p.id" class="row" @click="pick(p)">
        <span class="dot" :class="{ on: p.running }"></span>
        <img v-if="icons[p.id]" class="ic" :src="icons[p.id]" />
        <span v-else class="ic ph"></span>
        <span class="nm">{{ p.name }}</span>
      </li>
      <li v-if="projects.length === 0" class="empty">Keine Projekte</li>
    </ul>

    <footer class="ft">
      <button class="fbtn" @click="openMain">{{ $t("popup.open") }}</button>
      <button class="fbtn danger" @click="quit">{{ $t("popup.quit") }}</button>
    </footer>
  </div>
</template>

<style scoped>
.wrap {
  width: 300px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  background: #1e1e2e;
  color: #cdd6f4;
  border: 1px solid #313244;
  border-radius: 12px;
  overflow: hidden;
  font:
    14px/1.3 system-ui,
    -apple-system,
    sans-serif;
}

.list {
  list-style: none;
  margin: 0;
  padding: 5px;
  overflow-y: auto;
  flex: 1 1 auto;
  min-height: 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 7px 9px;
  border-radius: 9px;
  cursor: pointer;
  transition: background 0.12s ease;
}
.row:hover {
  background: #313244;
}

.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #585b70;
  flex: none;
}
.dot.on {
  background: #a6e3a1;
  box-shadow: 0 0 7px #a6e3a1a0;
}

.ic {
  width: 34px;
  height: 34px;
  border-radius: 8px;
  flex: none;
  object-fit: cover;
}
.ic.ph {
  background: #313244;
}

.nm {
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.empty {
  padding: 20px;
  text-align: center;
  color: #585b70;
}

.ft {
  display: flex;
  flex: none;
  gap: 6px;
  padding: 5px 6px 6px;
  border-top: 1px solid #313244;
}
.fbtn {
  flex: 1;
  padding: 8px;
  border: none;
  border-radius: 8px;
  background: #313244;
  color: #cdd6f4;
  font: inherit;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.12s ease;
}
.fbtn:hover {
  background: #45475a;
}
.fbtn.danger {
  color: #f38ba8;
}
.fbtn.danger:hover {
  background: #45303a;
}
</style>
