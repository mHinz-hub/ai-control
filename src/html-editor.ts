/// WYSIWYG-Editor für HTML-Notizen, direkt auf ProseMirror (MIT, ohne
/// Anbieter-Stufe). HTML ist hier nicht Umweg, sondern Format: `DOMParser`
/// liest den Rumpf der Notiz ins Dokumentmodell, `DOMSerializer` schreibt ihn
/// zurück — was gespeichert wird, ist genau das, was der Editor zeigt.
/// Markdown-Notizen behalten den Rohtext-Editor mit Vorschau.

import { baseKeymap, chainCommands, setBlockType, toggleMark } from "prosemirror-commands";
import { gapCursor } from "prosemirror-gapcursor";
// Strichcursor in den Lücken vor und hinter Blöcken — gehört zum Paket.
import "prosemirror-gapcursor/style/gapcursor.css";
import { history, redo, undo } from "prosemirror-history";
import { keymap } from "prosemirror-keymap";
import {
  DOMParser as PMDOMParser,
  DOMSerializer,
  Schema,
  type MarkType,
  type Node as PMNode,
  type NodeSpec,
} from "prosemirror-model";
import {
  addColumnAfter,
  addRowAfter,
  CellSelection,
  deleteColumn,
  deleteRow,
  deleteTable,
  goToNextCell,
  TableMap,
  tableNodes,
  tableEditing,
} from "prosemirror-tables";
import { schema as basic } from "prosemirror-schema-basic";
import { liftListItem, sinkListItem, splitListItem, wrapInList } from "prosemirror-schema-list";
import { EditorState, Plugin, TextSelection, type Command } from "prosemirror-state";
import { Decoration, DecorationSet, EditorView } from "prosemirror-view";
// Basisstile des Editors (Cursor, Auswahl) — gehören zum Paket.
import "prosemirror-view/style/prosemirror.css";
import { t } from "./messages";
import { openTableForm, type TableWahl } from "./table-form";

/// Schema: Grundbausteine plus Listen und Tabellen — mehr braucht eine Notiz
/// nicht, und jeder zusätzliche Knoten wäre HTML, das wir beim Zurücklesen
/// wieder verstehen müssten.
const listNodes = {
  ordered_list: {
    content: "list_item+",
    group: "block",
    parseDOM: [{ tag: "ol" }],
    toDOM: () => ["ol", 0] as const,
  },
  bullet_list: {
    content: "list_item+",
    group: "block",
    parseDOM: [{ tag: "ul" }],
    toDOM: () => ["ul", 0] as const,
  },
  list_item: {
    content: "paragraph block*",
    parseDOM: [{ tag: "li" }],
    toDOM: () => ["li", 0] as const,
    defining: true,
  },
};

const tabellen = tableNodes({ tableGroup: "block", cellContent: "block+", cellAttributes: {} });

/// Die Tabelle trägt eine Klasse: Rand und Textfluss stehen im Stylesheet,
/// nicht als Inline-Style in der Notiz. So bleibt die Datei lesbar, und ein
/// späterer Stilwechsel greift überall.
const tabelleMitKlasse: NodeSpec = {
  ...tabellen.table,
  attrs: { class: { default: "" } },
  toDOM: (node) => {
    const klasse = String(node.attrs.class ?? "");
    return ["table", klasse ? { class: klasse } : {}, ["tbody", 0]];
  },
  parseDOM: [
    {
      tag: "table",
      getAttrs: (dom) => ({ class: (dom as HTMLElement).getAttribute("class") ?? "" }),
    },
  ],
};

const schema = new Schema({
  nodes: basic.spec.nodes
    .append(listNodes)
    .append(tabellen)
    .update("table", tabelleMitKlasse),
  marks: basic.spec.marks,
});

/// Hebt die Zelle hervor, in der der Cursor steht. In einer leeren Tabelle
/// ist der Textcursor sonst der einzige Anhaltspunkt, und der steht in einem
/// leeren Absatz zwischen gleich aussehenden Kästchen.
const aktiveZelle = new Plugin({
  props: {
    decorations(state) {
      const $p = state.selection.$from;
      for (let d = $p.depth; d > 0; d--) {
        const node = $p.node(d);
        if (node.type === schema.nodes.table_cell || node.type === schema.nodes.table_header) {
          const pos = $p.before(d);
          return DecorationSet.create(state.doc, [
            Decoration.node(pos, pos + node.nodeSize, { class: "cell-active" }),
          ]);
        }
      }
      return DecorationSet.empty;
    },
  },
});

