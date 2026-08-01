// Archiv-Ansicht: Baum mit Notizen (Ordner + Dokumente, Verschmelzung
// gleichnamiger Paare), Notiz-Ansicht rechts (Titel, Inhalt, Kindliste),
// Aktionen (bearbeiten, umbenennen, löschen, anlegen), Suchtreffer-Vormerkung,
// Leerzustände.
import { describe, expect, it, vi } from "vitest";
import type { EpubBook } from "./epub-view";
import { drawioLeer, initWikiView } from "./wiki-view";

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

function setup(pending: string | null = null, drawio = false, marks: string[] = []) {
  document.body.innerHTML = `<div id="w"></div>`;
  const readDoc = vi.fn(() => Promise.resolve("# Inhalt\n\nText mit [[neu]]."));
  const readFile = vi.fn(() => Promise.resolve('{"a": 1}'));
  const writeDoc = vi.fn(() => Promise.resolve());
  const writeFile = vi.fn(() => Promise.resolve());
  const openEpub = vi.fn(() => Promise.resolve(BUCH));
  const openDrawio = vi.fn();
  const setTitle = vi.fn();
  const openWiki = vi.fn();
  const actions = {
    remove: vi.fn(),
    createFolder: vi.fn(),
    createDoc: vi.fn(() => Promise.resolve("id-neu-doc")),
    createHtml: vi.fn(() => Promise.resolve("id-neu-html")),
    createDrawio: vi.fn(() => Promise.resolve("skizze.drawio")),
    createText: vi.fn(() => Promise.resolve("path:daten.json")),
    importFiles: vi.fn(),
    reveal: vi.fn(),
    removeFolder: vi.fn(),
  };
  let pendingSelect = pending;
  const view = initWikiView(document.getElementById("w")!, {
    autoStart: true,
    readDoc,
    readFile,
    writeDoc,
    writeFile,
    openEpub,
    drawioAvailable: () => drawio,
    openDrawio,
    setTitle,
    openWiki,
    takePending: () => {
      const p = pendingSelect;
      pendingSelect = null;
      return p;
    },
    takeMarks: () => marks,
    actions,
  });
  return {
    view, readDoc, readFile, writeDoc, writeFile, openEpub, openDrawio, setTitle, openWiki, actions,
  };
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
      ],
    },
    {
      name: "konzepte",
      docs: [
        // Knotentext IM Ordner — jeder Knoten hat einen.
        doc("konzepte/index.md", "index", { id: "id-konzepte", title: "konzepte" }),
        doc("konzepte/2026-07-19_1005-neu.md", "neu"),
      ],
    },
    {
      name: "konzepte/panel",
      docs: [
        doc("konzepte/panel/index.md", "index", { id: "id-panel", title: "panel" }),
        doc("konzepte/panel/2026-07-19_1010-alt.md", "alt"),
      ],
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
    { name: "", docs: [] },
    {
      name: "konzepte",
      docs: [
        doc("konzepte/index.md", "index", { id: "id-konzepte", title: "Konzepte" }),
        doc("konzepte/2026-07-19_1005-neu.md", "neu"),
      ],
    },
  ],
});

const flush = () => new Promise((r) => setTimeout(r));

/// Eintrag des offenen Kontext-/Anlege-Menüs mit dieser Beschriftung.
const menuePunkt = (label: string) =>
  [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")].find(
    (b) => b.textContent === label,
  )!;

