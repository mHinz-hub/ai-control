/// Kachel-Ansicht der Archiv-Suchtreffer: rendert die Suchtreffer-Datei
/// (search_archive im MCP-Server) als klickbare Kacheln — Klick reicht den
/// absoluten Pfad und den relpath des Treffers an onOpen. Titel/Snippet/Pfad sind Fremdtext und
/// gehen nie durch innerHTML; die `**…**`-Marker im Snippet werden per
/// Split in <mark>-Elemente übersetzt.

import { t } from "./messages";
import { renderTile } from "./tiles";

interface Hit {
  id: string;
  relpath: string;
  title: string;
  /// Getroffenes Feld: `text` (Rumpf) oder `title`, `tags`, `description`,
  /// `name`.
  field?: string;
  snippet: string;
}

/// Beschriftung des getroffenen Feldes; der Rumpf braucht keine, dort ist der
/// Ausschnitt selbst die Auskunft.
const FELD_LABEL: Record<string, string> = {
  title: "search.fieldTitle",
  tags: "search.fieldTags",
  description: "search.fieldDescription",
  name: "search.fieldName",
};

interface SearchRun {
  query: string;
  tag?: string | null;
  home: string;
  hits: Hit[];
}

export interface SearchView {
  set(text: string): void;
  empty(): boolean;
}

/// Die Wörter, die FTS5 im Ausschnitt markiert hat — Grundlage der
/// Hervorhebung im geöffneten Dokument. Die Roh-Eingabe taugt dafür nicht:
/// Bei `arch*` steht dort das Muster, im Text aber „Archiv".
function marken(snippet: string): string[] {
  return snippet.split("**").filter((_, i) => i % 2);
}

export function initSearchView(
  container: HTMLElement,
  onOpen: (path: string, relpath: string, id: string, marken: string[]) => void,
  onSearch: (query: string) => void,
): SearchView {
  let count = 0;

  // Suchfeld oben, Treffer darunter; das Feld bleibt über Updates hinweg stehen.
  const bar = document.createElement("div");
  bar.className = "hit-search";
  const input = document.createElement("input");
  input.type = "search";
  input.placeholder = t("search.placeholder");
  // Live-Suche ab 3 Zeichen, entprellt (300 ms Debounce); endet die Eingabe
  // mitten im Wort, wird das letzte Wort als Präfix gesucht (arch → arch*) —
  // außer es ist ein #tag-Token. Enter sucht sofort und wörtlich — für exakte
  // FTS-Syntax (Phrasen, OR/NOT).
  let pending: number | undefined;
  input.addEventListener("input", () => {
    clearTimeout(pending);
    const q = input.value.trim();
    if (q.length < 3) {
      // Beim Löschen unter die Schwelle: alte Treffer weg, kurzer Hinweis.
      count = 0;
      results.textContent = "";
      if (q) {
        const head = document.createElement("div");
        head.className = "hit-head";
        head.textContent = t("search.minChars");
        results.append(head);
      }
      return;
    }
    const last = q.split(/\s+/).pop()!;
    const live =
      !last.startsWith("#") && /[\p{L}\p{N}]$/u.test(q) ? `${q}*` : q;
    pending = window.setTimeout(() => onSearch(live), 300);
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && input.value.trim()) {
      clearTimeout(pending);
      onSearch(input.value.trim());
    }
  });
  bar.append(input);
  const results = document.createElement("div");
  container.append(bar, results);

  function snippetEl(snippet: string): HTMLElement {
    const div = document.createElement("div");
    div.className = "hit-snippet";
    snippet.split("**").forEach((part, i) => {
      if (i % 2) {
        const m = document.createElement("mark");
        m.textContent = part;
        div.append(m);
      } else if (part) {
        div.append(document.createTextNode(part));
      }
    });
    return div;
  }

  function render(run: SearchRun) {
    results.textContent = "";
    const head = document.createElement("div");
    head.className = "hit-head";
    const what = run.query.trim() ? `„${run.query}“` : "";
    const tag = run.tag ? `#${run.tag}` : "";
    const scope = [what, tag].filter(Boolean).join(" · ");
    head.textContent = run.hits.length
      ? t("search.hits", { count: run.hits.length, scope })
      : t("search.noHits", { scope });
    results.append(head);
    for (const hit of run.hits) {
      // Ein Treffer außerhalb des Rumpfs sagt, wo er steckt — und bringt
      // keine Fundstelle mit, die sich im Dokument markieren ließe.
      const feld = hit.field && hit.field !== "text" ? FELD_LABEL[hit.field] : null;
      const teile: (HTMLElement | { cls: string; text: string })[] = [
        { cls: "hit-title", text: hit.title },
      ];
      if (feld) teile.push({ cls: "hit-field", text: t(feld) });
      teile.push(snippetEl(hit.snippet), { cls: "hit-path", text: hit.relpath });
      results.append(
        renderTile({
          cls: "hit-tile",
          parts: teile,
          onClick: () =>
            onOpen(
              `${run.home}/${hit.relpath}`,
              hit.relpath,
              hit.id,
              feld ? [] : marken(hit.snippet),
            ),
        }),
      );
    }
  }

  return {
    set(text: string) {
      if (!text.trim()) {
        count = 0;
        results.textContent = "";
        return;
      }
      const run: SearchRun = JSON.parse(text);
      count = run.hits.length;
      if (document.activeElement !== input) input.value = run.query;
      render(run);
    },
    empty: () => count === 0,
  };
}
