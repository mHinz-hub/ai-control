/// Kachel-Ansicht der persistenten ToDo-Liste: rendert die JSONL-Datei
/// (write_todos im MCP-Server) als Kacheln mit Kopier-, Bearbeitungs- und
/// Löschfunktion — Löschen geht als onDelete(id), Anlegen und Bearbeiten als
/// onSave(todo) an den Aufrufer, der neue Stand kommt über den Watcher
/// zurück. Der Plus-Knopf über der Liste öffnet das Formular leer, der Stift
/// auf der Kachel vorbefüllt (mit ID). Fälligkeit als Ampel-Badge (überfällig
/// rot, in den nächsten zwei Tagen gelb, sonst grün), Datum formatiert nach
/// Locale. Sortierung: fällige ToDos zuerst (aufsteigend), der Rest neueste
/// oben.

import { storedLocale, t } from "./messages";
import {
  copyAction,
  deleteAction,
  editAction,
  renderTile,
  stripInvisibles,
} from "./tiles";

interface Todo {
  id?: string;
  ts: number;
  text: string;
  note?: string;
  /// Fälligkeit als YYYY-MM-DD (lokales Datum).
  due?: string;
}

/// Formular-Ergebnis: mit ID bearbeiten (todos_update), ohne ID anlegen
/// (todos_add).
export interface TodoEdit {
  id?: string;
  text: string;
  note?: string;
  due?: string;
}

export interface TodoView {
  set(text: string): void;
  empty(): boolean;
}

function dueDate(due: string): Date {
  const [y, m, d] = due.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/// Ampel-Klasse zur Fälligkeit.
function dueClass(due: string): string {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((dueDate(due).getTime() - today.getTime()) / 86_400_000);
  return days < 0 ? "overdue" : days <= 2 ? "soon" : "later";
}

export function initTodoView(
  container: HTMLElement,
  onDelete: (id: string) => void,
  onSave: (todo: TodoEdit) => void,
): TodoView {
  let count = 0;

  const head = document.createElement("div");
  head.className = "todo-head";
  const addBtn = document.createElement("button");
  addBtn.className = "todo-add";
  addBtn.textContent = t("todos.add");
  addBtn.title = t("todos.addTitle");
  head.append(addBtn);

  const form = document.createElement("div");
  form.className = "todo-form";
  form.hidden = true;
  const text = document.createElement("input");
  text.type = "text";
  text.placeholder = t("todos.formText");
  const note = document.createElement("input");
  note.type = "text";
  note.placeholder = t("todos.formNote");
  const due = document.createElement("input");
  due.type = "date";
  due.title = t("todos.formDue");
  const cancel = document.createElement("button");
  cancel.className = "todo-form-cancel";
  cancel.textContent = t("todos.formCancel");
  const submit = document.createElement("button");
  submit.className = "todo-form-submit";
  const btns = document.createElement("div");
  btns.className = "todo-form-btns";
  btns.append(cancel, submit);
  form.append(text, note, due, btns);

  const list = document.createElement("div");
  container.append(head, form, list);

  /// ID des ToDos im Formular; undefined = Anlegen.
  let editing: string | undefined;
  const close = () => {
    form.hidden = true;
  };
  function open(todo?: Todo) {
    editing = todo?.id;
    text.value = todo?.text ?? "";
    note.value = todo?.note ?? "";
    due.value = todo?.due ?? "";
    submit.textContent = t(todo ? "todos.formSave" : "todos.formCreate");
    form.hidden = false;
    text.focus();
  }
  const fire = () => {
    onSave({
      id: editing,
      text: text.value,
      note: note.value.trim() || undefined,
      due: due.value || undefined,
    });
    close();
  };
  submit.addEventListener("click", fire);
  cancel.addEventListener("click", close);
  form.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      fire();
    } else if (e.key === "Escape") {
      close();
    }
  });
  addBtn.addEventListener("click", () => (form.hidden ? open() : close()));

  function render(todos: Todo[]) {
    list.textContent = "";
    const sorted = [...todos].sort((a, b) => {
      if (a.due && b.due)
        return a.due < b.due ? -1 : a.due > b.due ? 1 : b.ts - a.ts;
      if (a.due || b.due) return a.due ? -1 : 1;
      return b.ts - a.ts;
    });
    for (const todo of sorted) {
      const visible = stripInvisibles(todo.text);
      list.append(
        renderTile({
          cls: "cmd-tile",
          bodyCls: "cmd-body",
          parts: [
            { cls: "todo-text", text: visible },
            ...(todo.note ? [{ cls: "cmd-note", text: todo.note }] : []),
            ...(todo.due
              ? [
                  {
                    cls: `todo-due ${dueClass(todo.due)}`,
                    text: dueDate(todo.due).toLocaleDateString(storedLocale()),
                  },
                ]
              : []),
          ],
          actions: [
            copyAction(t("todos.copyOne"), () => visible),
            editAction(t("todos.editOne"), () => open(todo)),
            deleteAction(t("todos.removeOne"), () => onDelete(todo.id ?? "")),
          ],
        }),
      );
    }
  }

  return {
    set(text: string) {
      const todos: Todo[] = [];
      for (const line of text.split("\n")) {
        if (!line.trim()) continue;
        todos.push(JSON.parse(line));
      }
      count = todos.length;
      render(todos);
    },
    empty: () => count === 0,
  };
}
