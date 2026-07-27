/// Archiv-Ansicht des Panels, am Trilium-Layout orientiert: links ein Baum,
/// in dem die Notizen selbst hängen (Ordner = Notiz mit Kindern — ein Ordner
/// mit gleichnamigem Dokument daneben ist EINE Notiz mit Inhalt und Kindern),
/// rechts die Notiz-Ansicht: großer Titel mit Aktionen (bearbeiten,
/// umbenennen, löschen), darunter der gerenderte Inhalt bzw. für Notizen ohne
/// Inhalt die Kindliste. Titel, Beschreibungen und Pfade sind Fremdtext und
/// gehen nie durch innerHTML; der Markdown-Body läuft durch renderMarkdown.

import { renderEpub, type EpubBook } from "./epub-view";
import { initHtmlEditor } from "./html-editor";
import { renderMarkdown } from "./markdown";
import { t } from "./messages";
import { linkWikiRefs } from "./panel-view";
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
  /// Letzte Änderung (Datei-mtime, YYYY-MM-DD).
  modified: string;
  /// Notiz-Typ: `md` (Markdown), `html` oder `epub` (Buch — Ansicht statt
  /// Editor).
  kind: "md" | "html" | "epub";
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
/// kommt über den Wiki-Puffer zurück.
export interface WikiActions {
  remove(id: string): void;
  /// Neues Kind unterhalb des Knotens (ID; "" = Wurzel).
  createFolder(parent: string, name: string): void;
  /// Liefern die ID der neuen Notiz — die Ansicht öffnet sie im Editor.
  createDoc(parent: string, name: string): Promise<string>;
  createHtml(parent: string, name: string): Promise<string>;
}

export interface WikiCallbacks {
  /// Leeren Puffer sofort mit der Übersicht füllen (eigenes Fenster).
  autoStart?: boolean;
  /// Body eines Archiv-Dokuments (ohne Frontmatter) für die Notiz-Ansicht.
  readDoc(id: string): Promise<string>;
  /// Body einer Archiv-Notiz zurückschreiben (Bearbeiten im Archiv).
  writeDoc(id: string, text: string): Promise<void>;
  /// Buch öffnen: entpacken und Lesereihenfolge, Inhaltsverzeichnis und
  /// Metadaten aus seinen Verwaltungsdateien holen.
  openEpub(id: string): Promise<EpubBook>;
  /// Anzeige-Titel einer Notiz setzen (Klick auf den Titel).
  setTitle(id: string, title: string): void;
  /// Wiki-Ziel laden (`tag:` = Übersicht in den Puffer, Einstiegs-Chip).
  openWiki(name: string): void;
  /// Vorgemerkte Auswahl (Suchtreffer-Sprung): einmalig abholen.
  takePending?(): string | null;
  actions: WikiActions;
}

export interface WikiView {
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
}

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

function buildTree(p: Page): TreeNode {
  const root: TreeNode = { children: new Map(), docs: [] };
  const node = (path: string): TreeNode => {
    let n = root;
    if (!path) return n;
    for (const part of path.split("/")) {
      if (!n.children.has(part)) {
        n.children.set(part, { children: new Map(), docs: [] });
      }
      n = n.children.get(part)!;
    }
    return n;
  };
  for (const folder of p.folders) {
    if (folder.name) node(folder.name);
  }
  // Dokumente einhängen; ein Dokument mit gleichnamigem Ordner daneben wird
  // dessen Notiz-Inhalt statt eigenes Blatt.
  for (const folder of p.folders) {
    const parent = node(folder.name);
    for (const doc of folder.docs) {
      const twin = parent.children.get(doc.name);
      if (twin && !twin.content) twin.content = doc;
      else parent.docs.push(doc);
    }
  }
  // Wurzel-Konvention: `index.md` im Archiv-Root ist der Text des
  // Archiv-Knotens (die Wurzel hat keinen Namen für die Zwillingsregel).
  const idx = root.docs.findIndex((d) => d.name === "index");
  if (idx >= 0 && !root.content) {
    root.content = root.docs[idx];
    root.docs.splice(idx, 1);
  }
  return root;
}

