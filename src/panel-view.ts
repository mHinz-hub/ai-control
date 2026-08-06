import { renderMarkdown } from "./markdown";
import { writeText, writeHtml } from "@tauri-apps/plugin-clipboard-manager";
import { flash } from "./tiles";
import { t } from "./messages";

export interface PanelView {
  set(text: string): void;
  raw(): string;
  /// Schreibt eine offene Inhalts-Bearbeitung zurück (und beendet sie);
  /// aufgelöst, sobald der Entwurf gespeichert ist.
  flush(): Promise<void>;
}

/// Verkabelt einen Panel-Inhaltsbereich: MD/Roh-Umschalter und Copy-Button.
/// Der Rohtext bleibt die Quelle für „Kopieren"; die Ansicht rendert Markdown.
/// Entfernt Inline-Markdown (Emphasis, Code, Links), damit der Plain-Text-Titel
/// nicht die rohen Marker zeigt.
function stripInlineMd(s: string): string {
  return s
    .replace(/`([^`]*)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/_([^_]+)_/g, "$1")
    .replace(/~~([^~]+)~~/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .trim();
}

/// Titel für den Panel-Kopf: erste Überschrift (# …) oder sonst erste
/// nicht-leere Zeile, ohne Inline-Markdown; „Dokument" bei leerem Text.
function firstLine(text: string): string {
  let fallback = "";
  for (const line of text.split("\n")) {
    const t = line.trim();
    if (!t) continue;
    const h = t.replace(/^#+/, "").trim();
    if (t.startsWith("#") && h) return stripInlineMd(h);
    if (!fallback) fallback = t;
  }
  return fallback ? stripInlineMd(fallback) : t("panel.tabDraft");
}

/// Ersetzt die erste Überschrift (bzw. legt eine an) im Rohtext durch `title`.
function setHeading(text: string, title: string): string {
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (!t) continue;
    if (t.startsWith("#")) {
      const hashes = (t.match(/^#+/) as RegExpMatchArray)[0];
      lines[i] = `${hashes} ${title}`;
    } else {
      lines.splice(i, 0, `# ${title}`, "");
    }
    return lines.join("\n");
  }
  return `# ${title}\n`;
}

/// Macht `[[name]]`-Wikilinks im gerendertem Markdown klickbar. Läuft über die
/// Textknoten des DOM statt über den Rohtext, damit Vorkommen in Code-Spans
/// und Code-Blöcken (z. B. bash `[[ -f x ]]`) unangetastet bleiben.
export function linkWikiRefs(root: HTMLElement, onClick: (name: string) => void) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = n as Text;
    if (!t.textContent?.includes("[[")) continue;
    if (t.parentElement?.closest("code, pre, a")) continue;
    nodes.push(t);
  }
  for (const node of nodes) {
    const parts = node.textContent!.split(/\[\[([^\]]+)\]\]/g);
    if (parts.length < 3) continue;
    const frag = document.createDocumentFragment();
    parts.forEach((part, i) => {
      if (i % 2) {
        // `[[ziel]]` oder `[[ziel|label]]`.
        const sep = part.indexOf("|");
        const target = sep < 0 ? part : part.slice(0, sep);
        const label = sep < 0 ? part : part.slice(sep + 1);
        const a = document.createElement("a");
        a.href = "#";
        a.className = "wiki"; // Wikilink im Text — bleibt beim Begriff
        a.textContent = label;
        a.addEventListener("click", (e) => {
          e.preventDefault();
          onClick(target);
        });
        frag.append(a);
      } else if (part) {
        frag.append(part);
      }
    });
    node.replaceWith(frag);
  }
}

