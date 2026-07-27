// Modus-Logik (Tabs) und Kachel-Ansicht der Befehls-History.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initCommandsView, initPanelMode } from "./commands-view";

function modeSetup() {
  document.body.innerHTML = `
    <div id="tabs">
      <button data-mode="commands">Befehle</button>
      <button data-mode="draft">Dokument</button>
      <button data-mode="wiki">Wiki</button>
      <button data-mode="search">Suche</button>
    </div>
    <div id="title">Mein Titel</div>
    <div id="draft-el"></div>
    <div id="cc" hidden></div>
    <div id="sc" hidden></div>
    <div id="wc" hidden></div>`;
  const el = (id: string) => document.getElementById(id)!;
  const tab = (m: string) =>
    document.querySelector<HTMLElement>(`[data-mode="${m}"]`)!;
  const flush = vi.fn();
  const mode = initPanelMode({
    tabs: [
      { mode: "commands", btn: tab("commands"), content: el("cc"), label: "Befehle" },
      { mode: "draft", btn: tab("draft"), content: null, label: "Dokument" },
      { mode: "wiki", btn: tab("wiki"), content: el("wc"), label: "Wiki" },
      { mode: "search", btn: tab("search"), content: el("sc"), label: "Suche" },
    ],
    draftEls: [el("draft-el")],
    titleEl: el("title"),
    flush,
  });
  return { mode, flush, el, tab };
}

describe("initPanelMode", () => {
  it("startet im Dokument-Modus mit aktivem Tab", () => {
    const { el, tab } = modeSetup();
    expect(el("draft-el").hidden).toBe(false);
    expect(el("cc").hidden).toBe(true);
    expect(tab("draft").classList.contains("active")).toBe(true);
  });

  it("wechselt Sichtbarkeit, Titel und aktive Markierung", () => {
    const { mode, flush, el, tab } = modeSetup();
    mode.to("commands");
    expect(flush).toHaveBeenCalledOnce();
    expect(el("cc").hidden).toBe(false);
    expect(el("draft-el").hidden).toBe(true);
    expect(el("title").textContent).toBe("Befehle");
    expect(tab("commands").classList.contains("active")).toBe(true);
    expect(tab("draft").classList.contains("active")).toBe(false);
    mode.to("draft");
    expect(el("title").textContent).toBe("Mein Titel");
  });

  it("Tab-Klick wechselt den Modus", () => {
    const { el, tab } = modeSetup();
    tab("wiki").click();
    expect(el("wc").hidden).toBe(false);
    expect(el("draft-el").hidden).toBe(true);
  });

  it("clear hebt die Auswahl auf; danach wählt to() wieder aus", () => {
    const { mode, el, tab } = modeSetup();
    mode.clear();
    expect(document.querySelectorAll(".active").length).toBe(0);
    expect(mode.current()).toBe(null);
    mode.to("draft");
    expect(tab("draft").classList.contains("active")).toBe(true);
    expect(el("title").textContent).toBe("Mein Titel");
  });
});

describe("initCommandsView", () => {
  beforeEach(() => {
    document.body.innerHTML = `<div id="c"></div>`;
  });

  const jsonl = [
    JSON.stringify({ ts: 1700000000, session: true }),
    JSON.stringify({
      ts: 1700000100,
      commands: [{ cmd: "ls -la", note: "Liste", id: "id-1" }, { cmd: "pwd", id: "id-2" }],
    }),
  ].join("\n");

  it("rendert Kacheln, Session-Trenner und meldet empty korrekt", () => {
    const view = initCommandsView(document.getElementById("c")!, () => {});
    view.set(jsonl);
    expect(view.empty()).toBe(false);
    expect(document.querySelectorAll(".cmd-tile").length).toBe(2);
    expect(document.querySelectorAll(".cmd-sep").length).toBe(1);
    expect(document.querySelector(".cmd-text")!.textContent).toBe("ls -la");
    view.set("");
    expect(view.empty()).toBe(true);
  });

  /// Gelöscht wird über die stabile ID aus write_commands — keine Indizes,
  /// die der Datei hinterherhinken könnten.
  it("meldet Löschen mit der Eintrags-ID", () => {
    const onDelete = vi.fn();
    const view = initCommandsView(document.getElementById("c")!, onDelete);
    view.set(jsonl);
    const dels = document.querySelectorAll<HTMLElement>(".cmd-del");
    dels[1].click(); // zweite Kachel im Record 1
    expect(onDelete).toHaveBeenCalledWith("id-2");
  });
});
