import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

/// Der Ausgabestring geht per innerHTML ins Panel, das im selben Webview wie
/// das Terminal liegt. Nichts davon darf ausführbar sein.
describe("renderMarkdown", () => {
  it("rendert normales Markdown", () => {
    const html = renderMarkdown("# Titel\n\nText mit *Betonung*.");
    expect(html).toContain("<h1");
    expect(html).toContain("<em>Betonung</em>");
  });

  /// Entscheidend ist nicht, ob die Zeichenkette `onerror` vorkommt — als
  /// escapter Text ist sie harmlos —, sondern ob beim Einhängen ein echtes
  /// Element mit Handler entsteht.
  it("gibt rohes HTML als Text aus statt es zu interpretieren", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown('<img src=x onerror="alert(1)">');
    expect(el.querySelector("img")).toBeNull();
    expect(el.textContent).toContain("<img");
  });

  it("entschärft auch inline eingebettetes HTML", () => {
    const html = renderMarkdown("Text <script>alert(1)</script> weiter");
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });

  it("blockt javascript:-Links, behält aber den Linktext", () => {
    const html = renderMarkdown("[klick](javascript:alert(1))");
    expect(html).not.toContain("javascript:");
    expect(html).toContain("klick");
  });

  it("blockt javascript: auch mit Steuerzeichen und Großschreibung", () => {
    for (const href of ["JaVaScRiPt:alert(1)", `java${String.fromCharCode(0)}script:alert(1)`, " javascript:x"]) {
      const html = renderMarkdown(`[k](${href})`);
      expect(html.toLowerCase()).not.toContain("javascript:");
    }
  });

  it("blockt data:-Bilder", () => {
    expect(renderMarkdown("![a](data:image/svg+xml,<svg onload=alert(1)>)")).not.toContain("<img");
  });

  /// Ein <img> lädt ohne Zutun. Auswärtige Bildquellen sind darum ein
  /// Zero-Click-Beacon: IP, Zeitpunkt und im Pfad kodierte Daten gehen raus,
  /// sobald jemand ein vergiftetes Archiv-Dokument nur ansieht.
  it("lädt keine auswärtigen Bilder, auch nicht schema-relativ", () => {
    for (const src of [
      "https://evil.example/t.png",
      "http://evil.example/t.png",
      "//evil.example/t.png",
      "HTTPS://evil.example/t.png",
    ]) {
      const el = document.createElement("div");
      el.innerHTML = renderMarkdown(`![alt](${src})`);
      expect(el.querySelector("img"), `${src} durfte kein img erzeugen`).toBeNull();
      expect(el.textContent).toContain("alt");
    }
  });

  it("lässt lokale Bilder durch", () => {
    expect(renderMarkdown("![a](./bild.png)")).toContain("<img");
    expect(renderMarkdown("![a](/bild.png)")).toContain("<img");
  });

  /// Links dürfen auswärts zeigen — sie brauchen einen Klick.
  it("erlaubt auswärtige Links, aber ohne Referrer und Opener", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown("[a](https://example.org)");
    const a = el.querySelector("a")!;
    expect(a.getAttribute("href")).toBe("https://example.org");
    expect(a.getAttribute("rel")).toContain("noopener");
    expect(a.getAttribute("rel")).toContain("noreferrer");
  });

  it("escapt auch das Apostroph", () => {
    expect(renderMarkdown("Text mit ' Apostroph")).toContain("&#39;");
  });

  it("escapt Anführungszeichen im title, damit das Attribut nicht aufbricht", () => {
    const el = document.createElement("div");
    el.innerHTML = renderMarkdown('[k](https://example.org "a\\" onmouseover=alert(1)")');
    const a = el.querySelector("a")!;
    // Das Anführungszeichen bleibt Inhalt des title-Attributs, statt es zu
    // schließen und ein onmouseover-Attribut aufzumachen.
    expect(a.hasAttribute("onmouseover")).toBe(false);
    expect(a.getAttribute("title")).toContain('"');
  });
});