export function initPanelView(opts: {
  content: HTMLElement;
  copyBtn: HTMLElement;
  copyHtmlBtn?: HTMLElement;
  printBtn?: HTMLElement;
  modeBtn: HTMLElement;
  titleEl?: HTMLElement;
  editBtn?: HTMLElement;
  editContentBtn?: HTMLElement;
  langSelect?: HTMLSelectElement;
  /// Standard-Sprache der Rechtschreibprüfung (aus den App-Settings).
  defaultLang?: string;
  /// Wird mit dem neuen Rohtext aufgerufen, wenn Titel oder Inhalt geändert
  /// wurden.
  onCommit?: (text: string) => void | Promise<void>;
  /// Klick auf einen `[[name]]`-Wikilink im gerenderten Markdown.
  onWikiLink?: (name: string) => void;
}): PanelView {
  let rawText = "";
  let rendered = true;
  // Content-Edit-Zustand: während des Editierens werden eingehende Updates
  // gepuffert statt angewandt.
  let editing = false;
  let pending: string | null = null;
  let flushEditor: () => Promise<void> = async () => {};

  function draw() {
    if (rendered) {
      opts.content.className = "md";
      opts.content.innerHTML = renderMarkdown(rawText);
      if (opts.onWikiLink) linkWikiRefs(opts.content, opts.onWikiLink);
    } else {
      opts.content.className = "raw";
      opts.content.textContent = rawText;
    }
    opts.modeBtn.textContent = rendered ? t("panel.rendered") : t("panel.raw");
  }

  opts.modeBtn.addEventListener("click", () => {
    rendered = !rendered;
    draw();
  });

  opts.copyBtn.addEventListener("click", async () => {
    await writeText(rawText);
    flash(opts.copyBtn, "copied");
  });

  // Formatiert kopieren: HTML-Flavor für Word/Teams/Mail, Plain-Fallback
  // bleibt der Markdown-Rohtext.
  opts.copyHtmlBtn?.addEventListener("click", async () => {
    await writeHtml(renderMarkdown(rawText), rawText);
    flash(opts.copyHtmlBtn!, "copied");
  });

  // Druckdialog des Systems — dort lässt sich auch in eine PDF-Datei drucken.
  // Was gedruckt wird, regeln die @media-print-Regeln des Fensters.
  opts.printBtn?.addEventListener("click", () => window.print());

  // Titel-Edit: Edit-Button macht den Titel editierbar, Enter/Blur schreibt die
  // geänderte Überschrift zurück, Escape verwirft.
  const titleEl = opts.titleEl;
  if (opts.editBtn && titleEl && opts.onCommit) {
    const commit = () => {
      if (titleEl.getAttribute("contenteditable") !== "true") return;
      titleEl.removeAttribute("contenteditable");
      const nt = (titleEl.textContent || "").trim();
      if (nt && nt !== firstLine(rawText)) opts.onCommit!(setHeading(rawText, nt));
      else titleEl.textContent = firstLine(rawText);
    };
    opts.editBtn.addEventListener("click", () => {
      titleEl.setAttribute("contenteditable", "true");
      titleEl.focus();
      const r = document.createRange();
      r.selectNodeContents(titleEl);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(r);
    });
    titleEl.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        titleEl.blur();
      } else if (e.key === "Escape") {
        titleEl.textContent = firstLine(rawText);
        titleEl.removeAttribute("contenteditable");
        titleEl.blur();
      }
    });
    titleEl.addEventListener("blur", commit);
  }

  // Content-Edit: Button schaltet den Inhalt auf eine Rohtext-Textarea. Speichern
  // (Button erneut oder Cmd/Ctrl+Enter) schreibt zurück, Escape verwirft.
  // Eingehende Updates während des Editierens werden gepuffert.
  if (opts.editContentBtn && opts.onCommit) {
    const editBtn = opts.editContentBtn;
    const editor = document.createElement("textarea");
    editor.className = "panel-editor";
    editor.spellcheck = true;
    editor.hidden = true;
    opts.content.after(editor);

    // Sprache der Rechtschreibprüfung: Default aus den Settings, per Selector
    // pro Text überschreibbar.
    let lang = opts.defaultLang || "de";
    const sel = opts.langSelect;
    if (sel) {
      if ([...sel.options].some((o) => o.value === lang)) sel.value = lang;
      else lang = sel.value;
      sel.addEventListener("change", () => {
        lang = sel.value;
        editor.lang = lang;
        // Neuprüfung erzwingen.
        editor.spellcheck = false;
        editor.spellcheck = true;
        if (!editor.hidden) editor.focus();
      });
    }
    editor.lang = lang;

    const leave = () => {
      editing = false;
      editor.hidden = true;
      opts.content.hidden = false;
      editBtn.classList.remove("active", "changed");
    };
    const enter = () => {
      editing = true;
      pending = null;
      editor.value = rawText;
      opts.content.hidden = true;
      editor.hidden = false;
      editBtn.classList.add("active");
      editor.focus();
    };
    flushEditor = async () => {
      if (!editing) return;
      const v = editor.value;
      leave();
      await opts.onCommit!(v); // schreibt Datei -> panel-update -> set() rendert
    };
    const save = () => void flushEditor();
    const cancel = () => {
      const p = pending;
      leave();
      if (p !== null) applyText(p);
      else draw();
    };
    editBtn.addEventListener("click", () => (editing ? save() : enter()));
    editor.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        save();
      } else if (e.key === "Escape") {
        e.preventDefault();
        cancel();
      }
    });
  }

  function applyText(text: string) {
    rawText = text;
    if (titleEl && titleEl.getAttribute("contenteditable") !== "true") {
      titleEl.textContent = firstLine(text);
    }
    draw();
  }

  draw();
  return {
    set(text: string) {
      // Während einer Inhalts-Bearbeitung nicht überschreiben — puffern und den
      // Edit-Button als „geändert" markieren.
      if (editing) {
        pending = text;
        opts.editContentBtn?.classList.add("changed");
        return;
      }
      applyText(text);
    },
    raw: () => rawText,
    flush: () => flushEditor(),
  };
}
