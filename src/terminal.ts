import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { LigaturesAddon } from "@xterm/addon-ligatures";
// Fenster-Styles zuerst, damit die Kaskade der früheren <head>-Reihenfolge
// entspricht (Fenster-CSS vor xterm/fonts). CSP: kein style-src unsafe-inline.
import "./terminal-window.css";
import "@xterm/xterm/css/xterm.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/700.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { wirePanel } from "./panel-wiring";
import { applyTheme, THEMES } from "./themes";
import { flash, panelToast } from "./tiles";
import { initArchiveForm } from "./archive-form";
import { applyI18n, t } from "./messages";

// Debug-Instrumentierung: Fehler auf der Seite anzeigen und ins Dev-Log spiegeln.
function showError(msg: string) {
  const el = document.createElement("pre");
  el.style.cssText =
    "color:#f38ba8;padding:16px;font:12px Menlo,monospace;white-space:pre-wrap";
  el.textContent = msg;
  document.body.appendChild(el);
  invoke("term_log", { msg });
}
window.addEventListener("error", (e) =>
  showError(`error: ${e.message}\n${e.error?.stack ?? ""}`),
);
window.addEventListener("unhandledrejection", (e) =>
  showError(`rejection: ${e.reason}\n${e.reason?.stack ?? ""}`),
);

applyI18n();
const project = new URLSearchParams(location.search).get("project")!;
const win = getCurrentWebviewWindow();

// macOS nutzt die native Ampel (Overlay-Titelbar); sonst eigene Fensterknöpfe
// im Header (Linux ist dekorationslos, siehe terminal.rs).
const isMac = /Mac|Macintosh/.test(navigator.userAgent);
document.documentElement.dataset.platform = isMac ? "mac" : "other";
if (!isMac) {
  document
    .getElementById("win-min")
    ?.addEventListener("click", () => win.minimize());
  document
    .getElementById("win-max")
    ?.addEventListener("click", () => win.toggleMaximize());
  document
    .getElementById("win-close")
    ?.addEventListener("click", () => win.close());
  // Resize-Zonen: dekorationslose Fenster haben keinen nativen Rand.
  for (const g of document.querySelectorAll<HTMLElement>(".grip")) {
    g.addEventListener("mousedown", (e) => {
      e.preventDefault();
      win.startResizeDragging(g.dataset.dir as never);
    });
  }
}

