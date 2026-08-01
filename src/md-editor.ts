/// Rohtext-Editor für Markdown-Notizen, auf CodeMirror 6.
///
/// Markdown bleibt die Quelle: Wer ein Buch setzt, braucht die Konstrukte, die
/// pandoc kennt (Fenced Divs, Spans, Attribute) — ein WYSIWYG könnte sie nur
/// abbilden, was er auch bedienen kann, und würde alles andere beim
/// Zurückschreiben verlieren. Der Editor macht den Rohtext darum lesbar,
/// statt ihn zu ersetzen: Auszeichnungszeichen treten zurück, Struktur tritt
/// hervor, und die Handgriffe (Betonung, Link, Liste, Tabelle) liegen auf
/// Tasten.

import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { xml } from "@codemirror/lang-xml";
import { yaml } from "@codemirror/lang-yaml";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState, StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  keymap,
  drawSelection,
  highlightActiveLine,
  type DecorationSet,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";

export interface MdEditor {
  el: HTMLElement;
  value(): string;
  /// Text an der Cursorstelle einsetzen, Cursor dahinter.
  insert(text: string): void;
  focus(): void;
  destroy(): void;
  /// Scroll-Stand als Anteil 0…1 — Grundlage der mitlaufenden Vorschau.
  scrollAnteil(): number;
}

/// Fortsetzung einer Listen- oder Zitatzeile: das Präfix, das die nächste
/// Zeile bekommt. `""` heißt „Liste hier beenden" (der Eintrag war leer),
/// `null` heißt „keine Liste".
export function listenPrefix(zeile: string): string | null {
  const m = /^(\s*)(?:([-*+])|(\d+)([.)])|(>))(\s+)(.*)$/.exec(zeile);
  if (!m) return null;
  const [, einzug, punkt, nummer, trenner, zitat, abstand, rest] = m;
  if (!rest.trim()) return "";
  if (punkt) return `${einzug}${punkt}${abstand}`;
  if (zitat) return `${einzug}${zitat}${abstand}`;
  return `${einzug}${Number(nummer) + 1}${trenner}${abstand}`;
}

/// Markdown-Tabelle als Text: Kopfzeile optional, Zellen breit genug, dass
/// die Striche im Rohtext eine Tabelle ergeben und nicht eine Zeichenkette.
export function mdTabelle(spalten: number, zeilen: number, kopf: boolean): string {
  const zelle = (s: string) => ` ${s.padEnd(8)} `;
  const reihe = (f: (i: number) => string) =>
    `|${Array.from({ length: spalten }, (_, i) => zelle(f(i))).join("|")}|`;
  const out: string[] = [];
  out.push(reihe((i) => (kopf ? `Spalte ${i + 1}` : "")));
  out.push(`|${Array.from({ length: spalten }, () => " -------- ").join("|")}|`);
  for (let i = 0; i < zeilen; i++) out.push(reihe(() => ""));
  return `${out.join("\n")}\n`;
}

/// Auszeichnung um die Auswahl legen oder wieder abnehmen (fett, kursiv,
/// Code). Ohne Auswahl setzt sie das Zeichenpaar und stellt den Cursor
/// dazwischen.
function umschliessen(view: EditorView, marke: string): boolean {
  const { from, to } = view.state.selection.main;
  const text = view.state.sliceDoc(from, to);
  const n = marke.length;
  const drin =
    view.state.sliceDoc(Math.max(0, from - n), from) === marke &&
    view.state.sliceDoc(to, to + n) === marke;
  if (drin) {
    view.dispatch({
      changes: [
        { from: from - n, to: from },
        { from: to, to: to + n },
      ],
      selection: { anchor: from - n, head: to - n },
    });
    return true;
  }
  view.dispatch({
    changes: { from, to, insert: `${marke}${text}${marke}` },
    selection: { anchor: from + n, head: from + n + text.length },
  });
  return true;
}

/// Link auf der Auswahl: `[text](…)`, Cursor landet im Ziel.
function linkSetzen(view: EditorView): boolean {
  const { from, to } = view.state.selection.main;
  const text = view.state.sliceDoc(from, to);
  view.dispatch({
    changes: { from, to, insert: `[${text}]()` },
    selection: { anchor: from + text.length + 3 },
  });
  return true;
}

