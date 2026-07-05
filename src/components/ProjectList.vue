<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface Project {
  name: string;
  path: string;
  pool: string | null;
  running: boolean;
  terminal: { theme: string | null; icon: string | null; title: string | null };
}

interface Pool {
  id: string;
  name: string;
  credentialType: string;
  hasCredentials: boolean;
}

function poolReady(p: Project): boolean {
  if (!p.pool) return false;
  const pool = pools.value.find((x) => x.id === p.pool);
  return pool !== undefined && pool.hasCredentials;
}

// Projekt-Configs tragen die Pool-ID; angezeigt wird der Name aus pool.json.
function poolName(id: string | null): string | null {
  if (!id) return null;
  return pools.value.find((x) => x.id === id)?.name ?? id;
}

function startBlockedReason(p: Project): string | undefined {
  if (poolReady(p)) return undefined;
  return p.pool ? "projects.needsKey" : "projects.needsPool";
}

const projects = ref<Project[]>([]);
const pools = ref<Pool[]>([]);
const error = ref("");

// data-URLs pro (Projekt, Icon-Pfad) — Fehlversuche werden als "" gemerkt,
// damit der 3s-Poll nicht dieselbe Fehlermeldung wiederholt.
const icons = ref<Record<string, string>>({});

function iconKey(p: Project): string | null {
  return p.terminal.icon ? `${p.name}:${p.terminal.icon}` : null;
}

function projIcon(p: Project): string | undefined {
  const key = iconKey(p);
  return key ? icons.value[key] || undefined : undefined;
}

async function loadIcons() {
  for (const p of projects.value) {
    const key = iconKey(p);
    if (!key || key in icons.value) continue;
    try {
      const data = await invoke<string | null>("project_icon", {
        project: p.name,
      });
      icons.value[key] = data ?? "";
    } catch (e) {
      error.value = String(e);
      icons.value[key] = "";
    }
  }
}

