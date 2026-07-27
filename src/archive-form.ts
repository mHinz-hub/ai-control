/// Archiv-Dialog: wohin archivieren, mit welcher Beschreibung und welchen
/// Schlagwörtern. Modales Popup im Design der Archiv-Ansicht — abgedunkelter
/// Hintergrund, zentrierte Box. Der Zielordner wird im sichtbaren Baum des
/// Archivs gewählt (Wurzel plus alle Ordner, Klick übernimmt); für einen neuen
/// Ordner steht darunter ein Feld, in dem der gewählte Pfad ergänzt wird.
/// Enter archiviert, Escape oder ein Klick auf den Hintergrund schließt.
/// Gemeinsam für das angedockte Panel und das Archiv-Fenster.

import { t } from "./messages";

export interface ArchiveFormMeta {
  title?: string;
  folder?: string;
  description?: string;
  tags: string[];
}

/// Ordner-Knoten des Archivs für den Zielordner-Baum.
export interface ArchiveFolderNode {
  /// Technische ID des Knotens — das archivierte Ziel.
  id: string;
  /// Pfad (nur für den Tooltip).
  path: string;
  title: string;
}

export interface ArchiveFormOptions {
  /// Vorhandene Archiv-Ordner (Pfad + Titel) für den Zielordner-Baum.
  folders?(): Promise<ArchiveFolderNode[]>;
  /// Titel-Vorbelegung (erste Überschrift des Entwurfs), geladen beim Öffnen.
  title?(): Promise<string>;
  /// „Auf Platte legen": Entwurf als Datei an frei gewähltem Pfad ablegen.
  onSave?(): void;
}

