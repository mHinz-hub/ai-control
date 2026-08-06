/// Archiv-Ansicht des Panels, am Trilium-Layout orientiert: links ein Baum,
/// in dem die Notizen selbst hängen (Ordner = Notiz mit Kindern — ein Ordner
/// mit gleichnamigem Dokument daneben ist EINE Notiz mit Inhalt und Kindern),
/// rechts die Notiz-Ansicht: großer Titel mit Aktionen (bearbeiten,
/// umbenennen, löschen), darunter der gerenderte Inhalt bzw. für Notizen ohne
/// Inhalt die Kindliste. Titel, Beschreibungen und Pfade sind Fremdtext und
/// gehen nie durch innerHTML; der Markdown-Body läuft durch renderMarkdown.

import { load as yamlLoad } from "js-yaml";
import drawioViewerUrl from "./assets/drawio-viewer.min.js?url";
import { dataTree, xmlTree } from "./data-tree";
import { renderEpub, type EpubBook } from "./epub-view";
import { initHtmlEditor } from "./html-editor";
import { markiere } from "./highlight";
import { renderMarkdown } from "./markdown";
import {
  initMdEditor,
  mdTabelle,
  spracheZu,
  type MdEditor,
  type Sprache,
} from "./md-editor";
import { t } from "./messages";
import { linkWikiRefs } from "./panel-view";
import { openTableForm } from "./table-form";
import { deleteAction } from "./tiles";

interface DocEntry {
  /// Technische ID — bleibt über Umbenennen und Verschieben gleich; alle
  /// Aktionen und die Auswahl laufen darüber. Der Pfad ist nur Eigenschaft.
  id: string;
  relpath: string;
  name: string;
  title: string;
  description?: string | null;
  tags: string[];
  date?: string | null;
  backlinks: number;
  /// Letzte Änderung (Datei-mtime) als voller ISO-Zeitstempel
  /// (`YYYY-MM-DDTHH:MM:SSZ`) — Anzeige kürzt, Sortierung nutzt ihn ganz.
  modified: string;
  /// Notiz-Typ: `md` (Markdown), `html`, `epub` (Buch — Ansicht statt
  /// Editor) oder `file` (sonstige Datei — Rohtext-Ansicht).
  kind: "md" | "html" | "epub" | "file";
}

interface Folder {
  name: string;
  docs: DocEntry[];
}

interface Page {
  kind: "page";
  home: string;
  tag?: string | null;
  total: number;
  tags: { name: string; count: number }[];
  folders: Folder[];
}

/// Dokument-/Ordner-Operationen — laufen als Tauri-Commands, der neue Stand
/// kommt über den Archiv-Puffer zurück.
export interface ArchiveActions {
  remove(id: string): void;
  /// Neues Kind unterhalb des Knotens (ID; "" = Wurzel).
  createFolder(parent: string, name: string): void;
  /// Liefern die ID der neuen Notiz — die Ansicht öffnet sie im Editor.
  createDoc(parent: string, name: string): Promise<string>;
  createHtml(parent: string, name: string): Promise<string>;
  /// Datei-Dialog öffnen und die Auswahl in den Ordner des Knotens kopieren.
  importFiles(parent: string): void;
  /// Datei im Dateimanager des Systems zeigen (absoluter Pfad).
  reveal(path: string): void;
  /// Ordner samt Inhalt löschen (Pfad relativ zum Archiv-Home).
  removeFolder(path: string): void;
  /// Leeres Diagramm neben der Notiz anlegen; liefert den relpath.
  createDrawio(near: string, name: string): Promise<string>;
  /// Rohdaten-Datei anlegen (Klartext, JSON, YAML, XML); liefert ihre
  /// Pfad-Adresse (`path:<relpath>`).
  createText(parent: string, name: string, art: string): Promise<string>;
}

export interface ArchiveCallbacks {
  /// Leeren Puffer sofort mit der Übersicht füllen (eigenes Fenster).
  autoStart?: boolean;
  /// Body eines Archiv-Dokuments (ohne Frontmatter) für die Notiz-Ansicht.
  readDoc(id: string): Promise<string>;
  /// Rohtext einer sonstigen Archiv-Datei (`file`-Knoten).
  readFile(id: string): Promise<string>;
  /// Ist die draw.io-Desktop-App installiert? (Beim Start geprüft.)
  drawioAvailable(): boolean;
  /// `.drawio`-Datei in der draw.io-Desktop-App öffnen.
  openDrawio(id: string): void;
  /// Body einer Archiv-Notiz zurückschreiben (Bearbeiten im Archiv).
  writeDoc(id: string, text: string): Promise<void>;
  /// Rohdaten-Datei zurückschreiben — ohne Frontmatter, der Inhalt ist die
  /// Datei.
  writeFile(id: string, text: string): Promise<void>;
  /// Buch öffnen: entpacken und Lesereihenfolge, Inhaltsverzeichnis und
  /// Metadaten aus seinen Verwaltungsdateien holen.
  openEpub(id: string): Promise<EpubBook>;
  /// Bild als `data:`-Adresse — Vorschau in der Liste und Inhalt der Ansicht.
  readImage(id: string): Promise<string>;
  /// Bild in einem eigenen Fenster öffnen.
  openImage(id: string): void;
  /// Anzeige-Titel einer Notiz setzen (Klick auf den Titel).
  setTitle(id: string, title: string): void;
  /// Archiv-Ziel laden (`tag:` = Übersicht in den Puffer, Einstiegs-Chip).
  openArchive(name: string): void;
  /// Vorgemerkte Auswahl (Suchtreffer-Sprung): einmalig abholen.
  takePending?(): string | null;
  /// Wörter des Suchtreffers — sie werden im geöffneten Dokument markiert.
  takeMarks?(): string[];
  /// Kapitel des Treffers (Buch) — leer bei allem anderen.
  takePart?(): string;
  /// Nummer der gemeinten Fundstelle; 0 = die erste.
  takeSpot?(): number;
  actions: ArchiveActions;
}

export interface ArchiveView {
  set(text: string): void;
  /// Noch keine Seite im Puffer (Session-Start)?
  empty(): boolean;
}

/// Baum-Knoten: Unterordner als Kinder, Dokumente als Blätter; `content` ist
/// das mit dem Ordner verschmolzene gleichnamige Dokument (Notiz-Inhalt).
interface TreeNode {
  children: Map<string, TreeNode>;
  docs: DocEntry[];
  content?: DocEntry;
  /// Voller Ordnerpfad relativ zum Archiv-Home ("" = Wurzel).
  path: string;
}

type SortFeld = "name" | "changed" | "created";

/// Beschriftungen der Sortierfelder.
const SORTFELDER: Record<SortFeld, string> = {
  name: "archive.sortName",
  changed: "archive.sortChanged",
  created: "archive.sortCreated",
};

/// JS-Seite der slugify-Semantik aus archive.rs — für die lokale Auflösung
/// von Wikilinks gegen Name/Titel/Stem.
function slugify(s: string): string {
  let out = "";
  for (const c of s) {
    if (c === "ä" || c === "Ä") out += "ae";
    else if (c === "ö" || c === "Ö") out += "oe";
    else if (c === "ü" || c === "Ü") out += "ue";
    else if (c === "ß") out += "ss";
    else if (/[a-zA-Z0-9]/.test(c)) out += c.toLowerCase();
    else if (!out.endsWith("-")) out += "-";
  }
  return out.replace(/^-+|-+$/g, "").slice(0, 60);
}

function stem(relpath: string): string {
  const base = relpath.split("/").pop()!;
  return base.replace(/\.md$/, "");
}

/// Eine draw.io-Datei unter den sonstigen Dateien — bekommt den
/// Diagramm-Viewer statt der Rohtext-Ansicht.
function istDrawio(doc: { relpath: string }): boolean {
  return doc.relpath.endsWith(".drawio");
}

/// Ein Diagramm ohne Figuren (frisch angelegt) zeichnet nichts — es braucht
/// einen sichtbaren Platzhalter statt einer leeren Stelle. Gezählt werden nur
/// Figuren und Kanten; die beiden Wurzelzellen (`0`, `1`) stehen in jeder
/// Datei.
///
/// Speichert draw.io komprimiert, steht im `<diagram>` statt des Modells ein
/// Base64-Deflate-Text; auspacken kann ihn allein der Viewer, eine solche Datei
/// gilt hier also als gefüllt. Der frühere Test zählte `<mxCell`-Vorkommen im
/// Rohtext und hielt jedes komprimiert gespeicherte Diagramm für leer.
export function drawioLeer(xml: string): boolean {
  const dom = new DOMParser().parseFromString(xml, "application/xml");
  if (dom.querySelector("parsererror")) return false;
  for (const d of dom.querySelectorAll("diagram")) {
    if (!d.firstElementChild && d.textContent?.trim()) return false;
  }
  return dom.querySelectorAll("mxCell[vertex], mxCell[edge]").length === 0;
}

function buildTree(p: Page): TreeNode {
  const root: TreeNode = { children: new Map(), docs: [], path: "" };
  const node = (path: string): TreeNode => {
    let n = root;
    if (!path) return n;
    for (const part of path.split("/")) {
      if (!n.children.has(part)) {
        n.children.set(part, {
          children: new Map(),
          docs: [],
          path: n.path ? `${n.path}/${part}` : part,
        });
      }
      n = n.children.get(part)!;
    }
    return n;
  };
  for (const folder of p.folders) {
    if (folder.name) node(folder.name);
  }
  // Die index-Notiz eines Ordners ist sein Knotentext — für die Wurzel wie
  // für jeden Unterordner —, kein eigenes Blatt in der Übersicht.
  for (const folder of p.folders) {
    const parent = node(folder.name);
    for (const doc of folder.docs) {
      if (doc.name === "index" && !parent.content) parent.content = doc;
      else parent.docs.push(doc);
    }
  }
  return root;
}

