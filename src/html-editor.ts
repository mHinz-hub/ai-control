/// WYSIWYG-Editor für HTML-Notizen, direkt auf ProseMirror (MIT, ohne
/// Anbieter-Stufe). HTML ist hier nicht Umweg, sondern Format: `DOMParser`
/// liest den Rumpf der Notiz ins Dokumentmodell, `DOMSerializer` schreibt ihn
/// zurück — was gespeichert wird, ist genau das, was der Editor zeigt.
/// Markdown-Notizen behalten den Rohtext-Editor mit Vorschau.

import { baseKeymap, chainCommands, setBlockType, toggleMark } from "prosemirror-commands";
import { history, redo, undo } from "prosemirror-history";
import { keymap } from "prosemirror-keymap";
import { DOMParser as PMDOMParser, DOMSerializer, Schema, type MarkType } from "prosemirror-model";
import { addColumnAfter, addRowAfter, deleteTable, goToNextCell, tableNodes, tableEditing } from "prosemirror-tables";
import { schema as basic } from "prosemirror-schema-basic";
import { liftListItem, sinkListItem, splitListItem, wrapInList } from "prosemirror-schema-list";
import { EditorState, type Command } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
// Basisstile des Editors (Cursor, Auswahl) — gehören zum Paket.
import "prosemirror-view/style/prosemirror.css";
import { t } from "./messages";

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

const schema = new Schema({
  nodes: basic.spec.nodes
    .append(listNodes)
    .append(tableNodes({ tableGroup: "block", cellContent: "block+", cellAttributes: {} })),
  marks: basic.spec.marks,
});

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
        tableEditing(),
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

  /// Tabelle mit zwei Zeilen und zwei Spalten einsetzen.
  const insertTable: Command = (state, dispatch) => {
    const { table, table_row, table_cell, paragraph } = schema.nodes;
    const cell = () => table_cell.createAndFill()!;
    const row = table_row.create(null, [cell(), cell()]);
    const node = table.create(null, [row, row.copy(row.content)]);
    if (dispatch) dispatch(state.tr.replaceSelectionWith(node).scrollIntoView());
    void paragraph;
    return true;
  };

  bar.append(
    button("H1", t("html.h1"), () => heading(1)),
    button("H2", t("html.h2"), () => heading(2)),
    button("¶", t("html.paragraph"), () => run(setBlockType(schema.nodes.paragraph))),
    button("B", t("html.bold"), () => mark(schema.marks.strong)),
    button("I", t("html.italic"), () => mark(schema.marks.em)),
    button("•", t("html.bulletList"), () => run(wrapInList(schema.nodes.bullet_list))),
    button("1.", t("html.orderedList"), () => run(wrapInList(schema.nodes.ordered_list))),
    button("→", t("html.indent"), () => run(sinkListItem(schema.nodes.list_item))),
    button("←", t("html.outdent"), () => run(liftListItem(schema.nodes.list_item))),
    button("⊞", t("html.table"), () => run(insertTable)),
    button("+↓", t("html.rowAfter"), () => run(addRowAfter)),
    button("+→", t("html.colAfter"), () => run(addColumnAfter)),
    button("⌫", t("html.deleteTable"), () => run(deleteTable)),
    button("🔗", t("html.link"), () => setLink()),
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
