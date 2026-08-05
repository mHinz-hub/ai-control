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
  /// Angaben des Titelblatts aus dem OPF, Feld und Wert in ihrer Reihenfolge.
  meta?: [string, string][];
  layout: string;
  spine: EpubPage[];
  toc: EpubTocItem[];
}

/// Deutsche Namen der Dublin-Core-Felder; was hier fehlt, steht mit seinem
/// eigenen Namen da — ein unbekanntes Feld zu verschweigen wäre schlimmer.
const FELDNAME: Record<string, string> = {
  title: "Titel",
  creator: "Verfasser",
  contributor: "Beteiligt",
  publisher: "Verlag",
  date: "Jahr",
  identifier: "Kennung",
  language: "Sprache",
  rights: "Rechte",
  description: "Beschreibung",
  subject: "Schlagwort",
  source: "Vorlage",
  relation: "Bezug",
};

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

/// Wohin der Sprung aus der Suche führt: Kapitel-Href und die Wörter, die
/// darin zu markieren sind.
export interface EpubSprung {
  href: string;
  woerter: string[];
}

export function renderEpub(book: EpubBook, sprung?: EpubSprung): HTMLElement {
  const root = document.createElement("div");
  root.className = "epub";
  let index = 0;
  /// Beim Laden ans Ende der Seite springen — gesetzt, wenn man rückwärts
  /// über die Kapitelgrenze geblättert ist.
  let anDenFuss = false;
  /// Schriftgrad der Buchseite in Prozent.
  let schrift = 100;
  /// Marke, zu der die nächste geladene Seite springen soll.
  let zielMarke: string | undefined;

  // ---------- Inhaltsverzeichnis ----------
  const toc = document.createElement("nav");
  toc.className = "epub-toc";
  const kopf = document.createElement("div");
  kopf.className = "epub-toc-kopf";
  kopf.textContent = book.title;
  toc.append(kopf);
  // Je Zeile das Kapitel, auf das sie zeigt: daran erkennt die Anzeige, wo im
  // Buch der Leser gerade steht.
  const zeilen: { row: HTMLButtonElement; seite: number }[] = [];
  for (const item of book.toc) {
    const row = document.createElement("button");
    row.className = `epub-toc-item stufe${Math.min(item.level, 3)}`;
    row.style.paddingLeft = `${10 + item.level * 14}px`;
    row.textContent = item.title;
    const seite = pageOf(book, item.href);
    zeilen.push({ row, seite });
    row.addEventListener("click", () => {
      if (seite >= 0) show(seite, item.href.split("#")[1]);
      // Der Fokus gehört zurück an den Viewer: sonst bliebe er auf der Zeile
      // liegen, mit ihrem Rahmen, und die Pfeiltasten führen ins Verzeichnis
      // statt durch das Buch.
      root.focus();
    });
    toc.append(row);
  }

  /// Die Zeile des Kapitels hervorheben, in dem der Leser steht — das ist die
  /// letzte, deren Ziel nicht hinter der aktuellen Seite liegt.
  function markiereToc() {
    let treffer = -1;
    zeilen.forEach(({ seite }, i) => {
      if (seite >= 0 && seite <= index) treffer = i;
    });
    zeilen.forEach(({ row }, i) => row.classList.toggle("hier", i === treffer));
    // Wer sich durch ein langes Buch blättert, soll die Stelle im Verzeichnis
    // sehen, ohne dort zu suchen.
    zeilen[treffer]?.row.scrollIntoView({ block: "nearest" });
  }
  // Ein Buch ohne Nav und ohne NCX hat kein Inhaltsverzeichnis — dann bleibt
  // die Spalte weg, statt leer zu stehen.
  toc.hidden = book.toc.length === 0;

  // ---------- Seitenfläche ----------
  const stage = document.createElement("div");
  stage.className = "epub-stage";
  const frame = document.createElement("iframe");
  frame.className = "epub-frame";
  // Formulare und Navigation der Seite bleiben draußen, ihre Stylesheets,
  // Bilder und Schriften laden weiter. Skripte laufen, weil der Kern der
  // Seite eines mitgibt — das Blättern im Kapitel braucht den Scrollstand,
  // und ohne `allow-same-origin` bleibt die Seite in ihrem eigenen Ursprung:
  // an die App kommt sie nicht heran. Was das Buch selbst an Skripten
  // mitbringt, sperrt die Regel aus, die der Kern mitliefert.
  frame.setAttribute("sandbox", "allow-scripts");
  // Blättern am Rand der Seite: dieselbe Bewegung wie mit den Pfeiltasten,
  // für die Hand an der Maus. Sie liegen über der Seite und treten erst
  // hervor, wenn der Zeiger in ihre Nähe kommt.
  const zurueck = document.createElement("button");
  zurueck.className = "epub-blaettern links";
  zurueck.textContent = "‹";
  zurueck.title = t("epub.prev");
  const vor = document.createElement("button");
  vor.className = "epub-blaettern rechts";
  vor.textContent = "›";
  vor.title = t("epub.next");
  zurueck.addEventListener("click", () => {
    blaettern(-1);
    root.focus();
  });
  vor.addEventListener("click", () => {
    blaettern(1);
    root.focus();
  });
  stage.append(frame, zurueck, vor);

  // ---------- Fußzeile ----------
  const bar = document.createElement("div");
  bar.className = "epub-bar";
  const prev = document.createElement("button");
  prev.className = "epub-nav epub-prev";
  prev.textContent = "‹";
  prev.title = t("epub.prev");
  const next = document.createElement("button");
  next.className = "epub-nav epub-next";
  next.textContent = "›";
  next.title = t("epub.next");
  const count = document.createElement("span");
  count.className = "epub-count";
  // Die Druckseite, auf der der Leser steht — die Angabe, mit der zitiert
  // wird. Sie kommt aus den Marken im Satz, nicht aus der Zählung der Datei.
  const druckseite = document.createElement("span");
  druckseite.className = "epub-seite";
  const meta = document.createElement("span");
  meta.className = "epub-meta";
  meta.textContent = [book.creator, book.language].filter(Boolean).join(" · ");

  // ---------- Angaben zum Buch ----------
  // Ein Zitat braucht Verlag, Jahr und Auflage; die stehen im OPF, nicht auf
  // der Fußzeile. Der Knopf klappt sie auf — als Text, den man mit der Maus
  // greifen kann, nicht als Meldung, die wieder verschwindet.
  const info = document.createElement("button");
  info.className = "panel-btn epub-info";
  info.textContent = "ⓘ";
  info.title = t("epub.info");
  const angaben = document.createElement("div");
  angaben.className = "epub-angaben";
  angaben.hidden = true;
  const liste = document.createElement("dl");
  const felder: [string, string][] = book.meta?.length
    ? book.meta
    : ([["title", book.title], ["creator", book.creator ?? ""],
        ["language", book.language ?? ""]].filter((p) => p[1]) as [string, string][]);
  for (const [feld, wert] of felder) {
    const name = document.createElement("dt");
    name.textContent = FELDNAME[feld] ?? feld;
    const value = document.createElement("dd");
    value.textContent = wert;
    liste.append(name, value);
  }
  const kopie = document.createElement("button");
  kopie.className = "epub-kopie";
  kopie.textContent = t("epub.copy");
  kopie.addEventListener("click", () => {
    const text = felder.map(([f, w]) => `${FELDNAME[f] ?? f}: ${w}`).join("\n");
    void navigator.clipboard.writeText(text).then(() => {
      kopie.textContent = t("epub.copied");
      setTimeout(() => (kopie.textContent = t("epub.copy")), 1200);
    });
  });
  angaben.append(liste, kopie);
  info.addEventListener("click", () => (angaben.hidden = !angaben.hidden));

  // ---------- Schriftgröße ----------
  // Die Seite trägt keine feste Größe; der Grad des Wurzelelements zieht den
  // ganzen Satz mit. Gehalten wird er über den Kapitelwechsel hinweg — jede
  // neue Seite kommt in der Größe, in der die vorige stand.
  const kleiner = document.createElement("button");
  kleiner.className = "panel-btn epub-kleiner";
  kleiner.textContent = "A−";
  kleiner.title = t("epub.smaller");
  const groesser = document.createElement("button");
  groesser.className = "panel-btn epub-groesser";
  groesser.textContent = "A+";
  groesser.title = t("epub.larger");
  const stufe = (um: number) => {
    schrift = Math.max(70, Math.min(200, schrift + um));
    schriftSetzen();
    root.focus();
  };
  kleiner.addEventListener("click", () => stufe(-10));
  groesser.addEventListener("click", () => stufe(10));

  // ---------- Seitenweise lesen ----------
  // Der Fließtext kennt die Druckseite als Marke im Satz. Im Seitenmodus wird
  // sie zur Seite: der Abschnitt zwischen zwei Marken füllt die Fläche, und
  // geblättert wird von Druckseite zu Druckseite statt um eine Bildhöhe.
  const seitig = document.createElement("button");
  seitig.className = "panel-btn epub-seitig";
  seitig.textContent = "▤";
  seitig.title = t("epub.paged");
  let imSeitenmodus = false;
  seitig.addEventListener("click", () => {
    imSeitenmodus = !imSeitenmodus;
    seitig.classList.toggle("an", imSeitenmodus);
    // Im Seitenmodus sagt die Fläche selbst, wo die Seite endet — die Marken
    // im Satz sind dort überflüssig. Sie gehen aus und kommen zurück, wenn
    // der Modus endet; ihr Zustand bleibt gemerkt.
    markenZeigen(imSeitenmodus ? false : markenGewuenscht);
    marker.disabled = imSeitenmodus;
    frame.contentWindow?.postMessage({ ac: "seitig", an: imSeitenmodus }, "*");
    root.focus();
  });

  // ---------- Seitenumbrüche im Satz zeigen ----------
  // Wo im Druck die Seite umbricht, steht im Fließtext eine Marke — sichtbar
  // gemacht als senkrechter Strich mit der Zahl, wie in kritischen Ausgaben.
  // Damit ist auch mitten im Absatz zu sehen, wo eine Seite endet.
  const marker = document.createElement("button");
  marker.className = "panel-btn epub-marker";
  marker.textContent = "¶";
  marker.title = t("epub.marks");
  /// Was der Leser eingestellt hat — und was gerade gilt. Im Seitenmodus
  /// weicht beides voneinander ab.
  let markenGewuenscht = false;
  let markenAn = false;

  function markenZeigen(an: boolean) {
    markenAn = an;
    marker.classList.toggle("an", an);
    frame.contentWindow?.postMessage({ ac: "marker", an }, "*");
  }

  marker.addEventListener("click", () => {
    markenGewuenscht = !markenGewuenscht;
    markenZeigen(markenGewuenscht);
    root.focus();
  });

  // ---------- Inhaltsverzeichnis ein- und ausklappen ----------
  // Auf einem schmalen Fenster nimmt das Verzeichnis die Hälfte der Breite;
  // wer liest, braucht es nicht ständig.
  const klapp = document.createElement("button");
  klapp.className = "panel-btn epub-klapp";
  klapp.textContent = "☰";
  klapp.title = t("epub.toc");
  klapp.addEventListener("click", () => {
    toc.hidden = !toc.hidden;
    klapp.classList.toggle("zu", toc.hidden);
    root.focus();
  });

  // ---------- Tag und Nacht ----------
  // Das Buch setzt keine Farben — es erbt den weißen Grund des Rahmens. Für
  // dunkles Lesen wird darum die ganze Seite umgekehrt statt in jeder Datei
  // eine zweite Fassung vorzuhalten. Der Drehung des Farbtons hinterher bleibt
  // Blau blau und Rot rot; nur hell und dunkel tauschen.
  const nacht = document.createElement("button");
  nacht.className = "panel-btn epub-nacht";
  nacht.textContent = "◐";
  nacht.title = t("epub.night");
  nacht.addEventListener("click", () => {
    const an = stage.classList.toggle("nacht");
    nacht.textContent = an ? "◑" : "◐";
    nacht.title = t(an ? "epub.day" : "epub.night");
  });

  prev.addEventListener("click", () => {
    show(index - 1);
    root.focus();
  });
  next.addEventListener("click", () => {
    show(index + 1);
    root.focus();
  });
  // Blättern sitzt oben rechts, die Buchangabe links davon.
  bar.append(meta, druckseite, klapp, seitig, marker, kleiner, groesser, info,
             nacht, prev, count, next);

  const layout = document.createElement("div");
  layout.className = "epub-layout";
  layout.append(toc, stage);
  root.append(bar, angaben, layout);

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

  /// Blättern, wie man in einem Buch blättert: eine Bildhöhe weiter, und erst
  /// am Fuß des Kapitels ins nächste. Die Seite entscheidet, ob noch Platz
  /// ist — sie steht in einem anderen Ursprung, ihr Scrollstand ist von hier
  /// nicht zu lesen. Kommt sie an ihren Rand, meldet sie es zurück.
  function blaettern(richtung: number) {
    frame.contentWindow?.postMessage({ ac: "blaettern", richtung }, "*");
  }

  function schriftSetzen() {
    frame.contentWindow?.postMessage({ ac: "schrift", wert: schrift }, "*");
  }

  /// Am Rand des Kapitels angekommen: ins nächste. Rückwärts landet man an
  /// dessen Fuß, nicht an seinem Kopf — sonst übersprünge ein Schritt zurück
  /// alles, was man gerade gelesen hat.
  function kapitelWechsel(richtung: number) {
    if (richtung < 0 && index > 0) {
      anDenFuss = true;
      show(index - 1);
    } else if (richtung > 0 && index < book.spine.length - 1) {
      show(index + 1);
    }
  }

  function show(target: number, fragment?: string, woerter?: string[]) {
    index = Math.max(0, Math.min(book.spine.length - 1, target));
    const page = book.spine[index];
    // Die Fundstellen setzt der Kern beim Ausliefern ein; die erste trägt die
    // Sprungmarke `ac-hit`, zu der der Browser von selbst scrollt.
    const treffer = woerter?.length
      ? `?hit=${woerter.map(encodeURIComponent).join(",")}`
      : "";
    const marke = fragment ?? (woerter?.length ? "ac-hit" : undefined);
    zielMarke = marke;
    frame.src = bookBase(book.key) + page.href + treffer + (marke ? `#${marke}` : "");
    count.textContent = `${index + 1} / ${book.spine.length}`;
    prev.disabled = index === 0;
    next.disabled = index === book.spine.length - 1;
    markiereToc();
    fit();
    blaetterbar();
  }

  /// Die Pfeile an der Seite folgen dem, was wirklich geht: im ersten Kapitel
  /// weiter unten führt ein Schritt zurück nach oben, nicht ins Nichts. Wo
  /// die Seite steht, sagt sie selbst — bis zur ersten Meldung gilt ihr Kopf.
  /// Im Seitenmodus scrollt nichts; dort zählt, die wievielte Druckseite des
  /// Kapitels im Bild steht.
  function blaetterbar(oben = 0, rand = 0, seitig = false, von = 0, bis = 0) {
    const amKopf = seitig ? von <= 1 : oben <= 2;
    const amFuss = seitig ? von >= bis : oben >= rand - 2;
    zurueck.disabled = index === 0 && amKopf;
    vor.disabled = index === book.spine.length - 1 && amFuss;
  }

  /// Was die Fußzeile über die Stelle im Buch sagt: die Druckseite, und im
  /// Seitenmodus zusätzlich, die wievielte des Kapitels sie ist.
  /// Welche Druckseiten gerade im Bild stehen — eine oder zwei, je nachdem,
  /// wo der Umbruch liegt. Die Angabe ist die, mit der zitiert wird, und
  /// steht darum immer da. Der Stand im Kapitel kommt nur im Seitenmodus
  /// dazu: dort ist das Blättern gezählt, im Fließtext wäre es eine Zahl
  /// ohne Bezug.
  function stelleZeigen(seiten?: string[], von?: number, bis?: number) {
    if (!seiten?.length) {
      druckseite.textContent = "";
      return;
    }
    const wo = `S. ${seiten.join(" / ")}`;
    druckseite.textContent = imSeitenmodus && von && bis
      ? `${wo}  ·  Seite ${von} von ${bis}`
      : wo;
  }

  /// Zum nächsten oder vorigen Eintrag des Inhaltsverzeichnisses springen —
  /// eine Stufe gröber als das Blättern: Kapitel für Kapitel statt Seite für
  /// Seite. Gemessen wird an der Seite, auf der der Leser steht, nicht am
  /// zuletzt angeklickten Eintrag; er kann sich dorthin geblättert haben.
  function kapitelSprung(richtung: number) {
    const ziele = zeilen
      .map(({ seite }, i) => ({ seite, item: book.toc[i] }))
      .filter((z) => z.seite >= 0);
    const naechstes = richtung > 0
      ? ziele.find((z) => z.seite > index)
      : [...ziele].reverse().find((z) => z.seite < index);
    if (naechstes) show(naechstes.seite, naechstes.item.href.split("#")[1]);
  }

  frame.addEventListener("load", () => {
    fit();
    if (schrift !== 100) schriftSetzen();
    // Modus und Marker gelten dem Buch, nicht der einzelnen Seite: sie
    // überleben den Kapitelwechsel.
    if (markenAn) frame.contentWindow?.postMessage({ ac: "marker", an: true }, "*");
    if (imSeitenmodus) frame.contentWindow?.postMessage({ ac: "seitig", an: true }, "*");
    // Rückwärts geblättert: die neue Seite beginnt an ihrem Fuß.
    if (anDenFuss) {
      anDenFuss = false;
      frame.contentWindow?.postMessage({ ac: "anDenFuss" }, "*");
    } else if (zielMarke) {
      // Der Sprung über das Adressfragment fällt bei einem langen Kapitel in
      // den Augenblick, in dem der Satz noch nicht steht: die Seite landet an
      // einer Stelle, die es danach nicht mehr gibt, und bleibt leer. Sie
      // springt darum noch einmal, wenn sie fertig ist.
      frame.contentWindow?.postMessage({ ac: "marke", id: zielMarke }, "*");
    }
    zielMarke = undefined;
  });
  // Was die Buchseite meldet: ihren Scrollstand nach jeder Bewegung, und daß
  // sie an ihrem Rand steht — dann übernimmt der Viewer und wechselt das
  // Kapitel. Angenommen wird nur, was aus dem eigenen Rahmen kommt.
  window.addEventListener("message", (e) => {
    if (e.source !== frame.contentWindow) return;
    const d = e.data as {
      ac?: string;
      oben?: number;
      rand?: number;
      richtung?: number;
      key?: string;
      shift?: boolean;
      seite?: string;
      seiten?: string[];
      seitig?: boolean;
      von?: number;
      bis?: number;
    };
    if (d?.ac === "stand") {
      blaetterbar(d.oben ?? 0, d.rand ?? 0, !!d.seitig, d.von ?? 0, d.bis ?? 0);
      stelleZeigen(d.seiten ?? (d.seite ? [d.seite] : []), d.von, d.bis);
    }
    else if (d?.ac === "rand") kapitelWechsel(d.richtung ?? 0);
    else if (d?.ac === "taste") taste(d.key ?? "", d.shift ?? false);
  });
  new ResizeObserver(fit).observe(stage);
  // Blättern mit den Pfeiltasten; mit Umschalt eine Stufe gröber, von Kapitel
  // zu Kapitel.
  root.tabIndex = 0;
  /// Eine Pfeiltaste — gleich, ob sie im Viewer ankam oder auf der Buchseite,
  /// die sie herüberreicht. Ohne diesen zweiten Weg hörte das Blättern auf,
  /// sobald der Leser einmal ins Buch geklickt hat: der Fokus liegt dann im
  /// Rahmen, und der gibt nichts nach außen.
  function taste(key: string, shift: boolean): boolean {
    const vorwaerts = key === "ArrowRight" || key === "PageDown";
    const rueckwaerts = key === "ArrowLeft" || key === "PageUp";
    if (!vorwaerts && !rueckwaerts) return false;
    if (shift) kapitelSprung(vorwaerts ? 1 : -1);
    else blaettern(vorwaerts ? 1 : -1);
    return true;
  }
  root.addEventListener("keydown", (e) => {
    if (taste(e.key, e.shiftKey)) e.preventDefault();
  });
  // Aus der Suche: das Kapitel des Treffers, sonst der Anfang.
  const ziel = sprung ? pageOf(book, sprung.href) : -1;
  if (ziel >= 0) show(ziel, undefined, sprung!.woerter);
  else show(0);
  // Damit die Pfeiltasten sofort greifen, ohne erst hineinklicken zu müssen.
  requestAnimationFrame(() => root.focus());
  return root;
}
