/// Kachel-Ansicht der Command-History: rendert die JSONL-Datei
/// (write_commands im MCP-Server) als kopierbare Kacheln, Neuestes oben,
/// mit Zeitmarken und Session-Trennern. Löschen einer Kachel geht als
/// onDelete(id) an den Aufrufer; der neue Stand kommt über den
/// Watcher zurück. DOM wird per createElement gebaut — Befehle sind
/// Fremdtext und gehen nie durch innerHTML.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { storedLocale, t } from "./messages";


/// Entfernt Bidi- und Zero-Width-Steuerzeichen (U+200B–200F, U+202A–202E,
/// U+2060–2064, U+2066–2069, U+FEFF) aus Befehlstexten.
function stripInvisibles(s: string): string {
  return s.replace(/[\u200B-\u200F\u202A-\u202E\u2060-\u2064\u2066-\u2069\uFEFF]/g, "");
}

interface CommandEntry {
  cmd: string;
  note?: string;
  /// Stabile ID aus write_commands; Grundlage fürs Löschen.
  id?: string;
}

interface Record {
  ts: number;
  session?: boolean;
  commands?: CommandEntry[];
}

export interface CommandsView {
  set(text: string): void;
  empty(): boolean;
}

function fmtTime(ts: number): string {
  const d = new Date(ts * 1000);
  const loc = storedLocale();
  const hm = d.toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit" });
  return d.toDateString() === new Date().toDateString()
    ? hm
    : `${d.toLocaleDateString(loc)} ${hm}`;
}

/// Kurzes visuelles Feedback (copied/error) — der eine Flash-Helper fürs
/// ganze Panel.
export function flash(el: HTMLElement, cls: string, ms = 1200) {
  el.classList.add(cls);
  setTimeout(() => el.classList.remove(cls), ms);
}

/// Sichtbare Fehlermeldung im Panel: kurz eingeblendete Zeile oben rechts.
export function panelToast(msg: string) {
  const t = document.createElement("div");
  t.className = "panel-toast";
  t.textContent = msg;
  document.body.append(t);
  setTimeout(() => t.remove(), 5000);
}

function copyBtn(text: () => string): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.className = "panel-btn cmd-copy";
  btn.title = t("commands.copyOne");
  btn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 16 16"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5" /><path d="M10.5 3.2V3A1.5 1.5 0 0 0 9 1.5H3A1.5 1.5 0 0 0 1.5 3v6A1.5 1.5 0 0 0 3 10.5h.2" /></svg>';
  btn.addEventListener("click", async () => {
    await writeText(text());
    flash(btn, "copied");
  });
  return btn;
}

function deleteBtn(onClick: () => void): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.className = "panel-btn cmd-del";
  btn.title = t("commands.removeOne");
  btn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 16 16"><path d="M2.5 4.5h11" /><path d="M5.5 4.5V3a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1.5" /><path d="M4 4.5l.7 8.6a1 1 0 0 0 1 .9h4.6a1 1 0 0 0 1-.9l.7-8.6" /></svg>';
  btn.addEventListener("click", onClick);
  return btn;
}

