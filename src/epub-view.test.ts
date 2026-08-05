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
  const prev = el.querySelector<HTMLButtonElement>(".epub-prev")!;
  const next = el.querySelector<HTMLButtonElement>(".epub-next")!;
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

  /// Das Verzeichnis zeigt, wo der Leser steht — auch wenn er sich über die
  /// Kapitelgrenze hinweg dorthin geblättert hat.
  it("das Inhaltsverzeichnis folgt der Stelle im Buch", () => {
    const { el, next } = view(buch());
    const zeilen = [...el.querySelectorAll(".epub-toc-item")];
    expect(zeilen[0].classList.contains("hier")).toBe(true);
    // Seite 2 gehört noch zum ersten Eintrag, Seite 3 zum zweiten.
    next.click();
    expect(zeilen[0].classList.contains("hier")).toBe(true);
    next.click();
    expect(zeilen[1].classList.contains("hier")).toBe(true);
  });

  /// Umschalt springt eine Stufe gröber: von Kapitel zu Kapitel.
  it("Umschalt und Pfeil springen im Inhaltsverzeichnis", () => {
    const { el, count } = view(buch());
    const taste = (key: string) =>
      el.dispatchEvent(new KeyboardEvent("keydown", { key, shiftKey: true, bubbles: true }));
    taste("ArrowRight");
    expect(count.textContent).toBe("3 / 3");
    taste("ArrowLeft");
    expect(count.textContent).toBe("1 / 3");
  });

  /// Blättern geht an die Buchseite: sie allein kennt ihren Scrollstand und
  /// weiß, ob im Kapitel noch Platz ist.
  it("gibt das Blättern an die Buchseite weiter", () => {
    const { el, frame } = view(buch());
    const gesendet: unknown[] = [];
    frame.contentWindow!.postMessage = (d: unknown) => gesendet.push(d);
    const links = el.querySelector<HTMLButtonElement>(".epub-blaettern.links")!;
    const rechts = el.querySelector<HTMLButtonElement>(".epub-blaettern.rechts")!;
    // Am Anfang des Buchs führt kein Schritt zurück.
    expect(links.disabled).toBe(true);
    rechts.click();
    expect(gesendet).toEqual([{ ac: "blaettern", richtung: 1 }]);
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    expect(gesendet).toHaveLength(2);
  });

  /// Der Schriftgrad gilt der Seite im Rahmen, nicht der App — er geht als
  /// Nachricht hinüber und bleibt beim Kapitelwechsel stehen.
  it("stellt den Schriftgrad der Buchseite", () => {
    const { el, frame } = view(buch());
    const gesendet: { ac?: string; wert?: number }[] = [];
    frame.contentWindow!.postMessage = (d: unknown) => gesendet.push(d as never);
    el.querySelector<HTMLButtonElement>(".epub-groesser")!.click();
    el.querySelector<HTMLButtonElement>(".epub-groesser")!.click();
    el.querySelector<HTMLButtonElement>(".epub-kleiner")!.click();
    expect(gesendet.map((d) => d.wert)).toEqual([110, 120, 110]);
    expect(gesendet.every((d) => d.ac === "schrift")).toBe(true);
  });

  /// Die Druckseite ist die Angabe, mit der zitiert wird; sie kommt aus den
  /// Marken im Satz und wird von der Buchseite gemeldet.
  it("zeigt die Druckseite, auf der der Leser steht", () => {
    const { el, frame } = view(buch());
    const seite = el.querySelector<HTMLElement>(".epub-seite")!;
    expect(seite.textContent).toBe("");
    window.dispatchEvent(
      Object.assign(
        new MessageEvent("message", { data: { ac: "stand", oben: 0, rand: 9, seite: "135" } }),
        { source: frame.contentWindow },
      ),
    );
    expect(seite.textContent).toBe("S. 135");
  });

  /// Im Seitenmodus füllt der Abschnitt zwischen zwei Marken die Fläche; das
  /// entscheidet die Buchseite, der Viewer sagt ihr nur, daß er es will.
  it("schaltet den Seitenmodus an der Buchseite um", () => {
    const { el, frame } = view(buch());
    const gesendet: { ac?: string; an?: boolean }[] = [];
    frame.contentWindow!.postMessage = (d: unknown) => gesendet.push(d as never);
    const knopf = el.querySelector<HTMLButtonElement>(".epub-seitig")!;
    const marker = el.querySelector<HTMLButtonElement>(".epub-marker")!;
    knopf.click();
    // Im Seitenmodus sagt die Fläche selbst, wo die Seite endet — die Marken
    // im Satz gehen aus, ihr Schalter ist gesperrt.
    expect(gesendet).toEqual([
      { ac: "marker", an: false },
      { ac: "seitig", an: true },
    ]);
    expect(knopf.classList.contains("an")).toBe(true);
    expect(marker.disabled).toBe(true);
    knopf.click();
    expect(gesendet.at(-1)).toEqual({ ac: "seitig", an: false });
    expect(knopf.classList.contains("an")).toBe(false);
    expect(marker.disabled).toBe(false);
  });

  /// Der Wunsch des Lesers überlebt den Seitenmodus: waren die Marken an,
  /// sind sie es danach wieder.
  it("stellt die Umbruchmarken nach dem Seitenmodus wieder her", () => {
    const { el, frame } = view(buch());
    const gesendet: { ac?: string; an?: boolean }[] = [];
    frame.contentWindow!.postMessage = (d: unknown) => gesendet.push(d as never);
    const marker = el.querySelector<HTMLButtonElement>(".epub-marker")!;
    const knopf = el.querySelector<HTMLButtonElement>(".epub-seitig")!;
    marker.click();
    expect(marker.classList.contains("an")).toBe(true);
    knopf.click();
    expect(marker.classList.contains("an")).toBe(false);
    knopf.click();
    expect(marker.classList.contains("an")).toBe(true);
    expect(gesendet.filter((d) => d.ac === "marker").map((d) => d.an)).toEqual([
      true,
      false,
      true,
    ]);
  });

  /// Im Seitenmodus scrollt die Buchseite nicht — oben und rand sind beide 0.
  /// Der Pfeil darf davon nicht auf »geht nicht weiter« schließen, sonst ist er
  /// im letzten Kapitel tot, obwohl dort noch Druckseiten liegen.
  it("hält den Pfeil im Seitenmodus an den Druckseiten", () => {
    const { el, frame, next } = view(buch());
    next.click();
    next.click();
    const rechts = el.querySelector<HTMLButtonElement>(".epub-blaettern.rechts")!;
    const melde = (data: unknown) =>
      window.dispatchEvent(
        Object.assign(new MessageEvent("message", { data }), { source: frame.contentWindow }),
      );
    melde({ ac: "stand", oben: 0, rand: 0, seitig: true, von: 1, bis: 5 });
    expect(rechts.disabled).toBe(false);
    melde({ ac: "stand", oben: 0, rand: 0, seitig: true, von: 5, bis: 5 });
    expect(rechts.disabled).toBe(true);
  });

  it("klappt das Inhaltsverzeichnis weg", () => {
    const { el } = view(buch());
    const toc = el.querySelector<HTMLElement>(".epub-toc")!;
    const klapp = el.querySelector<HTMLButtonElement>(".epub-klapp")!;
    expect(toc.hidden).toBe(false);
    klapp.click();
    expect(toc.hidden).toBe(true);
    klapp.click();
    expect(toc.hidden).toBe(false);
  });

  /// Meldet die Seite ihren Rand, übernimmt der Viewer und wechselt das
  /// Kapitel — das ist der Übergang, den der Leser nicht merken soll.
  it("wechselt das Kapitel, wenn die Seite an ihrem Rand steht", () => {
    const { el, frame, count } = view(buch());
    const melde = (data: unknown) =>
      window.dispatchEvent(
        Object.assign(new MessageEvent("message", { data }), {
          source: frame.contentWindow,
        }),
      );
    melde({ ac: "rand", richtung: 1 });
    expect(count.textContent).toBe("2 / 3");
    melde({ ac: "rand", richtung: -1 });
    expect(count.textContent).toBe("1 / 3");
    // Fremde Botschaften bewegen nichts.
    window.dispatchEvent(new MessageEvent("message", { data: { ac: "rand", richtung: 1 } }));
    expect(count.textContent).toBe("1 / 3");
    expect(el.querySelector(".epub-count")).toBe(count);
  });
});
