/// Gemeinsame Panel-Verdrahtung für das angedockte Panel (terminal.ts) und
/// das abgelöste Fenster (panel.ts): die Entwurfs-Ansicht als Kern, alle
/// weiteren Tabs aus der Modul-Registry (src/modules) — Tab-Knöpfe und
/// Content-Container entstehen hier aus der Liste der aktiven Module,
/// Erstbefüllung per buffer_read, Updates über `<buffer>-update`-Events.
/// Die Element-IDs des Entwurfs-Markups sind in beiden HTML-Dateien
/// identisch; die Draft-Controls trägt das Markup als `.draft-only`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initPanelView, type PanelView } from "./panel-view";
import { initPanelMode, type ModeTab, type PanelMode } from "./commands-view";
import { panelToast } from "./tiles";
import { PANEL_TABS, type ModuleCtx, type ModuleView } from "./modules";
import { t } from "./messages";
import "./panel-tiles.css";

export interface PanelWiring {
  view: PanelView;
  /// Modul-Ansichten nach Tab-Modus; enthält nur Tabs aktiver Module.
  views: Map<string, ModuleView>;
  mode: { to(m: PanelMode): void; clear(): void; current(): PanelMode | null };
  /// Entwurfstext beim Start (für die Anfangs-Modus-Entscheidung).
  draft: string;
}

export async function wirePanel(
  project: string,
  onIncoming?: () => void,
  standalone = false,
): Promise<PanelWiring> {
  const titleEl = document.querySelector(".panel-title") as HTMLElement;
  const [enabled, defaultLang, draft] = await Promise.all([
    invoke<string[]>("enabled_modules", { project }),
    invoke<string>("spellcheck_lang"),
    invoke<string>("buffer_read", { project, buffer: "panel" }),
  ]);
  // Feste Aufgabenteilung: Session-Tabs (ToDo, Befehle) leben im angedockten
  // Panel, Archiv-Tabs (popupOnly: Archiv, Suche) im eigenen Panel-Fenster.
  // Im Dock erscheinen die Archiv-Tabs nur als Öffner-Knöpfe (terminal.ts
  // fängt den Klick und öffnet das Fenster).
  const tabs = PANEL_TABS.filter((tab) => enabled.includes(tab.module)).filter(
    (tab) => (standalone ? !!tab.popupOnly : true),
  );

  // Ohne Archiv-Modul (abgewählt oder kein Archiv-Home) verschwinden auch
  // die Archiv-Werkzeuge des Entwurfs-Tabs.
  if (!enabled.includes("archive")) {
    for (const sel of ["#panel-archive", "#panel-wiki-jump"]) {
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
    void invoke("wiki_open", { project, name }).catch((e) => panelToast(String(e)));

  const ctx: ModuleCtx = {
    project,
    standalone,
    toast: panelToast,
    openDoc: (path) =>
      void invoke("panel_load", { project, path }).catch((e) => panelToast(String(e))),
    openWiki,
  };

  const view = initPanelView({
    content: document.getElementById("panel-content")!,
    copyBtn: document.getElementById("panel-copy")!,
    copyHtmlBtn: document.getElementById("panel-copy-html")!,
    printBtn: document.getElementById("panel-print")!,
    modeBtn: document.getElementById("panel-mode")!,
    titleEl,
    editBtn: document.getElementById("panel-title-edit")!,
    editContentBtn: document.getElementById("panel-content-edit")!,
    langSelect: document.getElementById("panel-lang") as HTMLSelectElement,
    defaultLang,
    onCommit: (text) => invoke("panel_set", { project, text }),
    onWikiLink: openWiki,
  });

  // Tab-Leiste und Container aus der Registry. Die Container-IDs
  // (`<mode>-content`) sind zugleich die CSS-Anker der Ansichten; die
  // Container reihen sich hinter #panel-content ein.
  const tabsEl = document.getElementById("panel-tabs")!;
  const views = new Map<string, ModuleView>();
  const modeTabs: ModeTab[] = [];
  /// Tabs mit eigener Ansicht in DIESER Fläche (Öffner-Knöpfe zählen nicht).
  const activeTabs: typeof tabs = [];
  let anchor = document.getElementById("panel-content")!;
  for (const [i, tab] of tabs.entries()) {
    const btn = document.createElement("button");
    btn.className = "panel-btn";
    btn.dataset.mode = tab.mode;
    btn.textContent = t(tab.labelKey);
    btn.title = t(tab.titleKey);
    tabsEl.append(btn);
    // Der Trenner steht zwischen zwei Gruppen — hinter dem letzten Tab
    // trennt er nichts und entfällt (im Archiv-Fenster gibt es nur die
    // Archiv-Tabs).
    if (tab.sepAfter && i < tabs.length - 1) {
      const sep = document.createElement("span");
      sep.className = "tab-sep";
      tabsEl.append(sep);
    }
    // Archiv-Tabs im Dock: nur der Öffner-Knopf, keine Ansicht.
    if (tab.popupOnly && !standalone) {
      continue;
    }
    activeTabs.push(tab);
    let content: HTMLElement | null = null;
    if (tab.init) {
      content = document.createElement("div");
      content.id = `${tab.mode}-content`;
      content.hidden = true;
      anchor.after(content);
      anchor = content;
      views.set(tab.mode, tab.init(content, ctx));
    }
    modeTabs.push({
      mode: tab.mode,
      btn,
      content,
      label: t(tab.labelKey),
      onActivate: tab.onActivate
        ? () => tab.onActivate!(views.get(tab.mode)!, ctx)
        : undefined,
    });
  }
  // Im eigenen Fenster gibt es keinen Entwurf: Die Kopfzeile darunter trüge
  // nur den Tab-Namen, der schon in der Tab-Leiste steht.
  if (standalone) {
    document.querySelector<HTMLElement>(".panel-head")!.hidden = true;
  }

  const mode = initPanelMode({
    tabs: modeTabs,
    draftEls: [
      document.getElementById("panel-content")!,
      ...document.querySelectorAll<HTMLElement>(".draft-only"),
    ],
    titleEl,
    flush: () => void view.flush(),
  });

  // Sprung Dokument → Wiki: öffnet den Archiv-Navigator (Dokumentseiten im
  // Wiki gibt es nicht mehr — Dokumente öffnen immer im Dokument-Tab).
  document
    .getElementById("panel-wiki-jump")!
    .addEventListener("click", () => openWiki("tag:"));

  view.set(draft);
  await listen<string>("panel-update", (e) => {
    // Erst umschalten, dann setzen: to("draft") restauriert den gemerkten
    // Titel — der neue Inhalt (und damit sein Titel) muss danach kommen.
    mode.to("draft");
    view.set(e.payload);
    onIncoming?.();
  });
  await Promise.all(
    activeTabs
      .filter((tab) => tab.init)
      .map(async (tab) => {
        const v = views.get(tab.mode)!;
        v.set(await invoke<string>("buffer_read", { project, buffer: tab.buffer }));
        await listen<string>(`${tab.buffer}-update`, (e) => {
          v.set(e.payload);
          mode.to(tab.mode);
          onIncoming?.();
        });
      }),
  );

  return { view, views, mode, draft };
}
