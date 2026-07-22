// Dokument-Ansicht: Wikilink-Verlinkung im DOM und Editor-Flush.
import { describe, expect, it, vi } from "vitest";
import { initPanelView, linkWikiRefs } from "./panel-view";

describe("linkWikiRefs", () => {
  it("verlinkt [[ziel]] und [[ziel|Label]], Code bleibt unangetastet", () => {
    const root = document.createElement("div");
    root.innerHTML =
      "<p>Siehe [[adr-logging]] und [[notiz|die Notiz]].</p>" +
      "<pre><code>if [[ -f x ]]; then</code></pre>";
    const onClick = vi.fn();
    linkWikiRefs(root, onClick);
    const links = [...root.querySelectorAll<HTMLElement>("a.wiki")];
    expect(links.map((a) => a.textContent)).toEqual(["adr-logging", "die Notiz"]);
    links[1].click();
    expect(onClick).toHaveBeenCalledWith("notiz");
    expect(root.querySelector("code")!.textContent).toBe("if [[ -f x ]]; then");
  });
});

function viewSetup(onCommit: (text: string) => void | Promise<void>) {
  document.body.innerHTML = `
    <div id="content"></div>
    <button id="copy"></button>
    <button id="mode"></button>
    <button id="edit"></button>`;
  const view = initPanelView({
    content: document.getElementById("content")!,
    copyBtn: document.getElementById("copy")!,
    modeBtn: document.getElementById("mode")!,
    editContentBtn: document.getElementById("edit")!,
    onCommit,
  });
  const editor = document.querySelector<HTMLTextAreaElement>(".panel-editor")!;
  return { view, editor, editBtn: document.getElementById("edit")! };
}

describe("initPanelView — Editor", () => {
  it("flush speichert eine offene Bearbeitung und beendet sie", async () => {
    const onCommit = vi.fn();
    const { view, editor, editBtn } = viewSetup(onCommit);
    view.set("Alt");
    editBtn.click();
    expect(editor.hidden).toBe(false);
    editor.value = "Neu";
    await view.flush();
    expect(onCommit).toHaveBeenCalledWith("Neu");
    expect(editor.hidden).toBe(true);
  });

  it("flush ohne offene Bearbeitung tut nichts", async () => {
    const onCommit = vi.fn();
    const { view } = viewSetup(onCommit);
    view.set("Alt");
    await view.flush();
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("puffert eingehende Updates während der Bearbeitung", () => {
    const { view, editor, editBtn } = viewSetup(() => {});
    view.set("Alt");
    editBtn.click();
    view.set("Update von außen");
    expect(editBtn.classList.contains("changed")).toBe(true);
    editor.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(view.raw()).toBe("Update von außen");
  });
});
