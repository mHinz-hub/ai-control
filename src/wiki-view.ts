/// Wiki-Ansicht des Panels: rendert den Wiki-Puffer (show_archive/wiki_open).
/// Übersichts- und Schlagwort-Seiten kommen als strukturierte Daten (Kopfzeile,
/// Schlagwort-Chips, Ordner-Sektionen mit Dokumentzeilen), Dokumente als
/// gerendertes Markdown mit Backlinks. Titel, Beschreibungen und Pfade sind
/// Fremdtext und gehen nie durch innerHTML; nur der Markdown-Rumpf läuft wie
/// im Entwurf durch marked.

import { renderMarkdown } from "./markdown";
import { linkWikiRefs } from "./panel-view";
import { t } from "./messages";

interface DocEntry {
  name: string;
  title: string;
  description?: string | null;
  tags: string[];
  date?: string | null;
  backlinks: number;
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
  recent: DocEntry[];
  folders: Folder[];
}

interface DocPage {
  kind: "doc";
  home: string;
  relpath: string;
  name: string;
  title: string;
  tags: string[];
  backlinks: string[];
  markdown: string;
}

export interface WikiView {
  set(text: string): void;
  /// Noch keine Seite im Puffer (Session-Start)?
  empty(): boolean;
}

export function initWikiView(
  container: HTMLElement,
  onLink: (name: string) => void,
): WikiView {
  function chip(label: string, target: string, active = false): HTMLElement {
    const b = document.createElement("button");
    b.className = "wiki-chip" + (active ? " active" : "");
    b.textContent = label;
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      onLink(target);
    });
    return b;
  }

  /// Kopfzeile einer Seite: Titel + Home-Pfad links, Dokumentzahl rechts.
  function pageHead(p: Page): HTMLElement {
    const head = document.createElement("div");
    head.className = "wiki-head";
    const left = document.createElement("div");
    left.className = "wiki-head-left";
    const title = document.createElement("div");
    title.className = "wiki-head-title";
    title.textContent = p.tag ? `#${p.tag}` : t("wiki.archive");
    const sub = document.createElement("div");
    sub.className = "wiki-head-sub";
    sub.textContent = p.home;
    left.append(title, sub);
    const count = document.createElement("div");
    count.className = "wiki-head-right";
    count.textContent = t(p.total === 1 ? "wiki.docOne" : "wiki.docMany", { count: p.total });
    head.append(left, count);
    return head;
  }

  /// Schlagwort-Leiste: „Alle“ plus ein Chip pro Schlagwort mit Zähler; der
  /// aktive Filter ist markiert.
  function tagBar(p: Page): HTMLElement {
    const bar = document.createElement("div");
    bar.className = "wiki-chips";
    bar.append(chip(t("wiki.all"), "tag:", !p.tag));
    for (const t of p.tags) {
      bar.append(chip(`#${t.name} ${t.count}`, `tag:${t.name}`, p.tag === t.name));
    }
    return bar;
  }

  function docRow(doc: DocEntry): HTMLElement {
    const row = document.createElement("div");
    row.className = "wiki-doc";
    const line = document.createElement("div");
    line.className = "wiki-doc-line";
    const title = document.createElement("div");
    title.className = "wiki-doc-title";
    title.textContent = doc.title;
    line.append(title);
    if (doc.backlinks) {
      const back = document.createElement("div");
      back.className = "wiki-doc-back";
      back.title = t("wiki.backlinks");
      back.textContent = `↩ ${doc.backlinks}`;
      line.append(back);
    }
    if (doc.date) {
      const date = document.createElement("div");
      date.className = "wiki-doc-date";
      date.textContent = doc.date;
      line.append(date);
    }
    row.append(line);
    if (doc.description) {
      const desc = document.createElement("div");
      desc.className = "wiki-doc-desc";
      desc.textContent = doc.description;
      row.append(desc);
    }
    if (doc.tags.length) {
      const tags = document.createElement("div");
      tags.className = "wiki-doc-tags";
      for (const t of doc.tags) tags.append(chip(`#${t}`, `tag:${t}`));
      row.append(tags);
    }
    row.addEventListener("click", () => onLink(doc.name));
    return row;
  }

  function renderPage(p: Page) {
    container.append(pageHead(p));
    if (p.tags.length) container.append(tagBar(p));
    if (p.total === 0) {
      const empty = document.createElement("div");
      empty.className = "wiki-empty";
      const line = document.createElement("strong");
      line.textContent = p.tag ? t("wiki.emptyTag", { tag: p.tag }) : t("wiki.emptyArchive");
      empty.append(line);
      if (!p.tag) {
        empty.append(t("wiki.emptyHint"));
      }
      container.append(empty);
      return;
    }
    if (p.recent.length) {
      const eyebrow = document.createElement("div");
      eyebrow.className = "wiki-folder";
      eyebrow.textContent = t("wiki.recent");
      container.append(eyebrow);
      for (const doc of p.recent) container.append(docRow(doc));
    }
    for (const folder of p.folders) {
      if (folder.name) {
        const eyebrow = document.createElement("div");
        eyebrow.className = "wiki-folder";
        eyebrow.textContent = `${folder.name}/`;
        container.append(eyebrow);
      } else if (p.recent.length) {
        const eyebrow = document.createElement("div");
        eyebrow.className = "wiki-folder";
        eyebrow.textContent = t("wiki.root");
        container.append(eyebrow);
      }
      for (const doc of folder.docs) container.append(docRow(doc));
    }
  }

  function renderDoc(d: DocPage) {
    const head = document.createElement("div");
    head.className = "wiki-doc-head";
    const back = document.createElement("button");
    back.className = "wiki-back";
    back.textContent = t("wiki.back");
    back.addEventListener("click", () => onLink("tag:"));
    const path = document.createElement("div");
    path.className = "wiki-head-sub";
    path.textContent = d.relpath;
    head.append(back, path);
    container.append(head);
    if (d.tags.length) {
      const tags = document.createElement("div");
      tags.className = "wiki-chips";
      for (const t of d.tags) tags.append(chip(`#${t}`, `tag:${t}`));
      container.append(tags);
    }
    const body = document.createElement("div");
    body.className = "wiki-body";
    body.innerHTML = renderMarkdown(d.markdown);
    linkWikiRefs(body, onLink);
    container.append(body);
    if (d.backlinks.length) {
      const back = document.createElement("div");
      back.className = "wiki-backlinks";
      back.append(t("wiki.backlinksLabel"));
      d.backlinks.forEach((name, i) => {
        if (i) back.append(" · ");
        const a = document.createElement("a");
        a.href = "#";
        a.className = "wiki";
        a.textContent = name;
        a.addEventListener("click", (e) => {
          e.preventDefault();
          onLink(name);
        });
        back.append(a);
      });
      container.append(back);
    }
  }

  /// Leerer Puffer: Einstieg statt leerer Fläche.
  function renderIntro() {
    const empty = document.createElement("div");
    empty.className = "wiki-empty";
    const line = document.createElement("strong");
    line.textContent = t("wiki.noPage");
    empty.append(line);
    empty.append(chip(t("wiki.openOverview"), "tag:"));
    container.append(empty);
  }

  let loaded = false;
  return {
    set(text: string) {
      container.textContent = "";
      loaded = !!text.trim();
      if (!loaded) {
        renderIntro();
        return;
      }
      const data: Page | DocPage = JSON.parse(text);
      if (data.kind === "doc") renderDoc(data);
      else renderPage(data);
    },
    empty: () => !loaded,
  };
}
