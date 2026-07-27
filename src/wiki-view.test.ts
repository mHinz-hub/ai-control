// Archiv-Ansicht: Baum mit Notizen (Ordner + Dokumente, Verschmelzung
// gleichnamiger Paare), Notiz-Ansicht rechts (Titel, Inhalt, Kindliste),
// Aktionen (bearbeiten, umbenennen, löschen, anlegen), Suchtreffer-Vormerkung,
// Leerzustände.
import { describe, expect, it, vi } from "vitest";
import type { EpubBook } from "./epub-view";
import { initWikiView } from "./wiki-view";

/// Minimalbuch für die Viewer-Zweige: eine Seite, kein Inhaltsverzeichnis.
const BUCH: EpubBook = {
  key: "abc-1",
  title: "Tractatus",
  creator: "Wittgenstein",
  language: "de",
  layout: "reflowable",
  spine: [{ href: "OEBPS/kap1.xhtml" }],
  toc: [],
};

function setup(pending: string | null = null) {
  document.body.innerHTML = `<div id="w"></div>`;
  const readDoc = vi.fn(() => Promise.resolve("# Inhalt\n\nText mit [[neu]]."));
  const writeDoc = vi.fn(() => Promise.resolve());
  const openEpub = vi.fn(() => Promise.resolve(BUCH));
  const setTitle = vi.fn();
  const openWiki = vi.fn();
  const actions = {
    remove: vi.fn(),
    createFolder: vi.fn(),
    createDoc: vi.fn(() => Promise.resolve("id-neu-doc")),
    createHtml: vi.fn(() => Promise.resolve("id-neu-html")),
  };
  let pendingSelect = pending;
  const view = initWikiView(document.getElementById("w")!, {
    autoStart: true,
    readDoc,
    writeDoc,
    openEpub,
    setTitle,
    openWiki,
    takePending: () => {
      const p = pendingSelect;
      pendingSelect = null;
      return p;
    },
    actions,
  });
  return { view, readDoc, writeDoc, openEpub, setTitle, openWiki, actions };
}

const doc = (relpath: string, name: string, extra: object = {}) => ({
  id: `id-${name}`,
  relpath,
  name,
  title: name.toUpperCase(),
  description: null,
  tags: [],
  date: "2026-07-19",
  backlinks: 0,
  modified: "2026-07-24",
  kind: "md",
  ...extra,
});

const page = JSON.stringify({
  kind: "page",
  home: "/tmp/archiv",
  tag: null,
  total: 3,
  tags: [],
  folders: [
    {
      name: "",
      docs: [
        doc("2026-07-19_1000-wurzel-doc.md", "wurzel-doc", {
          backlinks: 2,
          description: "Beschreibung",
        }),
        // Knotentext des Ordners „konzepte" — jeder Knoten hat einen.
        doc("konzepte.md", "konzepte", { title: "konzepte" }),
      ],
    },
    {
      name: "konzepte",
      docs: [
        doc("konzepte/2026-07-19_1005-neu.md", "neu"),
        doc("konzepte/panel.md", "panel", { title: "panel" }),
      ],
    },
    {
      name: "konzepte/panel",
      docs: [doc("konzepte/panel/2026-07-19_1010-alt.md", "alt")],
    },
  ],
});

/// Ordner `konzepte` mit gleichnamigem Dokument daneben — eine Notiz mit
/// Inhalt und Kindern.
const mergedPage = JSON.stringify({
  kind: "page",
  home: "/tmp/archiv",
  tag: null,
  total: 2,
  tags: [],
  folders: [
    { name: "", docs: [doc("konzepte.md", "konzepte", { title: "Konzepte" })] },
    { name: "konzepte", docs: [doc("konzepte/2026-07-19_1005-neu.md", "neu")] },
  ],
});

const flush = () => new Promise((r) => setTimeout(r));