export function initCommandsView(
  container: HTMLElement,
  onDelete: (id: string) => void,
): CommandsView {
  let count = 0;

  function render(records: Record[]) {
    container.textContent = "";
    // Neuestes oben: rückwärts durch die Records; ein Session-Marker trennt
    // die davor liegenden (älteren) Einträge ab und steht daher unter ihnen.
    // Der Record-Index entspricht der nicht-leeren JSONL-Zeile — er
    // adressiert den Eintrag beim Löschen.
    for (let i = records.length - 1; i >= 0; i--) {
      const rec = records[i];
      if (rec.session) {
        const sep = document.createElement("div");
        sep.className = "cmd-sep";
        sep.textContent = `${t("commands.session")} · ${fmtTime(rec.ts)}`;
        container.append(sep);
        continue;
      }
      const cmds = rec.commands ?? [];
      if (!cmds.length) continue;
      const block = document.createElement("div");
      block.className = "cmd-rec";

      const head = document.createElement("div");
      head.className = "cmd-rec-head";
      const time = document.createElement("span");
      time.textContent = fmtTime(rec.ts);
      head.append(time);
      if (cmds.length > 1) {
        const all = document.createElement("button");
        all.className = "cmd-all";
        all.textContent = t("commands.copyAll");
        all.addEventListener("click", async () => {
          await writeText(cmds.map((c) => stripInvisibles(c.cmd)).join("\n"));
          flash(all, "copied");
        });
        head.append(all);
      }
      block.append(head);

      cmds.forEach((entry) => {
        // Unsichtbare Steuerzeichen (Bidi, Zero-Width) aus Anzeige UND
        // Kopie halten — sonst sieht der Nutzer einen anderen Befehl, als
        // die Zwischenablage enthält. Der Lösch-Abgleich läuft weiter über
        // den Original-String.
        const visible = stripInvisibles(entry.cmd);
        const tile = document.createElement("div");
        tile.className = "cmd-tile";
        const body = document.createElement("div");
        body.className = "cmd-body";
        const cmd = document.createElement("div");
        cmd.className = "cmd-text";
        cmd.textContent = visible;
        body.append(cmd);
        if (entry.note) {
          const note = document.createElement("div");
          note.className = "cmd-note";
          note.textContent = entry.note;
          body.append(note);
        }
        tile.append(body, copyBtn(() => visible), deleteBtn(() => onDelete(entry.id ?? "")));
        block.append(tile);
      });
      container.append(block);
    }
  }

  return {
    set(text: string) {
      const records: Record[] = [];
      for (const line of text.split("\n")) {
        if (!line.trim()) continue;
        records.push(JSON.parse(line));
      }
      count = records.filter((r) => r.commands?.length).length;
      render(records);
    },
    empty: () => count === 0,
  };
}

export type PanelMode = "draft" | "commands" | "search" | "wiki";

const LABEL: { [m in PanelMode]: string } = {
  draft: t("panel.tabDraft"),
  commands: t("panel.tabCommands"),
  search: t("panel.tabSearch"),
  wiki: t("panel.tabWiki"),
};

/// Vier Tabs Entwurf / Befehle / Suche / Wiki: blenden Entwurfs-Inhalt samt
/// zugehöriger Kopf-Controls gegen die jeweilige Ansicht aus. Ein Wechsel bei
/// offener Inhalts-Bearbeitung speichert den Entwurf (flush) statt zu
/// blockieren.
export function initPanelMode(opts: {
  tabsEl: HTMLElement;
  draftEls: HTMLElement[];
  commandsContent: HTMLElement;
  searchContent: HTMLElement;
  wikiContent: HTMLElement;
  titleEl: HTMLElement;
  flush: () => void;
}) {
  // `null` = kein Tab aktiv (Panel zugeklappt).
  let mode: PanelMode | null = "draft";
  let draftTitle = "";
  const tabs = [...opts.tabsEl.querySelectorAll<HTMLElement>("[data-mode]")];

  function apply() {
    opts.commandsContent.hidden = mode !== "commands";
    opts.searchContent.hidden = mode !== "search";
    opts.wikiContent.hidden = mode !== "wiki";
    for (const el of opts.draftEls) el.hidden = mode !== "draft";
    for (const t of tabs) t.classList.toggle("active", t.dataset.mode === mode);
    if (mode && mode !== "draft") opts.titleEl.textContent = LABEL[mode];
  }

  function to(m: PanelMode) {
    if (m === mode) return;
    opts.flush();
    if (mode === "draft") draftTitle = opts.titleEl.textContent || t("panel.tabDraft");
    mode = m;
    apply();
    if (m === "draft") opts.titleEl.textContent = draftTitle;
  }

  /// Auswahl aufheben (Panel zugeklappt) — der nächste to()-Aufruf oder
  /// Tab-Klick wählt wieder aus.
  function clear() {
    if (mode === "draft") draftTitle = opts.titleEl.textContent || t("panel.tabDraft");
    mode = null;
    for (const t of tabs) t.classList.remove("active");
  }

  for (const t of tabs) t.addEventListener("click", () => to(t.dataset.mode as PanelMode));
  apply();
  return { to, clear, current: () => mode };
}
