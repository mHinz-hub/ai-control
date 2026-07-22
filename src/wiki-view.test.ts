// Wiki-Ansicht: Übersichts-/Schlagwort-Seiten, Dokumentseite, Leerzustände.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initWikiView } from "./wiki-view";

function setup() {
  document.body.innerHTML = `<div id="w"></div>`;
  const onLink = vi.fn();
  const view = initWikiView(document.getElementById("w")!, onLink);
  return { view, onLink };
}

const doc = (name: string, extra: object = {}) => ({
  name,
  title: name.toUpperCase(),
  description: null,
  tags: [],
  date: "2026-07-19",
  backlinks: 0,
  ...extra,
});

const page = JSON.stringify({
  kind: "page",
  home: "/tmp/archiv",
  tag: null,
  total: 3,
  tags: [
    { name: "adr", count: 2 },
    { name: "infra", count: 1 },
  ],
  recent: [doc("neu")],
  folders: [
    { name: "", docs: [doc("wurzel-doc", { backlinks: 2, tags: ["adr"] })] },
    { name: "konzepte", docs: [doc("neu"), doc("alt")] },
  ],
});

describe("initWikiView — Seiten", () => {
  it("rendert Kopf, Chips, Zuletzt- und Ordner-Sektionen", () => {
    const { view } = setup();
    view.set(page);
    expect(document.querySelector(".wiki-head-title")!.textContent).toBe("Archiv");
    expect(document.querySelector(".wiki-head-right")!.textContent).toBe("3 Dokumente");
    const chips = [...document.querySelectorAll(".wiki-chips .wiki-chip")].map(
      (c) => c.textContent,
    );
    expect(chips).toEqual(["Alle", "#adr 2", "#infra 1"]);
    const eyebrows = [...document.querySelectorAll(".wiki-folder")].map(
      (e) => e.textContent,
    );
    expect(eyebrows).toEqual(["Zuletzt", "Wurzel", "konzepte/"]);
    expect(document.querySelector(".wiki-doc-back")!.textContent).toBe("↩ 2");
  });

  it("Dokumentzeile öffnet das Dokument, Tag-Chip die Tag-Seite", () => {
    const { view, onLink } = setup();
    view.set(page);
    const rows = document.querySelectorAll<HTMLElement>(".wiki-doc");
    rows[1].click(); // Wurzel-Dokument (nach der Zuletzt-Zeile)
    expect(onLink).toHaveBeenCalledWith("wurzel-doc");
    rows[1].querySelector<HTMLElement>(".wiki-doc-tags .wiki-chip")!.click();
    expect(onLink).toHaveBeenCalledWith("tag:adr");
    document.querySelector<HTMLElement>(".wiki-chips .wiki-chip")!.click();
    expect(onLink).toHaveBeenCalledWith("tag:");
  });

  it("Tag-Seite: Titel, aktiver Chip, Leerfall", () => {
    const { view } = setup();
    view.set(
      JSON.stringify({
        kind: "page",
        home: "/a",
        tag: "adr",
        total: 0,
        tags: [{ name: "adr", count: 1 }],
        recent: [],
        folders: [],
      }),
    );
    expect(document.querySelector(".wiki-head-title")!.textContent).toBe("#adr");
    const active = document.querySelector(".wiki-chip.active")!;
    expect(active.textContent).toBe("#adr 1");
    expect(document.querySelector(".wiki-empty strong")!.textContent).toBe(
      "Keine Dokumente mit #adr.",
    );
  });

  it("leeres Archiv erklärt das Archivieren", () => {
    const { view } = setup();
    view.set(
      JSON.stringify({
        kind: "page",
        home: "/a",
        tag: null,
        total: 0,
        tags: [],
        recent: [],
        folders: [],
      }),
    );
    expect(document.querySelector(".wiki-empty")!.textContent).toContain(
      "Das Archiv ist leer.",
    );
  });
});

describe("initWikiView — Dokument und Leerzustand", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("rendert Markdown, Rücksprung, Tags und Backlinks", () => {
    const { view, onLink } = setup();
    view.set(
      JSON.stringify({
        kind: "doc",
        home: "/a",
        relpath: "konzepte/2026-07-19_1000-x.md",
        name: "x",
        title: "X",
        tags: ["adr"],
        backlinks: ["anderes-doc"],
        markdown: "# Überschrift\n\nText mit [[ziel|Label]].\n",
      }),
    );
    expect(document.querySelector(".wiki-body h1")!.textContent).toBe("Überschrift");
    const wiki = document.querySelector<HTMLElement>(".wiki-body a.wiki")!;
    expect(wiki.textContent).toBe("Label");
    wiki.click();
    expect(onLink).toHaveBeenCalledWith("ziel");
    document.querySelector<HTMLElement>(".wiki-back")!.click();
    expect(onLink).toHaveBeenCalledWith("tag:");
    const back = document.querySelector<HTMLElement>(".wiki-backlinks a")!;
    back.click();
    expect(onLink).toHaveBeenCalledWith("anderes-doc");
  });

  it("leerer Puffer bietet den Übersichts-Einstieg", () => {
    const { view, onLink } = setup();
    view.set("");
    expect(view.empty()).toBe(true);
    document.querySelector<HTMLElement>(".wiki-empty .wiki-chip")!.click();
    expect(onLink).toHaveBeenCalledWith("tag:");
    view.set(page);
    expect(view.empty()).toBe(false);
  });
});