describe("initWikiView — Baum", () => {
  it("Baum zeigt Ordner und Dokumente, Wurzel ist gewählt", () => {
    const { view } = setup();
    view.set(page);
    expect(document.querySelector(".wiki-tree-root")!.className).toContain("active");
    const rows = [...document.querySelectorAll(".wiki-tree .wiki-tree-name")].map(
      (e) => e.textContent,
    );
    // Unter jedem Knoten erst Dokumente, dann Ordner (mit Titel), sortiert.
    expect(rows).toEqual(["WURZEL-DOC", "konzepte", "NEU", "panel", "ALT"]);
    // Wurzel-Ansicht rechts: Titel + Kindzähler.
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("Archiv");
    expect(document.querySelector(".wiki-children-caption")!.textContent).toBe(
      "2 Dokumente",
    );
  });

  it("gleichnamiges Dokument verschmilzt mit dem Ordner zur Notiz", async () => {
    const { view, readDoc } = setup();
    view.set(mergedPage);
    const rows = [...document.querySelectorAll(".wiki-tree .wiki-tree-name")].map(
      (e) => e.textContent,
    );
    // Kein eigenes Blatt für konzepte.md — der Ordnerknoten trägt den Titel.
    expect(rows).toEqual(["Konzepte", "NEU"]);
    // Auswahl des Knotens zeigt Inhalt UND Kindliste.
    document.querySelector<HTMLElement>(".wiki-tree summary .wiki-tree-name")!.click();
    await flush();
    expect(readDoc).toHaveBeenCalledWith("id-konzepte");
    expect(document.querySelector(".wiki-note-body h1")!.textContent).toBe("Inhalt");
    expect(document.querySelector(".wiki-note-children .wiki-doc-title")!.textContent).toBe(
      "NEU",
    );
  });
});

describe("initWikiView — Notiz-Ansicht", () => {
  it("Dokument-Klick lädt den Body in die Notiz-Ansicht", async () => {
    const { view, readDoc } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click(); // WURZEL-DOC
    await flush();
    expect(readDoc).toHaveBeenCalledWith("id-wurzel-doc");
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("WURZEL-DOC");
    // Meta als Popup am Info-Knopf.
    const pop = document.querySelector<HTMLElement>(".wiki-note-info-pop")!;
    expect(pop.hidden).toBe(true);
    document.querySelector<HTMLElement>('.wiki-note-actions [title^="Details"]')!.click();
    expect(pop.hidden).toBe(false);
    expect(pop.textContent).toContain("↩ 2");
    expect(document.querySelector(".wiki-note-body h1")!.textContent).toBe("Inhalt");
  });

  it("Wikilink im Inhalt wählt die Ziel-Notiz lokal aus", async () => {
    const { view, openWiki } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click(); // WURZEL-DOC
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-body a.wiki")!.click();
    expect(openWiki).not.toHaveBeenCalled();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("NEU");
  });

  it("Stift schaltet in den Editor, Speichern schreibt zurück", async () => {
    const { view, writeDoc } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-actions .panel-btn")!.click();
    await flush();
    const area = document.querySelector<HTMLTextAreaElement>(".wiki-note-editor")!;
    expect(area.value).toContain("# Inhalt");
    // Vorschau rechts zeigt den gerenderten Rohtext.
    expect(document.querySelector(".wiki-edit-preview h1")!.textContent).toBe("Inhalt");
    area.value = "# Neu\n\nGeändert.";
    area.dispatchEvent(new Event("input"));
    expect(document.querySelector(".wiki-edit-preview h1")!.textContent).toBe("Neu");
    document.querySelector<HTMLElement>(".wiki-form-submit")!.click();
    await flush();
    expect(writeDoc).toHaveBeenCalledWith("id-wurzel-doc", "# Neu\n\nGeändert.");
    // Nach dem Speichern wieder die Anzeige.
    expect(document.querySelector(".wiki-note-editor")).toBeNull();
    expect(document.querySelector(".wiki-note-body")).not.toBeNull();
  });

  it("Abbrechen verwirft und zeigt wieder an", async () => {
    const { view, writeDoc } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-actions .panel-btn")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-form-cancel")!.click();
    expect(writeDoc).not.toHaveBeenCalled();
    expect(document.querySelector(".wiki-note-body")).not.toBeNull();
  });

  it("Auswahl übersteht ein Puffer-Update, Verschwundenes fällt zur Wurzel", () => {
    const { view } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree summary .wiki-tree-name")!.click();
    view.set(page); // Update: Auswahl bleibt
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("konzepte");
    view.set(
      JSON.stringify({
        kind: "page",
        home: "/tmp/archiv",
        tag: null,
        total: 1,
        tags: [],
        folders: [{ name: "", docs: [doc("2026-07-19_1000-wurzel-doc.md", "wurzel-doc")] }],
      }),
    );
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("Archiv");
  });

  it("vorgemerkter Suchtreffer wählt die Notiz beim set() aus", async () => {
    const { view, readDoc } = setup("id-neu");
    view.set(page);
    await flush();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("NEU");
    expect(readDoc).toHaveBeenCalledWith("id-neu");
  });
});