// Doppelklick auf die Kopfleiste maximiert (Fenster-Konvention).
document.querySelector("header")!.addEventListener("dblclick", (e) => {
  if ((e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) {
    win.toggleMaximize();
  }
});

// Fensterhintergrund beim Öffnen kommt aus terminal.rs (theme_background);
// dort nicht gepflegte Themes fallen auf Mocha zurück (nur kurzer Moment
// bis die Webview lädt).

interface Project {
  id: string;
  name: string;
  path: string;
  pool: string | null;
  terminal: { theme: string | null; icon: string | null; title: string | null };
}
const projects = await invoke<Project[]>("list_projects");
const cfg = projects.find((p) => p.id === project);
if (cfg?.pool) {
  // Anzeigename statt Pool-ID (Ordnername).
  document.getElementById("pool")!.textContent = await invoke<string>(
    "pool_label",
    { pool: cfg.pool },
  );
}
document.getElementById("project-name")!.textContent =
  cfg?.terminal.title ?? cfg?.name ?? project;

// Projekt-Icon vor den Titel — dieselbe data-URL wie in der Projektliste.
if (cfg?.terminal.icon) {
  const data = await invoke<string | null>("project_icon", { project });
  if (data) {
    const img = document.getElementById("project-icon") as HTMLImageElement;
    img.src = data;
    img.hidden = false;
  }
}

const picked = THEMES[cfg?.terminal.theme ?? "mocha"];
const theme = picked.xterm;

// Seitenfarben ans Theme anpassen — alles Weitere hängt an den CSS-Variablen.
applyTheme(picked);

// Globale Schriftgröße (settings.json: terminalFontSize), ein Wert für alle.
const DEFAULT_FONT_SIZE = 13;
let fontSize = await invoke<number>("terminal_font_size");

const term = new Terminal({
  // Ligatur-Addon nutzt die Character-Joiner-API (Proposed API)
  allowProposedApi: true,
  fontFamily: '"JetBrains Mono", Menlo, monospace',
  fontSize,
  fontWeightBold: "600",
  lineHeight: 1.0,
  letterSpacing: 0,
  cursorBlink: true,
  cursorStyle: "bar",
  scrollback: 10000,
  theme,
});
const fit = new FitAddon();
term.loadAddon(fit);

// Ctrl/Cmd + +/-/0: Schriftgröße global anpassen, sofort im aktuellen Fenster.
function applyFontSize(px: number) {
  fontSize = Math.max(8, Math.min(24, px));
  term.options.fontSize = fontSize;
  fit.fit();
  invoke("set_terminal_font_size", { size: fontSize });
}

// Shift+Enter als CSI-u-Sequenz senden (Zeilenumbruch in Claude Code,
// auch bei nicht-leerer Zeile). Neben keydown muss auch keypress unter-
// drückt werden, sonst sendet xterm.js zusätzlich \r und submittet damit.
term.attachCustomKeyEventHandler((e) => {
  if (e.key === "Enter" && e.shiftKey) {
    if (e.type === "keydown") invoke("term_write", { data: "\x1b[13;2u" });
    return false;
  }
  if ((e.ctrlKey || e.metaKey) && !e.altKey) {
    if (e.key === "+" || e.key === "=") {
      if (e.type === "keydown") applyFontSize(fontSize + 1);
      return false;
    }
    if (e.key === "-" || e.key === "_") {
      if (e.type === "keydown") applyFontSize(fontSize - 1);
      return false;
    }
    if (e.key === "0") {
      if (e.type === "keydown") applyFontSize(DEFAULT_FONT_SIZE);
      return false;
    }
  }
  return true;
});

// Font muss geladen sein, bevor WebGL den Glyphen-Atlas aufbaut —
// beide verwendeten Gewichte (400 normal, 600 bold).
await document.fonts.load(`${fontSize}px "JetBrains Mono"`);
await document.fonts.load(`600 ${fontSize}px "JetBrains Mono"`);
term.open(document.getElementById("term")!);
// Reihenfolge laut Addon-Doku: Ligaturen vor WebGL aktivieren.
term.loadAddon(new LigaturesAddon());
term.loadAddon(new WebglAddon());
fit.fit();

function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

await win.listen<string>("pty-output", (e) =>
  term.write(b64ToBytes(e.payload)),
);
await win.listen("pty-exit", () =>
  term.write(`\r\n\x1b[90m${t("panel.processEnded")}\x1b[0m\r\n`),
);

term.onData((data) => invoke("term_write", { data }));
term.onResize(({ rows, cols }) => invoke("term_resize", { rows, cols }));
window.addEventListener("resize", () => fit.fit());

await invoke("term_start", { project, rows: term.rows, cols: term.cols });
term.focus();

// --- Andockbares Entwurfs-Panel -------------------------------------------
// Der Skill schreibt Entwürfe in eine Datei; terminal.rs meldet neuen Inhalt
// per `panel-update`. Das Panel blendet sich dann ein — außer es ist gerade in
// ein eigenes Fenster abgelöst (`panel-detached`).
const panel = document.getElementById("panel")!;
const splitter = document.getElementById("splitter")!;

let detached = false;
let hasContent = false;

function showPanel() {
  panel.hidden = false;
  splitter.hidden = false;
  fit.fit();
}
function hidePanel() {
  panel.hidden = true;
  splitter.hidden = true;
  fit.fit();
}

// Gemeinsame Verdrahtung (Views, Modi, Update-Events); jedes eingehende
// Update blendet das angedockte Panel ein, solange es nicht abgelöst ist.
const { view, views, mode } = await wirePanel(project, () => {
  hasContent = true;
  if (!detached) showPanel();
});
// Panel startet zugeklappt — außer das ToDo-Modul ist gewählt und die Liste
// nicht leer: dann öffnet das Panel beim Start mit der ToDo-Ansicht.
const todos = views.get("todo");
if (todos && !todos.empty()) {
  hasContent = true;
  showPanel();
  mode.to("todo");
} else {
  mode.clear();
}

// Tab-Klick im Header öffnet das (angedockte) Panel, falls es zu ist.
const headerTabs = document.getElementById("panel-tabs")!;
// Ausnahme Archiv & Suche: öffnen standardmäßig im eigenen Panel-Fenster
// (Popup) statt in der schmalen angedockten Spalte — Capture-Phase, damit
// weder der Modus-Wechsel des Tabs noch das Einblenden des Panels feuern.
headerTabs.addEventListener(
  "click",
  (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLElement>(
      'button[data-mode="wiki"], button[data-mode="search"]',
    );
    if (!btn) return;
    e.stopPropagation();
    e.preventDefault();
    void invoke("open_panel_window", { project, mode: btn.dataset.mode });
  },
  { capture: true },
);
headerTabs.addEventListener("click", () => {
  hasContent = true;
  if (!detached) showPanel();
});

// Solange das Panel abgelöst ist, hat das eigene Fenster die Tabs — die im
// Header verschwinden.
await listen("panel-detached", () => {
  detached = true;
  headerTabs.hidden = true;
  hidePanel();
  mode.clear();
});
await listen("panel-attached", () => {
  detached = false;
  headerTabs.hidden = false;
  if (hasContent) showPanel();
});
// Abgelöstes Fenster geschlossen (per ✕ oder OS): angedocktes Panel bleibt
// ausgeblendet, aber der nächste Entwurf darf es wieder einblenden.
await listen("panel-window-closed", () => {
  detached = false;
  headerTabs.hidden = false;
});

// Ablösen gibt es nicht mehr — die Flächen sind fest verteilt; der Knopf
// bleibt versteckt.
document.getElementById("panel-detach")!.hidden = true;
document.getElementById("panel-hide")!.addEventListener("click", () => {
  hidePanel();
  mode.clear();
});

// Archivieren: Button klappt das Formular (Ordner/Beschreibung/Schlagwörter)
// auf; Abschicken wählt notfalls erst das Archiv-Home per Dialog.
const archiveBtn = document.getElementById("panel-archive")!;
const archiveForm = initArchiveForm(
  archiveBtn,
  async (meta) => {
    try {
      const configured = await invoke<string | null>("panel_archive_dir_cmd", {
        project,
      });
      let dir: string | undefined;
      if (!configured) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const chosen = await open({
          directory: true,
          title: t("panel.chooseArchiveDir"),
        });
        if (!chosen) return;
        dir = chosen as string;
      }
      await view.flush(); // offene Bearbeitung erst speichern — archiviert, was zu sehen ist
      await invoke<string>("panel_archive_cmd", {
        project,
        dir,
        title: meta.title ?? null,
        folder: meta.folder ?? null,
        description: meta.description ?? null,
        tags: meta.tags,
      });
      flash(archiveBtn, "copied", 1400);
    } catch (e) {
      flash(archiveBtn, "error", 1400);
      panelToast(`Archivieren fehlgeschlagen: ${e}`);
      invoke("term_log", { msg: `archive: ${e}` });
    }
  },
  {
    // Ordner-Vorschläge aus dem Archiv; „Auf Platte legen" schreibt den
    // Entwurf an einen frei gewählten Pfad (ohne Archiv, ohne Frontmatter).
    folders: () => invoke("archive_folders", { project }),
    title: () => invoke("panel_title_cmd", { project }),
    onSave: async () => {
      try {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({
          title: t("archiveForm.saveTitle"),
          defaultPath: "entwurf.md",
        });
        if (!path) return;
        await view.flush();
        await invoke("panel_save_as", { project, path });
        flash(archiveBtn, "copied", 1400);
      } catch (e) {
        flash(archiveBtn, "error", 1400);
        panelToast(`Speichern fehlgeschlagen: ${e}`);
      }
    },
  },
);
archiveBtn.addEventListener("click", () => archiveForm.toggle());

