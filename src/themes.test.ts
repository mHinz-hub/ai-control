// Linienfarbe der Themes: Ränder müssen sich in jedem Theme von ihrer Fläche
// abheben. Die frühere Quelle (`black` des Terminal-Themes) lag in Dracula und
// Nord exakt auf der Flächenfarbe — Ränder waren dort unsichtbar.
import { describe, expect, it } from "vitest";
import { linie, THEMES } from "./themes";

/// Kontrastverhältnis nach WCAG (1 = kein Unterschied).
function kontrast(a: string, b: string): number {
  const hell = (h: string) => {
    const teile = [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
    const lin = teile.map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
  };
  const [oben, unten] = [hell(a), hell(b)].sort((x, y) => y - x);
  return (oben + 0.05) / (unten + 0.05);
}

describe("linie", () => {
  it("hebt sich in jedem Theme von der Fläche ab", () => {
    for (const [name, theme] of Object.entries(THEMES)) {
      const rand = linie(theme.header, theme.xterm.foreground);
      expect(kontrast(rand, theme.header), name).toBeGreaterThan(1.6);
    }
  });

  /// Gedämpfter und blasser Text müssen lesbar bleiben — in den hellen
  /// Themes lag `--muted` vorher bei Kontrast 1,1. Der Maßstab ist der
  /// Haupttext des Themes: solarized-dark bringt selbst nur 4,1, mehr kann
  /// eine Abstufung davon nicht haben.
  it("hält gedämpften und blassen Text lesbar", () => {
    for (const [name, theme] of Object.entries(THEMES)) {
      const flaeche = theme.header;
      const text = theme.xterm.foreground;
      const voll = kontrast(text, flaeche);
      expect(kontrast(linie(flaeche, text, 0.82), flaeche), name).toBeGreaterThan(
        Math.min(4.3, voll * 0.65),
      );
      expect(kontrast(linie(flaeche, text, 0.62), flaeche), name).toBeGreaterThan(
        Math.min(2.6, voll * 0.45),
      );
    }
  });

  it("bleibt zwischen Fläche und Text", () => {
    const rand = linie("#000000", "#ffffff", 0.5);
    expect(rand).toBe("#808080");
  });
});
