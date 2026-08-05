import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../messages";
import { initSearchView, type SearchView } from "../search-view";
import { initWikiView } from "../wiki-view";
import type { PanelTab } from "./index";

/// Suchtreffer-Sprung ins Archiv: der Treffer merkt den relpath vor, lädt die
/// Übersicht neu (wiki-update wechselt den Tab), und die Archiv-Ansicht holt
/// die Vormerkung beim nächsten set() ab.
let pendingSelect: string | null = null;
/// Die Wörter des Treffers — die Archiv-Ansicht markiert sie im geöffneten
/// Dokument und springt zur ersten Fundstelle.
let pendingMarks: string[] = [];
/// Kapitel des Treffers, wenn er aus einem Buch stammt.
let pendingPart = "";
/// Welche Fundstelle gemeint war — die Anzeige springt zu ihr.
let pendingSpot = 0;

export const wikiTab: PanelTab = {
  mode: "wiki",
  module: "archive",
  buffer: "wiki",
  popupOnly: true,
  labelKey: "panel.tabWiki",
  titleKey: "panel.tabWikiTitle",
  // Dokument-/Ordner-Operationen laufen als Commands; der neue Stand kommt
  // über den Wiki-Puffer (wiki-update) zurück, Fehler als Toast. Der
  // Notiz-Inhalt kommt pro Auswahl über archive_read; Bearbeiten lädt wie
  // bisher in den Dokument-Tab.
  init: (container, ctx) => {
    const run = (cmd: string, args: Record<string, string>) =>
      void invoke(cmd, { project: ctx.project, ...args }).catch((e) =>
        ctx.toast(String(e)),
      );
    // Einmal beim Start: gibt es die draw.io-Desktop-App? Entscheidet über
    // den Editier-Knopf an Diagramm-Dateien.
    let drawio = false;
    void invoke<boolean>("drawio_available").then((da) => (drawio = da));
    // Anlegen liefert die ID der neuen Notiz zurück.
    const create = (cmd: string, args: Record<string, string>) =>
      invoke<string>(cmd, { project: ctx.project, ...args }).catch((e) => {
        ctx.toast(String(e));
        throw e;
      });
    return initWikiView(container, {
      autoStart: ctx.standalone,
      readDoc: (id) => invoke("archive_read", { project: ctx.project, id }),
      readFile: (id) => invoke("archive_read_file", { project: ctx.project, id }),
      drawioAvailable: () => drawio,
      openDrawio: (id) => run("drawio_open", { id }),
      writeDoc: (id, text) => invoke("archive_write", { project: ctx.project, id, text }),
      writeFile: (id, text) => invoke("archive_write_text", { project: ctx.project, id, text }),
      openEpub: (id) => invoke("epub_open", { project: ctx.project, id }),
      readImage: (id) => invoke("archive_image", { project: ctx.project, id }),
      openImage: (id) => run("open_image_window", { id }),
      setTitle: (id, title) => run("archive_set_title", { id, title }),
      openWiki: ctx.openWiki,
      takePending: () => {
        const p = pendingSelect;
        pendingSelect = null;
        return p;
      },
      takeMarks: () => {
        const m = pendingMarks;
        pendingMarks = [];
        return m;
      },
      takePart: () => {
        const p = pendingPart;
        pendingPart = "";
        return p;
      },
      takeSpot: () => {
        const n = pendingSpot;
        pendingSpot = 0;
        return n;
      },
      actions: {
        remove: (id) => run("archive_delete", { id }),
        createFolder: (parent, name) => run("archive_create_folder", { parent, name }),
        createDoc: (parent, name) => create("archive_create_doc", { parent, name }),
        createHtml: (parent, name) => create("archive_create_html", { parent, name }),
        createText: (parent, name, art) => create("archive_create_text", { parent, name, art }),
        // Ohne Desktop-App gibt es keine Zeichenfläche — das sagt der Toast,
        // statt den Nutzer nach dem Anlegen ins Leere laufen zu lassen.
        createDrawio: (near, name) =>
          create("archive_create_drawio", { near, name }).then((rel) => {
            if (!drawio) ctx.toast(t("wiki.drawioMissing"));
            return rel;
          }),
        reveal: (path) => void invoke("reveal_path_cmd", { path }),
        removeFolder: (path) => run("archive_delete_folder", { path }),
        // Datei-Dialog, dann Kopie ins Archiv; der Watcher meldet den neuen
        // Stand über wiki-update.
        importFiles: (parent) =>
          void open({ multiple: true }).then((sel) => {
            const paths = typeof sel === "string" ? [sel] : sel;
            if (!paths || !paths.length) return;
            void invoke("archive_import", {
              project: ctx.project,
              parent,
              paths,
            }).catch((e) => ctx.toast(String(e)));
          }),
      },
    });
  },
  // Beim Aktivieren die Übersicht frisch laden: das Anlegen eines Dokuments
  // öffnet den Dokument-Tab, ohne den Wiki-Puffer anzufassen — und auch
  // direkt archivierte oder von Hand abgelegte Dateien erscheinen so.
  // Die Fenstergröße setzt allein open_panel_window beim Öffnen des Popups.
  onActivate: (_view, ctx) => ctx.openWiki("tag:"),
};

export const searchTab: PanelTab = {
  mode: "search",
  module: "archive",
  buffer: "search",
  popupOnly: true,
  labelKey: "panel.tabSearch",
  titleKey: "panel.tabSearchTitle",
  // Treffer-Klick öffnet die Notiz im Archiv-Tab: relpath vormerken, die
  // Übersicht frisch laden — das wiki-update wechselt den Tab und die
  // Ansicht wählt die vorgemerkte Notiz aus.
  init: (container, ctx) =>
    initSearchView(
      container,
      (_path, _relpath, id, marken, teil, nr) => {
        pendingSelect = id;
        pendingMarks = marken;
        pendingPart = teil;
        pendingSpot = nr ?? 0;
        ctx.openWiki("tag:");
      },
      (raw) => {
        // `#tag`-Tokens filtern aufs Schlagwort, der Rest ist die Volltext-Query.
        const words = raw.split(/\s+/).filter(Boolean);
        const tag = words.find((w) => w.startsWith("#"))?.slice(1) ?? null;
        const query = words.filter((w) => !w.startsWith("#")).join(" ");
        void invoke("search_run", { project: ctx.project, query, tag }).catch((e) =>
          ctx.toast(`Suche fehlgeschlagen: ${e}`),
        );
      },
      (id, teil, query) =>
        invoke("search_spots", { project: ctx.project, id, teil, query }),
    ),
  // Der Tab wird einmal gebaut und danach nur ein- und ausgeblendet; der
  // Fokus beim Bauen träfe also nur das erste Öffnen.
  onActivate: (view) => (view as SearchView).focus(),
};