// Datei-Knopf im Header: die gewählten Pfade gehen als Text ins Prompt der
// laufenden Session — gedacht für den gelegentlichen Fall, dass eine Datei
// genannt werden soll, ohne sie abzutippen. Der Knopf sitzt im Terminal-Fenster,
// weil `term_write` die PTY über das Fensterlabel adressiert (terminal.rs:852).
// Pfade unterhalb des Projektordners werden relativ eingesetzt, Pfade mit
// Leerzeichen in Anführungszeichen.
document.getElementById("term-files")!.addEventListener("click", async () => {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({
    multiple: true,
    defaultPath: cfg?.path,
    title: t("panel.pickFiles"),
  });
  if (!chosen) return;
  const root = cfg?.path.replace(/\/$/, "");
  const text = (chosen as string[])
    .map((p) =>
      root && p.startsWith(`${root}/`) ? p.slice(root.length + 1) : p,
    )
    .map((p) => (/\s/.test(p) ? `"${p}"` : p))
    .join(" ");
  await invoke("term_write", { data: `${text} ` });
  term.focus();
});

// Commit-Dialog: die Session ruft `show_commit` auf, der Watcher meldet die
// angefasste Signaldatei hier — das Terminal-Fenster öffnet daraufhin das
// Fenster. Ein eigener Knopf im Header entfällt.
await listen("commit-open", () => {
  void invoke("open_commit_window", { project });
});

// Splitter: Panelbreite ziehen.
splitter.addEventListener("mousedown", (e) => {
  e.preventDefault();
  const move = (ev: MouseEvent) => {
    const w = Math.max(
      240,
      Math.min(window.innerWidth - 200, window.innerWidth - ev.clientX),
    );
    panel.style.width = `${w}px`;
    fit.fit();
  };
  const up = () => {
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", up);
  };
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
});
