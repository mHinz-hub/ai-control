/// Gemeinsame Panel-Verdrahtung für das angedockte Panel (terminal.ts) und
/// das abgelöste Fenster (panel.ts): Entwurfs-Ansicht, Befehls- und
/// Such-Kacheln, Modus-Umschalter, Wikilink-Klick und die drei Update-Events.
/// Die Element-IDs sind in beiden HTML-Dateien identisch; die Draft-Controls
/// trägt das Markup als `.draft-only`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initPanelView, type PanelView } from "./panel-view";
import {
  initCommandsView,
  initPanelMode,
  panelToast,
  type CommandsView,
  type PanelMode,
} from "./commands-view";
import { initSearchView } from "./search-view";
import { initWikiView } from "./wiki-view";
import "./panel-tiles.css";

export interface PanelWiring {
  view: PanelView;
  cmdView: CommandsView;
  mode: { to(m: PanelMode): void; clear(): void; current(): PanelMode | null };
  /// Entwurfstext beim Start (für die Anfangs-Modus-Entscheidung).
  draft: string;
}

export async function wirePanel(
  project: string,
  onIncoming?: () => void,
): Promise<PanelWiring> {
  const titleEl = document.querySelector(".panel-title") as HTMLElement;
  const [defaultLang, draft, cmds, search, wiki, archiveHome] = await Promise.all([
    invoke<string>("spellcheck_lang"),
    invoke<string>("panel_read", { project }),
    invoke<string>("commands_read", { project }),
    invoke<string>("search_read", { project }),
    invoke<string>("wiki_read", { project }),
    invoke<string | null>("panel_archive_dir_cmd", { project }),
  ]);

  // Ohne Archiv (in den Projekt-Einstellungen abgewählt) gibt es nur Befehle
  // und Dokument — Wiki-/Suche-Tabs und die Archiv-Werkzeuge verschwinden.
  if (!archiveHome) {
    for (const sel of [
      '#panel-tabs [data-mode="wiki"]',
      '#panel-tabs [data-mode="search"]',
      "#panel-archive",
      "#panel-wiki-jump",
    ]) {
      const el = document.querySelector<HTMLElement>(sel)!;
      el.hidden = true;
      // Raus aus der draft-only-Menge, sonst blendet der Modus-Umschalter
      // die Archiv-Werkzeuge im Dokument-Modus wieder ein.
      el.classList.remove("draft-only");
    }
  }

  // Jeder Wiki-Sprung (Wikilink, Chip, Dokument-Sprung) geht als ein Invoke an
  // den Kern; das Ergebnis kommt über den Wiki-Puffer und `wiki-update` zurück.
  // Fehler (z. B. Ziel nicht im Archiv) erscheinen als Toast.
  const openWiki = (name: string) =>
    invoke("wiki_open", { project, name }).catch((e) => panelToast(String(e)));

  const view = initPanelView({
    content: document.getElementById("panel-content")!,
    copyBtn: document.getElementById("panel-copy")!,
    modeBtn: document.getElementById("panel-mode")!,
    titleEl,
    editBtn: document.getElementById("panel-title-edit")!,
    editContentBtn: document.getElementById("panel-content-edit")!,
    langSelect: document.getElementById("panel-lang") as HTMLSelectElement,
    defaultLang,
    onCommit: (text) => invoke("panel_set", { project, text }),
    onWikiLink: openWiki,
  });
  const cmdView = initCommandsView(
    document.getElementById("commands-content")!,
    (id) => invoke("commands_delete", { project, id }),
  );
  // Treffer-Klick lädt das Dokument in den Dokument-Tab (dort editier- und
  // archivierbar); der Sprung ins Wiki geht von dort aus.
  const searchView = initSearchView(
    document.getElementById("search-content")!,
    (path) =>
      void invoke("panel_load", { project, path }).catch((e) => panelToast(String(e))),
    (raw) => {
      // `#tag`-Tokens filtern aufs Schlagwort, der Rest ist die Volltext-Query.
      const words = raw.split(/\s+/).filter(Boolean);
      const tag = words.find((w) => w.startsWith("#"))?.slice(1) ?? null;
      const query = words.filter((w) => !w.startsWith("#")).join(" ");
      void invoke("search_run", { project, query, tag }).catch((e) =>
        panelToast(`Suche fehlgeschlagen: ${e}`),
      );
    },
  );
  const wikiView = initWikiView(document.getElementById("wiki-content")!, openWiki);
  const mode = initPanelMode({
    tabsEl: document.getElementById("panel-tabs")!,
    draftEls: [
      document.getElementById("panel-content")!,
      ...document.querySelectorAll<HTMLElement>(".draft-only"),
    ],
    commandsContent: document.getElementById("commands-content")!,
    searchContent: document.getElementById("search-content")!,
    wikiContent: document.getElementById("wiki-content")!,
    titleEl,
    flush: () => void view.flush(),
  });

  view.set(draft);
  cmdView.set(cmds);
  searchView.set(search);
  wikiView.set(wiki);

  // Wiki-Tab bei leerem Puffer (Session-Start): Übersicht direkt laden.
  document
    .querySelector<HTMLElement>('#panel-tabs [data-mode="wiki"]')!
    .addEventListener("click", () => {
      if (wikiView.empty()) void openWiki("tag:");
    });

  // Sprung Dokument → Wiki: löst den angezeigten Titel gegen das Archiv auf.
  document
    .getElementById("panel-wiki-jump")!
    .addEventListener("click", () => void openWiki(titleEl.textContent || ""));

  await listen<string>("panel-update", (e) => {
    // Erst umschalten, dann setzen: to("draft") restauriert den gemerkten
    // Titel — der neue Inhalt (und damit sein Titel) muss danach kommen.
    mode.to("draft");
    view.set(e.payload);
    onIncoming?.();
  });
  await listen<string>("commands-update", (e) => {
    cmdView.set(e.payload);
    mode.to("commands");
    onIncoming?.();
  });
  await listen<string>("search-update", (e) => {
    searchView.set(e.payload);
    mode.to("search");
    onIncoming?.();
  });
  await listen<string>("wiki-update", (e) => {
    wikiView.set(e.payload);
    mode.to("wiki");
    onIncoming?.();
  });

  return { view, cmdView, mode, draft };
}
