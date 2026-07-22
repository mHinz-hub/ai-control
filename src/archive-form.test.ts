// Archiv-Formular: Aufklappen, Meta-Aufbereitung, Tastatur.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initArchiveForm } from "./archive-form";

function setup() {
  document.body.innerHTML = `<button id="btn">Archiv</button>`;
  const onSubmit = vi.fn();
  const form = initArchiveForm(document.getElementById("btn")!, onSubmit);
  const root = document.querySelector<HTMLElement>(".archive-form")!;
  const inputs = [...root.querySelectorAll("input")];
  return { form, onSubmit, root, inputs };
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

  it("liefert getrimmte Meta; Leeres wird undefined, Tags gesplittet", () => {
    const { form, onSubmit, root, inputs } = setup();
    form.toggle();
    inputs[0].value = " konzepte/panel ";
    inputs[1].value = "";
    inputs[2].value = " adr, infra ,, ";
    root.querySelector<HTMLElement>(".archive-form-submit")!.click();
    expect(onSubmit).toHaveBeenCalledWith({
      folder: "konzepte/panel",
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
    inputs[0].value = "konzepte/panel";
    inputs[1].value = "Beschreibung A";
    inputs[2].value = "panel, wiki";
    root.querySelector<HTMLElement>(".archive-form-submit")!.click();

    form.toggle();
    expect(inputs.map((i) => i.value)).toEqual(["", "", ""]);
    root.querySelector<HTMLElement>(".archive-form-submit")!.click();
    expect(onSubmit).toHaveBeenLastCalledWith({
      folder: undefined,
      description: undefined,
      tags: [],
    });
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
