/// ePub-Viewer der Archiv-Ansicht: Inhaltsverzeichnis links, Buchseite rechts.
///
/// Das Buch liegt entpackt im Cache und wird über das `epub://`-Protokoll
/// ausgeliefert; jede Seite kommt als eigenes Dokument in ein iframe, damit
/// ihre relativen Verweise (Bilder, Stylesheets, Schriften) auflösen und ihr
/// CSS nicht auf die App durchschlägt.
///
/// Zwei Bauarten, die das Buch selbst festlegt (OPF, `rendition:layout`):
///   reflowable    — fließender Text, die Seite scrollt im iframe
///   pre-paginated — feste Seiten (Comics, Bilderbücher, Scans): das iframe
///                   bekommt die Maße aus dem Viewport-Meta der Seite und wird
///                   auf die Fläche skaliert, damit das Layout stehen bleibt.

import { t } from "./messages";

export interface EpubPage {
  /// Pfad relativ zur Buchwurzel im Cache.
  href: string;
  /// Seite einer Doppelseite: "left"/"right".
  spread?: string | null;
  width?: number | null;
  height?: number | null;
}

export interface EpubTocItem {
  title: string;
  href: string;
  level: number;
}

export interface EpubBook {
  key: string;
  title: string;
  creator?: string | null;
  language?: string | null;
  layout: string;
  spine: EpubPage[];
  toc: EpubTocItem[];
}

/// Adressbasis des entpackten Buchs. Eigenes Protokoll — die Segmente bleiben
/// echte Pfadsegmente, sonst liefen die relativen Verweise der Seiten ins
/// Leere. WebKit (macOS, Linux) spricht das Schema direkt an; WebView2 kann
/// keine eigenen Schemata registrieren, dort läuft es über den http-Alias.
function bookBase(key: string): string {
  const windows = /Windows/.test(navigator.userAgent);
  const root = windows ? "http://epub.localhost/" : "epub://localhost/";
  return `${root}${key}/`;
}

/// Seitenindex zu einem Ziel aus dem Inhaltsverzeichnis (Fragment zählt
/// nicht — es zeigt in eine Seite hinein).
function pageOf(book: EpubBook, href: string): number {
  const path = href.split("#")[0];
  return book.spine.findIndex((p) => p.href === path);
}

export function renderEpub(book: EpubBook): HTMLElement {
  const root = document.createElement("div");
  root.className = "epub";
  let index = 0;

  // ---------- Inhaltsverzeichnis ----------
  const toc = document.createElement("nav");
  toc.className = "epub-toc";
  for (const item of book.toc) {
    const row = document.createElement("button");
    row.className = "epub-toc-item";
    row.style.paddingLeft = `${8 + item.level * 14}px`;
    row.textContent = item.title;
    row.addEventListener("click", () => {
      const target = pageOf(book, item.href);
      if (target >= 0) show(target, item.href.split("#")[1]);
    });
    toc.append(row);
  }
  // Ein Buch ohne Nav und ohne NCX hat kein Inhaltsverzeichnis — dann bleibt
  // die Spalte weg, statt leer zu stehen.
  toc.hidden = book.toc.length === 0;

  // ---------- Seitenfläche ----------
  const stage = document.createElement("div");
  stage.className = "epub-stage";
  const frame = document.createElement("iframe");
  frame.className = "epub-frame";
  // Angezeigt wird das Buch, ausgeführt nichts: leere Sandbox — Skripte,
  // Formulare und Navigation der Seite bleiben draußen, ihre eigenen
  // Stylesheets, Bilder und Schriften laden weiter.
  frame.setAttribute("sandbox", "");
  stage.append(frame);

  // ---------- Fußzeile ----------
  const bar = document.createElement("div");
  bar.className = "epub-bar";
  const prev = document.createElement("button");
  prev.className = "epub-nav";
  prev.textContent = "‹";
  prev.title = t("epub.prev");
  const next = document.createElement("button");
  next.className = "epub-nav";
  next.textContent = "›";
  next.title = t("epub.next");
  const count = document.createElement("span");
  count.className = "epub-count";
  const meta = document.createElement("span");
  meta.className = "epub-meta";
  meta.textContent = [book.creator, book.language].filter(Boolean).join(" · ");
  prev.addEventListener("click", () => show(index - 1));
  next.addEventListener("click", () => show(index + 1));
  // Blättern sitzt oben rechts, die Buchangabe links davon.
  bar.append(meta, prev, count, next);

  const layout = document.createElement("div");
  layout.className = "epub-layout";
  layout.append(toc, stage);
  root.append(bar, layout);

  /// Feste Seiten auf die Fläche skalieren: die Seite behält ihre Maße, nur
  /// der Maßstab wechselt — anders als beim Umbruch, der das Layout zerlegte.
  function fit() {
    const page = book.spine[index];
    if (book.layout !== "pre-paginated" || !page?.width || !page?.height) {
      frame.style.transform = "";
      frame.style.width = "";
      frame.style.height = "";
      return;
    }
    frame.style.width = `${page.width}px`;
    frame.style.height = `${page.height}px`;
    const scale = Math.min(
      stage.clientWidth / page.width,
      stage.clientHeight / page.height,
    );
    frame.style.transform = `scale(${scale})`;
  }

  function show(target: number, fragment?: string) {
    index = Math.max(0, Math.min(book.spine.length - 1, target));
    const page = book.spine[index];
    frame.src = bookBase(book.key) + page.href + (fragment ? `#${fragment}` : "");
    count.textContent = `${index + 1} / ${book.spine.length}`;
    prev.disabled = index === 0;
    next.disabled = index === book.spine.length - 1;
    fit();
  }

  frame.addEventListener("load", fit);
  new ResizeObserver(fit).observe(stage);
  // Blättern mit den Pfeiltasten, sobald der Viewer den Fokus hat.
  root.tabIndex = 0;
  root.addEventListener("keydown", (e) => {
    if (e.key === "ArrowRight" || e.key === "PageDown") show(index + 1);
    else if (e.key === "ArrowLeft" || e.key === "PageUp") show(index - 1);
  });
  show(0);
  return root;
}