/// Tabellen-Ansicht mit Löschknöpfen: je Spalte einer darüber, je Zeile einer
/// links daneben. Ein Klick löscht sofort — ohne den Umweg über eine Auswahl.
/// Die Knöpfe sind klein und blass und sitzen mittig an ihrer Spalte bzw.
/// Zeile; die Maße kommen aus der gerenderten Tabelle, also stimmen sie auch
/// bei ungleichen Spalten und umflossenen Tabellen.
class TabelleMitGriffen {
  dom: HTMLElement;
  contentDOM: HTMLElement;
  private tabelle: HTMLTableElement;
  private spaltenLeiste: HTMLElement;
  private zeilenLeiste: HTMLElement;
  private beobachter: ResizeObserver | null = null;
  private node: PMNode;
  private view: EditorView;
  private getPos: () => number | undefined;

  constructor(node: PMNode, view: EditorView, getPos: () => number | undefined) {
    this.node = node;
    this.view = view;
    this.getPos = getPos;
    this.dom = document.createElement("div");
    this.dom.className = "pm-table-wrap";
    this.spaltenLeiste = document.createElement("div");
    this.spaltenLeiste.className = "pm-col-handles";
    this.spaltenLeiste.contentEditable = "false";
    this.zeilenLeiste = document.createElement("div");
    this.zeilenLeiste.className = "pm-row-handles";
    this.zeilenLeiste.contentEditable = "false";
    this.tabelle = document.createElement("table");
    this.contentDOM = this.tabelle.appendChild(document.createElement("tbody"));
    this.dom.append(this.spaltenLeiste, this.zeilenLeiste, this.tabelle);
    this.klasse();
    this.griffe();
    if (typeof ResizeObserver === "function") {
      this.beobachter = new ResizeObserver(() => this.masse());
      this.beobachter.observe(this.tabelle);
    }
  }

  update(node: PMNode): boolean {
    if (node.type !== this.node.type) return false;
    this.node = node;
    this.klasse();
    this.griffe();
    return true;
  }

  /// Rand und Textfluss hängen als Klasse am Knoten und gehören an die
  /// Tabelle selbst, nicht an den Wrapper.
  private klasse() {
    const k = String(this.node.attrs.class ?? "");
    this.tabelle.className = k;
    // Eine umflossene Tabelle zieht den Wrapper mit, sonst stünde der Text
    // unter einem leeren Block.
    this.dom.className = `pm-table-wrap${k.includes("fluss-") ? ` ${k.match(/fluss-\w+/)![0]}` : ""}`;
  }

  /// Je Spalte und je Zeile ein Löschknopf, adressiert über die
  /// Tabellenkarte.
  private griffe() {
    const karte = TableMap.get(this.node);
    this.spaltenLeiste.textContent = "";
    this.zeilenLeiste.textContent = "";
    for (let c = 0; c < karte.width; c++) {
      this.spaltenLeiste.append(
        this.griff("pm-col-handle", t("html.colDelete"), () => {
          const start = (this.getPos() ?? 0) + 1;
          const oben = this.view.state.doc.resolve(start + karte.map[c]);
          const unten = this.view.state.doc.resolve(
            start + karte.map[(karte.height - 1) * karte.width + c],
          );
          this.loesche(CellSelection.colSelection(oben, unten), deleteColumn);
        }),
      );
    }
    for (let r = 0; r < karte.height; r++) {
      this.zeilenLeiste.append(
        this.griff("pm-row-handle", t("html.rowDelete"), () => {
          const start = (this.getPos() ?? 0) + 1;
          const links = this.view.state.doc.resolve(start + karte.map[r * karte.width]);
          const rechts = this.view.state.doc.resolve(
            start + karte.map[r * karte.width + karte.width - 1],
          );
          this.loesche(CellSelection.rowSelection(links, rechts), deleteRow);
        }),
      );
    }
    this.masse();
  }