/// Markierung der Fundstellen: einmal beim Öffnen gesetzt, bleibt beim
/// Tippen an ihrer Stelle (die Positionen wandern mit den Änderungen).
const setzeTreffer = StateEffect.define<DecorationSet>();
const treffer = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(alt, tr) {
    for (const e of tr.effects) if (e.is(setzeTreffer)) return e.value;
    return alt.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

/// Alle Vorkommen der Wörter im Text, in Lesereihenfolge — die Zählung, auf
/// die sich die Nummer der Fundstelle bezieht.
export function stellen(text: string, woerter: string[]): { von: number; bis: number }[] {
  const klein = text.toLowerCase();
  const out: { von: number; bis: number }[] = [];
  for (const w of new Set(woerter.map((x) => x.trim().toLowerCase()).filter(Boolean))) {
    let i = klein.indexOf(w);
    while (i >= 0) {
      out.push({ von: i, bis: i + w.length });
      i = klein.indexOf(w, i + w.length);
    }
  }
  return out.sort((a, b) => a.von - b.von);
}

/// Farben: Die Auszeichnungszeichen (`#`, `*`, `-`, Klammern) treten zurück,
/// der Text tritt hervor — sonst liest sich Markdown als Zeichensuppe.
const stil = HighlightStyle.define([
  { tag: tags.processingInstruction, color: "var(--faint)" },
  { tag: tags.heading1, color: "var(--accent)", fontWeight: "700", fontSize: "1.25em" },
  { tag: tags.heading2, color: "var(--accent)", fontWeight: "700", fontSize: "1.15em" },
  { tag: [tags.heading3, tags.heading4, tags.heading5, tags.heading6], color: "var(--accent)", fontWeight: "700" },
  { tag: tags.strong, fontWeight: "700", color: "var(--text)" },
  { tag: tags.emphasis, fontStyle: "italic", color: "var(--text)" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.url, color: "var(--accent)", textDecoration: "underline" },
  { tag: [tags.monospace, tags.literal], color: "var(--ok)" },
  { tag: tags.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: tags.list, color: "var(--warn)" },
  { tag: tags.contentSeparator, color: "var(--line-strong)" },
  // Daten-Formate: Schlüssel und Marken in Akzentfarbe, Werte abgesetzt.
  { tag: [tags.propertyName, tags.tagName], color: "var(--accent)" },
  { tag: tags.attributeName, color: "var(--muted)" },
  { tag: [tags.string, tags.attributeValue], color: "var(--ok)" },
  { tag: [tags.number, tags.bool, tags.null, tags.keyword], color: "var(--warn)" },
  { tag: tags.comment, color: "var(--faint)", fontStyle: "italic" },
  { tag: tags.invalid, color: "var(--err)" },
]);

/// Farben und Maße kommen aus den Theme-Variablen des Fensters — der Editor
/// sieht damit aus wie der Rest des Panels, auch nach einem Theme-Wechsel.
const thema = EditorView.theme(
  {
    "&": {
      color: "var(--text)",
      backgroundColor: "transparent",
      height: "100%",
      fontSize: "13px",
    },
    ".cm-content": {
      fontFamily: '"JetBrains Mono", Menlo, monospace',
      lineHeight: "1.6",
      padding: "12px 14px",
      caretColor: "var(--accent)",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
    ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--line) 45%, transparent)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
      backgroundColor: "color-mix(in srgb, var(--accent) 30%, transparent)",
    },
    // Fundstellen der Suche — dieselbe Unterlegung wie in der Anzeige.
    ".cm-hit": {
      backgroundColor: "color-mix(in srgb, var(--warn) 40%, transparent)",
      borderRadius: "3px",
    },
    ".cm-scroller": { overflow: "auto" },
    ".cm-scroller::-webkit-scrollbar": { width: "8px" },
    ".cm-scroller::-webkit-scrollbar-thumb": {
      background: "var(--line)",
      borderRadius: "4px",
    },
  },
  { dark: true },
);

/// Sprachen, die der Editor kennt. `text` ist Klartext ohne Grammatik — die
/// Farben bleiben aus, alles andere (Tasten, Umbruch, Undo) gilt weiter.
export type Sprache = "markdown" | "json" | "yaml" | "xml" | "text";

/// Sprache aus der Dateiendung. Was hier nicht steht, ist keine Textdatei,
/// die der Editor öffnet.
export function spracheZu(relpath: string): Sprache | null {
  const ext = relpath.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "md" || ext === "markdown") return "markdown";
  if (ext === "json") return "json";
  if (ext === "yaml" || ext === "yml") return "yaml";
  if (ext === "xml") return "xml";
  if (ext === "txt" || ext === "text" || ext === "log") return "text";
  return null;
}