export function initWikiView(container: HTMLElement, cb: WikiCallbacks): WikiView {
  /// Zugeklappte Knoten und Auswahl — beides über die technische ID der
  /// Notiz ("" = Archiv-Wurzel); übersteht Umbenennen, Verschieben und
  /// Puffer-Updates.
  const closed = new Set<string>();
  let selected = "";
  let current: Page | null = null;
  let tree: TreeNode = { children: new Map(), docs: [] };
  /// Auswahl-Verlauf für den Zurück-Knopf der Notiz-Ansicht.
  const history: string[] = [];
  /// Bearbeitungsmodus der aktuellen Notiz (Editor statt Anzeige).
  let editing = false;
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

  /// Elternknoten einer Notiz — Ziel beim Anlegen von Kindern.
  function parentOf(id: string, from: TreeNode = tree): TreeNode | null {
    if (from.docs.some((d) => d.id === id)) return from;
    for (const child of from.children.values()) {
      if (child.content?.id === id) return from;
      const hit = parentOf(id, child);
      if (hit) return hit;
    }
    return null;
  }


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
    cb.openWiki(name);
  }

  // ---------- Kontextmenü (rechte Maustaste im Baum) ----------

  /// Eigenes Kontextmenü an der Mausposition; Klick daneben oder Escape
  /// schließt. Das Standard-Menü des Webviews ist im ganzen Archiv aus.
  function openMenu(x: number, y: number, items: { label: string; run(): void }[]) {
    container.querySelector(".wiki-menu")?.remove();
    const menu = document.createElement("div");
    menu.className = "wiki-menu";
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    for (const item of items) {
      const btn = document.createElement("button");
      btn.className = "wiki-menu-item";
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

  // ---------- Baum-Spalte ----------

  /// Typ-Symbol im Baum (Stroke-SVG wie die übrigen App-Icons): neutraler
  /// Knoten-Kreis für Ordner-Notizen, Blatt für Dokumente.
  function typeIcon(kind: "node" | "doc" | "html" | "epub"): HTMLElement {
    const span = document.createElement("span");
    span.className = "wiki-tree-icon";
    const shapes = {
      node: `<circle cx="8" cy="8" r="4.2"/>`,
      doc: `<path d="M4 1.5h5.5L12.5 5v9a.9.9 0 0 1-.9.9H4a.9.9 0 0 1-.9-.9V2.4a.9.9 0 0 1 .9-.9z"/><path d="M9.5 1.5V5H13"/><path d="M5.8 8h4.4M5.8 10.5h4.4"/>`,
      // HTML-Notiz: dasselbe Blatt, spitze Klammern statt Textzeilen.
      html: `<path d="M4 1.5h5.5L12.5 5v9a.9.9 0 0 1-.9.9H4a.9.9 0 0 1-.9-.9V2.4a.9.9 0 0 1 .9-.9z"/><path d="M9.5 1.5V5H13"/><path d="M6.6 8.2 5.2 9.6l1.4 1.4M9.4 8.2l1.4 1.4-1.4 1.4"/>`,
      // Buch: aufgeschlagene Doppelseite.
      epub: `<path d="M8 4.2C6.8 3.2 5.1 2.8 3 2.8v9.4c2.1 0 3.8.4 5 1.4 1.2-1 2.9-1.4 5-1.4V2.8c-2.1 0-3.8.4-5 1.4z"/><path d="M8 4.2v9.4"/>`,
    };
    span.innerHTML = `<svg width="17" height="17" viewBox="0 0 16 16">${shapes[kind]}</svg>`;
    return span;
  }

  function folderRow(name: string, full: string, node: TreeNode): HTMLElement {
    const key = node.content?.id ?? full;
    const det = document.createElement("details");
    det.className = "wiki-tree-folder";
    det.open = !closed.has(key);
    det.addEventListener("toggle", () => {
      if (det.open) closed.delete(key);
      else closed.add(key);
    });
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
    const arrow = document.createElement("span");
    arrow.className = "wiki-tree-arrow";
    arrow.textContent = "▸";
    arrow.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      det.open = !det.open;
    });
    const label = document.createElement("span");
    label.className = "wiki-tree-name";
    label.textContent = node.content?.title ?? name;
    sum.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const id = node.content?.id ?? "";
      openMenu(e.clientX, e.clientY, [
        { label: t("wiki.newDoc"), run: () => newDocForm(id) },
        { label: t("wiki.newHtml"), run: () => newHtmlForm(id) },
        { label: t("wiki.newFolder"), run: () => newFolderForm(id) },
      ]);
    });
    sum.append(arrow, typeIcon("node"), label);
    det.append(sum, renderChildren(node, full));
    return det;
  }

  function docRow(doc: DocEntry): HTMLElement {
    const row = document.createElement("button");
    row.className = "wiki-tree-doc";
    if (doc.id === selected) row.classList.add("active");
    const label = document.createElement("span");
    label.className = "wiki-tree-name";
    label.textContent = doc.title;
    row.append(typeIcon(doc.kind === "md" ? "doc" : doc.kind), label);
    row.addEventListener("click", () => select(doc.id));
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      // Ein Buch wird gelesen, nicht bearbeitet.
      const edit = {
        label: t("wiki.editDoc"),
        run: () => {
          select(doc.id);
          editing = true;
          renderMain();
        },
      };
      const remove = { label: t("wiki.deleteDoc"), run: () => cb.actions.remove(doc.id) };
      openMenu(e.clientX, e.clientY, doc.kind === "epub" ? [remove] : [edit, remove]);
    });
    return row;
  }

  function renderChildren(node: TreeNode, path: string): HTMLElement {
    const box = document.createElement("div");
    box.className = "wiki-tree-children";
    // Unter einem Knoten erst seine Dokumente (weniger eingerückt), dann
    // seine Ordner (stärker eingerückt) — Skizze: Knoten / Dok (4em) /
    // Ordner (6em).
    for (const doc of [...node.docs].sort((a, b) => a.title.localeCompare(b.title))) {
      box.append(docRow(doc));
    }
    for (const [name, child] of [...node.children].sort((a, b) =>
      a[0].localeCompare(b[0]),
    )) {
      box.append(folderRow(name, path ? `${path}/${name}` : name, child));
    }
    return box;
  }

  function renderTree(): HTMLElement {
    const aside = document.createElement("aside");
    aside.className = "wiki-tree";
    const head = document.createElement("div");
    head.className = "wiki-tree-head";
    const root = document.createElement("button");
    root.className = "wiki-tree-root" + (selected === "" ? " active" : "");
    root.textContent = t("wiki.archive");
    root.addEventListener("click", () => select(""));
    root.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openMenu(e.clientX, e.clientY, [
        { label: t("wiki.newDoc"), run: () => newDocForm("") },
        { label: t("wiki.newHtml"), run: () => newHtmlForm("") },
        { label: t("wiki.newFolder"), run: () => newFolderForm("") },
      ]);
    });
    head.append(root);
    aside.append(head, renderChildren(tree, ""));
    return aside;
  }

  // ---------- Anlege-Formular (Ordner/Dokument) ----------

  function newDocForm(parent: string) {
    openForm(t("wiki.newDoc"), t("wiki.docName"), "", (v) => {
      void cb.actions.createDoc(parent, v).then(openNew);
    });
  }

  function newHtmlForm(parent: string) {
    openForm(t("wiki.newHtml"), t("wiki.docName"), "", (v) => {
      void cb.actions.createHtml(parent, v).then(openNew);
    });
  }

  function newFolderForm(parent: string) {
    openForm(t("wiki.newFolder"), t("wiki.docName"), "", (v) =>
      cb.actions.createFolder(parent, v),
    );
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
    document.querySelector(".wiki-modal")?.remove();
    const backdrop = document.createElement("div");
    backdrop.className = "wiki-modal";
    const form = document.createElement("div");
    form.className = "wiki-form";
    const caption = document.createElement("div");
    caption.className = "wiki-form-title";
    caption.textContent = title;
    const input = document.createElement("input");
    input.className = "wiki-tree-input";
    input.placeholder = placeholder;
    input.value = initial;
    const row = document.createElement("div");
    row.className = "wiki-form-row";
    const submit = document.createElement("button");
    submit.className = "wiki-form-submit";
    submit.textContent = t("wiki.create");
    const cancel = document.createElement("button");
    cancel.className = "wiki-form-cancel";
    cancel.textContent = t("wiki.cancel");
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
    form.addEventListener("keydown", (e) => {
      if (e.key === "Enter") fire();
      else if (e.key === "Escape") backdrop.remove();
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
    head.className = "wiki-note-head";
    const row = document.createElement("div");
    row.className = "wiki-note-titlerow";
    const back = document.createElement("button");
    back.className = "wiki-note-back";
    back.title = t("wiki.back");
    back.textContent = "←";
    back.disabled = history.length === 0;
    back.addEventListener("click", goBack);
    row.append(back);
    const h = document.createElement("div");
    h.className = "wiki-note-title";
    h.textContent = title;
    // Klick auf den Titel bearbeitet ihn direkt (Frontmatter-Titel der
    // Notiz); Enter übernimmt, Escape verwirft. Der technische Datei-/
    // Ordnername bleibt davon unberührt — der sitzt im Baum-Kontextmenü.
    // Der Titel eines Buchs steht in seiner Datei — er wird hier nicht
    // umgeschrieben.
    const titleDoc = findDoc(selected) ?? nodeById(selected)?.content;
    if (titleDoc && titleDoc.kind !== "epub" && !editing) {
      h.classList.add("editable");
      h.title = t("wiki.titleEdit");
      h.addEventListener("click", () => {
        const input = document.createElement("input");
        input.className = "wiki-note-title-input";
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
    acts.className = "wiki-note-actions";
    acts.append(...actions);
    row.append(h, acts);
    head.append(row);
    // Meta (Datum, Schlagwörter, Verweise) als kleines Popup am Info-Knopf
    // rechts in den Aktionen; Klick daneben schließt es.
    if (meta.length) {
      const pop = document.createElement("div");
      pop.className = "wiki-note-info-pop";
      pop.hidden = true;
      const caption = document.createElement("div");
      caption.className = "wiki-info-caption";
      caption.textContent = t("wiki.infoCaption");
      pop.append(caption);
      for (const part of meta) {
        const line = document.createElement("div");
        line.className = "wiki-info-line";
        line.append(part);
        pop.append(line);
      }
      const info = document.createElement("button");
      info.className = "panel-btn";
      info.title = t("wiki.info");
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
  function noteBody(doc: DocEntry): HTMLElement {
    const body = document.createElement("div");
    body.className = "wiki-note-body";
    cb.readDoc(doc.id).then(
      (text) => {
        if (selected !== doc.id) return;
        // HTML-Notizen sind bereits Markup; Markdown läuft durch den
        // Renderer. Beides stammt aus dem eigenen Archiv und geht durch
        // dieselbe Wikilink-Verdrahtung.
        body.innerHTML = doc.kind === "html" ? text : renderMarkdown(text);
        linkWikiRefs(body, followWikiLink);
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
  function epubBody(doc: DocEntry): HTMLElement {
    const box = document.createElement("div");
    box.className = "wiki-note-epub";
    cb.openEpub(doc.id).then(
      (book) => {
        if (selected !== doc.id) return;
        box.append(renderEpub(book));
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
    box.className = "wiki-note-edit";
    const err = document.createElement("div");
    err.className = "wiki-note-error";
    err.hidden = true;
    const fail = (e: unknown) => {
      err.hidden = false;
      err.textContent = String(e);
    };

    // HTML-Notizen: WYSIWYG auf ProseMirror, das Format bleibt HTML.
    if (doc.kind === "html") {
      let editor: { html(): string; destroy(): void } | null = null;
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
    split.className = "wiki-edit-split";
    const area = document.createElement("textarea");
    area.className = "wiki-note-editor";
    area.spellcheck = false;
    const preview = document.createElement("div");
    preview.className = "wiki-note-body wiki-edit-preview";
    const draw = () => {
      preview.innerHTML =
        doc.kind === "html" ? area.value : renderMarkdown(area.value);
      linkWikiRefs(preview, followWikiLink);
    };
    cb.readDoc(doc.id).then((text) => {
      area.value = text;
      draw();
      area.focus();
    }, fail);
    area.addEventListener("input", draw);
    const save = () => {
      cb.writeDoc(doc.id, area.value).then(() => {
        editing = false;
        renderMain();
      }, fail);
    };
    area.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        save();
      }
    });
    split.append(area, preview);
    box.append(err, split);
    return { el: box, save };
  }

  /// Kopf-Aktionen im Bearbeitungsmodus: Speichern und Abbrechen.
  function editActions(save: () => void): HTMLElement[] {
    const ok = document.createElement("button");
    ok.className = "wiki-form-submit";
    ok.textContent = t("wiki.save");
    ok.addEventListener("click", save);
    const cancel = document.createElement("button");
    cancel.className = "wiki-form-cancel";
    cancel.textContent = t("wiki.cancel");
    cancel.addEventListener("click", () => {
      editing = false;
      renderMain();
    });
    return [ok, cancel];
  }

  function metaParts(doc: DocEntry): string[] {
    const parts = [];
    if (doc.date) parts.push(doc.date);
    if (doc.tags.length) parts.push(doc.tags.map((x) => `#${x}`).join(" "));
    if (doc.backlinks) parts.push(`↩ ${doc.backlinks}`);
    return parts;
  }

  function docActions(doc: DocEntry): HTMLElement[] {
    // Bücher werden gelesen und gelöscht, nicht bearbeitet.
    if (doc.kind === "epub") {
      return [deleteAction(t("wiki.deleteDoc"), () => cb.actions.remove(doc.id))];
    }
    const edit = document.createElement("button");
    edit.className = "panel-btn";
    edit.title = t("wiki.editDoc");
    edit.textContent = "✎";
    edit.addEventListener("click", () => {
      editing = true;
      renderMain();
    });
    return [edit, deleteAction(t("wiki.deleteDoc"), () => cb.actions.remove(doc.id))];
  }

  /// Kindzeile im Dokument-Abschnitt: Titel links, Anlage-/Änderungsdatum
  /// rechts; Knoten ohne Inhaltsdatei zeigen nur den Titel.
  function childRow(title: string, doc: DocEntry | null, onOpen: () => void): HTMLElement {
    const row = document.createElement("div");
    row.className = "wiki-doc";
    const line = document.createElement("div");
    line.className = "wiki-doc-line";
    const head = document.createElement("div");
    head.className = "wiki-doc-title";
    head.textContent = title;
    line.append(head);
    if (doc) {
      const dates = document.createElement("div");
      dates.className = "wiki-doc-date";
      const parts = [];
      if (doc.date) parts.push(t("wiki.createdAt", { date: doc.date }));
      parts.push(t("wiki.changedAt", { date: doc.modified }));
      dates.textContent = parts.join(" · ");
      line.append(dates);
    }
    row.append(line);
    row.addEventListener("click", onOpen);
    return row;
  }

  function renderMain() {
    const p = current!;
    const main = container.querySelector<HTMLElement>(".wiki-main")!;
    main.textContent = "";
    // Der Buch-Viewer füllt die Fläche und blättert selbst; die Notiz-Ansicht
    // scrollt. Beides im selben Bereich, also die Umschaltung hier.
    main.classList.remove("epub-mode");

    if (isLeaf(selected)) {
      const doc = findDoc(selected);
      if (!doc) return;
      if (doc.kind === "epub") {
        main.classList.add("epub-mode");
        main.append(noteHead(doc.title, metaParts(doc), docActions(doc)), epubBody(doc));
        return;
      }
      if (editing) {
        const ed = noteEditor(doc);
        main.append(noteHead(doc.title, [], editActions(ed.save)), ed.el);
        return;
      }
      const head = noteHead(doc.title, metaParts(doc), docActions(doc));
      main.append(head, noteBody(doc));
      return;
    }

    const node = nodeById(selected);
    if (!node) return;
    const title = node.content?.title ?? t("wiki.archive");
    const children = [...node.children.keys()].length + node.docs.length;
    const meta: string[] = node.content ? metaParts(node.content) : [];

    const add = document.createElement("button");
    add.className = "wiki-add";
    add.title = t("wiki.newDoc");
    add.textContent = "+";
    add.addEventListener("click", () => newDocForm(selected));
    const actions: HTMLElement[] = node.content
      ? [add, ...docActions(node.content)]
      : [add];
    if (editing && node.content) {
      const ed = noteEditor(node.content);
      main.append(noteHead(title, [], editActions(ed.save)), ed.el);
      return;
    }
    const head = noteHead(title, meta, actions);
    main.append(head);
    if (node.content) {
      main.append(noteBody(node.content));
    } else {
      // Jede Ordner-Notiz trägt von Haus aus Text — im Default ihren Namen.
      const body = document.createElement("div");
      body.className = "wiki-note-body default";
      body.textContent = title;
      main.append(body);
    }

    if (children === 0 && !node.content) {
      const empty = document.createElement("div");
      empty.className = "wiki-empty";
      const line = document.createElement("strong");
      line.textContent = p.total === 0 ? t("wiki.emptyArchive") : t("wiki.emptyFolder");
      empty.append(line);
      if (p.total === 0) empty.append(t("wiki.emptyHint"));
      main.append(empty);
      return;
    }
    const list = document.createElement("div");
    list.className = "wiki-note-children";
    const caption = document.createElement("div");
    caption.className = "wiki-children-caption";
    caption.textContent = t(children === 1 ? "wiki.docOne" : "wiki.docMany", {
      count: children,
    });
    list.append(caption);
    for (const [name, child] of [...node.children].sort((a, b) => a[0].localeCompare(b[0]))) {
      const childId = child.content?.id ?? "";
      list.append(
        childRow(child.content?.title ?? name, child.content ?? null, () =>
          select(childId),
        ),
      );
    }
    for (const doc of [...node.docs].sort((a, b) => a.title.localeCompare(b.title))) {
      list.append(childRow(doc.title, doc, () => select(doc.id)));
    }
    main.append(list);
  }

  function select(target: string, remember = true) {
    if (remember && target !== selected) history.push(selected);
    editing = false;
    selected = target;
    // Vorfahren aufklappen, sonst bleibt die Hervorhebung unsichtbar.
    for (let p = parentOf(target); p?.content; p = parentOf(p.content.id)) {
      closed.delete(p.content.id);
    }
    if (current) render();
  }

  function render() {
    container.textContent = "";
    const layout = document.createElement("div");
    layout.className = "wiki-layout";
    const main = document.createElement("div");
    main.className = "wiki-main";
    layout.append(renderTree(), main);
    container.append(layout);
    renderMain();
  }

  /// Leerer Puffer: die Übersicht direkt anfordern — das Archiv startet
  /// von selbst, ohne Einstiegs-Klick. Nur einmal, das Update kommt über
  /// den Wiki-Puffer zurück.
  let requested = false;

  // Standard-Kontextmenü des Webviews im Archiv aus — es gibt nur unser
  // eigenes an Baumzeilen.
  container.addEventListener("contextmenu", (e) => e.preventDefault());

  let loaded = false;
  return {
    set(text: string) {
      container.textContent = "";
      loaded = !!text.trim();
      if (!loaded) {
        current = null;
        if (cb.autoStart && !requested) {
          requested = true;
          cb.openWiki("tag:");
        }
        return;
      }
      current = JSON.parse(text);
      tree = buildTree(current!);
      // Vorgemerkte Auswahl (Suchtreffer-Sprung) schlägt die gemerkte.
      const pending = cb.takePending?.();
      if (pending && findDoc(pending)) {
        select(pending);
        return;
      }
      // Die gewählte Notiz kann nach Umbenennen/Löschen weg sein — dann
      // zurück zur Wurzel.
      const exists = !!findDoc(selected) || !!nodeById(selected);
      if (selected && !exists) {
        selected = "";
      }
      render();
      applyPendingEdit();
    },
    empty: () => !loaded,
  };
}
