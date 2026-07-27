/// Gemeinsamer Kachel-Baustein der Panel-Ansichten (Befehle, Suchtreffer,
/// künftig ToDo): deklarativer Tile-Renderer plus die geteilten Aktions-
/// Knöpfe und Feedback-Helfer. DOM entsteht per createElement — Inhalte sind
/// Fremdtext und gehen nie durch innerHTML (die SVG-Icons der Aktionen sind
/// eigene Literale).

import { writeText } from "@tauri-apps/plugin-clipboard-manager";

/// Entfernt Bidi- und Zero-Width-Steuerzeichen (U+200B–200F, U+202A–202E,
/// U+2060–2064, U+2066–2069, U+FEFF) aus Fremdtexten — sonst sieht der
/// Nutzer einen anderen Text, als die Zwischenablage enthält.
export function stripInvisibles(s: string): string {
  return s.replace(/[\u200B-\u200F\u202A-\u202E\u2060-\u2064\u2066-\u2069\uFEFF]/g, "");
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

/// Kopier-Knopf: legt `text()` in die Zwischenablage, quittiert mit Flash.
export function copyAction(title: string, text: () => string): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.className = "panel-btn cmd-copy";
  btn.title = title;
  btn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 16 16"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5" /><path d="M10.5 3.2V3A1.5 1.5 0 0 0 9 1.5H3A1.5 1.5 0 0 0 1.5 3v6A1.5 1.5 0 0 0 3 10.5h.2" /></svg>';
  btn.addEventListener("click", async () => {
    await writeText(text());
    flash(btn, "copied");
  });
  return btn;
}

export function editAction(title: string, onClick: () => void): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.className = "panel-btn cmd-edit";
  btn.title = title;
  btn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 16 16"><path d="M11.1 2.4a1.4 1.4 0 0 1 2 2l-8 8-2.9.9.9-2.9z" /><path d="M9.9 3.6l2 2" /></svg>';
  btn.addEventListener("click", onClick);
  return btn;
}

export function deleteAction(title: string, onClick: () => void): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.className = "panel-btn cmd-del";
  btn.title = title;
  btn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 16 16"><path d="M2.5 4.5h11" /><path d="M5.5 4.5V3a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1.5" /><path d="M4 4.5l.7 8.6a1 1 0 0 0 1 .9h4.6a1 1 0 0 0 1-.9l.7-8.6" /></svg>';
  btn.addEventListener("click", onClick);
  return btn;
}

export interface TileSpec {
  cls: string;
  /// Inhalts-Zeilen in Reihenfolge; fertige Elemente (z. B. Snippet mit
  /// <mark>-Hervorhebung) gehen als Element durch.
  parts: (HTMLElement | { cls: string; text: string })[];
  /// Umschließt die parts als eigener Container (Layout neben den actions),
  /// z. B. "cmd-body".
  bodyCls?: string;
  /// Aktions-Knöpfe hinter dem Body (copyAction/deleteAction).
  actions?: HTMLElement[];
  onClick?: () => void;
}

export function renderTile(spec: TileSpec): HTMLElement {
  const tile = document.createElement("div");
  tile.className = spec.cls;
  let host = tile;
  if (spec.bodyCls) {
    host = document.createElement("div");
    host.className = spec.bodyCls;
    tile.append(host);
  }
  for (const part of spec.parts) {
    if (part instanceof HTMLElement) {
      host.append(part);
      continue;
    }
    const div = document.createElement("div");
    div.className = part.cls;
    div.textContent = part.text;
    host.append(div);
  }
  if (spec.actions) tile.append(...spec.actions);
  if (spec.onClick) tile.addEventListener("click", spec.onClick);
  return tile;
}