export function initArchiveView(container: HTMLElement, cb: ArchiveCallbacks): ArchiveView {
  /// Auswahl über die technische ID der Notiz ("" = Archiv-Wurzel);
  /// übersteht Umbenennen, Verschieben und Puffer-Updates. Der Baum kennt
  /// keinen eigenen Klapp-Zustand: offen ist genau der Pfad zur Auswahl.
  let selected = "";
  let current: Page | null = null;
  let tree: TreeNode = { children: new Map(), docs: [], path: "" };
  /// Auswahl-Verlauf für den Zurück-Knopf der Notiz-Ansicht.
  const history: string[] = [];
  /// Bearbeitungsmodus der aktuellen Notiz (Editor statt Anzeige).
  let editing = false;
  /// Vorschau des offenen Markdown-Editors neu zeichnen. Ein Puffer-Update
  /// während der Bearbeitung geht nur hierüber: Der Editortext bleibt stehen,
  /// aber die eingebetteten Diagramme werden neu gelesen — genau das braucht
  /// der Ablauf „Diagramm einfügen, in draw.io zeichnen, speichern".
  let previewRedraw: (() => void) | null = null;
  /// Eben angelegte Notiz: sobald sie in der Übersicht auftaucht, wird sie
  /// ausgewählt und der Editor geöffnet.
  let pendingEdit: string | null = null;

  /// Neue Notiz übernehmen — je nachdem, ob die frische Übersicht schon da
  /// ist, sofort oder beim nächsten Puffer-Update.
  function openNew(id: string) {
    pendingEdit = id;
    if (findDoc(id)) applyPendingEdit();
  }

  function applyPendingEdit() {
    if (!pendingEdit || !findDoc(pendingEdit)) return;
    const id = pendingEdit;
    pendingEdit = null;
    select(id);
    editing = true;
    renderMain();
  }

  /// Ist die ID ein Blatt (Notiz ohne Kinder)?
  const isLeaf = (id: string) => !!id && !nodeById(id);

  /// Notiz zur ID (Blatt oder Knotentext).
  function findDoc(id: string): DocEntry | null {
    if (!current || !id) return null;
    for (const f of current.folders) {
      const hit = f.docs.find((d) => d.id === id);
      if (hit) return hit;
    }
    return null;
  }

  /// Baumknoten zur ID seines Knotentexts ("" = Wurzel).
  function nodeById(id: string, from: TreeNode = tree): TreeNode | null {
    if (!id) return tree;
    if (from.content?.id === id) return from;
    for (const child of from.children.values()) {
      const hit = nodeById(id, child);
      if (hit) return hit;
    }
    return null;
  }

  /// Ordner-Kette zu einer ID (wurzelnah zuerst, ohne Wurzel; der Ordner der
  /// ID selbst ist das letzte Glied).
  function ancestors(id: string, from: TreeNode, acc: TreeNode[] = []): TreeNode[] | null {
    if (from.content?.id === id || from.docs.some((d) => d.id === id)) return acc;
    for (const child of from.children.values()) {
      const hit = ancestors(id, child, [...acc, child]);
      if (hit) return hit;
    }
    return null;
  }

  /// Knoten zu einem Ordnerpfad im aktuellen Baum.
  function nodeByPath(path: string): TreeNode | null {
    let n = tree;
    if (!path) return n;
    for (const part of path.split("/")) {
      const next = n.children.get(part);
      if (!next) return null;
      n = next;
    }
    return n;
  }

  /// Elternknoten einer Notiz — Ziel beim Anlegen von Kindern.
  /// Wikilink lokal auflösen (Name, Titel, Stem — Slug-Vergleich wie im
  /// Backend); ohne Treffer übernimmt das Backend (Dokument-Tab).
  function followWikiLink(name: string) {
    const want = slugify(name);
    for (const f of current?.folders ?? []) {
      for (const d of f.docs) {
        if ([d.name, d.title, stem(d.relpath)].some((s) => slugify(s) === want)) {
          select(d.id);
          return;
        }
      }
    }
    cb.openArchive(name);
  }

  // ---------- Kontextmenü (rechte Maustaste im Baum) ----------

  /// Eigenes Kontextmenü an der Mausposition; Klick daneben oder Escape
  /// schließt. Das Standard-Menü des Webviews ist im ganzen Archiv aus.
  function openMenu(x: number, y: number, items: { label: string; run(): void }[]) {
    container.querySelector(".archive-menu")?.remove();
    const menu = document.createElement("div");
    menu.className = "archive-menu";
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    for (const item of items) {
      const btn = document.createElement("button");
      btn.className = "archive-menu-item";
      btn.textContent = item.label;
      btn.addEventListener("click", () => {
        menu.remove();
        item.run();
      });
      menu.append(btn);
    }
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      // Mousedown auf einem Menüpunkt: erst der Klick führt aus — das Menü
      // räumt der Punkt selbst weg.
      if (e instanceof MouseEvent && menu.contains(e.target as Node)) return;
      menu.remove();
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
    setTimeout(() => {
      document.addEventListener("mousedown", close);
      document.addEventListener("keydown", close);
    });
    container.append(menu);
  }

  /// Sortier-Menü: eine Zeile je Feld, Reihenfolge der Zeilen = Rangfolge
  /// der Sortierung (oben zuerst, bei Gleichstand die nächste). Ein Klick auf
  /// ↑/↓ setzt die Richtung UND hebt die Zeile an die Spitze — ohne Ziffern,
  /// die Liste selbst ist die Aussage. Das Menü bleibt offen, damit sich die
  /// Staffelung in einem Zug legen lässt.
  function openSortMenu(x: number, y: number, onChange: () => void) {
    container.querySelector(".archive-menu")?.remove();
    const menu = document.createElement("div");
    menu.className = "archive-menu archive-sort-menu";
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    const zeichnen = () => {
      menu.textContent = "";
      sortOrder.forEach(([feld, richtung], rang) => {
        const zeile = document.createElement("div");
        zeile.className = "archive-sort-row" + (rang === 0 ? " first" : "");
        const name = document.createElement("span");
        name.className = "archive-sort-name";
        name.textContent = t(SORTFELDER[feld]);
        const pfeile = document.createElement("div");
        pfeile.className = "archive-sort-dirs";
        for (const dir of ["asc", "desc"] as const) {
          const b = document.createElement("button");
          b.className =
            "archive-sort-dir" + (richtung === dir && rang === 0 ? " active" : "");
          b.textContent = dir === "asc" ? "↑" : "↓";
          b.title = t(dir === "asc" ? "archive.sortAsc" : "archive.sortDesc");
          b.addEventListener("click", () => {
            sortOrder = [
              [feld, dir],
              ...sortOrder.filter(([f]) => f !== feld),
            ];
            zeichnen();
            onChange();
          });
          pfeile.append(b);
        }
        zeile.append(name, pfeile);
        menu.append(zeile);
      });
    };
    zeichnen();
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      if (e instanceof MouseEvent && menu.contains(e.target as Node)) return;
      menu.remove();
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
    setTimeout(() => {
      document.addEventListener("mousedown", close);
      document.addEventListener("keydown", close);
    });
    container.append(menu);
  }

  // ---------- Baum-Spalte ----------

  /// Typ-Symbol (Stroke-SVG wie die übrigen App-Icons): Ordner für Knoten,
  /// je ein Symbol pro Dateityp für die Übersicht.
  function typeIcon(
    kind: "node" | "nodeOpen" | "doc" | "html" | "epub" | "file" | "diagram" | "image",
  ): HTMLElement {
    const span = document.createElement("span");
    span.className = "archive-tree-icon";
    const shapes = {
      // Geschlossener Ordner mit Reiter.
      node: `<path d="M1.8 4.6a1 1 0 0 1 1-1h3.1l1.5 1.7h5.8a1 1 0 0 1 1 1v6.1a1 1 0 0 1-1 1H2.8a1 1 0 0 1-1-1z"/>`,
      // Offener Ordner: Rückwand plus aufgeklappte Front.
      nodeOpen: `<path d="M1.8 12.4V4.6a1 1 0 0 1 1-1h3.1l1.5 1.7h5.2v1.7"/><path d="M3.9 7.9h10.3l-1.7 4.8a1 1 0 0 1-.9.7H1.8z"/>`,
      doc: `<path d="M4 1.5h5.5L12.5 5v9a.9.9 0 0 1-.9.9H4a.9.9 0 0 1-.9-.9V2.4a.9.9 0 0 1 .9-.9z"/><path d="M9.5 1.5V5H13"/><path d="M5.8 8h4.4M5.8 10.5h4.4"/>`,
      // HTML-Notiz: dasselbe Blatt, spitze Klammern statt Textzeilen.
      html: `<path d="M4 1.5h5.5L12.5 5v9a.9.9 0 0 1-.9.9H4a.9.9 0 0 1-.9-.9V2.4a.9.9 0 0 1 .9-.9z"/><path d="M9.5 1.5V5H13"/><path d="M6.6 8.2 5.2 9.6l1.4 1.4M9.4 8.2l1.4 1.4-1.4 1.4"/>`,
      // Buch: aufgeschlagene Doppelseite.
      epub: `<path d="M8 4.2C6.8 3.2 5.1 2.8 3 2.8v9.4c2.1 0 3.8.4 5 1.4 1.2-1 2.9-1.4 5-1.4V2.8c-2.1 0-3.8.4-5 1.4z"/><path d="M8 4.2v9.4"/>`,
      // Sonstige Datei: dasselbe Blatt, geschweifte Klammern.
      file: `<path d="M4 1.5h5.5L12.5 5v9a.9.9 0 0 1-.9.9H4a.9.9 0 0 1-.9-.9V2.4a.9.9 0 0 1 .9-.9z"/><path d="M9.5 1.5V5H13"/><path d="M6.8 7.6c-1 0-.4 1.6-1.5 1.6 1.1 0 .5 1.6 1.5 1.6M9.2 7.6c1 0 .4 1.6 1.5 1.6-1.1 0-.5 1.6-1.5 1.6"/>`,
      // Diagramm: zwei Kästen mit Verbinder.
      diagram: `<rect x="1.8" y="2.2" width="5.4" height="3.6" rx="0.8"/><rect x="8.8" y="10.2" width="5.4" height="3.6" rx="0.8"/><path d="M4.5 5.8v3.4a1.6 1.6 0 0 0 1.6 1.6h5.4"/>`,
      // Bild: Rahmen mit Sonne und Bergzug.
      image: `<rect x="2" y="2.6" width="12" height="10.8" rx="1"/><circle cx="5.6" cy="6" r="1.1"/><path d="M2.6 12.2l3.4-3.4 2.4 2.4 3.1-3.1 1.9 1.9"/>`,
    };
    // Ohne width/height: die Größe kommt aus dem Stylesheet (Regel 1.4).
    span.innerHTML = `<svg viewBox="0 0 16 16">${shapes[kind]}</svg>`;
    return span;
  }

  const BILDENDUNGEN = ["png", "jpg", "jpeg", "gif", "svg", "webp", "avif", "bmp"];

  /// Ist die Datei ein Bild? Entscheidet über Vorschau, Ansicht und Fenster.
  function istBild(doc: DocEntry): boolean {
    if (doc.kind !== "file") return false;
    const ext = doc.relpath.toLowerCase().split(".").pop() ?? "";
    return BILDENDUNGEN.includes(ext);
  }

  /// Vorschauen laden erst, wenn ihre Zeile ins Blickfeld kommt: ein Ordner
  /// mit hundert Ausschnitten soll die Übersicht nicht aufhalten.
  const vorschauSicht = new IntersectionObserver((eintraege) => {
    for (const e of eintraege) {
      if (!e.isIntersecting) continue;
      const el = e.target as HTMLImageElement;
      vorschauSicht.unobserve(el);
      cb.readImage(el.dataset.doc!).then(
        (daten) => (el.src = daten),
        () => el.classList.add("archive-tree-thumb-leer"),
      );
    }
  });

  /// Übersichts-Symbol einer Datei: Typ aus Notiz-Art bzw. Datei-Endung. Ein
  /// Bild zeigt sich selbst — bei Zeichnungen sagt das Symbol nichts, das
  /// Bildchen alles.
  function docIcon(doc: DocEntry): HTMLElement {
    if (doc.kind === "md") return typeIcon("doc");
    if (doc.kind !== "file") return typeIcon(doc.kind);
    const ext = doc.relpath.toLowerCase().split(".").pop() ?? "";
    if (ext === "drawio") return typeIcon("diagram");
    if (BILDENDUNGEN.includes(ext)) {
      const thumb = document.createElement("img");
      thumb.className = "archive-tree-thumb";
      thumb.alt = "";
      thumb.dataset.doc = doc.id;
      vorschauSicht.observe(thumb);
      return thumb;
    }
    return typeIcon("file");
  }

  /// Liegt die Auswahl in diesem Teilbaum (Knoten selbst, ein Dokument darin
  /// oder tiefer)?
  function containsSelected(node: TreeNode): boolean {
    if (node.content?.id === selected) return true;
    if (node.docs.some((d) => d.id === selected)) return true;
    return [...node.children.values()].some(containsSelected);
  }

  function folderRow(name: string, full: string, node: TreeNode): HTMLElement {
    const det = document.createElement("details");
    det.className = "archive-tree-folder";
    // Offen ist genau der Strang zur Auswahl — nie mehrere Äste zugleich.
    det.open = containsSelected(node);
    const sum = document.createElement("summary");
    sum.classList.add(node.content ? "has-content" : "book");
    if (node.content?.id === selected) {
      sum.classList.add("active");
    }
    // Die ganze Zeile wählt die Notiz aus (wie in Trilium); NUR der Pfeil
    // klappt — deshalb das summary-Default unterbinden und selbst schalten.
    sum.addEventListener("click", (e) => {
      e.preventDefault();
      select(node.content?.id ?? "");
    });
    // Das Ordner-Symbol zeigt den Zustand (offen/geschlossen); Klick wählt
    // den Ordner aus — das öffnet seinen Strang und schließt alle anderen.
    const icon = typeIcon(det.open ? "nodeOpen" : "node");
    const label = document.createElement("span");
    label.className = "archive-tree-name";
    label.textContent = node.content?.title ?? name;
    sum.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const id = node.content?.id;
      // Ohne Knotentext gibt es keine Ziel-ID — die Aktionen liefen sonst
      // kommentarlos an der Wurzel. Die frische Übersicht trägt sie nach.
      openMenu(
        e.clientX,
        e.clientY,
        id ? anlegeMenue(id) : [{ label: t("archive.reload"), run: () => cb.openArchive("tag:") }],
      );
    });
    sum.append(icon, label);
    det.append(sum, renderChildren(node, full));
    return det;
  }

  /// Der Baum zeigt nur die Ordnerstruktur; Dokumente stehen ausschließlich
  /// in der Übersicht des jeweiligen Ordners.
  function renderChildren(node: TreeNode, path: string): HTMLElement {
    const box = document.createElement("div");
    box.className = "archive-tree-children";
    for (const [name, child] of [...node.children].sort((a, b) =>
      a[0].localeCompare(b[0]),
    )) {
      box.append(folderRow(name, path ? `${path}/${name}` : name, child));
    }
    return box;
  }

  function renderTree(): HTMLElement {
    const aside = document.createElement("aside");
    aside.className = "archive-tree";
    const head = document.createElement("div");
    head.className = "archive-tree-head";
    const root = document.createElement("button");
    root.className = "archive-tree-root" + (selected === "" ? " active" : "");
    // Die Wurzel trägt ihren echten Namen: den Ordnernamen des Archiv-Home —
    // wie jeder andere Ordner im Baum. Offenes Symbol: immer aufgeklappt.
    const rootLabel = document.createElement("span");
    rootLabel.className = "archive-tree-name";
    rootLabel.textContent =
      current?.home.replace(/\/+$/, "").split("/").pop() || t("archive.archive");
    root.append(typeIcon("nodeOpen"), rootLabel);
    root.addEventListener("click", () => select(""));
    root.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openMenu(e.clientX, e.clientY, anlegeMenue(""));
    });
    head.append(root);
    aside.append(head, renderChildren(tree, ""));
    return aside;
  }

  /// Bestätigungsdialog fürs Löschen: Frage plus Löschen/Abbrechen, gleiche
  /// Optik wie die Anlege-Dialoge.
  function confirmBox(text: string, onOk: () => void) {
    document.querySelector(".archive-modal")?.remove();
    const backdrop = document.createElement("div");
    backdrop.className = "archive-modal";
    const form = document.createElement("div");
    form.className = "archive-form";
    const caption = document.createElement("div");
    caption.className = "archive-form-title";
    caption.textContent = text;
    const row = document.createElement("div");
    row.className = "archive-form-row";
    const ok = document.createElement("button");
    ok.className = "archive-form-submit";
    ok.textContent = t("archive.deleteFolder");
    ok.addEventListener("click", () => {
      backdrop.remove();
      onOk();
    });
    const cancel = document.createElement("button");
    cancel.className = "archive-form-cancel";
    cancel.textContent = t("archive.cancel");
    cancel.addEventListener("click", () => backdrop.remove());
    backdrop.addEventListener("mousedown", (e) => {
      if (e.target === backdrop) backdrop.remove();
    });
    row.append(ok, cancel);
    form.append(caption, row);
    backdrop.append(form);
    container.append(backdrop);
  }

  // ---------- Anlege-Formular (Ordner/Dokument) ----------

  function newDocForm(parent: string) {
    openForm(t("archive.newDoc"), t("archive.docName"), "", (v) => {
      void cb.actions.createDoc(parent, v).then(openNew);
    });
  }

  function newHtmlForm(parent: string) {
    openForm(t("archive.newHtml"), t("archive.docName"), "", (v) => {
      void cb.actions.createHtml(parent, v).then(openNew);
    });
  }

  function newFolderForm(parent: string) {
    openForm(t("archive.newFolder"), t("archive.docName"), "", (v) =>
      cb.actions.createFolder(parent, v),
    );
  }

  /// Was sich unter einem Knoten anlegen lässt — gleiche Liste am Plus wie im
  /// Kontextmenü des Baums.
  function anlegeMenue(parent: string): { label: string; run(): void }[] {
    return [
      { label: t("archive.newFolder"), run: () => newFolderForm(parent) },
      { label: t("archive.newDoc"), run: () => newDocForm(parent) },
      { label: t("archive.newHtml"), run: () => newHtmlForm(parent) },
      { label: t("archive.new_text"), run: () => newTextForm(parent, "text") },
      { label: t("archive.new_json"), run: () => newTextForm(parent, "json") },
      { label: t("archive.new_yaml"), run: () => newTextForm(parent, "yaml") },
      { label: t("archive.new_xml"), run: () => newTextForm(parent, "xml") },
      { label: t("archive.addFiles"), run: () => cb.actions.importFiles(parent) },
    ];
  }

  /// Rohdaten-Datei: Klartext, JSON, YAML oder XML. Sie bekommt kein
  /// Frontmatter — der Inhalt ist die Datei, angesprochen wird sie über den
  /// Pfad.
  function newTextForm(parent: string, art: "text" | "json" | "yaml" | "xml") {
    openForm(t(`archive.new_${art}`), t("archive.docName"), "", (v) => {
      // Wie bei Notizen: sobald die neue Datei in der Übersicht steht, wird
      // sie ausgewählt und geöffnet.
      void cb.actions.createText(parent, v, art).then(openNew);
    });
  }

  /// Modaler Dialog mit einem Textfeld: Anlegen/Abbrechen; Enter legt an,
  /// Escape oder Klick auf den Hintergrund schließt. Leere oder ungültige
  /// Eingaben meldet das Backend als Toast.
  function openForm(
    title: string,
    placeholder: string,
    initial: string,
    onSubmit: (value: string) => void,
  ) {
    document.querySelector(".archive-modal")?.remove();
    const backdrop = document.createElement("div");
    backdrop.className = "archive-modal";
    const form = document.createElement("div");
    form.className = "archive-form";
    const caption = document.createElement("div");
    caption.className = "archive-form-title";
    caption.textContent = title;
    const input = document.createElement("input");
    input.className = "archive-tree-input";
    input.placeholder = placeholder;
    input.value = initial;
    const row = document.createElement("div");
    row.className = "archive-form-row";
    const submit = document.createElement("button");
    submit.className = "archive-form-submit";
    submit.textContent = t("archive.create");
    const cancel = document.createElement("button");
    cancel.className = "archive-form-cancel";
    cancel.textContent = t("archive.cancel");
    const fire = () => {
      const value = input.value.trim();
      backdrop.remove();
      onSubmit(value);
    };
    submit.addEventListener("click", fire);
    cancel.addEventListener("click", () => backdrop.remove());
    backdrop.addEventListener("mousedown", (e) => {
      if (e.target === backdrop) backdrop.remove();
    });
    // Escape schließt auch dann, wenn der Fokus nicht im Formular sitzt.
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      backdrop.remove();
      document.removeEventListener("keydown", esc);
    };
    document.addEventListener("keydown", esc);
    form.addEventListener("keydown", (e) => {
      if (e.key === "Enter") fire();
    });
    row.append(submit, cancel);
    form.append(caption, input, row);
    backdrop.append(form);
    container.append(backdrop);
    input.focus();
    input.select();
  }

  // ---------- Notiz-Ansicht rechts ----------

  /// Ein Schritt zurück im Auswahl-Verlauf; verschwundene Ziele werden
  /// übersprungen.
  function goBack() {
    while (history.length) {
      const prev = history.pop()!;
      const exists = !!findDoc(prev) || !!nodeById(prev);
      if (exists) {
        select(prev, false);
        return;
      }
    }
  }

  /// Kopf der Notiz-Ansicht: Zurück-Knopf, großer Titel, Meta-Zeile, Aktionen.
  function noteHead(
    title: string,
    meta: (string | HTMLElement)[],
    actions: HTMLElement[],
  ): HTMLElement {
    const head = document.createElement("div");
    head.className = "archive-note-head";
    const row = document.createElement("div");
    row.className = "archive-note-titlerow";
    const back = document.createElement("button");
    back.className = "archive-note-back";
    back.title = t("archive.back");
    back.textContent = "←";
    back.disabled = history.length === 0;
    back.addEventListener("click", goBack);
    row.append(back);
    const h = document.createElement("div");
    h.className = "archive-note-title";
    h.textContent = title;
    // Klick auf den Titel bearbeitet ihn direkt (Frontmatter-Titel der
    // Notiz); Enter übernimmt, Escape verwirft. Der technische Datei-/
    // Ordnername bleibt davon unberührt — der sitzt im Baum-Kontextmenü.
    // Der Titel eines Buchs steht in seiner Datei, der einer sonstigen Datei
    // ist ihr Name — beide werden hier nicht umgeschrieben.
    const titleDoc = findDoc(selected) ?? nodeById(selected)?.content;
    if (titleDoc && titleDoc.kind !== "epub" && titleDoc.kind !== "file" && !editing) {
      h.classList.add("editable");
      h.title = t("archive.titleEdit");
      h.addEventListener("click", () => {
        const input = document.createElement("input");
        input.className = "archive-note-title-input";
        input.value = title;
        input.addEventListener("keydown", (e) => {
          e.stopPropagation();
          if (e.key === "Enter") cb.setTitle(titleDoc.id, input.value.trim());
          else if (e.key === "Escape") input.replaceWith(h);
        });
        input.addEventListener("blur", () => input.replaceWith(h));
        h.replaceWith(input);
        input.focus();
        input.select();
      });
    }
    const acts = document.createElement("div");
    acts.className = "archive-note-actions";
    acts.append(...actions);
    row.append(h, acts);
    head.append(row);
    // Meta (Datum, Schlagwörter, Verweise) als kleines Popup am Info-Knopf
    // rechts in den Aktionen; Klick daneben schließt es.
    if (meta.length) {
      const pop = document.createElement("div");
      pop.className = "archive-note-info-pop";
      pop.hidden = true;
      const caption = document.createElement("div");
      caption.className = "archive-info-caption";
      caption.textContent = t("archive.infoCaption");
      pop.append(caption);
      for (const part of meta) {
        const line = document.createElement("div");
        line.className = "archive-info-line";
        line.append(part);
        pop.append(line);
      }
      const info = document.createElement("button");
      info.className = "panel-btn";
      info.title = t("archive.info");
      info.textContent = "ⓘ";
      const close = (e: MouseEvent) => {
        if (e.target === info) return;
        pop.hidden = true;
        document.removeEventListener("click", close);
      };
      info.addEventListener("click", () => {
        pop.hidden = !pop.hidden;
        if (!pop.hidden) {
          setTimeout(() => document.addEventListener("click", close));
        }
      });
      acts.append(info);
      row.append(pop);
    }
    return head;
  }

  /// Gerenderter Markdown-Body; lädt asynchron nach — verworfen, wenn die
  /// Auswahl inzwischen gewechselt hat. Ladefehler erscheinen im Body statt
  /// still zu verschwinden.
  /// Wörter des Suchtreffers, bis das Dokument sie zeigt. Der Sprung aus der
  /// Suche kommt vor dem Inhalt an: Erst lädt die Übersicht, dann die Notiz —
  /// markiert wird also, sobald der Text steht.
  let fundstellen: string[] = [];
  /// Für welche Notiz die Wörter gelten — beim Wechsel auf eine andere sind
  /// sie hinfällig.
  let fundstellenId = "";
  /// Kapitel des Treffers, wenn er aus einem Buch stammt.
  let fundstellenTeil = "";
  /// Gemeinte Fundstelle innerhalb des Dokuments (0 = die erste).
  let fundstelleNr = 0;

  /// Die markierten Stellen der offenen Anzeige — daraus bestimmt der
  /// Wechsel in den Editor, welche Fundstelle gemeint war.
  let marken: HTMLElement[] = [];

  /// Fundstellen hervorheben und zur ersten scrollen. Die Wörter bleiben
  /// stehen: Der Editor braucht sie beim Umschalten noch.
  function zeigeFundstellen(body: HTMLElement) {
    if (!fundstellen.length || selected !== fundstellenId) return;
    markiere(body, fundstellen);
    marken = [...body.querySelectorAll<HTMLElement>("mark.archive-hit")];
    (marken[fundstelleNr] ?? marken[0])?.scrollIntoView({ block: "center" });
  }

  /// Nummer der Fundstelle, die beim Umschalten in den Editor im Bild stand:
  /// die dem senkrechten Zentrum der Ansicht nächste. Ohne sichtbare Marke
  /// die erste.
  function sichtbareFundstelle(): number {
    if (marken.length < 2) return 0;
    const flaeche = container.querySelector(".archive-note-scroll");
    const kasten = flaeche?.getBoundingClientRect();
    const mitte = kasten ? kasten.top + kasten.height / 2 : window.innerHeight / 2;
    let beste = 0;
    let abstand = Infinity;
    marken.forEach((m, i) => {
      const r = m.getBoundingClientRect();
      const d = Math.abs(r.top + r.height / 2 - mitte);
      if (d < abstand) {
        abstand = d;
        beste = i;
      }
    });
    return beste;
  }

  function noteBody(doc: DocEntry): HTMLElement {
    const body = document.createElement("div");
    body.className = "archive-note-body";
    cb.readDoc(doc.id).then(
      (text) => {
        if (selected !== doc.id) return;
        // HTML-Notizen sind bereits Markup; Markdown läuft durch den
        // Renderer. Beides stammt aus dem eigenen Archiv und geht durch
        // dieselbe Wikilink-Verdrahtung.
        body.innerHTML = doc.kind === "html" ? text : renderMarkdown(text);
        linkWikiRefs(body, followWikiLink);
        hydrateDrawio(body, doc.relpath);
        zeigeFundstellen(body);
      },
      (e) => {
        body.textContent = String(e);
      },
    );
    return body;
  }

  /// Ansicht einer sonstigen Datei: JSON, YAML und XML als faltbarer Baum,
  /// alles andere unverändert als <pre>. Was sich nicht parsen lässt, zeigt
  /// den Parserfehler über dem Rohtext — nicht still den Rohtext allein.
  /// Binärdateien melden den Lesefehler des Backends.
  function fileBody(doc: DocEntry): HTMLElement {
    const body = document.createElement("div");
    body.className = "archive-note-body";
    const roh = (text: string) => {
      const pre = document.createElement("pre");
      pre.className = "archive-note-plain";
      pre.textContent = text;
      return pre;
    };
    const baum = (text: string): HTMLElement => {
      const p = doc.relpath.toLowerCase();
      try {
        if (p.endsWith(".json")) return dataTree(JSON.parse(text));
        if (p.endsWith(".yaml") || p.endsWith(".yml")) return dataTree(yamlLoad(text));
        if (p.endsWith(".xml")) {
          const dom = new DOMParser().parseFromString(text, "text/xml");
          const fehler = dom.querySelector("parsererror");
          if (fehler) throw new Error(fehler.textContent ?? "XML-Fehler");
          return xmlTree(dom.documentElement);
        }
      } catch (e) {
        const box = document.createElement("div");
        const meldung = document.createElement("div");
        meldung.className = "archive-note-error";
        meldung.textContent = String(e);
        box.append(meldung, roh(text));
        return box;
      }
      return roh(text);
    };
    cb.readFile(doc.id).then(
      (text) => {
        if (selected !== doc.id) return;
        body.append(baum(text));
        zeigeFundstellen(body);
      },
      (e) => {
        body.append(roh(String(e)));
      },
    );
    return body;
  }

  /// Bild-Ansicht einer Bilddatei: das Bild in die Fläche eingepaßt, ein Klick
  /// öffnet es im eigenen Fenster. Groß und nebeneinander gehört es dorthin —
  /// hier steht es, damit die Auswahl in der Liste etwas zeigt.
  function bildBody(doc: DocEntry): HTMLElement {
    const box = document.createElement("div");
    box.className = "archive-note-bild";
    const img = document.createElement("img");
    img.alt = doc.title;
    img.title = t("image.openWindow");
    img.addEventListener("click", () => cb.openImage(doc.id));
    cb.readImage(doc.id).then(
      (daten) => (img.src = daten),
      (e) => {
        const fehler = document.createElement("div");
        fehler.className = "archive-note-error";
        fehler.textContent = String(e);
        box.replaceChildren(fehler);
      },
    );
    box.append(img);
    return box;
  }

  /// Diagramm-Ansicht einer `.drawio`-Datei: der gebündelte draw.io-Viewer
  /// rendert das XML read-only (Zoom-Toolbar, Layer). Das Skript lädt beim
  /// ersten Diagramm einmalig nach — es wiegt 2,6 MB und gehört nicht ins
  /// Panel-Bundle.
  let drawioViewer: Promise<void> | null = null;
  function ladeDrawioViewer(): Promise<void> {
    if (!drawioViewer) {
      drawioViewer = new Promise((ok, nein) => {
        const s = document.createElement("script");
        s.src = drawioViewerUrl;
        s.addEventListener("load", () => ok());
        s.addEventListener("error", () => nein(new Error("draw.io-Viewer lädt nicht")));
        document.head.append(s);
      });
    }
    return drawioViewer;
  }

  /// Referenz eines eingebetteten Diagramms (relativ zur Notiz, `./`, `../`
  /// oder `/` = Archiv-Wurzel) zum Archiv-relpath auflösen.
  function drawioRelpath(docRel: string, ref: string): string {
    const teile = ref.startsWith("/") ? [] : docRel.split("/").slice(0, -1);
    for (const t of ref.split("/")) {
      if (t === "." || t === "") continue;
      if (t === "..") teile.pop();
      else teile.push(t);
    }
    return teile.join("/");
  }

  /// Viewer anlegen und das Diagramm einmal einpassen: mit Zoom-Toolbar
  /// unterdrückt der Viewer sein auto-fit (zoomEnabled), fitGraph holt genau
  /// diesen Schritt nach — er übersetzt an die Zeichnungsgrenzen und skaliert
  /// in die Containerbreite. Danach gelten die Zoom-Knöpfe normal.
  ///
  /// `fitGraph` allein rechnet nur die Breite (`graph.fit` läuft dort mit
  /// ignoreHeight), ein hohes Diagramm stünde also über den Rand hinaus und
  /// müsste gescrollt werden. Die Obergrenze `maxFitScale` erledigt die
  /// Höhe: die Skala, bei der die Zeichnung gerade in die Fläche passt.
  ///
  /// Das gilt nur für die Diagramm-Ansicht mit ihrer festen Bühne (`buehne`).
  /// Ein in eine Notiz eingebettetes Diagramm wächst umgekehrt mit seinem
  /// Inhalt: Dort ist die Container-Höhe vor dem Zeichnen noch die des leeren
  /// Platzhalters — als Deckel genommen, schrumpfte die Zeichnung auf nichts.
  function zeigeDrawio(el: Element, buehne = false) {
    type Graph = {
      border: number;
      container: HTMLElement;
      view: { scale: number };
      getGraphBounds(): { height: number };
    };
    type Viewer = {
      graph?: Graph;
      fitGraph?: (max?: number) => void;
      addListener?: (ev: string, f: () => void) => void;
    };
    const gv = (
      window as {
        GraphViewer?: { createViewerForElement(e: Element, cb?: (v: Viewer) => void): void };
      }
    ).GraphViewer;
    gv?.createViewerForElement(el, (v) => {
      const einpassen = () => {
        const g = v.graph;
        if (!buehne || !g) {
          v.fitGraph?.();
          return;
        }
        const platz = g.container.clientHeight - 2 * g.border - 2;
        const hoch = g.getGraphBounds().height / g.view.scale;
        v.fitGraph?.(hoch > 0 && platz > 0 ? Math.min(1, platz / hoch) : undefined);
        // Der Viewer stellt beim Einpassen auf `hidden` — danach ließe sich
        // ein hineingezoomtes Diagramm nicht mehr verschieben.
        g.container.style.overflow = "auto";
      };
      if (v.fitGraph) einpassen();
      else v.addListener?.("render", einpassen);
      // Fenstergröße geändert: neu einpassen, damit das Diagramm die Fläche
      // ausfüllt, ohne über sie hinauszuragen.
      if (buehne && typeof ResizeObserver === "function") {
        let erste = true;
        new ResizeObserver(() => {
          if (erste) {
            erste = false;
            return;
          }
          einpassen();
        }).observe(el);
      }
    });
  }

  /// Füllt `![](x.drawio)`-Platzhalter einer gerenderten Notiz mit dem
  /// draw.io-Viewer; Doppelklick öffnet die Desktop-App. Lesefehler stehen
  /// im Platzhalter statt still zu verschwinden.
  function hydrateDrawio(body: HTMLElement, docRel: string) {
    for (const span of body.querySelectorAll<HTMLElement>(".md-drawio")) {
      const ref = span.dataset.drawio ?? "";
      // Ressourcen liegen im versteckten Ordner der Notiz und stehen damit
      // außerhalb des Archiv-Index — sie werden über den Pfad angesprochen.
      const id = `path:${drawioRelpath(docRel, ref)}`;
      Promise.all([cb.readFile(id), ladeDrawioViewer()]).then(
        ([xml]) => {
          if (drawioLeer(xml)) {
            span.textContent = t("archive.emptyDiagram");
            if (cb.drawioAvailable()) {
              span.title = t("archive.editDrawio");
              span.addEventListener("dblclick", () => cb.openDrawio(id));
            }
            return;
          }
          const el = document.createElement("div");
          // auto-fit + max-width wie im offiziellen drawio-Embed: der Viewer
          // skaliert in die verfügbare Breite und setzt am Ursprung an — ohne
          // beides ragt ein breites oder weit vom Nullpunkt gezeichnetes
          // Diagramm nach rechts hinaus bzw. wird abgeschnitten.
          el.style.maxWidth = "100%";
          el.setAttribute(
            "data-mxgraph",
            JSON.stringify({ xml, nav: true, resize: false, "auto-fit": true }),
          );
          span.replaceChildren(el);
          span.classList.add("md-drawio-live");
          zeigeDrawio(el);
          if (cb.drawioAvailable()) {
            span.title = t("archive.editDrawio");
            span.addEventListener("dblclick", () => cb.openDrawio(id));
          }
        },
        (e) => {
          span.textContent = `${ref}: ${e}`;
        },
      );
    }
  }

  function drawioBody(doc: DocEntry): HTMLElement {
    const body = document.createElement("div");
    body.className = "archive-note-body archive-note-drawio";
    Promise.all([cb.readFile(doc.id), ladeDrawioViewer()]).then(
      ([xml]) => {
        if (selected !== doc.id) return;
        if (drawioLeer(xml)) {
          body.textContent = t("archive.emptyDiagram");
          return;
        }
        const el = document.createElement("div");
        el.style.maxWidth = "100%";
        // Volle Fläche als Inline-Höhe: Der Viewer behandelt einen Container
        // mit gesetzter Höhe als Bühne — er lässt sie stehen (statt sie auf
        // die Diagrammhöhe zu ziehen) und zentriert die Zeichnung darin.
        el.style.height = "100%";
        // resize ausdrücklich false: fehlt der Schlüssel ganz, prüft der
        // Viewer `0 != resize` (undefined → wahr) und schaltet resizeContainer
        // ein — der Container wird dann auf Diagramm-Pixelbreite gesetzt,
        // die Zeichnung springt an ihre gespeicherten Koordinaten und der
        // Überstand wird rechts gekappt.
        el.setAttribute(
          "data-mxgraph",
          JSON.stringify({ xml, nav: true, resize: false, "auto-fit": true, toolbar: "zoom layers" }),
        );
        body.append(el);
        zeigeDrawio(el, true);
      },
      (e) => {
        body.textContent = String(e);
      },
    );
    return body;
  }

  /// Buchansicht statt Notiz-Body: der Viewer holt sich das entpackte Buch
  /// samt Lesereihenfolge und Inhaltsverzeichnis. Ladefehler (kaputtes ZIP,
  /// fehlendes OPF) stehen im Body, statt still zu verschwinden.
  function epubBody(doc: DocEntry, kopf: HTMLElement): HTMLElement {
    const box = document.createElement("div");
    box.className = "archive-note-epub";
    // Aus der Suche gekommen: Das Buch öffnet beim Kapitel des Treffers, und
    // die Fundstellen darin sind markiert.
    const sprung =
      fundstellenTeil && fundstellen.length && selected === fundstellenId
        ? { href: fundstellenTeil, woerter: [...fundstellen] }
        : undefined;
    cb.openEpub(doc.id).then(
      (book) => {
        if (selected !== doc.id) return;
        const viewer = renderEpub(book, sprung);
        box.append(viewer);
        // Was das Buch als Ganzes betrifft — Auskunft, Schriftgrad, Verzeichnis,
        // Tag und Nacht —, steht bei den übrigen Aktionen in der Titelzeile,
        // vor dem Papierkorb, und trägt dort deren Größe. Nur das Blättern
        // bleibt unten am Buch.
        const leiste = kopf.querySelector(".archive-note-actions");
        const oben = [".epub-klapp", ".epub-seitig", ".epub-marker", ".epub-kleiner",
                      ".epub-groesser", ".epub-info", ".epub-nacht"]
          .map((sel) => viewer.querySelector(sel))
          .filter((el): el is Element => !!el);
        if (leiste && oben.length) leiste.prepend(...oben);
      },
      (e) => {
        box.textContent = String(e);
      },
    );
    return box;
  }

  /// Markdown-Editor der Notiz: links der Rohtext, rechts die Live-Vorschau
  /// (dieselbe Darstellung wie die Anzeige, Wikilinks klickbar). Der Text
  /// wandert unverändert zurück in die Datei — kein Rundlauf über ein
  /// Dokumentmodell, das Zeilenumbrüche, Listenmarker oder `[[ziel]]`
  /// normalisieren würde. Fehler erscheinen über dem Editor.
  function noteEditor(doc: DocEntry): { el: HTMLElement; save(): void } {
    const box = document.createElement("div");
    box.className = "archive-note-edit";
    const err = document.createElement("div");
    err.className = "archive-note-error";
    err.hidden = true;
    const fail = (e: unknown) => {
      err.hidden = false;
      err.textContent = String(e);
    };

    // HTML-Notizen: WYSIWYG auf ProseMirror, das Format bleibt HTML.
    if (doc.kind === "html") {
      let editor: { html(): string; destroy(): void } | null = null;
      box.addEventListener("keydown", (e) => {
        if (e.key !== "Escape") return;
        e.preventDefault();
        editor?.destroy();
        abortEdit();
      });
      cb.readDoc(doc.id).then((text) => {
        if (selected !== doc.id || !editing) return;
        const ed = initHtmlEditor(text);
        editor = ed;
        box.append(ed.el);
        ed.focus();
      }, fail);
      return {
        el: box,
        save: () => {
          if (!editor) return;
          cb.writeDoc(doc.id, editor.html()).then(() => {
            editor?.destroy();
            editing = false;
            renderMain();
          }, fail);
        },
      };
    }
    const split = document.createElement("div");
    split.className = "archive-edit-split";
    const preview = document.createElement("div");
    preview.className = "archive-note-body archive-edit-preview";
    let editor: MdEditor | null = null;
    const draw = () => {
      const text = editor?.value() ?? "";
      preview.innerHTML = doc.kind === "html" ? text : renderMarkdown(text);
      linkWikiRefs(preview, followWikiLink);
      hydrateDrawio(preview, doc.relpath);
    };
    const save = () => {
      cb.writeDoc(doc.id, editor?.value() ?? "").then(() => {
        editing = false;
        renderMain();
      }, fail);
    };
    // Die Vorschau läuft mit: derselbe Anteil an der Scrollstrecke wie im
    // Rohtext — Zeilen lassen sich nicht zuordnen, die Stelle im Text schon.
    const mitlaufen = (anteil: number) => {
      const platz = preview.scrollHeight - preview.clientHeight;
      if (platz > 0) preview.scrollTop = anteil * platz;
    };

    // Diagramm einfügen: leere .drawio-Datei im Ressourcen-Ordner, Referenz an
    // der Cursorstelle, dann direkt die Desktop-App zum Zeichnen.
    const bar = document.createElement("div");
    bar.className = "archive-edit-toolbar";
    const werkzeug = (label: string, titel: string, run: () => void) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "archive-edit-tool";
      b.textContent = label;
      b.title = titel;
      b.addEventListener("click", run);
      return b;
    };
    const dia = werkzeug(t("archive.newDiagram"), t("archive.newDiagram"), () => {
      openForm(t("archive.newDiagram"), t("archive.docName"), "", (v) => {
        void cb.actions.createDrawio(doc.id, v).then((rel) => {
          // Das Diagramm liegt im Ressourcen-Ordner der Notiz; der Verweis
          // steht relativ zu ihr (`./.<notiz>.res/<name>.drawio`).
          const dir = doc.relpath.split("/").slice(0, -1).join("/");
          const ref_ = dir && rel.startsWith(`${dir}/`) ? rel.slice(dir.length + 1) : rel;
          editor?.insert(`![](./${ref_})`);
          draw();
          // Die Datei liegt jetzt auf der Platte — der Verweis darauf gehört
          // dorthin, nicht in einen Editor, den ein Abbrechen wegwirft.
          cb.writeDoc(doc.id, editor?.value() ?? "").catch(fail);
          if (cb.drawioAvailable()) cb.openDrawio(`path:${rel}`);
        });
      });
    });
    if (!cb.drawioAvailable()) dia.title = t("archive.drawioMissing");
    const tab = werkzeug("⊞", t("html.table"), () =>
      openTableForm(bar, ({ spalten, zeilen, kopf }) => {
        editor?.insert(mdTabelle(spalten, zeilen, kopf));
        draw();
      }),
    );
    bar.append(dia, tab);

    cb.readDoc(doc.id).then((text) => {
      if (selected !== doc.id || !editing) return;
      editor = initMdEditor({
        text,
        // Aus der Suche gekommen: dieselben Wörter markieren und zu der
        // Stelle springen, die beim Lesen im Bild stand.
        fundstellen: fundstellen.length
          ? { woerter: fundstellen, nummer: marken.length ? sichtbareFundstelle() : fundstelleNr }
          : undefined,
        onChange: draw,
        onSave: save,
        onCancel: abortEdit,
        onScroll: mitlaufen,
      });
      split.prepend(editor.el);
      draw();
      editor.focus();
    }, fail);
    previewRedraw = draw;

    split.append(preview);
    box.append(err, bar, split);
    return { el: box, save };
  }

  /// Editor für Rohdaten-Dateien (Klartext, JSON, YAML, XML): eine Fläche
  /// ohne Vorschau — das Format ist die Anzeige. Gespeichert wird die Datei,
  /// wie sie dasteht, ohne Frontmatter.
  function textEditor(doc: DocEntry, sprache: Sprache): { el: HTMLElement; save(): void } {
    const box = document.createElement("div");
    box.className = "archive-note-edit";
    const err = document.createElement("div");
    err.className = "archive-note-error";
    err.hidden = true;
    const fail = (e: unknown) => {
      err.hidden = false;
      err.textContent = String(e);
    };
    let editor: MdEditor | null = null;
    const save = () => {
      cb.writeFile(doc.id, editor?.value() ?? "").then(() => {
        editing = false;
        renderMain();
      }, fail);
    };
    cb.readFile(doc.id).then((text) => {
      if (selected !== doc.id || !editing) return;
      editor = initMdEditor({
        text,
        sprache,
        fundstellen: fundstellen.length
          ? { woerter: fundstellen, nummer: sichtbareFundstelle() }
          : undefined,
        onChange: () => {},
        onSave: save,
        onCancel: abortEdit,
        onScroll: () => {},
      });
      box.append(editor.el);
      editor.focus();
    }, fail);
    box.append(err);
    return { el: box, save };
  }

  /// Kopf-Aktionen im Bearbeitungsmodus: Speichern und Abbrechen.
  function editActions(save: () => void): HTMLElement[] {
    const ok = document.createElement("button");
    ok.className = "archive-form-submit";
    ok.textContent = t("archive.save");
    ok.addEventListener("click", save);
    const cancel = document.createElement("button");
    cancel.className = "archive-form-cancel";
    cancel.textContent = t("archive.cancel");
    cancel.addEventListener("click", abortEdit);
    return [ok, cancel];
  }

  /// Bearbeiten verwerfen — dasselbe wie Abbrechen (Regel 4.4).
  function abortEdit() {
    editing = false;
    renderMain();
  }

  function metaParts(doc: DocEntry): string[] {
    const parts = [];
    if (doc.date) parts.push(doc.date);
    if (doc.tags.length) parts.push(doc.tags.map((x) => `#${x}`).join(" "));
    if (doc.backlinks) parts.push(`↩ ${doc.backlinks}`);
    return parts;
  }

  function docActions(doc: DocEntry): HTMLElement[] {
    // Bücher und sonstige Dateien werden angezeigt und gelöscht, nicht
    // bearbeitet. Diagramme bearbeitet die draw.io-Desktop-App, wenn sie
    // installiert ist.
    if (doc.kind === "epub" || doc.kind === "file") {
      const acts: HTMLElement[] = [];
      if (istDrawio(doc) && cb.drawioAvailable()) {
        const edit = document.createElement("button");
        edit.className = "panel-btn archive-drawio-edit";
        edit.title = t("archive.editDrawio");
        edit.textContent = "✎";
        edit.addEventListener("click", () => cb.openDrawio(doc.id));
        acts.push(edit);
      }
      // Ein Bild gehört groß und neben seinesgleichen — dafür das Fenster.
      if (istBild(doc)) {
        const fenster = document.createElement("button");
        fenster.className = "panel-btn archive-bild-fenster";
        fenster.title = t("image.openWindow");
        fenster.textContent = "⧉";
        fenster.addEventListener("click", () => cb.openImage(doc.id));
        acts.push(fenster);
      }
      // Textdateien (Klartext, JSON, YAML, XML) bearbeitet der Editor mit
      // der Grammatik ihrer Endung.
      if (spracheZu(doc.relpath)) {
        const edit = document.createElement("button");
        edit.className = "panel-btn";
        edit.title = t("archive.editDoc");
        edit.textContent = "✎";
        edit.addEventListener("click", () => {
          editing = true;
          renderMain();
        });
        acts.push(edit);
      }
      acts.push(deleteAction(t("archive.deleteDoc"), () => cb.actions.remove(doc.id)));
      return acts;
    }
    const edit = document.createElement("button");
    edit.className = "panel-btn";
    edit.title = t("archive.editDoc");
    edit.textContent = "✎";
    edit.addEventListener("click", () => {
      editing = true;
      renderMain();
    });
    return [edit, deleteAction(t("archive.deleteDoc"), () => cb.actions.remove(doc.id))];
  }

  /// Kindzeile im Dokument-Abschnitt: Titel links, Anlage-/Änderungsdatum
  /// rechts; Knoten ohne Inhaltsdatei zeigen nur den Titel.
  function childRow(
    title: string,
    doc: DocEntry | null,
    onOpen: () => void,
    folder: { path: string; leer: boolean } | null = null,
  ): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "archive-doc-row";
    const row = document.createElement("div");
    row.className = folder ? "archive-doc" : "archive-doc archive-doc-entry";
    const line = document.createElement("div");
    line.className = "archive-doc-line";
    // Typ-Symbol groß über beide Zeilen; rechts davon die Textspalte:
    // Titel, darunter die Beschreibung.
    const textcol = document.createElement("div");
    textcol.className = "archive-doc-text";
    const head = document.createElement("div");
    head.className = "archive-doc-title";
    head.textContent = title;
    textcol.append(head);
    if (doc?.description) {
      const desc = document.createElement("div");
      desc.className = "archive-doc-desc";
      desc.textContent = doc.description;
      textcol.append(desc);
    }
    line.append(folder || !doc ? typeIcon("node") : docIcon(doc), textcol);
    if (doc) {
      // Zweizeilig rechts: erstellt über geändert; der volle Zeitstempel
      // der Änderung steht im Hover.
      const dates = document.createElement("div");
      dates.className = "archive-doc-date";
      if (doc.date) {
        const created = document.createElement("div");
        created.textContent = t("archive.createdAt", { date: doc.date });
        dates.append(created);
      }
      const changed = document.createElement("div");
      changed.textContent = t("archive.changedAt", { date: doc.modified.slice(0, 10) });
      changed.title = doc.modified.replace("T", " ").replace("Z", " UTC");
      dates.append(changed);
      line.append(dates);
    }
    row.append(line);
    // Aktionen HINTER der Kachel (außerhalb, immer sichtbar): im
    // Dateimanager zeigen und Löschen — für Dokumente wie für Ordner.
    const acts = document.createElement("div");
    acts.className = "archive-row-actions";
    const rel = folder ? folder.path : doc?.relpath;
    if (rel) {
      const reveal = document.createElement("button");
      reveal.className = "panel-btn";
      reveal.title = t("archive.reveal");
      reveal.innerHTML = `<svg viewBox="0 0 16 16"><path d="M6.4 3.6H3.6a1.1 1.1 0 0 0-1.1 1.1v7.7a1.1 1.1 0 0 0 1.1 1.1h7.7a1.1 1.1 0 0 0 1.1-1.1V9.6"/><path d="M9.6 2.5h3.9v3.9"/><path d="M13.3 2.7 7.9 8.1"/></svg>`;
      reveal.addEventListener("click", (e) => {
        e.stopPropagation();
        cb.actions.reveal(`${current!.home.replace(/\/+$/, "")}/${rel}`);
      });
      acts.append(reveal);
    }
    const del = folder
      ? deleteAction(t("archive.deleteFolder"), () =>
          // Ein voller Ordner fragt nach — remove_dir_all nimmt den ganzen
          // Teilbaum mit; ein leerer löscht direkt.
          folder.leer
            ? cb.actions.removeFolder(folder.path)
            : confirmBox(t("archive.confirmDeleteFolder", { name: title }), () =>
                cb.actions.removeFolder(folder.path),
              ),
        )
      : doc
        ? deleteAction(t("archive.deleteDoc"), () => cb.actions.remove(doc.id))
        : null;
    if (del) {
      del.addEventListener("click", (e) => e.stopPropagation());
      acts.append(del);
    }
    wrap.append(row, acts);
    row.addEventListener("click", onOpen);
    // Das Kontextmenü der Dokument-Zeile (Bearbeiten/Löschen) — vorher am
    // Baum-Blatt, jetzt an der Übersichts-Zeile.
    if (doc && !folder) {
      row.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        const edit = {
          label: t("archive.editDoc"),
          run: () => {
            select(doc.id);
            editing = true;
            renderMain();
          },
        };
        const remove = { label: t("archive.deleteDoc"), run: () => cb.actions.remove(doc.id) };
        // Bücher und sonstige Dateien werden angezeigt, nicht bearbeitet.
        openMenu(
          e.clientX,
          e.clientY,
          doc.kind === "epub" || doc.kind === "file" ? [remove] : [edit, remove],
        );
      });
    }
    return wrap;
  }

  function renderMain() {
    const p = current!;
    const main = container.querySelector<HTMLElement>(".archive-main")!;
    main.textContent = "";
    // Der alte Editor ist mit dem Inhalt weg; einen neuen setzt noteEditor.
    previewRedraw = null;
    // Der Buch-Viewer füllt die Fläche und blättert selbst; die Notiz-Ansicht
    // scrollt. Beides im selben Bereich, also die Umschaltung hier.
    main.classList.remove("epub-mode");
    // Die Knopfzeile bleibt fest oben stehen; gescrollt wird der Inhalt —
    // auch horizontal, wenn ein Diagramm breiter ist als das Fenster.
    const scroller = (...els: HTMLElement[]) => {
      const s = document.createElement("div");
      s.className = "archive-note-scroll";
      s.append(...els);
      return s;
    };

    if (isLeaf(selected)) {
      const doc = findDoc(selected);
      if (!doc) return;
      if (doc.kind === "epub") {
        main.classList.add("epub-mode");
        const kopf = noteHead(doc.title, metaParts(doc), docActions(doc));
        main.append(kopf, epubBody(doc, kopf));
        return;
      }
      if (doc.kind === "file") {
        // Das Diagramm bekommt die Restfläche als eigene Bühne: es wird beim
        // Öffnen ganz hineingezoomt, also scrollt hier nichts.
        if (istDrawio(doc)) {
          main.append(noteHead(doc.title, metaParts(doc), docActions(doc)), drawioBody(doc));
          return;
        }
        // Ein Bild zeigt sich, statt seine Bytes als Text zu buchstabieren.
        if (istBild(doc)) {
          main.append(noteHead(doc.title, metaParts(doc), docActions(doc)), bildBody(doc));
          return;
        }
        const sprache = spracheZu(doc.relpath);
        if (editing && sprache) {
          const ed = textEditor(doc, sprache);
          main.append(noteHead(doc.title, [], editActions(ed.save)), scroller(ed.el));
          return;
        }
        main.append(noteHead(doc.title, metaParts(doc), docActions(doc)), scroller(fileBody(doc)));
        return;
      }
      if (editing) {
        const ed = noteEditor(doc);
        main.append(noteHead(doc.title, [], editActions(ed.save)), scroller(ed.el));
        return;
      }
      const head = noteHead(doc.title, metaParts(doc), docActions(doc));
      main.append(head, scroller(noteBody(doc)));
      return;
    }

    const node = nodeById(selected);
    if (!node) return;
    // Ohne eigene Notiz trägt der Knoten keinen Titel — der Ordnername steht
    // im Baum, ein gesetztes Wort in der Titelzeile wäre nur Füllung.
    const title = node.content?.title ?? "";
    const children = [...node.children.keys()].length + node.docs.length;
    const meta: string[] = node.content ? metaParts(node.content) : [];

    // Anlegen mit Auswahl: Ordner, Markdown-Notiz, HTML-Notiz, Dateien —
    // dieselben Wege wie im Kontextmenü des Baums, nur erreichbar ohne
    // rechte Maustaste.
    const add = document.createElement("button");
    add.className = "archive-add";
    add.title = t("archive.newEntry");
    add.textContent = "+";
    add.addEventListener("click", (e) => {
      e.stopPropagation();
      const r = add.getBoundingClientRect();
      openMenu(r.left, r.bottom + 4, anlegeMenue(selected));
    });
    const actions: HTMLElement[] = node.content
      ? [add, ...docActions(node.content)]
      : [add];
    if (editing && node.content) {
      const ed = noteEditor(node.content);
      main.append(noteHead(title, [], editActions(ed.save)), scroller(ed.el));
      return;
    }
    // Suchfeld (filtert nur die angezeigte Liste) und Sortier-Widget der
    // Datei-Übersicht — beide in der Titelzeile, keine Spaltenfilter.
    const filter = document.createElement("input");
    filter.className = "archive-filter";
    filter.type = "search";
    filter.title = t("archive.filterDocs");
    filter.setAttribute("aria-label", t("archive.filterDocs"));
    filter.value = docFilter;
    // Sortier-Knopf: zeigt die geltende erste Stufe im Klartext, öffnet das
    // Rangfolge-Menü.
    const sort = document.createElement("button");
    sort.className = "archive-sort";
    const sortLabel = () => {
      const [feld, richtung] = sortOrder[0];
      sort.textContent = `${t(SORTFELDER[feld])} ${richtung === "asc" ? "↑" : "↓"}`;
    };
    sortLabel();

    const docOrder = (a: DocEntry, b: DocEntry) => {
      for (const [feld, richtung] of sortOrder) {
        const wert = (d: DocEntry) =>
          feld === "name"
            ? d.title.toLowerCase()
            : feld === "changed"
              ? d.modified
              : (d.date ?? d.modified);
        const cmp = wert(a).localeCompare(wert(b));
        if (cmp) return richtung === "asc" ? cmp : -cmp;
      }
      return 0;
    };

    const buildList = () => {
      const list = document.createElement("div");
      list.className = "archive-note-children";
      const q = docFilter.trim().toLowerCase();
      const ordner = [...node.children]
        .filter(
          ([name, child]) => !q || (child.content?.title ?? name).toLowerCase().includes(q),
        )
        .sort((a, b) => a[0].localeCompare(b[0]));
      const docs = [...node.docs].filter((d) => !q || d.title.toLowerCase().includes(q)).sort(docOrder);
      const caption = document.createElement("div");
      caption.className = "archive-children-caption";
      const n = ordner.length + docs.length;
      caption.textContent = t(n === 1 ? "archive.docOne" : "archive.docMany", { count: n });
      list.append(caption);
      for (const [name, child] of ordner) {
        const childId = child.content?.id ?? "";
        list.append(
          childRow(child.content?.title ?? name, child.content ?? null, () => select(childId), {
            path: child.path,
            leer: child.docs.length === 0 && child.children.size === 0,
          }),
        );
      }
      for (const doc of docs) {
        list.append(childRow(doc.title, doc, () => select(doc.id)));
      }
      return list;
    };
    let listEl: HTMLElement | null = null;
    const refresh = () => {
      if (!listEl) return;
      const neu = buildList();
      listEl.replaceWith(neu);
      listEl = neu;
    };
    filter.addEventListener("input", () => {
      docFilter = filter.value;
      refresh();
    });
    sort.addEventListener("click", () => {
      const r = sort.getBoundingClientRect();
      openSortMenu(r.left, r.bottom + 4, () => {
        sortLabel();
        refresh();
      });
    });

    // Suche und Sortierung gehören zusammen; abgesetzt wird nur der Block
    // der Aktions-Knöpfe dahinter.
    const trenner = document.createElement("span");
    trenner.className = "archive-head-sep";
    const head = noteHead(title, meta, [filter, sort, trenner, ...actions]);
    const scroll = scroller();
    main.append(head, scroll);
    if (node.content) {
      scroll.append(noteBody(node.content));
    } else {
      // Jede Ordner-Notiz trägt von Haus aus Text — im Default ihren Namen.
      const body = document.createElement("div");
      body.className = "archive-note-body default";
      body.textContent = title;
      scroll.append(body);
    }

    if (children === 0 && !node.content) {
      const empty = document.createElement("div");
      empty.className = "archive-empty";
      const line = document.createElement("strong");
      line.textContent = p.total === 0 ? t("archive.emptyArchive") : t("archive.emptyFolder");
      empty.append(line);
      if (p.total === 0) empty.append(t("archive.emptyHint"));
      scroll.append(empty);
      return;
    }
    listEl = buildList();
    scroll.append(listEl);
  }

  function select(target: string, remember = true) {
    if (remember && target !== selected) history.push(selected);
    // Fundstellen gelten für ihre Notiz; eine andere Auswahl räumt sie weg.
    if (target !== fundstellenId) {
      fundstellen = [];
      fundstellenTeil = "";
      marken = [];
    }
    editing = false;
    docFilter = "";
    selected = target;
    if (current) render();
  }

  // Breite der Baum-Spalte; per Zieh-Griff verstellbar, gilt für die
  // Lebensdauer des Fensters.
  let treeWidth = 230;

  // Suchfeld und Sortierung der Datei-Übersicht; der Filter gilt je Auswahl,
  // die Sortierung für die Lebensdauer des Fensters.
  let docFilter = "";
  /// Rangfolge der Sortierung: erste Stufe zuerst, bei Gleichstand
  /// entscheidet die nächste. Alle Felder sind immer enthalten — die
  /// Reihenfolge ist die Aussage, nicht eine Auswahl.
  let sortOrder: [SortFeld, "asc" | "desc"][] = [
    ["name", "asc"],
    ["changed", "desc"],
    ["created", "desc"],
  ];

  function render() {
    container.textContent = "";
    const layout = document.createElement("div");
    layout.className = "archive-layout";
    const main = document.createElement("div");
    main.className = "archive-main";
    const tree = renderTree();
    tree.style.flexBasis = `${treeWidth}px`;
    const grip = document.createElement("div");
    grip.className = "archive-splitter";
    grip.addEventListener("mousedown", (e) => {
      e.preventDefault();
      const startX = e.clientX;
      const start = treeWidth;
      const move = (ev: MouseEvent) => {
        treeWidth = Math.min(560, Math.max(140, start + ev.clientX - startX));
        tree.style.flexBasis = `${treeWidth}px`;
      };
      const up = () => {
        window.removeEventListener("mousemove", move);
        window.removeEventListener("mouseup", up);
      };
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
    });
    layout.append(tree, grip, main);
    container.append(layout);
    renderMain();
  }

  /// Leerer Puffer: die Übersicht direkt anfordern — das Archiv startet
  /// von selbst, ohne Einstiegs-Klick. Nur einmal, das Update kommt über
  /// den Archiv-Puffer zurück.
  let requested = false;

  // Standard-Kontextmenü des Webviews im Archiv aus — es gibt nur unser
  // eigenes an Baumzeilen.
  container.addEventListener("contextmenu", (e) => e.preventDefault());

  let loaded = false;
  return {
    set(text: string) {
      loaded = !!text.trim();
      // Geleert wird beim Neuzeichnen (render) — hier nur der leere Puffer.
      if (!loaded) {
        container.textContent = "";
        current = null;
        if (cb.autoStart && !requested) {
          requested = true;
          cb.openArchive("tag:");
        }
        return;
      }
      current = JSON.parse(text);
      const alt = tree;
      tree = buildTree(current!);
      // Vorgemerkte Auswahl (Suchtreffer-Sprung) schlägt die gemerkte; die
      // Wörter des Treffers markiert die Notiz, sobald ihr Text da ist.
      const pending = cb.takePending?.();
      if (pending && findDoc(pending)) {
        fundstellen = cb.takeMarks?.() ?? [];
        fundstellenTeil = cb.takePart?.() ?? "";
        fundstelleNr = cb.takeSpot?.() ?? 0;
        fundstellenId = pending;
        select(pending);
        return;
      }
      // Eine laufende Bearbeitung bleibt stehen: Neuzeichnen baute den Editor
      // neu auf und lädt den Text von der Platte — der eben eingefügte
      // Diagramm-Verweis wäre weg, sobald draw.io speichert und der
      // Archiv-Watcher die Übersicht nachschiebt. Die Vorschau wird trotzdem
      // frisch gezeichnet, damit das eben gezeichnete Diagramm erscheint.
      if (editing) {
        previewRedraw?.();
        return;
      }
      // Die gewählte Notiz kann nach Umbenennen/Löschen weg sein — dann eine
      // Ebene hinauf: der tiefste Vorfahre aus dem alten Baum, den es noch
      // gibt; zuletzt die Wurzel.
      const exists = !!findDoc(selected) || !!nodeById(selected);
      if (selected && !exists) {
        const kette = ancestors(selected, alt) ?? [];
        let ziel = "";
        for (let i = kette.length - 1; i >= 0; i--) {
          const n = nodeByPath(kette[i].path);
          if (n) {
            ziel = n.content?.id ?? "";
            break;
          }
        }
        selected = ziel;
      }
      render();
      applyPendingEdit();
    },
    empty: () => !loaded,
  };
}
