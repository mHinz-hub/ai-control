/// Dialog „Tabelle einfügen" — Spalten, Zeilen, Kopfzeile. Gemeinsam für den
/// HTML-Editor (ProseMirror-Tabelle) und den Markdown-Editor (Pipe-Tabelle):
/// Beide fragen dasselbe, also fragt es dieselbe Fläche.

import { t } from "./messages";

export interface TableWahl {
  spalten: number;
  zeilen: number;
  kopf: boolean;
  /// Sichtbare Zellränder.
  rand: boolean;
  /// Textfluss um die Tabelle: als Block, oder links/rechts umflossen.
  fluss: "block" | "links" | "rechts";
}

/// Hängt den Dialog hinter `after` ein. Ein zweiter Aufruf löst den ersten ab.
/// Eingabe schließt mit `onOk`, Escape und Abbrechen verwerfen.
///
/// `erweitert` blendet Rand und Textfluss ein — sie hängen an Klassen der
/// Tabelle und gibt es darum nur dort, wo das Format sie trägt (HTML-Notiz).
export function openTableForm(
  after: HTMLElement,
  onOk: (wahl: TableWahl) => void,
  erweitert = false,
) {
  after.parentElement?.querySelector(".table-form")?.remove();
  const box = document.createElement("form");
  box.className = "table-form";

  const zahl = (label: string, wert: number) => {
    const l = document.createElement("label");
    l.textContent = label;
    const i = document.createElement("input");
    i.type = "number";
    i.min = "1";
    i.max = "20";
    i.value = String(wert);
    l.append(i);
    box.append(l);
    return i;
  };
  const spalten = zahl(t("html.cols"), 3);
  const zeilen = zahl(t("html.rows"), 2);

  const haken = (text: string, an: boolean) => {
    const l = document.createElement("label");
    l.className = "table-form-check";
    const i = document.createElement("input");
    i.type = "checkbox";
    i.checked = an;
    l.append(i, document.createTextNode(text));
    box.append(l);
    return i;
  };
  const kopf = haken(t("html.tableHeader"), true);
  const rand = erweitert ? haken(t("html.tableBorder"), true) : null;

  let fluss: HTMLSelectElement | null = null;
  if (erweitert) {
    const l = document.createElement("label");
    l.textContent = t("html.tableFlow");
    fluss = document.createElement("select");
    for (const [wert, label] of [
      ["block", t("html.flowBlock")],
      ["links", t("html.flowLeft")],
      ["rechts", t("html.flowRight")],
    ] as const) {
      const o = document.createElement("option");
      o.value = wert;
      o.textContent = label;
      fluss.append(o);
    }
    l.append(fluss);
    box.append(l);
  }

  const ok = document.createElement("button");
  ok.type = "submit";
  ok.className = "wiki-form-submit";
  ok.textContent = t("html.insert");
  const ab = document.createElement("button");
  ab.type = "button";
  ab.className = "wiki-form-cancel";
  ab.textContent = t("html.cancel");
  const knoepfe = document.createElement("div");
  knoepfe.className = "table-form-actions";
  knoepfe.append(ok, ab);
  box.append(knoepfe);

  const zu = () => box.remove();
  ab.addEventListener("click", zu);
  box.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      zu();
    }
  });
  box.addEventListener("submit", (e) => {
    e.preventDefault();
    zu();
    onOk({
      spalten: Number(spalten.value),
      zeilen: Number(zeilen.value),
      kopf: kopf.checked,
      rand: rand ? rand.checked : true,
      fluss: (fluss?.value ?? "block") as TableWahl["fluss"],
    });
  });
  after.after(box);
  spalten.focus();
  spalten.select();
}
