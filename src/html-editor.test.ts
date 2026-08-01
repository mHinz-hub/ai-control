// HTML-Editor: Tabelle einfügen (Dialog), Rand und Textfluss als Klasse,
// Griffe zum Auswählen ganzer Zeilen und Spalten.
import { describe, expect, it } from "vitest";
import { initHtmlEditor } from "./html-editor";

const werkzeug = (label: string) =>
  [...document.querySelectorAll<HTMLElement>(".html-tool")].find((b) => b.textContent === label)!;

const druecke = (el: HTMLElement) =>
  el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));

/// Editor mit einer Tabelle: Spalten, Zeilen, Kopfzeile, Rand, Fluss.
function mitTabelle(opts: { rand?: boolean; fluss?: string } = {}) {
  document.body.innerHTML = "";
  const ed = initHtmlEditor("<p>Text</p>");
  document.body.append(ed.el);
  druecke(werkzeug("⊞"));
  const form = document.querySelector<HTMLFormElement>(".table-form")!;
  if (opts.rand === false) {
    form.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1].checked = false;
  }
  if (opts.fluss) form.querySelector("select")!.value = opts.fluss;
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  return ed;
}

describe("initHtmlEditor — Tabelle", () => {
  it("Dialog legt Kopfzeile und Größe fest", () => {
    const ed = mitTabelle();
    const html = ed.html();
    expect((html.match(/<th>/g) ?? []).length).toBe(3);
    expect((html.match(/<tr>/g) ?? []).length).toBe(3);
  });

  it("Rand und Fluss stehen als Klasse an der Tabelle", () => {
    const ed = mitTabelle({ rand: false, fluss: "links" });
    expect(ed.html()).toContain('<table class="randlos fluss-links">');
    // Der Fluss-Knopf ändert sie ohne Neubau.
    druecke(werkzeug("◨"));
    expect(ed.html()).toContain('<table class="randlos fluss-rechts">');
    druecke(werkzeug("▤"));
    expect(ed.html()).toContain('<table class="randlos">');
  });

  /// Je Zeile und Spalte ein Löschknopf; ein Klick nimmt genau diese weg —
  /// ohne vorher etwas auszuwählen.
  it("Knopf an Zeile und Spalte löscht sie sofort", () => {
    const ed = mitTabelle();
    expect(document.querySelectorAll(".pm-row-handle")).toHaveLength(3);
    expect(document.querySelectorAll(".pm-col-handle")).toHaveLength(3);

    druecke(document.querySelectorAll<HTMLElement>(".pm-row-handle")[2]);
    expect((ed.html().match(/<tr>/g) ?? []).length).toBe(2);

    druecke(document.querySelectorAll<HTMLElement>(".pm-col-handle")[1]);
    expect((ed.html().match(/<th>/g) ?? []).length).toBe(2);
  });
});
