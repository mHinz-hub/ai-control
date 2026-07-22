import "./panel-window.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import { emit } from "@tauri-apps/api/event";
import { wirePanel } from "./panel-wiring";
import { flash, panelToast } from "./commands-view";
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
  name: string;
  terminal: { theme: string | null };
}
const projects = await invoke<Project[]>("list_projects");
const picked = THEMES[projects.find((p) => p.name === project)?.terminal.theme ?? "mocha"];
applyTheme(picked);
// Fensterhintergrund des Panel-Fensters ist die Kopf-Fläche, nicht das
// Terminal-Dunkel.
document.documentElement.style.background = picked.header;
document.body.style.background = picked.header;

const { view, cmdView, mode, draft } = await wirePanel(project);
if (!draft.trim() && !cmdView.empty()) mode.to("commands");

// Archivieren: wie im angedockten Panel — Formular aufklappen, Abschicken
// wählt notfalls erst das Archiv-Home per Dialog.
const archiveBtn = document.getElementById("panel-archive")!;
const archiveForm = initArchiveForm(archiveBtn, async (meta) => {
  try {
    const configured = await invoke<string | null>("panel_archive_dir_cmd", {
      project,
    });
    let dir: string | undefined;
    if (!configured) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const chosen = await open({ directory: true, title: t("panel.chooseArchiveDir") });
      if (!chosen) return;
      dir = chosen as string;
    }
    await view.flush(); // offene Bearbeitung erst speichern — archiviert, was zu sehen ist
    const path = await invoke<string>("panel_archive_cmd", {
      project,
      dir,
      folder: meta.folder ?? null,
      description: meta.description ?? null,
      tags: meta.tags,
    });
    flash(archiveBtn, "copied", 1400);
    invoke("reveal_path_cmd", { path });
  } catch (e) {
    flash(archiveBtn, "error", 1400);
    panelToast(`Archivieren fehlgeschlagen: ${e}`);
  }
});
archiveBtn.addEventListener("click", () => archiveForm.toggle());

// „Andocken": angedocktes Panel wieder einblenden, dann dieses Fenster schließen.
document.getElementById("panel-dock")!.addEventListener("click", async () => {
  await emit("panel-attached");
  win.close();
});

// „Schließen": Fenster zu, ohne wieder anzudocken (Panel bleibt aus, bis ein
// neuer Entwurf kommt).
document.getElementById("panel-close")!.addEventListener("click", () => win.close());
