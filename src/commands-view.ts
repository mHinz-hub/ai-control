/// Kachel-Ansicht der Command-History: rendert die JSONL-Datei
/// (write_commands im MCP-Server) als kopierbare Kacheln, Neuestes oben,
/// mit Zeitmarken und Session-Trennern. Löschen einer Kachel geht als
/// onDelete(id) an den Aufrufer; der neue Stand kommt über den
/// Watcher zurück. DOM wird per createElement gebaut — Befehle sind
/// Fremdtext und gehen nie durch innerHTML.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { storedLocale, t } from "./messages";
import { copyAction, deleteAction, flash, renderTile, stripInvisibles } from "./tiles";

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
          // Leerzeile zwischen den Befehlen: mehrzeilige Kommandos (Heredoc,
          // fortgesetzte Zeilen) sind sonst nicht vom nächsten zu trennen.
          await writeText(cmds.map((c) => stripInvisibles(c.cmd)).join("\n\n"));
          flash(all, "copied");
        });
        head.append(all);
      }
      block.append(head);

      cmds.forEach((entry) => {
        // Unsichtbare Steuerzeichen (Bidi, Zero-Width) aus Anzeige UND
        // Kopie halten (stripInvisibles); der Lösch-Abgleich läuft weiter
        // über den Original-String.
        const visible = stripInvisibles(entry.cmd);
        block.append(
          renderTile({
            cls: "cmd-tile",
            bodyCls: "cmd-body",
            parts: [
              { cls: "cmd-text", text: visible },
              ...(entry.note ? [{ cls: "cmd-note", text: entry.note }] : []),
            ],
            actions: [
              copyAction(t("commands.copyOne"), () => visible),
              deleteAction(t("commands.removeOne"), () => onDelete(entry.id ?? "")),
            ],
          }),
        );
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

export type PanelMode = string;

/// Ein Tab im Modus-Umschalter; kommt aus der Modul-Registry (panel-wiring).
export interface ModeTab {
  mode: PanelMode;
  btn: HTMLElement;
  /// Content-Container des Tabs; null für den Entwurfs-Tab, dessen
  /// Sichtbarkeit über draftEls läuft.
  content: HTMLElement | null;
  /// Titelzeilen-Text, wenn der Tab aktiv ist (der Entwurfs-Tab behält
  /// seinen Dokument-Titel).
  label: string;
  onActivate?: () => void;
}

/// Modus-Umschalter über die Tabs der Registry: blendet Entwurfs-Inhalt samt
/// zugehöriger Kopf-Controls gegen die jeweilige Ansicht aus. Ein Wechsel bei
/// offener Inhalts-Bearbeitung speichert den Entwurf (flush) statt zu
/// blockieren.
export function initPanelMode(opts: {
  tabs: ModeTab[];
  draftEls: HTMLElement[];
  titleEl: HTMLElement;
  flush: () => void;
}) {
  // `null` = kein Tab aktiv (Panel zugeklappt).
  let mode: PanelMode | null = "draft";
  let draftTitle = "";

  function apply() {
    for (const tab of opts.tabs) {
      if (tab.content) tab.content.hidden = mode !== tab.mode;
      tab.btn.classList.toggle("active", tab.mode === mode);
    }
    for (const el of opts.draftEls) el.hidden = mode !== "draft";
    const active = opts.tabs.find((tab) => tab.mode === mode);
    if (mode && mode !== "draft" && active) opts.titleEl.textContent = active.label;
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
    for (const tab of opts.tabs) tab.btn.classList.remove("active");
  }

  for (const tab of opts.tabs) {
    tab.btn.addEventListener("click", () => {
      to(tab.mode);
      tab.onActivate?.();
    });
  }
  apply();
  return { to, clear, current: () => mode };
}
