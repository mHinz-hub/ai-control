import "./panel-window.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { wirePanel } from "./panel-wiring";
import { flash, panelToast } from "./tiles";
import { initArchiveForm } from "./archive-form";
import { applyTheme, THEMES } from "./themes";
import { applyI18n, t } from "./messages";
import { initZoom } from "./zoom";

// Abgelöstes Panel-Fenster: liest den aktuellen Entwurf einmal ein und folgt
// danach denselben Update-Events wie das angedockte Panel; startet in
// „Befehle", wenn es (noch) keinen Entwurf gibt.
applyI18n();
initZoom(document.getElementById("zoom-anker")!, "panel");
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
// Seite der Fensterknöpfe folgt dem Desktop (GNOME/Cinnamon, KDE, XFCE). Das
// Flag steuert die eigene Knopfgruppe; macOS zeichnet die Ampel selbst und
// reserviert ihr über data-platform Platz — dort bleibt es ungesetzt.
if (!isMac) {
  void invoke<boolean>("window_buttons_left").then((links) => {
    if (links) document.documentElement.dataset.winbtns = "left";
  });
}
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

// Ein früherer Geometrie-Merker (panel-geometry:<projekt> im localStorage)
// ist ersatzlos raus: Er hat jede Fenstergeometrie ungeprüft konserviert und
// beim nächsten Öffnen wieder eingespielt — bei mehreren Monitoren mit
// verheerenden Folgen. Der Altbestand wird hier ausgeräumt, damit er nie
// wieder greift; die Öffnungsgröße kommt fest aus dem Rust-Fensterbau.
localStorage.removeItem(`panel-geometry:${project}`);

// Farben ans Theme koppeln — derselbe Look wie das angedockte Panel im
// Terminal-Fenster (die CSS-Defaults sind Mocha).
interface Project {
  id: string;
  name: string;
  terminal: { theme: string | null; icon: string | null; title: string | null };
}
const projects = await invoke<Project[]>("list_projects");
const cfg = projects.find((p) => p.id === project);
const picked = THEMES[cfg?.terminal.theme ?? "mocha"];
applyTheme(picked);

// Kopfzeile wie im Terminal-Fenster: Projekt-Icon und Titel links.
document.getElementById("project-name")!.textContent =
  cfg?.terminal.title ?? cfg?.name ?? project;
if (cfg?.terminal.icon) {
  const data = await invoke<string | null>("project_icon", { project });
  if (data) {
    const img = document.getElementById("project-icon") as HTMLImageElement;
    img.src = data;
    img.hidden = false;
  }
}
// Fensterhintergrund des Panel-Fensters ist die Kopf-Fläche, nicht das
// Terminal-Dunkel.
document.documentElement.style.background = picked.header;
document.body.style.background = picked.header;

// Welche Fläche dieses Fenster zeigt, sagt der Öffner: das Archiv (Wiki und
// Suche) oder die Sitzung (Entwurf, Befehle, Aufgaben).
const flaeche = new URLSearchParams(location.search).get("flaeche") ?? "archiv";
const { view, mode, draft } = await wirePanel(project, undefined, true, flaeche);
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

// Andocken: die Sitzungsfläche geht in das Dock des Terminal-Fensters zurück
// — mit dem Tab, der hier zuletzt offen war. Das Archiv hat kein Dock, dort
// bleibt der Knopf versteckt.
const dockBtn = document.getElementById("panel-dock")!;
dockBtn.hidden = flaeche !== "sitzung";
dockBtn.addEventListener("click", async () => {
  await emit("panel-attached", mode.current() ?? "commands");
  await win.close();
});

// „Schließen": Fenster zu, ohne wieder anzudocken (Panel bleibt aus, bis ein
// neuer Entwurf kommt). Stand der Entwurf vorn, ist er damit verworfen — der
// leere Puffer nimmt seinen Reiter überall mit.
document.getElementById("panel-close")!.addEventListener("click", async () => {
  if (mode.current() === "draft") await invoke("panel_clear", { project });
  await win.close();
});
