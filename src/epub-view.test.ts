// ePub-Viewer: Lesereihenfolge blättern, Inhaltsverzeichnis springt in den
// Spine, feste Seiten behalten ihre Maße.
import { describe, expect, it } from "vitest";
import { renderEpub, type EpubBook } from "./epub-view";

const buch = (extra: Partial<EpubBook> = {}): EpubBook => ({
  key: "abc-1",
  title: "Tractatus",
  creator: "Wittgenstein",
  language: "de",
  layout: "reflowable",
  spine: [
    { href: "OEBPS/kap1.xhtml" },
    { href: "OEBPS/kap2.xhtml" },
    { href: "OEBPS/kap3.xhtml" },
  ],
  toc: [
    { title: "Erstes", href: "OEBPS/kap1.xhtml", level: 0 },
    { title: "Drittes", href: "OEBPS/kap3.xhtml#z", level: 1 },
  ],
  ...extra,
});

function view(book: EpubBook) {
  const el = renderEpub(book);
  document.body.replaceChildren(el);
  const frame = el.querySelector<HTMLIFrameElement>(".epub-frame")!;
  const [prev, next] = [...el.querySelectorAll<HTMLButtonElement>(".epub-nav")];
  const count = el.querySelector<HTMLElement>(".epub-count")!;
  return { el, frame, prev, next, count };
}

describe("renderEpub", () => {
  it("blättert durch den Spine und zählt die Seiten", () => {
    const { frame, prev, next, count } = view(buch());
    expect(frame.src).toContain("abc-1/OEBPS/kap1.xhtml");
    expect(count.textContent).toBe("1 / 3");
    // Am Anfang gibt es kein Zurück.
    expect(prev.disabled).toBe(true);

    next.click();
    expect(frame.src).toContain("OEBPS/kap2.xhtml");
    expect(count.textContent).toBe("2 / 3");
    expect(prev.disabled).toBe(false);

    next.click();
    expect(count.textContent).toBe("3 / 3");
    expect(next.disabled).toBe(true);
    // Über das Ende hinaus geht es nicht weiter.
    next.click();
    expect(count.textContent).toBe("3 / 3");
  });

  it("springt über das Inhaltsverzeichnis, Fragment inklusive", () => {
    const { el, frame, count } = view(buch());
    const items = [...el.querySelectorAll<HTMLElement>(".epub-toc-item")];
    expect(items.map((i) => i.textContent)).toEqual(["Erstes", "Drittes"]);
    items[1].click();
    expect(count.textContent).toBe("3 / 3");
    expect(frame.src).toContain("OEBPS/kap3.xhtml#z");
  });

  it("ohne Inhaltsverzeichnis bleibt die Spalte weg", () => {
    const { el } = view(buch({ toc: [] }));
    expect(el.querySelector<HTMLElement>(".epub-toc")!.hidden).toBe(true);
  });

  it("feste Seiten behalten ihre Maße", () => {
    const { frame } = view(
      buch({
        layout: "pre-paginated",
        spine: [{ href: "OEBPS/s1.xhtml", width: 800, height: 1200 }],
      }),
    );
    expect(frame.style.width).toBe("800px");
    expect(frame.style.height).toBe("1200px");
    expect(frame.style.transform).toContain("scale(");
  });

  /// Fließender Text bekommt keine feste Größe aufgezwungen — er füllt die
  /// Fläche und scrollt selbst.
  it("fließender Text bleibt ohne Maßstab", () => {
    const { frame } = view(buch());
    expect(frame.style.width).toBe("");
    expect(frame.style.transform).toBe("");
  });
});