async function refresh() {
  try {
    projects.value = await invoke<Project[]>("list_projects");
    pools.value = await invoke<Pool[]>("list_pools");
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
  await loadIcons();
}

async function stop(project: Project) {
  try {
    await invoke("stop_project", { project: project.name });
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

async function openTerminal(project: Project) {
  try {
    await invoke("open_terminal", { project: project.name });
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

interface PendingRestart {
  name: string;
  from: string | null;
  to: string;
}

const pending = ref<PendingRestart | null>(null);
const restarting = ref(false);

async function assign(project: Project, event: Event) {
  const pool = (event.target as HTMLSelectElement).value;
  const from = project.pool;
  try {
    await invoke("assign_pool", { project: project.name, pool });
    if (project.running && pool !== from) {
      pending.value = { name: project.name, from, to: pool };
    }
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

async function restartNow() {
  const p = pending.value!;
  restarting.value = true;
  try {
    await invoke("restart_project", { project: p.name });
    pending.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
    pending.value = null;
  }
  restarting.value = false;
}

const THEME_NAMES: [string, string][] = [
  ["mocha", "Catppuccin Mocha"],
  ["dracula", "Dracula"],
  ["solarized-dark", "Solarized Dark"],
  ["gruvbox", "Gruvbox"],
  ["one-dark", "One Dark"],
  ["nord", "Nord"],
  ["tokyo-night", "Tokyo Night"],
  ["monokai", "Monokai"],
  ["rose-pine", "Rosé Pine"],
  ["everforest", "Everforest"],
  ["solarized-light", "Solarized Light"],
  ["catppuccin-latte", "Catppuccin Latte"],
  ["one-light", "One Light"],
];

interface TerminalSettings {
  name: string;
  path: string;
  theme: string;
  icon: string | null;
  title: string;
  todo: boolean;
  workDirs: string[];
}

const settings = ref<TerminalSettings | null>(null);

async function openSettings(p: Project) {
  try {
    const todo = await invoke<boolean>("todo_state", { project: p.name });
    const workDirs = await invoke<string[]>("project_work_dirs", {
      project: p.name,
    });
    settings.value = {
      name: p.name,
      path: p.path,
      theme: p.terminal.theme ?? "mocha",
      icon: p.terminal.icon,
      title: p.terminal.title ?? "",
      todo,
      workDirs,
    };
  } catch (e) {
    error.value = String(e);
  }
}

// Projektordner neu zuordnen (Root, in dem die Session startet); verschoben
// wird nichts — der gewählte Ordner ist der neue Ort des Projekts.
async function changeProjectDir() {
  const s = settings.value!;
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir !== "string") return;
  try {
    await invoke("set_project_dir", { project: s.name, dir });
    await refresh();
    const p = projects.value.find((x) => x.name === s.name);
    if (p) s.path = p.path;
  } catch (e) {
    error.value = String(e);
  }
}

// Arbeitsordner schreiben direkt in die Projekt-settings.json
// (additionalDirectories + Edit-Permission) — kein Speichern-Schritt.
async function addWorkDir() {
  const s = settings.value!;
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir !== "string") return;
  try {
    await invoke("add_work_dir", { project: s.name, dir });
    s.workDirs = await invoke<string[]>("project_work_dirs", { project: s.name });
  } catch (e) {
    error.value = String(e);
  }
}

async function removeWorkDir(dir: string) {
  const s = settings.value!;
  try {
    await invoke("remove_work_dir", { project: s.name, dir });
    s.workDirs = await invoke<string[]>("project_work_dirs", { project: s.name });
  } catch (e) {
    error.value = String(e);
  }
}

async function pickIcon() {
  const file = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Icon", extensions: ["png", "icns"] }],
  });
  if (typeof file === "string") settings.value!.icon = file;
}

async function saveSettings() {
  const s = settings.value!;
  try {
    const title = s.title.trim();
    await invoke("set_terminal_config", {
      project: s.name,
      theme: s.theme === "mocha" ? null : s.theme,
      icon: s.icon,
      title: title === "" ? null : title,
    });
    await invoke("set_todo", { project: s.name, enabled: s.todo });
    settings.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

interface Wizard {
  name: string;
  pool: string;
  dirMode: "default" | "custom";
  dir: string | null;
  workDirMode: "none" | "default" | "custom";
  workDir: string | null;
  title: string;
  theme: string;
  todo: boolean;
}

const wizard = ref<Wizard | null>(null);

function openWizard() {
  error.value = "";
  wizard.value = {
    name: "",
    pool: "",
    dirMode: "default",
    dir: null,
    workDirMode: "none",
    workDir: null,
    title: "",
    theme: "mocha",
    todo: false,
  };
}

// Bestehenden Ordner als Projekt aufnehmen — Name ist der Ordnername.
async function addFolder() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir !== "string") return;
  try {
    await invoke("add_project", { path: dir });
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

async function pickProjectDir() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") {
    wizard.value!.dir = dir;
    wizard.value!.dirMode = "custom";
  }
}

async function pickWorkDir() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") {
    wizard.value!.workDir = dir;
    wizard.value!.workDirMode = "custom";
  }
}

async function createProject() {
  const w = wizard.value!;
  const name = w.name.trim();
  const workDir =
    w.workDirMode === "default"
      ? `~/projects/${name}`
      : w.workDirMode === "custom"
        ? w.workDir
        : null;
  const title = w.title.trim();
  try {
    await invoke("create_project_full", {
      name,
      // custom: gewählter Ordner ist der Ablageort, das Projekt entsteht darin
      dir: w.dirMode === "custom" ? `${w.dir}/${name}` : null,
      pool: w.pool === "" ? null : w.pool,
      workDir,
      createWorkDir: w.workDirMode === "default",
      terminal: {
        theme: w.theme === "mocha" ? null : w.theme,
        icon: null,
        title: title === "" ? null : title,
      },
      todo: w.todo,
    });
    wizard.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

interface PendingDelete {
  name: string;
  workDirs: string[];
  deleteWorkDirs: boolean;
}

const pendingDelete = ref<PendingDelete | null>(null);

async function askDelete(p: Project) {
  error.value = "";
  try {
    const workDirs = await invoke<string[]>("project_work_dirs", {
      project: p.name,
    });
    pendingDelete.value = { name: p.name, workDirs, deleteWorkDirs: false };
  } catch (e) {
    error.value = String(e);
  }
}

async function confirmDelete() {
  const d = pendingDelete.value!;
  try {
    await invoke("delete_project", {
      name: d.name,
      deleteWorkDirs: d.deleteWorkDirs,
    });
    pendingDelete.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
    pendingDelete.value = null;
  }
}

// Nur den Registry-Eintrag entfernen; der Ordner bleibt.
async function unlinkProject() {
  const d = pendingDelete.value!;
  try {
    await invoke("remove_project", { name: d.name });
    pendingDelete.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
    pendingDelete.value = null;
  }
}

let timer: number;
onMounted(() => {
  refresh();
  timer = window.setInterval(refresh, 3000);
});
onUnmounted(() => window.clearInterval(timer));
</script>

<template>
  <div class="toolbar">
    <button class="primary" @click="openWizard">
      {{ $t("projects.newProject") }}
    </button>
    <button :title="$t('projects.addFolderTitle')" @click="addFolder">
      {{ $t("projects.addFolder") }}
    </button>
  </div>

  <p v-if="error" class="error">{{ error }}</p>
  <table class="grid">
    <colgroup>
      <col class="col-dot" />
      <col />
      <col class="col-pool" />
      <col class="col-actions" />
    </colgroup>
    <thead>
      <tr>
        <th></th>
        <th>{{ $t("projects.project") }}</th>
        <th>{{ $t("projects.pool") }}</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="p in projects" :key="p.name">
        <td class="cell-dot">
          <span class="dot" :class="{ on: p.running }"></span>
        </td>
        <td class="cell-name">
          <span class="name-row">
            <img v-if="projIcon(p)" class="proj-icon" :src="projIcon(p)" />
            <strong>{{ p.name }}</strong>
          </span>
          <small>{{ p.path }}</small>
        </td>
        <td>
          <select :value="p.pool ?? ''" @change="assign(p, $event)">
            <option value="" disabled>{{ $t("projects.noPool") }}</option>
            <option v-for="pool in pools" :key="pool.id" :value="pool.id">
              {{ pool.name }}
            </option>
          </select>
        </td>
        <td class="cell-actions">
          <span class="row-actions">
            <button
              class="gear"
              :title="$t('projects.terminalSettings')"
              @click="openSettings(p)"
            >
              ⚙︎
            </button>
            <button
              class="gear danger"
              :disabled="p.running"
              :title="p.running ? $t('projects.deleteRunning') : $t('projects.delete')"
              @click="askDelete(p)"
            >
              🗑
            </button>
            <button v-if="p.running" class="stop" @click="stop(p)">
              {{ $t("projects.stop") }}
            </button>
            <button
              v-else
              class="start"
              :disabled="!poolReady(p)"
              :title="startBlockedReason(p) && $t(startBlockedReason(p)!)"
              @click="openTerminal(p)"
            >
              {{ $t("projects.start") }}
            </button>
          </span>
        </td>
      </tr>
    </tbody>
  </table>

  <div v-if="wizard" class="overlay" @click.self="wizard = null">
    <form class="dialog" @submit.prevent="createProject">
      <h3>{{ $t("projects.wizardTitle") }}</h3>
      <label class="field">
        {{ $t("projects.name") }}
        <input v-model="wizard.name" autofocus required />
      </label>
      <label class="field">
        {{ $t("projects.projectDir") }}
        <span class="icon-row">
          <select v-model="wizard.dirMode">
            <option value="default">
              ~/claude-projects/{{ wizard.name.trim() || "…" }}
            </option>
            <option value="custom">{{ $t("projects.projectDirCustom") }}</option>
          </select>
          <button
            v-if="wizard.dirMode === 'custom'"
            type="button"
            @click="pickProjectDir"
          >
            {{ $t("projects.chooseFolder") }}
          </button>
        </span>
      </label>
      <label v-if="wizard.dirMode === 'custom' && wizard.dir" class="field">
        <span></span>
        <span class="icon-path">{{ wizard.dir }}/{{ wizard.name.trim() || "…" }}</span>
      </label>
      <label class="field">
        {{ $t("projects.pool") }}
        <select v-model="wizard.pool">
          <option value="">{{ $t("projects.noPool") }}</option>
          <option v-for="pool in pools" :key="pool.id" :value="pool.id">
            {{ pool.name }}
          </option>
        </select>
      </label>
      <label class="field">
        {{ $t("projects.workDir") }}
        <span class="icon-row">
          <select v-model="wizard.workDirMode">
            <option value="none">{{ $t("projects.workDirNone") }}</option>
            <option value="default">
              ~/projects/{{ wizard.name.trim() || "…" }}
            </option>
            <option value="custom">{{ $t("projects.workDirCustom") }}</option>
          </select>
          <button
            v-if="wizard.workDirMode === 'custom'"
            type="button"
            @click="pickWorkDir"
          >
            {{ $t("projects.chooseFolder") }}
          </button>
        </span>
      </label>
      <label v-if="wizard.workDirMode === 'custom' && wizard.workDir" class="field">
        <span></span>
        <span class="icon-path">{{ wizard.workDir }}</span>
      </label>
      <label class="field">
        {{ $t("projects.title") }}
        <input v-model="wizard.title" :placeholder="wizard.name.trim()" />
      </label>
      <label class="field">
        {{ $t("projects.theme") }}
        <select v-model="wizard.theme">
          <option v-for="[value, label] in THEME_NAMES" :key="value" :value="value">
            {{ label }}
          </option>
        </select>
      </label>
      <label class="field">
        {{ $t("projects.todo") }}
        <span class="checkline">
          <input v-model="wizard.todo" type="checkbox" />
          {{ $t("projects.todoDesc") }}
        </span>
      </label>
      <p class="hint">{{ $t("projects.wizardHint") }}</p>
      <div class="actions">
        <button type="button" @click="wizard = null">
          {{ $t("projects.cancel") }}
        </button>
        <button
          type="submit"
          class="primary"
          :disabled="
            wizard.name.trim() === '' ||
            (wizard.dirMode === 'custom' && !wizard.dir) ||
            (wizard.workDirMode === 'custom' && !wizard.workDir)
          "
        >
          {{ $t("projects.create") }}
        </button>
      </div>
    </form>
  </div>

  <div v-if="pendingDelete" class="overlay" @click.self="pendingDelete = null">
    <div class="dialog">
      <h3>{{ $t("projects.deleteTitle", { name: pendingDelete.name }) }}</h3>
      <p class="hint">{{ $t("projects.deleteWarning", { name: pendingDelete.name }) }}</p>
      <template v-if="pendingDelete.workDirs.length">
        <label class="checkline">
          <input v-model="pendingDelete.deleteWorkDirs" type="checkbox" />
          {{ $t("projects.deleteWorkDirs") }}
        </label>
        <ul class="affected">
          <li v-for="wd in pendingDelete.workDirs" :key="wd">{{ wd }}</li>
        </ul>
        <p v-if="!pendingDelete.deleteWorkDirs" class="hint">
          {{ $t("projects.workDirsStay") }}
        </p>
      </template>
      <p class="hint">{{ $t("projects.unlinkHint") }}</p>
      <div class="actions">
        <button type="button" @click="pendingDelete = null">
          {{ $t("projects.cancel") }}
        </button>
        <button @click="unlinkProject">
          {{ $t("projects.unlink") }}
        </button>
        <button class="danger" @click="confirmDelete">
          {{ $t("projects.delete") }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="settings" class="overlay">
    <div class="dialog">
      <h3>{{ $t("projects.terminal", { name: settings.name }) }}</h3>
      <label class="field">
        {{ $t("projects.title") }}
        <input v-model="settings.title" :placeholder="settings.name" />
      </label>
      <label class="field">
        {{ $t("projects.theme") }}
        <select v-model="settings.theme">
          <option v-for="[value, label] in THEME_NAMES" :key="value" :value="value">
            {{ label }}
          </option>
        </select>
      </label>
      <label class="field">
        {{ $t("projects.dockIcon") }}
        <span class="icon-row">
          <button @click="pickIcon">{{ $t("projects.chooseFile") }}</button>
          <button v-if="settings.icon" @click="settings.icon = null">
            {{ $t("projects.remove") }}
          </button>
          <span v-if="settings.icon" class="icon-path hover-pop">
            {{ settings.icon.split("/").pop() }}
            <span class="pop pop-path">{{ settings.icon }}</span>
          </span>
          <span v-else class="icon-path">{{ $t("projects.defaultIcon") }}</span>
        </span>
      </label>
      <label class="field field-top">
        {{ $t("projects.workDir") }}
        <span class="workdirs">
          <span class="icon-row">
            <span class="icon-path">{{ settings.path }}</span>
            <button @click="changeProjectDir">{{ $t("projects.changeDir") }}</button>
          </span>
          <span v-for="wd in settings.workDirs" :key="wd" class="icon-row">
            <span class="icon-path">{{ wd }}</span>
            <button @click="removeWorkDir(wd)">{{ $t("projects.remove") }}</button>
          </span>
          <button @click="addWorkDir">{{ $t("projects.addWorkDir") }}</button>
        </span>
      </label>
      <label class="field">
        {{ $t("projects.todo") }}
        <span class="checkline">
          <input v-model="settings.todo" type="checkbox" />
          {{ $t("projects.todoDesc") }}
        </span>
      </label>
      <p class="hint">{{ $t("projects.appliesNextStart") }}</p>
      <div class="actions">
        <button @click="settings = null">{{ $t("projects.cancel") }}</button>
        <button class="primary" @click="saveSettings">
          {{ $t("projects.save") }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="pending" class="overlay">
    <div class="dialog">
      <h3>{{ $t("projects.stillRunning", { name: pending.name }) }}</h3>
      <p>{{ $t("projects.poolChangeSaved") }}</p>
      <div class="pools">
        <span class="chip">{{ poolName(pending.from) ?? $t("projects.noPoolAssigned") }}</span>
        <span class="arrow">→</span>
        <span class="chip new">{{ poolName(pending.to) }}</span>
      </div>
      <p class="hint">{{ $t("projects.restartHint") }}</p>
      <div class="actions">
        <button @click="pending = null">{{ $t("projects.keepRunning") }}</button>
        <button class="primary" :disabled="restarting" @click="restartNow">
          {{ restarting ? $t("projects.restarting") : $t("projects.restartNow") }}
        </button>
      </div>
    </div>
  </div>
</template>
