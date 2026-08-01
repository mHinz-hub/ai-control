// Fundstellen markieren: alle Vorkommen, unabhängig von Groß- und
// Kleinschreibung, ohne die Struktur des Dokuments zu zerstören.
import { expect, it } from "vitest";
import { markiere } from "./highlight";

const box = (html: string) => {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
};

it("markiert jedes Vorkommen und liefert die erste Marke", () => {
  const el = box("<p>Kessel und <b>kesselhaus</b></p><p>nichts</p>");
  const erste = markiere(el, ["kessel"]);
  const marken = el.querySelectorAll("mark.wiki-hit");
  expect(marken).toHaveLength(2);
  expect(marken[0].textContent).toBe("Kessel");
  expect(marken[1].textContent).toBe("kessel");
  expect(erste).toBe(marken[0]);
  // Der übrige Text bleibt, auch die Auszeichnung drumherum.
  expect(el.querySelector("b")!.textContent).toBe("kesselhaus");
  expect(el.textContent).toBe("Kessel und kesselhausnichts");
});

it("mehrere Wörter, Fundstellen in Reihenfolge", () => {
  const el = box("<p>alpha beta gamma</p>");
  markiere(el, ["gamma", "alpha"]);
  const marken = [...el.querySelectorAll("mark")].map((m) => m.textContent);
  expect(marken).toEqual(["alpha", "gamma"]);
});

it("ohne Treffer bleibt alles unverändert", () => {
  const el = box("<p>alpha</p>");
  expect(markiere(el, ["zeta"])).toBeNull();
  expect(markiere(el, [" ", ""])).toBeNull();
  expect(el.innerHTML).toBe("<p>alpha</p>");
});
