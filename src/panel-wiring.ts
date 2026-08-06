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
  /// Entwurfs-Ansicht — nur auf der Sitzungsfläche; das Archiv-Fenster kennt
  /// keinen Entwurf.
  view: PanelView | null;
  /// Modul-Ansichten nach Tab-Modus; enthält nur Tabs aktiver Module.
  views: Map<string, ModuleView>;
  mode: {
    to(m: PanelMode): void;
    clear(): void;
    current(): PanelMode | null;
    /// Auf den Standard-Reiter schalten: ToDo, sonst Befehle.
    standard(): void;
  };
}

export async function wirePanel(
  project: string,
  onIncoming?: () => void,
  standalone = false,
  /// Welche Fläche ein abgelöstes Fenster zeigt: „archiv" oder „sitzung".
  flaeche = "archiv",
): Promise<PanelWiring> {
  const titleEl = document.querySelector(".panel-title") as HTMLElement;
  // Der Entwurf gehört zur Sitzung: angedockt und im abgelösten
  // Sitzungs-Fenster. Das Archiv-Fenster liest ihn nicht einmal — es
  // bearbeitet seine Dokumente selbst, mit seinen eigenen Werkzeugen.
  const sitzung = !standalone || flaeche === "sitzung";
  const [enabled, defaultLang, draft] = await Promise.all([
    invoke<string[]>("enabled_modules", { project }),
    invoke<string>("spellcheck_lang"),
    sitzung ? invoke<string>("buffer_read", { project, buffer: "panel" }) : "",
  ]);
  // Zwei Flächen: das Archiv (Archiv, Suche) und die Sitzung (Entwurf, Befehle,
  // Aufgaben). Angedockt zeigt das Panel die Sitzung und die Archiv-Tabs als
  // Öffner-Knöpfe (terminal.ts fängt den Klick). Abgelöst zeigt ein Fenster
  // genau eine der beiden Flächen — beide dürfen nebeneinander stehen.
  const tabs = PANEL_TABS.filter((tab) => enabled.includes(tab.module)).filter(
    (tab) => (!standalone ? true : flaeche === "archiv" ? !!tab.popupOnly : !tab.popupOnly),
  );

  // Ohne Archiv-Modul (abgewählt oder kein Archiv-Home) verschwinden auch
  // die Archiv-Werkzeuge des Entwurfs-Tabs.
  if (!enabled.includes("archive")) {
    for (const sel of ["#panel-archive", "#panel-archive-jump"]) {
      const el = document.querySelector<HTMLElement>(sel)!;
      el.hidden = true;
      // Raus aus der draft-only-Menge, sonst blendet der Modus-Umschalter
      // die Archiv-Werkzeuge im Dokument-Modus wieder ein.
      el.classList.remove("draft-only");
    }
  }

  // Jeder Archiv-Sprung (Wikilink, Chip, Dokument-Sprung) geht als ein Invoke an
  // den Kern; das Ergebnis kommt über den Archiv-Puffer und `archive-update` zurück.
  // Fehler (z. B. Ziel nicht im Archiv) erscheinen als Toast.
  const openArchive = (name: string) =>
    void invoke("archive_open", { project, name }).catch((e) => panelToast(String(e)));

  const ctx: ModuleCtx = {
    project,
    standalone,
    toast: panelToast,
    openArchive,
  };

  // Entwurfs-Ansicht samt Werkzeugleiste gibt es nur auf der Sitzungsfläche.
  // Im Archiv-Fenster bleibt ihr Container leer und aus dem Weg — er ist dort
  // nur noch der Anker, hinter dem sich die Modul-Container einreihen.
  const draftContent = document.getElementById("panel-content")!;
  const view = sitzung
    ? initPanelView({
        content: draftContent,
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
        onWikiLink: openArchive,
      })
    : null;
  if (!view) draftContent.hidden = true;

  // Tab-Leiste und Container aus der Registry. Die Container-IDs
  // (`<mode>-content`) sind zugleich die CSS-Anker der Ansichten; die
  // Container reihen sich hinter #panel-content ein.
  const tabsEl = document.getElementById("panel-tabs")!;
  const views = new Map<string, ModuleView>();
  const modeTabs: ModeTab[] = [];
  /// Tabs mit eigener Ansicht in DIESER Fläche (Öffner-Knöpfe zählen nicht).
  const activeTabs: typeof tabs = [];
  let anchor = document.getElementById("panel-content")!;

  // Der Entwurf ist auf der Sitzungsfläche ein Reiter wie ToDo und Befehle —
  // sonst führte der Weg zu den ToDos vom Entwurf weg, ohne zurück. Seine
  // Ansicht hängt an den draftEls, einen Container braucht er nicht. Er zeigt
  // sich, sobald es einen Entwurf gibt; ohne Entwurf bleibt sein Platz stehen
  // (`visibility` statt `hidden`), damit die Leiste ihre Breite behält.
  let entwurfBtn: HTMLButtonElement | null = null;
  /// Reiter zeigen oder seinen Platz leer stehen lassen.
  const entwurfZeigen = (da: boolean) => {
    if (entwurfBtn) entwurfBtn.style.visibility = da ? "" : "hidden";
  };
  if (sitzung) {
    entwurfBtn = document.createElement("button");
    entwurfBtn.className = "panel-btn";
    entwurfBtn.dataset.mode = "draft";
    entwurfBtn.textContent = t("panel.tabEntwurf");
    entwurfZeigen(!!draft.trim());
    tabsEl.append(entwurfBtn);
    modeTabs.push({
      mode: "draft",
      btn: entwurfBtn,
      content: null,
      label: t("panel.tabEntwurf"),
      titel: t("panel.tabEntwurfTitle"),
      kurz: t("panel.tabEntwurfKurz"),
    });
  }
  for (const [i, tab] of tabs.entries()) {
    const btn = document.createElement("button");
    btn.className = "panel-btn";
    btn.dataset.mode = tab.mode;
    btn.textContent = t(tab.labelKey);
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
      titel: t(tab.titleKey),
      kurz: tab.kurzKey ? t(tab.kurzKey) : undefined,
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

  const kern = initPanelMode({
    tabs: modeTabs,
    draftEls: view
      ? [draftContent, ...document.querySelectorAll<HTMLElement>(".draft-only")]
      : [],
    titleEl,
    flush: () => void view?.flush(),
  });
  // Ein Reiter ist immer gewählt: ohne Entwurf die Aufgaben, ohne die die
  // Befehle. Nur wenn es weder das eine noch das andere Modul gibt, bleibt
  // die Fläche ohne Auswahl.
  const mode = {
    ...kern,
    standard() {
      const m = ["todo", "commands"].find((tab) => views.has(tab));
      if (m) kern.to(m);
      else kern.clear();
    },
  };

  // Der Entwurf und das Archiv sind zwei Fenster: Der Sprung aus der
  // Entwurfs-Leiste holt erst das Archiv-Fenster nach vorn (oder baut es) und
  // schickt es dann in den Namensraum der Schlagwörter.
  if (view) {
    document.getElementById("panel-archive-jump")!.addEventListener("click", async () => {
      await invoke("open_panel_window", { project, mode: "archive" });
      openArchive("tag:");
    });

    view.set(draft);
    await listen<string>("panel-update", (e) => {
      // Erst umschalten, dann setzen: to("draft") restauriert den gemerkten
      // Titel — der neue Inhalt (und damit sein Titel) muss danach kommen.
      // Der Entwurf gewinnt: wer einen Text ins Panel schreibt, will ihn
      // sehen, auch wenn zuletzt ToDos oder Befehle offen standen. Der Reiter
      // folgt ihm: Text da, Reiter da — verworfener Entwurf (leerer Puffer)
      // nimmt ihn wieder mit, und die Ansicht geht zu den Aufgaben, damit die
      // Fläche nicht leer stehen bleibt.
      const leer = !e.payload.trim();
      entwurfZeigen(!leer);
      if (leer) {
        view.set(e.payload);
        if (mode.current() === "draft") mode.standard();
        return;
      }
      mode.to("draft");
      view.set(e.payload);
      onIncoming?.();
    });
  }
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

  return { view, views, mode };
}