describe("initWikiView — Baum", () => {
  it("Baum zeigt nur Ordner, Wurzel ist gewählt", () => {
    const { view } = setup();
    view.set(page);
    expect(document.querySelector(".wiki-tree-root")!.className).toContain("active");
    const rows = [...document.querySelectorAll(".wiki-tree-children .wiki-tree-name")].map(
      (e) => e.textContent,
    );
    // Nur die Ordnerstruktur, sortiert — Dokumente stehen in der Übersicht.
    expect(rows).toEqual(["konzepte", "panel"]);
    // Wurzel-Ansicht rechts: Titel + Kindzähler.
    // Die Wurzel ohne eigene Notiz trägt keinen Titel — der Ordnername steht im Baum.
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("");
    expect(document.querySelector(".wiki-children-caption")!.textContent).toBe(
      "2 Dokumente",
    );
  });

  it("gleichnamiges Dokument verschmilzt mit dem Ordner zur Notiz", async () => {
    const { view, readDoc } = setup();
    view.set(mergedPage);
    const rows = [...document.querySelectorAll(".wiki-tree-children .wiki-tree-name")].map(
      (e) => e.textContent,
    );
    // Kein eigenes Blatt für konzepte.md — der Ordnerknoten trägt den Titel.
    expect(rows).toEqual(["Konzepte"]);
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
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click(); // WURZEL-DOC
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
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click(); // WURZEL-DOC
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-body a.wiki")!.click();
    expect(openWiki).not.toHaveBeenCalled();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("NEU");
  });

  it("Stift schaltet in den Editor, Speichern schreibt zurück", async () => {
    const { view, writeDoc } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-actions .panel-btn")!.click();
    await flush();
    // Rohtext links (CodeMirror), gerenderte Vorschau rechts.
    const roh = document.querySelector(".wiki-note-editor .cm-content")!;
    expect(roh.textContent).toContain("# Inhalt");
    expect(document.querySelector(".wiki-edit-preview h1")!.textContent).toBe("Inhalt");
    document.querySelector<HTMLElement>(".wiki-form-submit")!.click();
    await flush();
    expect(writeDoc).toHaveBeenCalledWith("id-wurzel-doc", "# Inhalt\n\nText mit [[neu]].");
    // Nach dem Speichern wieder die Anzeige.
    expect(document.querySelector(".wiki-note-editor")).toBeNull();
    expect(document.querySelector(".wiki-note-body")).not.toBeNull();
  });

  it("Abbrechen verwirft und zeigt wieder an", async () => {
    const { view, writeDoc } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
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
    // Die Wurzel ohne eigene Notiz trägt keinen Titel — der Ordnername steht im Baum.
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("");
  });

  it("vorgemerkter Suchtreffer wählt die Notiz beim set() aus", async () => {
    const { view, readDoc } = setup("id-neu");
    view.set(page);
    await flush();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("NEU");
    expect(readDoc).toHaveBeenCalledWith("id-neu");
    // Ohne Wörter aus dem Treffer bleibt der Text unmarkiert.
    expect(document.querySelector(".wiki-note-body mark")).toBeNull();
  });

  /// Der Sprung aus der Suche bringt die gefundenen Wörter mit; die Notiz
  /// hebt sie hervor, sobald ihr Text geladen ist.
  it("Suchtreffer markiert die Fundstellen in der Notiz", async () => {
    const { view } = setup("id-neu", false, ["Inhalt"]);
    view.set(page);
    await flush();
    const marken = document.querySelectorAll(".wiki-note-body mark.wiki-hit");
    expect(marken).toHaveLength(1);
    expect(marken[0].textContent).toBe("Inhalt");
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
      readFile: vi.fn(() => Promise.resolve("")),
      writeDoc: vi.fn(() => Promise.resolve()),
      writeFile: vi.fn(() => Promise.resolve()),
      openEpub: vi.fn(() => Promise.resolve(BUCH)),
      drawioAvailable: () => false,
      openDrawio: vi.fn(),
      setTitle: vi.fn(),
      openWiki: vi.fn(),
      actions: {
        remove: vi.fn(),
        createFolder: vi.fn(),
        createDoc: vi.fn(() => Promise.resolve("")),
        createHtml: vi.fn(() => Promise.resolve("")),
        createDrawio: vi.fn(() => Promise.resolve("")),
        createText: vi.fn(() => Promise.resolve("")),
        importFiles: vi.fn(),
        reveal: vi.fn(),
        removeFolder: vi.fn(),
      },
    });
    view.set(htmlPage);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
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
    // Kontextmenü der Übersichts-Zeile: für Bücher nur Löschen.
    document
      .querySelector<HTMLElement>(".wiki-doc-entry")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    expect(
      [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")].map(
        (i) => i.textContent,
      ),
    ).toEqual(["Dokument löschen"]);
    document.querySelector(".wiki-menu")?.remove();
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    await flush();
    expect(openEpub).toHaveBeenCalledWith("id-tractatus");
    expect(readDoc).not.toHaveBeenCalled();
    expect(document.querySelector(".epub-frame")).not.toBeNull();
    expect(document.querySelector(".wiki-main")!.className).toContain("epub-mode");
    // Kopf: nur Löschen, kein Stift; Titel nicht klickbar.
    expect(document.querySelector(".wiki-note-actions .panel-btn")!.textContent).not.toBe("✎");
    expect(document.querySelector(".wiki-note-title")!.className).not.toContain("editable");
  });

  const dateiSeite = JSON.stringify({
    kind: "page",
    home: "/tmp/archiv",
    tag: null,
    total: 1,
    tags: [],
    folders: [{ name: "", docs: [doc("daten.json", "daten", { kind: "file" })] }],
  });

  /// Eine sonstige Datei zeigt der Datei-Viewer: JSON als faltbarer Baum,
  /// kein Bearbeiten, Titel nicht klickbar.
  it("zeigt sonstige Dateien an, JSON als faltbaren Baum", async () => {
    const { view, readFile, readDoc } = setup();
    view.set(dateiSeite);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    await flush();
    expect(readFile).toHaveBeenCalledWith("id-daten");
    expect(readDoc).not.toHaveBeenCalled();
    const baum = document.querySelector(".dt-root")!;
    expect(baum.querySelector(".dt-key")!.textContent).toBe("a");
    expect(baum.querySelector(".dt-num")!.textContent).toBe("1");
    expect(document.querySelector(".wiki-note-title")!.className).not.toContain("editable");
  });

  /// Rohdaten-Dateien (JSON, YAML, XML, Klartext) lassen sich bearbeiten —
  /// mit der Grammatik ihrer Endung und ohne Frontmatter beim Speichern.
  it("Textdatei: Stift öffnet den Editor, Speichern schreibt die Datei", async () => {
    const { view, writeFile } = setup();
    view.set(dateiSeite);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    await flush();
    const stift = [...document.querySelectorAll<HTMLElement>(".wiki-note-actions .panel-btn")].find(
      (b) => b.textContent === "✎",
    )!;
    expect(stift).not.toBeNull();
    stift.click();
    await flush();
    expect(document.querySelector(".wiki-note-editor .cm-content")!.textContent).toContain('"a"');
    document.querySelector<HTMLElement>(".wiki-form-submit")!.click();
    await flush();
    expect(writeFile).toHaveBeenCalledWith("id-daten", '{"a": 1}');
  });

  const diagrammSeite = JSON.stringify({
    kind: "page",
    home: "/tmp/archiv",
    tag: null,
    total: 1,
    tags: [],
    folders: [{ name: "", docs: [doc("skizze.drawio", "skizze", { kind: "file" })] }],
  });

  /// Mit installierter draw.io-Desktop-App trägt das Diagramm den
  /// Editier-Knopf; der Klick öffnet die Datei dort.
  it("Diagramm-Knopf öffnet draw.io, wenn installiert", () => {
    const { view, openDrawio } = setup(null, true);
    view.set(diagrammSeite);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    const knopf = document.querySelector<HTMLElement>(".wiki-drawio-edit")!;
    expect(knopf).not.toBeNull();
    knopf.click();
    expect(openDrawio).toHaveBeenCalledWith("id-skizze");
    expect(document.querySelector(".wiki-note-drawio")).not.toBeNull();
  });

  /// Der Leer-Test entscheidet, ob der Viewer läuft oder der Platzhalter
  /// „leeres Diagramm" steht: gezeichnete Dateien müssen durch, auch die
  /// komprimiert gespeicherten.
  it("erkennt gefüllte Diagramme, auch komprimiert gespeicherte", () => {
    const leer =
      '<mxfile><diagram id="d1" name="Seite-1"><mxGraphModel><root>' +
      '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
      "</root></mxGraphModel></diagram></mxfile>";
    const gezeichnet =
      '<mxfile host="Electron"><diagram id="d1" name="Seite-1"><mxGraphModel dx="800" dy="600"><root>' +
      '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
      '<mxCell id="2" value="A" style="rounded=0;" vertex="1" parent="1">' +
      '<mxGeometry x="360" y="200" width="120" height="60" as="geometry"/></mxCell>' +
      "</root></mxGraphModel></diagram></mxfile>";
    const gepackt =
      '<mxfile host="Electron"><diagram id="d1" name="Seite-1">' +
      "7VpNc+I4EP01HJPCBpMcE0IyszXZTS2ZmtkjWA3WRJa8kkxgfv22bBmMTQiZzWx2" +
      "</diagram></mxfile>";
    expect(drawioLeer(leer)).toBe(true);
    expect(drawioLeer(gezeichnet)).toBe(false);
    expect(drawioLeer(gepackt)).toBe(false);
  });

  it("ohne draw.io-Installation fehlt der Editier-Knopf", () => {
    const { view } = setup();
    view.set(diagrammSeite);
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    expect(document.querySelector(".wiki-drawio-edit")).toBeNull();
  });

  it("Kontextmenü bietet den Datei-Import unter dem Knoten an", () => {
    const { view, actions } = setup();
    view.set(mergedPage);
    document
      .querySelector<HTMLElement>(".wiki-tree summary")!
      .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const items = [...document.querySelectorAll<HTMLElement>(".wiki-menu-item")];
    items.find((i) => i.textContent === "Dateien hinzufügen …")!.click();
    expect(actions.importFiles).toHaveBeenCalledWith("id-konzepte");
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
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click(); // WURZEL-DOC
    await flush();
    document.querySelector<HTMLElement>(".wiki-tree summary .wiki-tree-name")!.click(); // konzepte
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("konzepte");
    document.querySelector<HTMLElement>(".wiki-note-back")!.click();
    await flush();
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("WURZEL-DOC");
    document.querySelector<HTMLElement>(".wiki-note-back")!.click();
    // Die Wurzel ohne eigene Notiz trägt keinen Titel — der Ordnername steht im Baum.
    expect(document.querySelector(".wiki-note-title")!.textContent).toBe("");
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
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
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
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
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
    document.querySelector<HTMLElement>(".wiki-doc-entry")!.click();
    await flush();
    document.querySelector<HTMLElement>(".wiki-note-actions .cmd-del")!.click();
    expect(actions.remove).toHaveBeenCalledWith("id-wurzel-doc");
  });

  it("Plus im Notiz-Kopf: Formular legt Dokument im gewählten Ordner an", () => {
    const { view, actions } = setup();
    view.set(page);
    document.querySelector<HTMLElement>(".wiki-tree summary .wiki-tree-name")!.click();
    // Das Plus öffnet ein Menü (Ordner, Dokument, HTML-Notiz, Dateien).
    document.querySelector<HTMLElement>(".wiki-note-actions .wiki-add")!.click();
    menuePunkt("Neues Dokument").click();
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
    menuePunkt("Neues Dokument").click();
    document.querySelector<HTMLElement>(".wiki-form-cancel")!.click();
    expect(document.querySelector(".wiki-form")).toBeNull();

    document.querySelector<HTMLElement>(".wiki-note-actions .wiki-add")!.click();
    menuePunkt("Neues Dokument").click();
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
      .querySelector<HTMLElement>(".wiki-doc-entry")!
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
