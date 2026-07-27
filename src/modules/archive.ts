import { invoke } from "@tauri-apps/api/core";
import { initSearchView } from "../search-view";
import { initWikiView } from "../wiki-view";
import type { PanelTab } from "./index";

/// Suchtreffer-Sprung ins Archiv: der Treffer merkt den relpath vor, lädt die
/// Übersicht neu (wiki-update wechselt den Tab), und die Archiv-Ansicht holt
/// die Vormerkung beim nächsten set() ab.
let pendingSelect: string | null = null;

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
    // Anlegen liefert die ID der neuen Notiz zurück.
    const create = (cmd: string, args: Record<string, string>) =>
      invoke<string>(cmd, { project: ctx.project, ...args }).catch((e) => {
        ctx.toast(String(e));
        throw e;
      });
    return initWikiView(container, {
      autoStart: ctx.standalone,
      readDoc: (id) => invoke("archive_read", { project: ctx.project, id }),
      writeDoc: (id, text) => invoke("archive_write", { project: ctx.project, id, text }),
      openEpub: (id) => invoke("epub_open", { project: ctx.project, id }),
      setTitle: (id, title) => run("archive_set_title", { id, title }),
      openWiki: ctx.openWiki,
      takePending: () => {
        const p = pendingSelect;
        pendingSelect = null;
        return p;
      },
      actions: {
        remove: (id) => run("archive_delete", { id }),
        createFolder: (parent, name) => run("archive_create_folder", { parent, name }),
        createDoc: (parent, name) => create("archive_create_doc", { parent, name }),
        createHtml: (parent, name) => create("archive_create_html", { parent, name }),
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
  // Trenner: Archiv-Fenster-Tabs (Archiv, Suche) links, Session-Tabs rechts.
  sepAfter: true,
  // Treffer-Klick öffnet die Notiz im Archiv-Tab: relpath vormerken, die
  // Übersicht frisch laden — das wiki-update wechselt den Tab und die
  // Ansicht wählt die vorgemerkte Notiz aus.
  init: (container, ctx) =>
    initSearchView(
      container,
      (_path, _relpath, id) => {
        pendingSelect = id;
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
    ),
};
