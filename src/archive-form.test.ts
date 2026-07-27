// Archiv-Formular: Aufklappen, Meta-Aufbereitung, Tastatur.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initArchiveForm } from "./archive-form";

function setup() {
  document.body.innerHTML = `<button id="btn">Archiv</button>`;
  const onSubmit = vi.fn();
  const onSave = vi.fn();
  const folders = vi.fn(() =>
    Promise.resolve([
      { id: "id-konzepte", path: "konzepte", title: "Konzepte" },
      { id: "id-panel", path: "konzepte/panel", title: "Panel" },
    ]),
  );
  const form = initArchiveForm(document.getElementById("btn")!, onSubmit, {
    folders,
    onSave,
  });
  const root = document.querySelector<HTMLElement>(".archive-form")!;
  const inputs = [...root.querySelectorAll("input")];
  return { form, onSubmit, onSave, folders, root, inputs };
}

describe("initArchiveForm", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("klappt auf und zu", () => {
    const { form, root } = setup();
    expect(root.hidden).toBe(true);
    form.toggle();
    expect(root.hidden).toBe(false);
    form.toggle();
    expect(root.hidden).toBe(true);
  });

  it("liefert getrimmte Meta; Leeres wird undefined, Tags gesplittet", async () => {
    const { form, onSubmit, root, inputs } = setup();
    form.toggle();
    await new Promise((r) => setTimeout(r));
    root
      .querySelector<HTMLElement>('.archive-browse-row[data-path="id-konzepte"]')!
      .click();
    // Reihenfolge der Felder: Titel, Beschreibung, Schlagwörter.
    inputs[0].value = "  Titel im Archiv  ";
    inputs[1].value = "";
    inputs[2].value = " adr, infra ,, ";
    root.querySelector<HTMLElement>(".archive-form-submit")!.click();
    expect(onSubmit).toHaveBeenCalledWith({
      title: "Titel im Archiv",
      folder: "id-konzepte",
      description: undefined,
      tags: ["adr", "infra"],
    });
    expect(root.hidden).toBe(true);
  });

  /// Sonst gelten Ordner und Schlagwörter des vorigen Dokuments unbemerkt für
  /// das nächste — wer direkt Enter drückt, archiviert es falsch einsortiert.
  it("startet nach dem Archivieren wieder leer", () => {
    const { form, onSubmit, root, inputs } = setup();
    form.toggle();
    inputs[0].value = "Titel A";
    inputs[1].value = "Beschreibung A";
    inputs[2].value = "panel, wiki";
    root.querySelector<HTMLElement>(".archive-form-submit")!.click();

    form.toggle();
    expect(inputs.map((i) => i.value)).toEqual(["", "", ""]);
    root.querySelector<HTMLElement>(".archive-form-submit")!.click();
    expect(onSubmit).toHaveBeenLastCalledWith({
      title: undefined,
      folder: undefined,
      description: undefined,
      tags: [],
    });
  });

  it("Baum startet eingeklappt, Pfeil klappt auf, Klick wählt aus", async () => {
    const { form, folders, root } = setup();
    form.toggle();
    await new Promise((r) => setTimeout(r));
    expect(folders).toHaveBeenCalled();

    // Eingeklappt: Wurzel und erste Ebene sichtbar, Unterebene verborgen.
    const visible = () =>
      [...root.querySelectorAll<HTMLElement>(".archive-browse-row")].filter(
        (r) => !r.closest<HTMLElement>(".archive-kids[hidden]"),
      );
    expect(visible().map((r) => r.dataset.path)).toEqual(["", "id-konzepte"]);

    // Logische Sicht: Titel des Knotentexts, Pfad an der Zeile.
    const konzepte = visible()[1];
    expect(konzepte.querySelector(".wiki-tree-name")!.textContent).toBe("Konzepte");
    expect(konzepte.title).toBe("konzepte");

    // Pfeil klappt auf, ohne die Auswahl zu ändern.
    konzepte.querySelector<HTMLElement>(".archive-arrow")!.click();
    expect(visible().map((r) => r.dataset.path)).toEqual(["", "id-konzepte", "id-panel"]);
    // Aufklappen ändert die Auswahl nicht.
    expect(visible()[0].className).toContain("active");

    // Klick auf die Zeile wählt aus.
    visible()[2].click();
    expect(visible()[2].className).toContain("active");
    expect(visible()[0].className).not.toContain("active");
  });

  it("Ladefehler des Baums steht im Kasten", async () => {
    document.body.innerHTML = `<button id="btn">Archiv</button>`;
    const form = initArchiveForm(document.getElementById("btn")!, vi.fn(), {
      folders: () => Promise.reject(new Error("kein Archiv-Ordner gesetzt")),
    });
    form.toggle();
    await new Promise((r) => setTimeout(r));
    expect(
      document.querySelector(".archive-browse-error")!.textContent,
    ).toContain("kein Archiv-Ordner gesetzt");
  });

  it("Auf Platte legen ruft onSave und schließt, ohne zu archivieren", () => {
    const { form, onSave, onSubmit, root } = setup();
    form.toggle();
    root.querySelector<HTMLElement>(".archive-form-save")!.click();
    expect(onSave).toHaveBeenCalledOnce();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(root.hidden).toBe(true);
  });

  it("Abbrechen schließt ohne Abschicken", () => {
    const { form, onSubmit, root } = setup();
    form.toggle();
    root.querySelector<HTMLElement>(".archive-form-cancel")!.click();
    expect(root.hidden).toBe(true);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("Klick auf den Hintergrund schließt, Klick in die Box nicht", () => {
    const { form, onSubmit, root } = setup();
    form.toggle();
    root.querySelector<HTMLElement>(".wiki-form")!.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true }),
    );
    expect(root.hidden).toBe(false);
    root.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(root.hidden).toBe(true);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  /// Im Terminal-Fenster liegt der Fokus oft in der Shell — Escape muss auch
  /// dann greifen, wenn die Taste nicht im Dialog ankommt.
  it("Escape schließt auch von außerhalb des Dialogs", () => {
    const { form, onSubmit, root } = setup();
    form.toggle();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(root.hidden).toBe(true);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("Enter schickt ab, Escape schließt ohne Abschicken", () => {
    const { form, onSubmit, root } = setup();
    form.toggle();
    root.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(root.hidden).toBe(true);
    expect(onSubmit).not.toHaveBeenCalled();
    form.toggle();
    root.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(onSubmit).toHaveBeenCalledOnce();
    expect(root.hidden).toBe(true);
  });
});
