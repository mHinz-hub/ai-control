// Schreibhilfen des Markdown-Editors: Listen laufen beim Enter weiter,
// Tabellen kommen als lesbares Gerüst.
import { describe, expect, it } from "vitest";
import {
  initMdEditor,
  listenPrefix,
  mdTabelle,
  stellen,
  xmlMangel,
  yamlMangel,
} from "./md-editor";

describe("listenPrefix", () => {
  it("setzt Aufzählung, Nummerierung und Zitat fort", () => {
    expect(listenPrefix("- Punkt")).toBe("- ");
    expect(listenPrefix("  * Punkt")).toBe("  * ");
    expect(listenPrefix("+ Punkt")).toBe("+ ");
    expect(listenPrefix("3. Dritter")).toBe("4. ");
    expect(listenPrefix("  10) Zehnter")).toBe("  11) ");
    expect(listenPrefix("> Zitat")).toBe("> ");
  });

  it("beendet die Liste beim leeren Eintrag", () => {
    expect(listenPrefix("- ")).toBe("");
    expect(listenPrefix("  1.   ")).toBe("");
  });

  it("lässt gewöhnliche Zeilen in Ruhe", () => {
    expect(listenPrefix("Text")).toBeNull();
    expect(listenPrefix("")).toBeNull();
    expect(listenPrefix("-kein Abstand")).toBeNull();
    expect(listenPrefix("# Überschrift")).toBeNull();
  });
});

describe("mdTabelle", () => {
  it("baut Kopfzeile, Trennlinie und leere Zeilen", () => {
    const t = mdTabelle(2, 1, true).split("\n");
    expect(t[0]).toBe("| Spalte 1 | Spalte 2 |");
    expect(t[1]).toBe("| -------- | -------- |");
    expect(t[2]).toBe("|          |          |");
    expect(t[3]).toBe("");
  });

  it("lässt den Kopf auf Wunsch leer, behält aber die Trennlinie", () => {
    // Ohne Trennlinie wäre es keine Tabelle mehr, nur Striche.
    const t = mdTabelle(3, 2, false).split("\n");
    expect(t[0]).toBe("|          |          |          |");
    expect(t[1]).toBe("| -------- | -------- | -------- |");
    expect(t).toHaveLength(5);
  });
});

describe("Fundstellen", () => {
  it("zählt alle Vorkommen in Lesereihenfolge", () => {
    const text = "Kessel, dann Rohr, dann kessel";
    expect(stellen(text, ["kessel"])).toEqual([
      { von: 0, bis: 6 },
      { von: 24, bis: 30 },
    ]);
    // Mehrere Wörter zählen gemeinsam, sortiert nach Position.
    expect(stellen(text, ["rohr", "kessel"]).map((s) => s.von)).toEqual([0, 13, 24]);
    expect(stellen(text, ["fehlt"])).toEqual([]);
  });

  /// Der Editor springt nicht zum ersten Vorkommen, sondern zu dem, das beim
  /// Lesen im Bild stand.
  it("springt zur gemeinten Fundstelle und markiert alle", () => {
    document.body.innerHTML = "";
    const ed = initMdEditor({
      text: "Kessel, dann Rohr, dann kessel",
      fundstellen: { woerter: ["kessel"], nummer: 1 },
      onChange: () => {},
      onSave: () => {},
      onCancel: () => {},
      onScroll: () => {},
    });
    document.body.append(ed.el);
    // Alle Vorkommen sind unterlegt; angesprungen wird die zweite Stelle
    // (`nummer: 1`) — ihre Position liefert `stellen`.
    expect(ed.el.querySelectorAll(".cm-hit")).toHaveLength(2);
    expect(stellen(ed.value(), ["kessel"])[1]).toEqual({ von: 24, bis: 30 });
  });
});

describe("Prüfung der Datenformate", () => {
  it("YAML: meldet den Fehler mit Stelle", () => {
    expect(yamlMangel("a: 1\nb: 2\n")).toBeNull();
    const m = yamlMangel("a:\n b: [1,\n")!;
    expect(m).not.toBeNull();
    expect(m.von).toBeGreaterThan(0);
    expect(m.text).toBeTruthy();
  });

  it("XML: meldet unausgeglichene Marken, lässt gültiges durch", () => {
    expect(xmlMangel('<?xml version="1.0"?>\n<a><b/></a>')).toBeNull();
    expect(xmlMangel("")).toBeNull();
    const m = xmlMangel("<a><b></a>")!;
    expect(m).not.toBeNull();
    expect(m.text).toBeTruthy();
  });
});