export function initArchiveForm(
  _anchor: HTMLElement,
  onSubmit: (meta: ArchiveFormMeta) => void,
  opts: ArchiveFormOptions = {},
): { toggle(): void } {
  function field(labelText: string, placeholder: string): HTMLInputElement {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "wiki-tree-input";
    input.placeholder = placeholder;
    box.append(labelled(labelText, input));
    return input;
  }

  /// Beschriftete Gruppe. `label` nur für echte Eingabefelder: ein <label>
  /// leitet Klicks auf sein erstes beschriftbares Kind um — Knöpfe im Baum
  /// würden dadurch alle auf dieselbe Zeile wirken.
  function labelled(
    labelText: string,
    control: HTMLElement,
    tag: "label" | "div" = "label",
  ): HTMLElement {
    const group = document.createElement(tag);
    group.className = "archive-form-label";
    const caption = document.createElement("span");
    caption.textContent = labelText;
    group.append(caption, control);
    return group;
  }

  // Backdrop trägt die Modal-Optik, die Box das Formular — dieselben Klassen
  // wie die Dialoge der Archiv-Ansicht.
  const form = document.createElement("div");
  form.className = "archive-form wiki-modal";
  form.hidden = true;
  const box = document.createElement("div");
  box.className = "wiki-form archive-form-box";
  const caption = document.createElement("div");
  caption.className = "wiki-form-title";
  caption.textContent = t("archiveForm.title");
  box.append(caption);

  const title = field(t("archiveForm.titleLabel"), t("archiveForm.docTitle"));

  // --- Zielordner: durchlaufbarer Baum ------------------------------------
  // Logische Sicht des Archivs (Knotentitel plus Pfad an der Zeile), startet
  // eingeklappt. Die gewählte Zeile ist die Anzeige des Ziels — kein zweites
  // Feld daneben.
  const browse = document.createElement("div");
  browse.className = "archive-browse";
  box.append(labelled(t("archiveForm.folderLabel"), browse, "div"));

  /// Gewählter Zielordner (Pfad relativ zum Archiv-Home, "" = Wurzel).
  let selected = "";

  /// Aufgeklappte Knoten (Pfade) — überleben das Neuzeichnen des Baums.
  const openPaths = new Set<string>();

  interface Node {
    id: string;
    path: string;
    title: string;
    children: Node[];
  }

  /// Flache Pfadliste zu einem Baum; Zwischenebenen ohne eigenen Eintrag
  /// entstehen mit ihrem Ordnernamen als Titel.
  function buildNodes(folders: ArchiveFolderNode[]): Node[] {
    const byPath = new Map<string, Node>();
    const roots: Node[] = [];
    const ensure = (path: string, title: string, id = ""): Node => {
      const found = byPath.get(path);
      if (found) return found;
      const node: Node = { id, path, title, children: [] };
      byPath.set(path, node);
      const cut = path.lastIndexOf("/");
      if (cut < 0) roots.push(node);
      else {
        const parent = path.slice(0, cut);
        ensure(parent, parent.split("/").pop()!).children.push(node);
      }
      return node;
    };
    for (const f of [...folders].sort((a, b) => a.path.localeCompare(b.path))) {
      ensure(f.path, f.title, f.id);
    }
    return roots;
  }

  /// Ein Knoten: Zeile (Pfeil klappt, Rest wählt aus) plus Kinderliste.
  function nodeEl(node: Node, depth: number): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "archive-node";
    const row = document.createElement("div");
    row.className = "archive-browse-row";
    row.dataset.path = node.id;
    row.style.setProperty("--depth", String(depth));

    const arrow = document.createElement("button");
    arrow.className = "archive-arrow";
    arrow.type = "button";
    if (node.children.length) {
      arrow.textContent = "▸";
      arrow.title = t("archiveForm.expand");
    } else {
      arrow.classList.add("blank");
    }
    const icon = document.createElement("span");
    icon.className = "wiki-tree-icon";
    icon.innerHTML = `<svg width="17" height="17" viewBox="0 0 16 16"><circle cx="8" cy="8" r="4.2"/></svg>`;
    const label = document.createElement("span");
    label.className = "wiki-tree-name";
    label.textContent = node.title;
    // Der Pfad würde die Zeile überbreit machen; er steht als Tooltip dran.
    row.title = node.path;
    row.append(arrow, icon, label);

    const kids = document.createElement("div");
    kids.className = "archive-kids";
    kids.hidden = !openPaths.has(node.path);
    for (const child of node.children) kids.append(nodeEl(child, depth + 1));

    arrow.addEventListener("click", (e) => {
      e.stopPropagation();
      if (!node.children.length) return;
      kids.hidden = !kids.hidden;
      if (kids.hidden) openPaths.delete(node.path);
      else openPaths.add(node.path);
      arrow.textContent = kids.hidden ? "▸" : "▾";
    });
    arrow.textContent = node.children.length ? (kids.hidden ? "▸" : "▾") : "";
    row.addEventListener("click", () => select(node.id));

    wrap.append(row, kids);
    return wrap;
  }

  function select(path: string) {
    selected = path;
    mark();
  }

  /// Die gewählte Zeile hervorheben — sie ist die einzige Anzeige des Ziels.
  function mark() {
    for (const row of browse.querySelectorAll<HTMLElement>(
      ".archive-browse-row",
    )) {
      row.classList.toggle("active", row.dataset.path === selected);
    }
  }

  function renderBrowse(folders: ArchiveFolderNode[]) {
    const root = document.createElement("div");
    root.className = "archive-browse-row";
    root.dataset.path = "";
    root.style.setProperty("--depth", "0");
    const blank = document.createElement("span");
    blank.className = "archive-arrow blank";
    const icon = document.createElement("span");
    icon.className = "wiki-tree-icon";
    icon.innerHTML = `<svg width="17" height="17" viewBox="0 0 16 16"><circle cx="8" cy="8" r="4.2"/></svg>`;
    const label = document.createElement("span");
    label.className = "wiki-tree-name";
    label.textContent = t("wiki.archive");
    root.append(blank, icon, label);
    root.addEventListener("click", () => select(""));
    browse.replaceChildren(root);
    for (const node of buildNodes(folders)) browse.append(nodeEl(node, 1));
    mark();
  }

  const desc = field(
    t("archiveForm.descriptionLabel"),
    t("archiveForm.description"),
  );
  const tags = field(t("archiveForm.tagsLabel"), t("archiveForm.tags"));

  const row = document.createElement("div");
  row.className = "wiki-form-row";
  const submit = document.createElement("button");
  submit.className = "archive-form-submit wiki-form-submit";
  submit.textContent = t("archiveForm.submit");
  const cancel = document.createElement("button");
  cancel.className = "archive-form-cancel wiki-form-cancel";
  cancel.textContent = t("archiveForm.cancel");
  row.append(submit);
  if (opts.onSave) {
    const save = document.createElement("button");
    save.className = "archive-form-save wiki-form-submit";
    save.textContent = t("archiveForm.save");
    save.title = t("archiveForm.saveTitle");
    save.addEventListener("click", () => {
      opts.onSave!();
      close();
    });
    row.append(save);
  }
  row.append(cancel);
  box.append(row);
  form.append(box);
  document.body.append(form);

  const meta = (): ArchiveFormMeta => ({
    title: title.value.trim() || undefined,
    folder: selected || undefined,
    description: desc.value.trim() || undefined,
    tags: tags.value
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean),
  });
  // Leeren, weil das Formular zum Fenster gehört und nicht zum Dokument.
  const close = () => {
    form.hidden = true;
    selected = "";
    title.value = "";
    desc.value = "";
    tags.value = "";
    document.removeEventListener("keydown", onKey);
  };
  // Escape auch dann, wenn der Fokus nicht im Dialog sitzt (Terminal-Fenster
  // gibt Tasten sonst an die Shell weiter).
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };
  const open = () => {
    form.hidden = false;
    document.addEventListener("keydown", onKey);
    title.focus();
    // Vorbelegung: erste Überschrift des Entwurfs, änderbar vor der Übernahme.
    void opts.title?.().then((v) => {
      title.value = v;
      title.select();
    });
    // Frisch laden: zwischen zwei Dialogen können Ordner entstanden sein.
    // Scheitert das Laden, steht der Grund im Kasten — ein leerer Kasten
    // ohne Erklärung war der Fehler der Vorversion.
    browse.replaceChildren();
    // .catch statt zweitem then-Argument: so fallen auch Fehler beim Aufbau
    // des Baums auf, nicht nur beim Laden.
    opts
      .folders?.()
      .then(renderBrowse)
      .catch((e) => {
        const err = document.createElement("div");
        err.className = "archive-browse-error";
        err.textContent = String(e);
        browse.replaceChildren(err);
      });
  };
  const fire = () => {
    onSubmit(meta());
    close();
  };
  submit.addEventListener("click", fire);
  cancel.addEventListener("click", close);
  form.addEventListener("mousedown", (e) => {
    if (e.target === form) close();
  });
  form.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      fire();
    } else if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  });

  return { toggle: () => (form.hidden ? open() : close()) };
}