const GRAMMATIK: Record<Sprache, () => Extension[]> = {
  markdown: () => [markdown()],
  json: () => [json()],
  yaml: () => [yaml()],
  xml: () => [xml()],
  text: () => [],
};

/// Fundstellen aus der Suche: die Wörter und die laufende Nummer der Stelle,
/// die beim Lesen im Bild stand. Der Editor springt genau dorthin — sonst
/// landet man beim ersten Vorkommen und sucht die Stelle erneut.
export interface Fundstellen {
  woerter: string[];
  /// 0-basiert, gezählt über alle Vorkommen aller Wörter im Text.
  nummer: number;
}

export interface MdEditorOpts {
  text: string;
  /// Voreinstellung Markdown — der häufigste Fall im Archiv.
  sprache?: Sprache;
  /// Beim Öffnen anspringen und markieren.
  fundstellen?: Fundstellen;
  /// Nach jeder Änderung — die Vorschau zeichnet daraufhin neu.
  onChange(): void;
  onSave(): void;
  onCancel(): void;
  /// Beim Scrollen; der Anteil ist 0…1.
  onScroll(anteil: number): void;
}

export function initMdEditor(opts: MdEditorOpts): MdEditor {
  const el = document.createElement("div");
  el.className = "wiki-note-editor";

  const tasten = keymap.of([
    { key: "Mod-s", run: () => (opts.onSave(), true) },
    { key: "Escape", run: () => (opts.onCancel(), true) },
    { key: "Mod-b", run: (v) => umschliessen(v, "**") },
    { key: "Mod-i", run: (v) => umschliessen(v, "*") },
    { key: "Mod-k", run: linkSetzen },
    {
      // Aufzählungen, nummerierte Listen und Zitate laufen weiter; ein leerer
      // Eintrag beendet sie, statt endlos Striche zu setzen.
      key: "Enter",
      run: (v) => {
        const zeile = v.state.doc.lineAt(v.state.selection.main.head);
        const prefix = listenPrefix(zeile.text);
        if (prefix === null) return false;
        if (prefix === "") {
          v.dispatch({ changes: { from: zeile.from, to: zeile.to, insert: "" } });
          return true;
        }
        const pos = v.state.selection.main.head;
        v.dispatch({
          changes: { from: pos, insert: `\n${prefix}` },
          selection: { anchor: pos + 1 + prefix.length },
          scrollIntoView: true,
        });
        return true;
      },
    },
  ]);

  const erweiterungen: Extension[] = [
    history(),
    drawSelection(),
    highlightActiveLine(),
    EditorView.lineWrapping,
    ...GRAMMATIK[opts.sprache ?? "markdown"](),
    syntaxHighlighting(stil),
    treffer,
    tasten,
    keymap.of([...historyKeymap, ...defaultKeymap, indentWithTab]),
    thema,
    EditorView.updateListener.of((u) => {
      if (u.docChanged) opts.onChange();
    }),
    EditorView.domEventHandlers({
      scroll: (_e, view) => {
        const s = view.scrollDOM;
        const platz = s.scrollHeight - s.clientHeight;
        opts.onScroll(platz > 0 ? s.scrollTop / platz : 0);
      },
    }),
  ];

  const view = new EditorView({
    state: EditorState.create({ doc: opts.text, extensions: erweiterungen }),
    parent: el,
  });

  // Fundstellen der Suche hervorheben und die gemeinte anspringen.
  if (opts.fundstellen?.woerter.length) {
    const alle = stellen(opts.text, opts.fundstellen.woerter);
    if (alle.length) {
      view.dispatch({
        effects: setzeTreffer.of(
          Decoration.set(
            alle.map((s) => Decoration.mark({ class: "cm-hit" }).range(s.von, s.bis)),
          ),
        ),
      });
      const ziel = alle[Math.min(opts.fundstellen.nummer, alle.length - 1)];
      view.dispatch({
        selection: { anchor: ziel.von, head: ziel.bis },
        effects: EditorView.scrollIntoView(ziel.von, { y: "center" }),
      });
    }
  }

  return {
    el,
    value: () => view.state.doc.toString(),
    insert(text: string) {
      const { from, to } = view.state.selection.main;
      view.dispatch({
        changes: { from, to, insert: text },
        selection: { anchor: from + text.length },
      });
      view.focus();
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
    scrollAnteil() {
      const s = view.scrollDOM;
      const platz = s.scrollHeight - s.clientHeight;
      return platz > 0 ? s.scrollTop / platz : 0;
    },
  };
}
