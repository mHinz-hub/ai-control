/// Fundstellen im geöffneten Dokument markieren.
///
/// Der Suchtreffer bringt die Wörter mit, die FTS5 im Ausschnitt markiert hat
/// — nicht die Roh-Eingabe. Damit trifft die Hervorhebung genau das, was die
/// Suche gefunden hat, auch bei Präfix-Suche (`arch*` → „Archiv").

/// Umschließt jedes Vorkommen der Wörter im Text unterhalb von `root` mit
/// `<mark class="wiki-hit">` und liefert die erste Marke — das Ziel zum
/// Hinscrollen. Groß- und Kleinschreibung spielen keine Rolle.
export function markiere(root: HTMLElement, woerter: string[]): HTMLElement | null {
  const gesucht = [...new Set(woerter.map((w) => w.trim().toLowerCase()).filter(Boolean))];
  if (!gesucht.length) return null;
  let erste: HTMLElement | null = null;

  const lauf = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const knoten: Text[] = [];
  for (let n = lauf.nextNode(); n; n = lauf.nextNode()) knoten.push(n as Text);

  for (const k of knoten) {
    const text = k.nodeValue ?? "";
    const klein = text.toLowerCase();
    // Alle Fundstellen dieses Textknotens, links nach rechts, ohne Überlappung.
    const stellen: { von: number; bis: number }[] = [];
    for (const w of gesucht) {
      let i = klein.indexOf(w);
      while (i >= 0) {
        stellen.push({ von: i, bis: i + w.length });
        i = klein.indexOf(w, i + w.length);
      }
    }
    if (!stellen.length) continue;
    stellen.sort((a, b) => a.von - b.von);

    const teile = document.createDocumentFragment();
    let pos = 0;
    for (const s of stellen) {
      if (s.von < pos) continue;
      if (s.von > pos) teile.append(text.slice(pos, s.von));
      const m = document.createElement("mark");
      m.className = "wiki-hit";
      m.textContent = text.slice(s.von, s.bis);
      teile.append(m);
      erste ??= m;
      pos = s.bis;
    }
    if (pos < text.length) teile.append(text.slice(pos));
    k.parentNode?.replaceChild(teile, k);
  }
  return erste;
}