  private griff(klasse: string, titel: string, run: () => void): HTMLElement {
    const g = document.createElement("div");
    g.className = klasse;
    g.title = titel;
    g.textContent = "×";
    g.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      run();
    });
    return g;
  }

  /// Erst die Zeile bzw. Spalte auswählen, dann im selben Zug löschen — der
  /// Befehl arbeitet auf der Auswahl, sichtbar wird sie nie.
  private loesche(sel: CellSelection, befehl: Command) {
    this.view.dispatch(this.view.state.tr.setSelection(sel));
    befehl(this.view.state, this.view.dispatch, this.view);
    this.view.focus();
  }

  /// Griffe auf die gerenderten Zell- und Zeilenmaße setzen.
  private masse() {
    const zeilen = [...this.contentDOM.children] as HTMLElement[];
    if (!zeilen.length) return;
    const zellen = [...zeilen[0].children] as HTMLElement[];
    [...this.spaltenLeiste.children].forEach((g, i) => {
      const breite = zellen[i]?.offsetWidth;
      if (breite) (g as HTMLElement).style.width = `${breite}px`;
    });
    [...this.zeilenLeiste.children].forEach((g, i) => {
      const hoehe = zeilen[i]?.offsetHeight;
      if (hoehe) (g as HTMLElement).style.height = `${hoehe}px`;
    });
  }

  /// Die Griffe stehen im DOM des Knotens, gehören aber nicht zum Inhalt —
  /// ProseMirror soll ihre Änderungen nicht zurücklesen.
  ignoreMutation(m: { type: string; target: Node }): boolean {
    return (
      m.type === "attributes" ||
      this.spaltenLeiste.contains(m.target) ||
      this.zeilenLeiste.contains(m.target)
    );
  }

  destroy() {
    this.beobachter?.disconnect();
  }
}

export interface HtmlEditor {
  el: HTMLElement;
  /// Aktueller Rumpf als HTML.
  html(): string;
  focus(): void;
  destroy(): void;
}

/// Knopf der Werkzeugleiste; `run` liefert true, wenn der Befehl greift.
function button(label: string, title: string, run: () => void): HTMLElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "html-tool";
  b.textContent = label;
  b.title = title;
  // mousedown statt click: sonst verliert die Auswahl im Editor den Fokus,
  // bevor der Befehl auf ihr arbeiten kann.
  b.addEventListener("mousedown", (e) => {
    e.preventDefault();
    run();
  });
  return b;
}

