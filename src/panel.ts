import "./panel-window.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import { wirePanel } from "./panel-wiring";
import { flash, panelToast } from "./tiles";
import { initArchiveForm } from "./archive-form";
import { applyTheme, THEMES } from "./themes";
import { applyI18n, t } from "./messages";

// Abgelöstes Panel-Fenster: liest den aktuellen Entwurf einmal ein und folgt
// danach denselben Update-Events wie das angedockte Panel; startet in
// „Befehle", wenn es (noch) keinen Entwurf gibt.
applyI18n();
const project = new URLSearchParams(location.search).get("project")!;
const win = getCurrentWebviewWindow();

// Debug-Instrumentierung wie im Terminal-Fenster: Fehler auf der Seite
// anzeigen und ins Dev-Log spiegeln — ein toter Start ist sonst unsichtbar.
function showError(msg: string) {
  const el = document.createElement("pre");
  el.style.cssText =
    "color:#f38ba8;padding:16px;font:12px Menlo,monospace;white-space:pre-wrap";
  el.textContent = msg;
  document.body.appendChild(el);
  invoke("term_log", { msg: `panel: ${msg}` });
}
window.addEventListener("error", (e) =>
  showError(`error: ${e.message}\n${e.error?.stack ?? ""}`),
);
window.addEventListener("unhandledrejection", (e) =>
  showError(`rejection: ${e.reason}\n${e.reason?.stack ?? ""}`),
);

// Dekorationsloses Fenster (Linux): Plattform-Flag + Resize-Zonen wie im
// Terminal-Fenster; macOS behält die native Deko.
const isMac = /Mac|Macintosh/.test(navigator.userAgent);
document.documentElement.dataset.platform = isMac ? "mac" : "other";
if (!isMac) {
  document
    .getElementById("win-min")!
    .addEventListener("click", () => win.minimize());
  document
    .getElementById("win-max")!
    .addEventListener("click", () => win.toggleMaximize());
  for (const g of document.querySelectorAll<HTMLElement>(".grip")) {
    g.addEventListener("mousedown", (e) => {
      e.preventDefault();
      win.startResizeDragging(g.dataset.dir as never);
    });
  }
}

// Doppelklick auf die Kopfleiste maximiert (Fenster-Konvention).
document.querySelector(".panel-topbar")!.addEventListener("dblclick", (e) => {
  if ((e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) {
    win.toggleMaximize();
  }
});

// Fenstergeometrie merken und beim nächsten Öffnen wiederherstellen; unter
// Wayland vergibt der Compositor die Position, dann greift nur die Größe.
const GEO_KEY = `panel-geometry:${project}`;
const savedGeo = localStorage.getItem(GEO_KEY);
if (savedGeo) {
  const g = JSON.parse(savedGeo);
  await win.setSize(new PhysicalSize(g.w, g.h));
  await win.setPosition(new PhysicalPosition(g.x, g.y));
}
let geoTimer: number | undefined;
const saveGeo = () => {
  clearTimeout(geoTimer);
  geoTimer = window.setTimeout(async () => {
    const pos = await win.outerPosition();
    const size = await win.innerSize();
    localStorage.setItem(
      GEO_KEY,
      JSON.stringify({ x: pos.x, y: pos.y, w: size.width, h: size.height }),
    );
  }, 300);
};
await win.onMoved(saveGeo);
await win.onResized(saveGeo);

// Farben ans Theme koppeln — derselbe Look wie das angedockte Panel im
// Terminal-Fenster (die CSS-Defaults sind Mocha).
interface Project {
  id: string;
  name: string;
  terminal: { theme: string | null };
}
const projects = await invoke<Project[]>("list_projects");
const picked =
  THEMES[projects.find((p) => p.id === project)?.terminal.theme ?? "mocha"];
applyTheme(picked);
// Fensterhintergrund des Panel-Fensters ist die Kopf-Fläche, nicht das
// Terminal-Dunkel.
document.documentElement.style.background = picked.header;
document.body.style.background = picked.header;

const { view, mode, draft } = await wirePanel(project, undefined, true);
// Das Panel-Fenster ist die Archiv-Fläche: gewünschter Tab aus der URL,
// sonst das Archiv; ein hereingereichter Entwurf (Bearbeiten) gewinnt.
const initialMode = new URLSearchParams(location.search).get("mode");
if (initialMode) {
  mode.to(initialMode);
} else if (!draft.trim()) {
  mode.to("wiki");
}

// Archivieren: wie im angedockten Panel — Formular aufklappen, Abschicken
// wählt notfalls erst das Archiv-Home per Dialog.
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

// Öffner-Klick im Terminal-Header bei stehendem Fenster: auf den
// gewünschten Tab schalten (das Fokussieren macht der Kern).
await win.listen<string>("panel-mode", (e) => mode.to(e.payload));

// Andocken gibt es nicht mehr — die Flächen sind fest verteilt (Session-Tabs
// im Dock, Archiv hier); der Knopf bleibt versteckt.
document.getElementById("panel-dock")!.hidden = true;

// „Schließen": Fenster zu, ohne wieder anzudocken (Panel bleibt aus, bis ein
// neuer Entwurf kommt).
document
  .getElementById("panel-close")!
  .addEventListener("click", () => win.close());
