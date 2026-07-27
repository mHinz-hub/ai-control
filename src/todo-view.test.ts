// Kachel-Ansicht der ToDo-Liste: Sortierung, Ampel-Badge, Löschen per ID,
// Formular für Anlegen und Bearbeiten.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initTodoView } from "./todo-view";

function iso(offsetDays: number): string {
  const d = new Date(2026, 6, 22 + offsetDays); // Basis 2026-07-22 (fake time)
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

describe("initTodoView", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 6, 22, 12, 0, 0));
    document.body.innerHTML = `<div id="c"></div>`;
  });
  afterEach(() => vi.useRealTimers());

  const jsonl = [
    JSON.stringify({ id: "a", ts: 100, text: "ohne Datum alt" }),
    JSON.stringify({ id: "b", ts: 200, text: "ohne Datum neu" }),
    JSON.stringify({ id: "c", ts: 150, text: "überfällig", due: iso(-1) }),
    JSON.stringify({ id: "d", ts: 150, text: "bald", note: "mit Notiz", due: iso(2) }),
    JSON.stringify({ id: "e", ts: 150, text: "später", due: iso(10) }),
  ].join("\n");

  it("sortiert fällige zuerst und hängt Ampel-Badges an", () => {
    const view = initTodoView(document.getElementById("c")!, () => {}, () => {});
    view.set(jsonl);
    expect(view.empty()).toBe(false);
    const texts = [...document.querySelectorAll(".todo-text")].map(
      (el) => el.textContent,
    );
    // due aufsteigend, danach ohne Datum neueste oben.
    expect(texts).toEqual([
      "überfällig",
      "bald",
      "später",
      "ohne Datum neu",
      "ohne Datum alt",
    ]);
    const badges = [...document.querySelectorAll(".todo-due")];
    expect(badges.map((b) => b.className)).toEqual([
      "todo-due overdue",
      "todo-due soon",
      "todo-due later",
    ]);
    // Datum nach Locale (Testsprache de).
    expect(badges[0].textContent).toBe(new Date(2026, 6, 21).toLocaleDateString("de"));
    expect(document.querySelector(".cmd-note")!.textContent).toBe("mit Notiz");
  });

  it("meldet Löschen mit der Eintrags-ID", () => {
    const onDelete = vi.fn();
    const view = initTodoView(document.getElementById("c")!, onDelete, () => {});
    view.set(jsonl);
    const dels = document.querySelectorAll<HTMLElement>(".cmd-del");
    dels[0].click(); // erste Kachel = überfälliges ToDo (id "c")
    expect(onDelete).toHaveBeenCalledWith("c");
  });

  it("leerer Puffer meldet empty", () => {
    const view = initTodoView(document.getElementById("c")!, () => {}, () => {});
    view.set("");
    expect(view.empty()).toBe(true);
    expect(document.querySelectorAll(".cmd-tile").length).toBe(0);
  });

  it("Plus-Knopf öffnet das leere Formular und meldet Anlegen ohne ID", () => {
    const onSave = vi.fn();
    initTodoView(document.getElementById("c")!, () => {}, onSave);
    const form = document.querySelector<HTMLElement>(".todo-form")!;
    expect(form.hidden).toBe(true);
    document.querySelector<HTMLElement>(".todo-add")!.click();
    expect(form.hidden).toBe(false);
    const [text, note, due] = form.querySelectorAll("input");
    text.value = " neu ";
    note.value = "";
    due.value = iso(1);
    form.querySelector<HTMLElement>(".todo-form-submit")!.click();
    // Text geht ungetrimmt raus (trimmt das Backend), leere Notiz entfällt.
    expect(onSave).toHaveBeenCalledWith({
      id: undefined,
      text: " neu ",
      note: undefined,
      due: iso(1),
    });
    expect(form.hidden).toBe(true);
  });

  it("Stift öffnet das Formular vorbefüllt und meldet Speichern mit ID", () => {
    const onSave = vi.fn();
    const view = initTodoView(document.getElementById("c")!, () => {}, onSave);
    view.set(jsonl);
    const form = document.querySelector<HTMLElement>(".todo-form")!;
    // zweite Kachel = "bald" (id "d") mit Notiz und Fälligkeit.
    document.querySelectorAll<HTMLElement>(".cmd-edit")[1].click();
    const [text, note, due] = form.querySelectorAll("input");
    expect([text.value, note.value, due.value]).toEqual([
      "bald",
      "mit Notiz",
      iso(2),
    ]);
    text.value = "bald geändert";
    form.querySelector<HTMLElement>(".todo-form-submit")!.click();
    expect(onSave).toHaveBeenCalledWith({
      id: "d",
      text: "bald geändert",
      note: "mit Notiz",
      due: iso(2),
    });
  });

  it("Abbrechen schließt das Formular ohne zu melden", () => {
    const onSave = vi.fn();
    initTodoView(document.getElementById("c")!, () => {}, onSave);
    document.querySelector<HTMLElement>(".todo-add")!.click();
    const form = document.querySelector<HTMLElement>(".todo-form")!;
    form.querySelector<HTMLElement>(".todo-form-cancel")!.click();
    expect(form.hidden).toBe(true);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("Escape schließt das Formular ohne zu melden", () => {
    const onSave = vi.fn();
    initTodoView(document.getElementById("c")!, () => {}, onSave);
    document.querySelector<HTMLElement>(".todo-add")!.click();
    const form = document.querySelector<HTMLElement>(".todo-form")!;
    form.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(form.hidden).toBe(true);
    expect(onSave).not.toHaveBeenCalled();
  });
});