export function initHtmlEditor(html: string): HtmlEditor {
  const el = document.createElement("div");
  el.className = "html-editor";
  const bar = document.createElement("div");
  bar.className = "html-toolbar";
  const host = document.createElement("div");
  host.className = "html-surface";
  el.append(bar, host);

  // Rumpf der Notiz ins Dokumentmodell lesen.
  const source = document.createElement("div");
  source.innerHTML = html;
  const doc = PMDOMParser.fromSchema(schema).parse(source);

  const view = new EditorView(host, {
    nodeViews: {
      table: (node, v, getPos) => new TabelleMitGriffen(node, v, getPos),
    },
    state: EditorState.create({
      doc,
      plugins: [
        history(),
        keymap({
          "Mod-z": undo,
          "Mod-y": redo,
          "Mod-Shift-z": redo,
          "Mod-b": toggleMark(schema.marks.strong),
          "Mod-i": toggleMark(schema.marks.em),
          Enter: chainCommands(splitListItem(schema.nodes.list_item), baseKeymap.Enter),
          Tab: goToNextCell(1),
          "Shift-Tab": goToNextCell(-1),
        }),
        keymap(baseKeymap),
        // Steht eine Tabelle ganz oben oder ganz unten, gibt es dort keine
        // Textstelle für den Cursor. Der Lückencursor setzt einen Strich in
        // diese Lücke; wer dort tippt, bekommt einen Absatz.
        gapCursor(),
        tableEditing(),
        aktiveZelle,
      ],
    }),
  });

  const run = (cmd: Command) => {
    cmd(view.state, view.dispatch, view);
    view.focus();
  };
  const mark = (m: MarkType) => run(toggleMark(m));
  const heading = (level: number) =>
    run(setBlockType(schema.nodes.heading, { level }));

  /// Tabelle einsetzen — Größe, Kopfzeile, Rand und Textfluss kommen aus dem
  /// Dialog. Der Cursor landet in der ersten Zelle: Sonst blinkt er irgendwo
  /// neben einem leeren Gitter, und man sieht weder, wo man ist, noch was da
  /// steht.
  function insertTable(wahl: TableWahl) {
    const { table, table_row, table_cell, table_header } = schema.nodes;
    const zeile = (typ: typeof table_cell) =>
      table_row.create(
        null,
        Array.from({ length: wahl.spalten }, () => typ.createAndFill()!),
      );
    const reihen = [];
    if (wahl.kopf) reihen.push(zeile(table_header));
    for (let i = 0; i < wahl.zeilen; i++) reihen.push(zeile(table_cell));
    const klassen = [
      wahl.rand ? "" : "randlos",
      wahl.fluss === "block" ? "" : `fluss-${wahl.fluss}`,
    ].filter(Boolean);
    const node = table.create({ class: klassen.join(" ") }, reihen);
    const start = view.state.selection.from;
    let tr = view.state.tr.replaceSelectionWith(node);
    tr = tr.setSelection(TextSelection.near(tr.doc.resolve(start + 1)));
    view.dispatch(tr.scrollIntoView());
    view.focus();
  }

  /// Textfluss der Tabelle, in der der Cursor steht: als Block oder links
  /// bzw. rechts umflossen. Die Wahl aus dem Einfüge-Dialog lässt sich damit
  /// jederzeit ändern, ohne die Tabelle neu zu bauen.
  function setzeFluss(art: "block" | "links" | "rechts") {
    const $p = view.state.selection.$from;
    for (let d = $p.depth; d > 0; d--) {
      if ($p.node(d).type !== schema.nodes.table) continue;
      const knoten = $p.node(d);
      const klassen = String(knoten.attrs.class ?? "")
        .split(/\s+/)
        .filter((c) => c && !c.startsWith("fluss-"));
      if (art !== "block") klassen.push(`fluss-${art}`);
      view.dispatch(
        view.state.tr.setNodeMarkup($p.before(d), undefined, { class: klassen.join(" ") }),
      );
      view.focus();
      return;
    }
  }

  /// Werkzeuge in Gruppen: Blockart, Zeichenauszeichnung, Listen, Tabelle.
  /// Zwischen den Gruppen ein senkrechter Strich — eine Reihe aus vierzehn
  /// gleich aussehenden Knöpfen zwingt sonst jedes Mal zum Suchen.
  const trenner = () => {
    const s = document.createElement("span");
    s.className = "html-tool-sep";
    return s;
  };
  bar.append(
    button("H1", t("html.h1"), () => heading(1)),
    button("H2", t("html.h2"), () => heading(2)),
    button("¶", t("html.paragraph"), () => run(setBlockType(schema.nodes.paragraph))),
    trenner(),
    button("B", t("html.bold"), () => mark(schema.marks.strong)),
    button("I", t("html.italic"), () => mark(schema.marks.em)),
    button("🔗", t("html.link"), () => setLink()),
    trenner(),
    button("•", t("html.bulletList"), () => run(wrapInList(schema.nodes.bullet_list))),
    button("1.", t("html.orderedList"), () => run(wrapInList(schema.nodes.ordered_list))),
    button("→", t("html.indent"), () => run(sinkListItem(schema.nodes.list_item))),
    button("←", t("html.outdent"), () => run(liftListItem(schema.nodes.list_item))),
    trenner(),
    button("▤", t("html.flowBlock"), () => setzeFluss("block")),
    button("◧", t("html.flowLeft"), () => setzeFluss("links")),
    button("◨", t("html.flowRight"), () => setzeFluss("rechts")),
    trenner(),
    button("⊞", t("html.table"), () => openTableForm(bar, insertTable, true)),
    button("+↓", t("html.rowAfter"), () => run(addRowAfter)),
    button("+→", t("html.colAfter"), () => run(addColumnAfter)),
    // Zeile und Spalte löschen die Knöpfe an der Tabelle selbst.
    button("⌫", t("html.deleteTable"), () => run(deleteTable)),
  );

  /// Link auf der Auswahl setzen oder entfernen.
  function setLink() {
    const { from, to } = view.state.selection;
    if (from === to) return;
    const has = view.state.doc.rangeHasMark(from, to, schema.marks.link);
    if (has) {
      run(toggleMark(schema.marks.link));
      return;
    }
    const href = window.prompt(t("html.linkPrompt"), "https://");
    if (!href) return;
    run(toggleMark(schema.marks.link, { href }));
  }

  return {
    el,
    html() {
      const fragment = DOMSerializer.fromSchema(schema).serializeFragment(
        view.state.doc.content,
      );
      const out = document.createElement("div");
      out.append(fragment);
      return out.innerHTML;
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}