describe("initWikiView — HTML-Notizen", () => {
  const htmlPage = JSON.stringify({
    kind: "page",
    home: "/tmp/archiv",
    tag: null,
    total: 1,
    tags: [],
    folders: [
      { name: "", docs: [doc("seite.html", "seite", { kind: "html" })] },
    ],
  });

  it("zeigt HTML ohne Markdown-Renderer und trägt ein eigenes Symbol", async () => {
    document.body.innerHTML = `<div id="w"></div>`;
    const readDoc = vi.fn(() => Promise.resolve("<p>Roher <b>Text</b></p>"));
    const view = initWikiView(document.getElementById("w")!, {
      autoStart: true,
      readDoc,
      writeDoc: vi.fn(() => Promise.resolve()),
      openEpub: vi.fn(() => Promise.resolve(BUCH)),
      setTitle: vi.fn(),
      openWiki: vi.fn(),
      actions: {
        remove: vi.fn(),
        createFolder: vi.fn(),
        createDoc: vi.fn(() => Promise.resolve("")),
        createHtml: vi.fn(() => Promise.resolve("")),
      },
    });
    view.set(htmlPage);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    // Der Rumpf steht als Markup in der Ansicht — kein <p> aus Markdown.
    expect(document.querySelector(".wiki-note-body b")!.textContent).toBe("Text");
    expect(readDoc).toHaveBeenCalledWith("id-seite");
  });

  const buchSeite = JSON.stringify({
    kind: "page",
    home: "/tmp/archiv",
    tag: null,
    total: 1,
    tags: [],
    folders: [{ name: "", docs: [doc("tractatus.epub", "tractatus", { kind: "epub" })] }],
  });

  /// Ein Buch wird gelesen: Viewer statt Notiz-Body, kein Bearbeiten — weder
  /// im Kopf noch im Kontextmenü, und der Titel bleibt unverändert.
  it("öffnet Bücher im Viewer statt im Editor", async () => {
    const { view, openEpub, readDoc } = setup();
    view.set(buchSeite);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    expect(openEpub).toHaveBeenCalledWith("id-tractatus");
    expect(readDoc).not.toHaveBeenCalled();
    expect(document.querySelector(".epub-frame")).not.toBeNull();
    expect(document.querySelector(".wiki-main")!.className).toContain("epub-mode");
    // Kopf: nur Löschen, kein Stift; Titel nicht klickbar.
    expect(document.querySelector(".wiki-note-actions .panel-btn")!.textContent).not.toBe("✎");
    expect(document.querySelector(".wiki-note-title")!.className).not.toContain("editable");

    document
      .querySelector<HTMLElement>(".wiki-tree-doc")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const items = [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")];
    expect(items.map((i) => i.textContent)).toEqual(["Dokument löschen"]);
  });

  it("Kontextmenü legt HTML-Notizen unter dem Knoten an", () => {
    const { view, actions } = setup();
    view.set(mergedPage);
    document
      .querySelector<HTMLElement>(".wiki-tree summary")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const items = [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")];
    items.find((i) => i.textContent === "Neue HTML-Notiz")!.click();
    const input = document.querySelector<HTMLInputElement>(".wiki-form input")!;
    input.value = "Seite";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(actions.createHtml).toHaveBeenCalledWith("id-konzepte", "Seite");
  });
});

describe("initWikiView — Anlegen öffnet den Editor", () => {
  it("wählt die neue Notiz aus und zeigt den Editor", async () => {
    const { view, actions } = setup();
    view.set(page);
    document
      .querySelector<HTMLElement>(".wiki-tree summary")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const items = [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")];
    items.find((i) => i.textContent === "Neues Dokument")!.click();
    const input = document.querySelector<HTMLInputElement>(".wiki-form input")!;
    input.value = "Frisch";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(actions.createDoc).toHaveBeenCalledWith("id-konzepte", "Frisch");
    await flush();

    // Die Übersicht kommt nach; erst dann greift die Vormerkung.
    expect(document.querySelector(".wiki-note-editor")).toBeNull();
    view.set(
      JSON.stringify({
        kind: "page",
        home: "/tmp/archiv",
        tag: null,
        total: 1,
        tags: [],
        folders: [
          {
            name: "",
            docs: [doc("frisch.md", "frisch", { id: "id-neu-doc", title: "Frisch" })],
          },
        ],
      }),
    );
    await flush();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("Frisch");
    expect(document.querySelector(".wiki-note-editor")).not.toBeNull();
  });
});

describe("initWikiView — Zurück", () => {
  it("Zurück-Knopf führt zur vorherigen Auswahl", async () => {
    const { view } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click(); // WURZEL-DOC
    await flush();
    document.querySelector<HTMLElement>(".wiki-tree summary .wiki-tree-name")!.click(); // konzepte
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("konzepte");
    document.querySelector<HTMLElement>(".wiki-note-back")!.click();
    await flush();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("WURZEL-DOC");
    document.querySelector<HTMLElement>(".wiki-note-back")!.click();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("Archiv");
    // Verlauf leer: Knopf deaktiviert.
    expect(
      document.querySelector<HTMLButtonElement>(".wiki-note-back")!.disabled,
    ).toBe(true);
  });
});

describe("initWikiView — Aktionen", () => {
  it("Klick auf den Titel bearbeitet ihn, Enter setzt den Frontmatter-Titel", async () => {
    const { view, setTitle } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-title.editable")!.click();
    const input = document.querySelector<HTMLInputElement>(".wiki-note-title-input")!;
    expect(input.value).toBe("WURZEL-DOC");
    input.value = "Neuer Titel";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(setTitle).toHaveBeenCalledWith("id-wurzel-doc", "Neuer Titel");
  });

  it("Escape verwirft die Titel-Bearbeitung", async () => {
    const { view, setTitle } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-title.editable")!.click();
    const input = document.querySelector<HTMLInputElement>(".wiki-note-title-input")!;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("WURZEL-DOC");
    expect(setTitle).not.toHaveBeenCalled();
  });

  it("Löschen meldet mit relpath", async () => {
    const { view, actions } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree-doc")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-actions .cmd-del")!.click();
    expect(actions.remove).toHaveBeenCalledWith("id-wurzel-doc");
  });

  it("Plus im Notiz-Kopf: Formular legt Dokument im gewählten Ordner an", () => {
    const { view, actions } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree summary .wiki-tree-name")!.click();
    document.querySelector<HTMLElement>(".wiki-note-actions .wiki-add")!.click();
    const form = document.querySelector<HTMLElement>(".wiki-form")!;
    expect(form.querySelector(".wiki-form-title")!.textContent).toBe("Neues Dokument");
    const input = form.querySelector<HTMLInputElement>("input")!;
    input.value = "Deploy Notiz";
    form.querySelector<HTMLElement>(".wiki-form-submit")!.click();
    expect(actions.createDoc).toHaveBeenCalledWith("id-konzepte", "Deploy Notiz");
    expect(document.querySelector(".wiki-form")).toBeNull();
  });

  it("Abbrechen und Escape schließen das Formular ohne Aktion", () => {
    const { view, actions } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-note-actions .wiki-add")!.click();
    document.querySelector<HTMLElement>(".wiki-form-cancel")!.click();
    expect(document.querySelector(".wiki-form")).toBeNull();

    document.querySelector<HTMLElement>(".wiki-note-actions .wiki-add")!.click();
    document
      .querySelector<HTMLElement>(".wiki-form")!
      .dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(document.querySelector(".wiki-form")).toBeNull();
    expect(actions.createDoc).not.toHaveBeenCalled();
  });



  it("Rechtsklick auf Knoten: Menü legt darunter an (Eltern-ID)", () => {
    const { view, actions } = setup();
    view.set(mergedPage);
    document
      .querySelector<HTMLElement>(".wiki-tree summary")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const items = [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")];
    items.find((i) => i.textContent === "Neuer Ordner")!.click();
    const input = document.querySelector<HTMLInputElement>(".wiki-form input")!;
    input.value = "neu";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(actions.createFolder).toHaveBeenCalledWith("id-konzepte", "neu");
  });

  it("Rechtsklick auf Dokument: Menü löscht mit ID", () => {
    const { view, actions } = setup();
    view.set(page);
    document
      .querySelector<HTMLElement>(".wiki-tree-doc")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const items = [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")];
    items.find((i) => i.textContent === "Dokument löschen")!.click();
    expect(actions.remove).toHaveBeenCalledWith("id-wurzel-doc");
    expect(document.querySelector(".wiki-menu")).toBeNull();
  });
});

describe("initWikiView — Leerzustände", () => {
  it("leeres Archiv erklärt das Archivieren", () => {
    const { view } = setup();
    view.set(
      JSON.stringify({
        kind: "page",
        home: "/a",
        tag: null,
        total: 0,
        tags: [],
        folders: [],
      }),
    );
    expect(document.querySelector(".wiki-empty")!.textContent).toContain(
      "Das Archiv ist leer.",
    );
  });

  it("leerer Puffer fordert die Übersicht von selbst an", () => {
    const { view, openWiki } = setup();
    view.set("");
    expect(view.empty()).toBe(true);
    expect(openWiki).toHaveBeenCalledWith("tag:");
    // Nur einmal — das Update kommt über den Puffer zurück.
    view.set("");
    expect(openWiki).toHaveBeenCalledTimes(1);
    view.set(page);
    expect(view.empty()).toBe(false);
  });
});
