// Stylesheets der Fenster: Syntax gegen denselben Parser prüfen, den der
// Release-Build benutzt.
//
// Anlass: Ein `///`-Kommentar (Rust-/TS-Gewohnheit) ist in CSS ein leerer
// Selektor. Im Dev-Server fällt das nicht auf — dort wird nichts minifiziert.
// Erst `npm run build` schickt die Datei durch lightningcss, und dort bricht
// der Build ab. Zwischen Commit und Paketbau lag damit eine Lücke, die weder
// vue-tsc noch die übrigen Tests schließen: Beide fassen CSS nicht an.
//
// Die Dateien kommen über `import.meta.glob` statt über `node:fs` — damit
// bleibt der Test im selben Typ-Universum wie der übrige Frontend-Code
// (tsconfig.app.json kennt nur `vite/client`, keine Node-Typen).

import { transform } from "lightningcss";
import { describe, expect, it } from "vitest";

const sheets = import.meta.glob("./*.css", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const encode = (css: string) => new TextEncoder().encode(css);

describe("Stylesheets", () => {
  it("liegen überhaupt vor", () => {
    // Ohne diese Zusicherung liefe die Prüfung unten über eine leere Liste
    // grün durch, sobald sich der Ablageort ändert.
    expect(Object.keys(sheets).length).toBeGreaterThan(0);
  });

  it.each(Object.keys(sheets))("%s übersteht die Minifizierung", (name) => {
    expect(() =>
      transform({ filename: name, code: encode(sheets[name]), minify: true }),
    ).not.toThrow();
  });

  /// Der konkrete Fehler von damals — als Nachweis, dass die Prüfung ihn
  /// tatsächlich fängt und nicht bloß nie etwas findet.
  it("fängt einen Doc-Kommentar im CSS", () => {
    expect(() =>
      transform({
        filename: "test.css",
        code: encode("/// Kommentar\n.a { color: red; }\n"),
        minify: true,
      }),
    ).toThrow();
  });
});
