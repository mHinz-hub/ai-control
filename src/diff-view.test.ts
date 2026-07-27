// Diff-Ansicht: Zeilenarten und mitlaufende Zeilennummern.
import { describe, expect, it } from "vitest";
import { diffClass, renderDiff } from "./diff-view";

/// (Klasse, linke Nummer, rechte Nummer, Text) je Zeile.
function rows(text: string) {
  const box = document.createElement("pre");
  box.append(renderDiff(text));
  return [...box.querySelectorAll<HTMLElement>(".dl")].map((row) => {
    const [left, right] = [...row.querySelectorAll<HTMLElement>(".dln")];
    return {
      kind: row.className.replace("dl ", "").trim(),
      left: left.textContent,
      right: right.textContent,
      text: row.querySelector("code")!.textContent,
    };
  });
}

describe("Zeilenart", () => {
  it("liest Dateiköpfe nur vor dem ersten Hunk", () => {
    expect(diffClass("--- a/x.md", false)).toBe("d-meta");
    expect(diffClass("+++ b/x.md", false)).toBe("d-meta");
    // Nach dem Hunk-Kopf ist dasselbe Zeichenmuster Inhalt, kein Dateikopf.
    expect(diffClass("--- a/x.md", true)).toBe("d-del");
    expect(diffClass("+++ b/x.md", true)).toBe("d-add");
  });

  it("erkennt Hunk, Zugang, Abgang und Kontext", () => {
    expect(diffClass("@@ -1,2 +1,3 @@", false)).toBe("d-hunk");
    expect(diffClass("+neu", true)).toBe("d-add");
    expect(diffClass("-weg", true)).toBe("d-del");
    expect(diffClass(" gleich", true)).toBe("");
    expect(diffClass("\\ No newline at end of file", true)).toBe("d-meta");
  });
});

describe("Zeilennummern", () => {
  it("laufen ab dem Hunk-Kopf mit", () => {
    const r = rows(
      ["@@ -10,3 +20,3 @@", " kontext", "-weg", "+neu", " ende"].join("\n"),
    );
    expect(r.map((x) => x.kind)).toEqual(["d-hunk", "", "d-del", "d-add", ""]);
    // Kontext zählt beide Seiten hoch, Abgang nur links, Zugang nur rechts.
    expect(r.map((x) => [x.left, x.right])).toEqual([
      ["", ""],
      ["10", "20"],
      ["11", ""],
      ["", "21"],
      ["12", "22"],
    ]);
  });

  it("verschieben sich nicht durch eine entfernte ---Zeile im Rumpf", () => {
    // Genau der Fall aus dem Archiv: eine gelöschte YAML-Trennlinie.
    const r = rows(
      ["--- a/n.md", "+++ b/n.md", "@@ -1,3 +1,2 @@", "----", " titel: x", " rest"].join("\n"),
    );
    expect(r[3]).toMatchObject({ kind: "d-del", left: "1", right: "" });
    expect(r[4]).toMatchObject({ kind: "", left: "2", right: "1" });
    expect(r[5]).toMatchObject({ kind: "", left: "3", right: "2" });
  });

  it("lässt die Dateiköpfe ohne Nummern", () => {
    const r = rows(["diff --git a/x b/x", "index 1..2 100644", "--- a/x"].join("\n"));
    expect(r.every((x) => x.kind === "d-meta" && x.left === "" && x.right === "")).toBe(
      true,
    );
  });
});
