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
  /// Kapitel innerhalb der Datei (Bücher); sonst leer.
  teil?: string;
  /// Buchtitel bei einem Kapitel-Treffer; sonst leer.
  buch?: string;
  /// Fundstellen im Dokument bzw. Kapitel.
  count?: number;
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
  /// Fokus ins Suchfeld — wer die Suche öffnet, will tippen.
  focus(): void;
}

/// Die Wörter, die FTS5 im Ausschnitt markiert hat — Grundlage der
/// Hervorhebung im geöffneten Dokument. Die Roh-Eingabe taugt dafür nicht:
/// Bei `arch*` steht dort das Muster, im Text aber „Archiv".
function marken(snippet: string): string[] {
  return snippet.split("**").filter((_, i) => i % 2);
}

/// Eine Fundstelle im Dokument: laufende Nummer, Druckseite, Lage auf der
/// Seite und der Satz drumherum.
export interface Stelle {
  nr: number;
  seite: string;
  lage: string;
  zeile: string;
}

export function initSearchView(
  container: HTMLElement,
  onOpen: (
    path: string,
    relpath: string,
    id: string,
    marken: string[],
    teil: string,
    nr?: number,
  ) => void,
  onSearch: (query: string) => void,
  stellen?: (id: string, teil: string, query: string) => Promise<Stelle[]>,
): SearchView {
  let count = 0;

  // Suchfeld oben, Treffer darunter; das Feld bleibt über Updates hinweg stehen.
  const bar = document.createElement("div");
  bar.className = "hit-search";
  const input = document.createElement("input");
  input.type = "search";
  input.placeholder = t("search.placeholder");
  // Live-Suche ab 3 Zeichen, entprellt (300 ms Debounce). Ein Stern wird
  // nicht mehr angehängt: Der Index tokenisiert in Trigramme, jede Eingabe
  // trifft damit von sich aus auch Wortteile. Enter sucht sofort.
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
    pending = window.setTimeout(() => onSearch(q), 300);
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
  // Wer die Suche öffnet, will tippen. Der Fokus wartet einen Rahmen ab: vor
  // dem Einhängen in die Seite nimmt ihn kein Feld an.
  requestAnimationFrame(() => input.focus());

  /// Die Kopfzeile eines Treffers: Bei einem Kapitel steht das Buch davor —
  /// „Teil II" allein sagt nichts, sobald mehr als ein Band im Archiv liegt.
  function herkunft(hit: Hit): string {
    return hit.buch && hit.title ? `${hit.buch} › ${hit.title}` : hit.buch || hit.title;
  }

  /// Wo der Treffer liegt: der Pfad unter dem Archiv-Home, mit › statt /.
  /// Beim Buch bleibt der Dateiname weg, sein Titel steht schon in der
  /// Kopfzeile; sonst fällt nur die Endung.
  function ordner(hit: Hit): string {
    const teile = hit.relpath.split("/");
    if (hit.buch) teile.pop();
    else teile.push(teile.pop()!.replace(/\.[^.]+$/, ""));
    return teile.join(" › ");
  }

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
    // Gekappt wird nichts; ab tausend Treffern ist die Anfrage aber zu weit,
    // um die Liste noch zu lesen.
    if (run.hits.length >= 1000) {
      const warn = document.createElement("div");
      warn.className = "hit-warn";
      warn.textContent = t("search.tooMany");
      results.append(warn);
    }
    for (const hit of run.hits) {
      // Ein Treffer außerhalb des Rumpfs sagt, wo er steckt — und bringt
      // keine Fundstelle mit, die sich im Dokument markieren ließe.
      const feld = hit.field && hit.field !== "text" ? FELD_LABEL[hit.field] : null;
      const teile: (HTMLElement | { cls: string; text: string })[] = [
        { cls: "hit-title", text: herkunft(hit) },
      ];
      if (feld) teile.push({ cls: "hit-field", text: t(feld) });
      // Ein Treffer ist ein Dokument oder Kapitel; die Zahl sagt, wie viel
      // darin steckt.
      else if ((hit.count ?? 0) > 1)
        teile.push({ cls: "hit-count", text: t("search.spots", { count: hit.count! }) });
      teile.push(snippetEl(hit.snippet), { cls: "hit-path", text: ordner(hit) });
      const kachel = renderTile({
        cls: "hit-tile",
        parts: teile,
        onClick: () =>
          onOpen(
            `${run.home}/${hit.relpath}`,
            hit.relpath,
            hit.id,
            feld ? [] : marken(hit.snippet),
            hit.teil ?? "",
          ),
      });
      results.append(kachel);
      // Fundstellen erst auf Verlangen: Ein Kapitel kann tausende haben.
      if (!feld && (hit.count ?? 0) > 1 && stellen) {
        const auf = document.createElement("button");
        auf.className = "hit-more";
        auf.textContent = t("search.showSpots");
        const liste = document.createElement("div");
        liste.className = "hit-spots";
        liste.hidden = true;
        auf.addEventListener("click", (e) => {
          e.stopPropagation();
          if (!liste.hidden) {
            liste.hidden = true;
            auf.textContent = t("search.showSpots");
            return;
          }
          liste.hidden = false;
          auf.textContent = t("search.hideSpots");
          if (liste.childElementCount) return;
          void stellen(hit.id, hit.teil ?? "", run.query).then((sts) => {
            for (const st of sts) {
              const zeile = document.createElement("div");
              zeile.className = "hit-spot";
              if (st.seite) {
                const s = document.createElement("span");
                s.className = "hit-spot-page";
                s.textContent = `S. ${st.seite}${st.lage ? ` ${st.lage}` : ""}`;
                zeile.append(s);
              }
              const txt = document.createElement("span");
              txt.className = "hit-spot-text";
              st.zeile.split("**").forEach((teil, i) => {
                if (i % 2) {
                  const m = document.createElement("mark");
                  m.textContent = teil;
                  txt.append(m);
                } else if (teil) txt.append(document.createTextNode(teil));
              });
              zeile.append(txt);
              zeile.addEventListener("click", () =>
                onOpen(
                  `${run.home}/${hit.relpath}`,
                  hit.relpath,
                  hit.id,
                  marken(hit.snippet),
                  hit.teil ?? "",
                  st.nr,
                ),
              );
              liste.append(zeile);
            }
          });
        });
        kachel.append(auf);
        results.append(liste);
      }
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
    focus: () => input.focus(),
  };
}
