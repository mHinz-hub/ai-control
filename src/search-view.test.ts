// Suchfeld-Logik (Live-Suche, Debounce, Schwelle) und Treffer-Kacheln.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initSearchView } from "./search-view";

function setup() {
  document.body.innerHTML = `<div id="s"></div>`;
  const onOpen = vi.fn();
  const onSearch = vi.fn();
  const view = initSearchView(document.getElementById("s")!, onOpen, onSearch);
  const input = document.querySelector<HTMLInputElement>(".hit-search input")!;
  const type = (v: string) => {
    input.value = v;
    input.dispatchEvent(new Event("input"));
  };
  const enter = () =>
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  return { view, onOpen, onSearch, input, type, enter };
}

const run = JSON.stringify({
  query: "arch",
  tag: null,
  home: "/tmp/archiv",
  hits: [{ id: "id-x", relpath: "a/2026-01-01_0000-x.md", title: "X", snippet: "ein **arch**iv" }],
});

describe("initSearchView", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("sucht ab 3 Zeichen nach 300 ms mit Präfix-Stern", () => {
    const { onSearch, type } = setup();
    type("arch");
    expect(onSearch).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(onSearch).toHaveBeenCalledWith("arch*");
  });

  it("hängt an #tag-Tokens keinen Stern an", () => {
    const { onSearch, type } = setup();
    type("panel #adr");
    vi.advanceTimersByTime(300);
    expect(onSearch).toHaveBeenCalledWith("panel #adr");
  });

  it("entprellt: nur die letzte Eingabe zählt", () => {
    const { onSearch, type } = setup();
    type("arc");
    vi.advanceTimersByTime(200);
    type("arch");
    vi.advanceTimersByTime(300);
    expect(onSearch).toHaveBeenCalledOnce();
    expect(onSearch).toHaveBeenCalledWith("arch*");
  });

  it("räumt beim Löschen unter 3 Zeichen auf und zeigt den Hinweis", () => {
    const { view, onSearch, type } = setup();
    view.set(run);
    expect(document.querySelectorAll(".hit-tile").length).toBe(1);
    type("ar");
    expect(onSearch).not.toHaveBeenCalled();
    expect(document.querySelectorAll(".hit-tile").length).toBe(0);
    expect(document.querySelector(".hit-head")!.textContent).toContain(
      "Mindestens 3 Zeichen",
    );
    type("");
    expect(document.querySelector(".hit-head")).toBe(null);
  });

  it("Enter sucht sofort und wörtlich", () => {
    const { onSearch, type, enter } = setup();
    type("panel arch");
    enter();
    expect(onSearch).toHaveBeenCalledWith("panel arch");
  });

  it("rendert Treffer; Klick liefert den absoluten Pfad", () => {
    const { view, onOpen } = setup();
    view.set(run);
    const tile = document.querySelector<HTMLElement>(".hit-tile")!;
    expect(tile.querySelector(".hit-title")!.textContent).toBe("X");
    expect(tile.querySelectorAll("mark").length).toBe(1);
    tile.click();
    expect(onOpen).toHaveBeenCalledWith(
      "/tmp/archiv/a/2026-01-01_0000-x.md",
      "a/2026-01-01_0000-x.md",
      "id-x",
    );
  });

  it("zeigt „Keine Treffer“ bei leerem Ergebnis", () => {
    const { view } = setup();
    view.set(JSON.stringify({ query: "nix", tag: "adr", home: "/a", hits: [] }));
    expect(document.querySelector(".hit-head")!.textContent).toBe(
      "Keine Treffer für „nix“ · #adr",
    );
    expect(view.empty()).toBe(true);
  });

  it("übernimmt die Query eines fremden Suchlaufs ins Feld", () => {
    const { view, input } = setup();
    view.set(run);
    expect(input.value).toBe("arch");
  });
});
