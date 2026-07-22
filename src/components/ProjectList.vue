<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface Project {
  id: string;
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
  return p.terminal.icon ? `${p.id}:${p.terminal.icon}` : null;
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
        project: p.id,
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
    await invoke("stop_project", { project: project.id });
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

async function openTerminal(project: Project) {
  try {
    await invoke("open_terminal", { project: project.id });
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

interface PendingRestart {
  id: string;
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
    await invoke("assign_pool", { project: project.id, pool });
    if (project.running && pool !== from) {
      pending.value = { id: project.id, name: project.name, from, to: pool };
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
    await invoke("restart_project", { project: p.id });
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

// Anzeige: Home-Anteil ~-kontrahiert; der volle Pfad steht im Hover-Pop.
function contractHome(p: string): string {
  return p.replace(/^\/(?:home|Users)\/[^/]+/, "~");
}

interface TerminalSettings {
  id: string;
  name: string;
  path: string;
  theme: string;
  icon: string | null;
  title: string;
  todo: boolean;
  workDirs: string[];
  archiveHome: string | null;
}

const settings = ref<TerminalSettings | null>(null);

async function openSettings(p: Project) {
  try {
    const todo = await invoke<boolean>("todo_state", { project: p.id });
    const workDirs = await invoke<string[]>("project_work_dirs", {
      project: p.id,
    });
    const archiveHome = await invoke<string | null>("panel_archive_dir_cmd", {
      project: p.id,
    });
    settings.value = {
      id: p.id,
      name: p.name,
      path: p.path,
      theme: p.terminal.theme ?? "mocha",
      icon: p.terminal.icon,
      title: p.terminal.title ?? "",
      todo,
      workDirs,
      archiveHome,
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
    await invoke("set_project_dir", { project: s.id, dir });
    await refresh();
    const p = projects.value.find((x) => x.id === s.id);
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
    await invoke("add_work_dir", { project: s.id, dir });
    s.workDirs = await invoke<string[]>("project_work_dirs", { project: s.id });
  } catch (e) {
    error.value = String(e);
  }
}

async function removeWorkDir(dir: string) {
  const s = settings.value!;
  try {
    await invoke("remove_work_dir", { project: s.id, dir });
    s.workDirs = await invoke<string[]>("project_work_dirs", { project: s.id });
  } catch (e) {
    error.value = String(e);
  }
}

// Archiv-Home schreibt wie die Arbeitsordner direkt (Config + Permissions);
// ohne Archiv zeigt das Panel nur Befehle und Dokument. Greift ab dem
// nächsten Session-Start. Ist schon eins gesetzt, fragt ein Dialog nach —
// mit der Option, die Dokumente mitzunehmen (nichts wird implizit verschoben).
interface PendingArchiveChange {
  dir: string;
  migrate: boolean;
}
const pendingArchive = ref<PendingArchiveChange | null>(null);

async function chooseArchive() {
  const s = settings.value!;
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir !== "string") return;
  if (s.archiveHome && s.archiveHome !== dir) {
    pendingArchive.value = { dir, migrate: false };
    return;
  }
  try {
    await invoke("set_archive_home_cmd", { project: s.id, dir });
    s.archiveHome = await invoke<string | null>("panel_archive_dir_cmd", {
      project: s.id,
    });
  } catch (e) {
    error.value = String(e);
  }
}

async function confirmArchiveChange() {
  const s = settings.value!;
  const a = pendingArchive.value!;
  try {
    await invoke("change_archive_home_cmd", { project: s.id, dir: a.dir, migrate: a.migrate });
    s.archiveHome = await invoke<string | null>("panel_archive_dir_cmd", {
      project: s.id,
    });
    pendingArchive.value = null;
  } catch (e) {
    error.value = String(e);
    pendingArchive.value = null;
  }
}

async function clearArchive() {
  const s = settings.value!;
  try {
    await invoke("clear_archive_home_cmd", { project: s.id });
    s.archiveHome = null;
  } catch (e) {
    error.value = String(e);
  }
}

async function pickIcon() {
  const file = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Icon", extensions: ["png", "svg"] }],
  });
  if (typeof file === "string") settings.value!.icon = file;
}

async function saveSettings() {
  const s = settings.value!;
  try {
    const title = s.title.trim();
    await invoke("set_terminal_config", {
      project: s.id,
      theme: s.theme === "mocha" ? null : s.theme,
      icon: s.icon,
      title: title === "" ? null : title,
    });
    await invoke("set_todo", { project: s.id, enabled: s.todo });
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

interface DeletePreview {
  name: string;
  projectDir: string;
  aiControlDir: boolean;
  archivePermission: boolean;
  todoHook: boolean;
  panelFiles: number;
  archiveHome: string | null;
  archiveDocs: number;
  workDirs: string[];
}

// Eskalationsleiter: jede Stufe schließt die vorige ein.
type DeleteScope = "integration" | "archive" | "full";

interface PendingDelete {
  id: string;
  preview: DeletePreview;
  scope: DeleteScope;
  deleteWorkDirs: boolean;
}

const pendingDelete = ref<PendingDelete | null>(null);

async function askDelete(p: Project) {
  error.value = "";
  try {
    const preview = await invoke<DeletePreview>("delete_preview", { project: p.id });
    pendingDelete.value = { id: p.id, preview, scope: "integration", deleteWorkDirs: false };
  } catch (e) {
    error.value = String(e);
  }
}

async function confirmDelete() {
  const d = pendingDelete.value!;
  try {
    await invoke("delete_project_scoped", {
      project: d.id,
      scope: d.scope,
      deleteWorkDirs: d.scope === "full" && d.deleteWorkDirs,
    });
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
  <div class="list-scroll">
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
      <tr v-for="p in projects" :key="p.id">
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
  </div>

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
      <h3>{{ $t("projects.deleteTitle", { name: pendingDelete.preview.name }) }}</h3>

      <label class="scope">
        <input v-model="pendingDelete.scope" type="radio" value="integration" />
        <span>
          <strong>{{ $t("projects.scopeIntegration") }}</strong>
          <small>{{ $t("projects.scopeIntegrationDesc") }}</small>
        </span>
      </label>
      <label class="scope">
        <input
          v-model="pendingDelete.scope"
          type="radio"
          value="archive"
          :disabled="!pendingDelete.preview.archiveHome"
        />
        <span>
          <strong>{{ $t("projects.scopeArchive") }}</strong>
          <small v-if="pendingDelete.preview.archiveHome">
            {{ $t("projects.scopeArchiveDesc", { path: pendingDelete.preview.archiveHome }) }}
            ({{ $t("projects.docCount", pendingDelete.preview.archiveDocs) }})
          </small>
          <small v-else>{{ $t("projects.archiveNone") }}</small>
        </span>
      </label>
      <label class="scope">
        <input v-model="pendingDelete.scope" type="radio" value="full" />
        <span>
          <strong>{{ $t("projects.scopeFull") }}</strong>
          <small>{{ $t("projects.scopeFullDesc", { path: pendingDelete.preview.projectDir }) }}</small>
        </span>
      </label>
      <label
        v-if="pendingDelete.scope === 'full' && pendingDelete.preview.workDirs.length"
        class="checkline"
      >
        <input v-model="pendingDelete.deleteWorkDirs" type="checkbox" />
        {{ $t("projects.deleteWorkDirs") }}
      </label>

      <p class="hint">{{ $t("projects.deletePreviewTitle") }}</p>
      <ul class="affected">
        <li>{{ $t("projects.artRegistry") }}</li>
        <li v-if="pendingDelete.preview.aiControlDir">{{ $t("projects.artAiControl") }}</li>
        <li v-if="pendingDelete.preview.archivePermission">{{ $t("projects.artArchivePerm") }}</li>
        <li v-if="pendingDelete.preview.todoHook">{{ $t("projects.artTodoHook") }}</li>
        <li v-if="pendingDelete.preview.panelFiles">
          {{ $t("projects.artPanelFiles", pendingDelete.preview.panelFiles) }}
        </li>
        <li>{{ $t("projects.artDesktop") }}</li>
        <li v-if="pendingDelete.scope !== 'integration' && pendingDelete.preview.archiveHome">
          {{ $t("projects.artArchive", { path: pendingDelete.preview.archiveHome }) }}
          ({{ $t("projects.docCount", pendingDelete.preview.archiveDocs) }})
        </li>
        <li v-if="pendingDelete.scope === 'full'">
          {{ $t("projects.artProjectDir", { path: pendingDelete.preview.projectDir }) }}
        </li>
        <template v-if="pendingDelete.scope === 'full' && pendingDelete.deleteWorkDirs">
          <li v-for="wd in pendingDelete.preview.workDirs" :key="wd">
            {{ $t("projects.artWorkDir", { path: wd }) }}
          </li>
        </template>
      </ul>

      <div class="actions">
        <button type="button" @click="pendingDelete = null">
          {{ $t("projects.cancel") }}
        </button>
        <button class="danger" @click="confirmDelete">
          {{ $t("projects.deleteConfirm") }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="settings" class="overlay">
    <div class="dialog settings-dialog">
      <button class="close" :title="$t('projects.cancel')" @click="settings = null">✕</button>
      <h3>{{ $t("projects.terminal", { name: settings.name }) }}</h3>

      <div class="sbody">
      <section class="sgroup">
        <h4 class="eyebrow">{{ $t("projects.groupAppearance") }}</h4>
        <label class="srow">
          <span class="slbl">{{ $t("projects.title") }}</span>
          <span class="sval">
            <input v-model="settings.title" :placeholder="settings.name" />
          </span>
          <span class="sacts"></span>
        </label>
        <label class="srow">
          <span class="slbl">{{ $t("projects.theme") }}</span>
          <span class="sval">
            <select v-model="settings.theme">
              <option v-for="[value, label] in THEME_NAMES" :key="value" :value="value">
                {{ label }}
              </option>
            </select>
          </span>
          <span class="sacts"></span>
        </label>
        <div class="srow">
          <span class="slbl">{{ $t("projects.dockIcon") }}</span>
          <span class="sval">
            <span v-if="settings.icon" class="spath hover-pop">
              {{ settings.icon.split("/").pop() }}
              <span class="pop pop-path">{{ settings.icon }}</span>
            </span>
            <span v-else class="spath muted">{{ $t("projects.defaultIcon") }}</span>
          </span>
          <span class="sacts">
            <button @click="pickIcon">{{ $t("projects.chooseFile") }}</button>
            <button v-if="settings.icon" @click="settings.icon = null">
              {{ $t("projects.remove") }}
            </button>
          </span>
        </div>
        <p class="ghint">{{ $t("projects.appliesNextStart") }}</p>
      </section>

      <section class="sgroup instant">
        <h4 class="eyebrow">{{ $t("projects.groupFolders") }}</h4>
        <div class="srow">
          <span class="slbl">{{ $t("projects.projectDir") }}</span>
          <span class="sval">
            <span class="spath hover-pop">
              {{ contractHome(settings.path) }}
              <span class="pop pop-path">{{ settings.path }}</span>
            </span>
          </span>
          <span class="sacts">
            <button @click="changeProjectDir">{{ $t("projects.changeDir") }}</button>
          </span>
        </div>
        <div v-for="(wd, i) in settings.workDirs" :key="wd" class="srow">
          <span class="slbl">{{ i === 0 ? $t("projects.workDir") : "" }}</span>
          <span class="sval">
            <span class="spath hover-pop">
              {{ contractHome(wd) }}
              <span class="pop pop-path">{{ wd }}</span>
            </span>
          </span>
          <span class="sacts">
            <button @click="removeWorkDir(wd)">{{ $t("projects.remove") }}</button>
          </span>
        </div>
        <div class="srow">
          <span class="slbl">{{ settings.workDirs.length ? "" : $t("projects.workDir") }}</span>
          <span class="sval"></span>
          <span class="sacts">
            <button @click="addWorkDir">{{ $t("projects.addWorkDir") }}</button>
          </span>
        </div>
        <div class="srow">
          <span class="slbl">{{ $t("projects.archive") }}</span>
          <span class="sval">
            <span v-if="settings.archiveHome" class="spath hover-pop">
              {{ contractHome(settings.archiveHome) }}
              <span class="pop pop-path">{{ settings.archiveHome }}</span>
            </span>
            <span v-else class="spath muted">{{ $t("projects.archiveNone") }}</span>
          </span>
          <span class="sacts">
            <button @click="chooseArchive">{{ $t("projects.changeDir") }}</button>
            <button v-if="settings.archiveHome" @click="clearArchive">
              {{ $t("projects.remove") }}
            </button>
          </span>
        </div>
        <p class="ghint">{{ $t("projects.writesImmediately") }}</p>
      </section>

      <section class="sgroup">
        <h4 class="eyebrow">{{ $t("projects.groupSession") }}</h4>
        <label class="checkline todo-line">
          <input v-model="settings.todo" type="checkbox" />
          <span>{{ $t("projects.todo") }} — {{ $t("projects.todoDesc") }}</span>
        </label>
      </section>
      </div>

      <div class="actions">
        <button @click="settings = null">{{ $t("projects.cancel") }}</button>
        <button class="primary" @click="saveSettings">
          {{ $t("projects.save") }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="pendingArchive && settings" class="overlay" @click.self="pendingArchive = null">
    <div class="dialog">
      <h3>{{ $t("projects.archiveChangeTitle") }}</h3>
      <p>
        {{ $t("projects.archiveChangeText", {
          old: contractHome(settings.archiveHome ?? ""),
          neu: contractHome(pendingArchive.dir),
        }) }}
      </p>
      <label class="checkline">
        <input v-model="pendingArchive.migrate" type="checkbox" />
        {{ $t("projects.archiveMigrate") }}
      </label>
      <p class="hint">{{ $t("projects.archiveMigrateHint") }}</p>
      <div class="actions">
        <button type="button" @click="pendingArchive = null">
          {{ $t("projects.cancel") }}
        </button>
        <button class="primary" @click="confirmArchiveChange">
          {{ $t("projects.archiveChangeConfirm") }}
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

<style scoped>
/* Lösch-Dialog: drei Stufen als Radio-Zeilen mit Beschreibung. */
.scope {
  display: flex;
  align-items: flex-start;
  gap: 0.6rem;
  cursor: pointer;
}

.scope input {
  margin-top: 0.2rem;
}

.scope span {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
}

.scope strong {
  font-size: 0.9rem;
  color: var(--text);
}

.scope small {
  color: var(--overlay);
  font-size: 0.78rem;
}

.scope:has(input:disabled) {
  opacity: 0.55;
  cursor: default;
}

/* Settings-Dialog: festes Grid Label | Wert | Aktionen in drei Gruppen. */
.settings-dialog {
  position: relative;
  width: min(48rem, 94vw);
  max-width: none;
  /* Feste, großzügige Höhe — Kopf und Aktionszeile stehen, nur der
     Mittelteil (.sbody) scrollt, wenn er nicht passt. */
  height: min(60rem, 94vh);
  overflow: hidden;
  gap: 0.7rem;
  border-color: var(--surface2);
}

.sbody {
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  margin: 0 -0.5rem;
  padding: 0 0.5rem;
}

/* Aktions-Buttons gleich breit — ruhige Aktionsspalte; das Schließen-Kreuz
   ist davon ausgenommen. */
.settings-dialog .sacts button,
.settings-dialog .actions button {
  width: 10.5rem;
}

.settings-dialog .close {
  position: absolute;
  top: 0.7rem;
  right: 0.7rem;
  width: 1.8rem;
  height: 1.8rem;
  padding: 0;
  line-height: 1;
  color: var(--overlay);
  background: none;
  border: none;
}

.settings-dialog .close:hover {
  color: var(--text);
}

.sgroup {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  padding: 0.7rem 0.9rem;
}

/* Sofort-Schreiber (Ordner/Archiv) als abgesetzte Fläche — hier gibt es
   keinen Speichern-Schritt, der Rest des Dialogs speichert erst am Ende. */
.sgroup.instant {
  background: var(--crust);
  border: 1px solid var(--surface0);
  border-radius: 9px;
}

.eyebrow {
  margin: 0 0 0.15rem;
  font-size: 0.68rem;
  font-weight: 600;
  letter-spacing: 0.09em;
  text-transform: uppercase;
  color: var(--overlay);
}

.srow {
  display: grid;
  grid-template-columns: 7.5rem 1fr auto;
  align-items: center;
  gap: 0.75rem;
  min-height: 1.9rem;
  color: var(--subtext);
  font-size: 0.85rem;
}

.sval {
  display: flex;
  align-items: center;
  min-width: 0;
}

.sval input,
.sval select {
  width: 100%;
}

.sacts {
  display: flex;
  gap: 0.4rem;
  justify-content: flex-end;
}

/* Todo-Zeile: volle Dialogbreite, der Text wird nie gekürzt. */
.todo-line {
  font-size: 0.85rem;
  color: var(--subtext);
}

.spath {
  font-family: var(--mono);
  font-size: 0.75rem;
  color: var(--subtext);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.spath.muted {
  color: var(--overlay);
}

.ghint {
  margin: 0.1rem 0 0;
  font-size: 0.75rem;
  color: var(--overlay);
}

.settings-dialog .actions {
  margin-top: 0.25rem;
}
</style>
