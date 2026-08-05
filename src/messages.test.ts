// Übersetzungstabellen und der Vue-freie t()/applyI18n()-Helfer.
import { afterEach, describe, expect, it } from "vitest";

import { applyI18n, messages, storedLocale, t } from "./messages";

/// Alle Punkt-Pfade eines Nachrichtenbaums, für den Vollständigkeitsabgleich.
function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === "object" && v !== null
      ? keys(v, `${prefix}${k}.`)
      : [`${prefix}${k}`],
  );
}

afterEach(() => {
  try {
    window.localStorage.removeItem("lang");
  } catch {
    /* Storage in der Testumgebung nicht verfügbar */
  }
  document.body.textContent = "";
});

describe("Nachrichtentabellen", () => {
  it("führen dieselben Schlüssel in beiden Sprachen", () => {
    expect(keys(messages.en).sort()).toEqual(keys(messages.de).sort());
  });

  it("lassen keinen Text unübersetzt stehen", () => {
    // Gleiche Texte sind fast immer eine vergessene Übersetzung. Erlaubt sind
    // nur Eigennamen und Zeichen ohne Sprache.
    const erlaubt = new Set([
      "Pool",
      "Pools",
      "Name",
      "Theme",
      "Wiki",
      "ToDo",
      // Kurzform des ToDo-Tabs — in beiden Sprachen derselbe Buchstabe.
      "T",
      "MD",
      "Autostart",
      "Input",
      "Output",
      "Session",
      "Cache ↑",
      "Cache ↓",
      "+ oAuth",
      "+ apiKey",
      "+ ToDo",
      "System",
      "Commit",
      "Terminal",
      "Terminal — {name}",
      ".desktop-Starter",
      ".ai-central/ (Config + Icon)",
      "Archiv-Berechtigung in .claude/settings.json",
    ]);
    const gleich = keys(messages.de).filter((k) => {
      const de = pick(messages.de, k);
      return de === pick(messages.en, k) && !erlaubt.has(de);
    });
    expect(gleich).toEqual([]);
  });
});

function pick(obj: object, key: string): string {
  return key
    .split(".")
    .reduce<never>((o, part) => (o as Record<string, never>)[part], obj as never);
}

describe("t()", () => {
  it("löst Punkt-Pfade auf", () => {
    expect(t("panel.tabDraft")).toBe("Dokument");
  });

  it("setzt Platzhalter ein", () => {
    expect(t("search.noHits", { scope: "„x“" })).toBe("Keine Treffer für „x“");
  });

  it("lässt unbekannte Platzhalter stehen", () => {
    expect(t("search.noHits", { falsch: "x" })).toBe("Keine Treffer für {scope}");
  });

  it("gibt bei fehlendem Schlüssel den Pfad zurück", () => {
    expect(t("panel.gibtEsNicht")).toBe("panel.gibtEsNicht");
    expect(t("nichts.da.tief")).toBe("nichts.da.tief");
  });

  it("folgt der gespeicherten Sprache", () => {
    window.localStorage.setItem("lang", "en");
    expect(storedLocale()).toBe("en");
    expect(t("panel.tabDraft")).toBe("Document");
  });

  it("fällt bei unbekannter Sprache auf die Browsersprache zurück", () => {
    window.localStorage.setItem("lang", "kli");
    expect(storedLocale()).toBe("de"); // test-setup.ts pinnt navigator.language
  });
});

describe("applyI18n()", () => {
  it("beschriftet Text und Attribute", () => {
    document.body.innerHTML = `
      <button data-i18n="panel.tabWiki" data-i18n-title="panel.tabWikiTitle">alt</button>
      <button data-i18n-aria="panel.close"></button>
      <input data-i18n-placeholder="search.placeholder" />`;
    applyI18n();
    const btn = document.querySelector("button")!;
    expect(btn.textContent).toBe("Archiv");
    expect(btn.title).toBe("Archiv-Notizen");
    expect(document.querySelectorAll("button")[1].getAttribute("aria-label")).toBe(
      "Schließen",
    );
    expect(document.querySelector("input")!.placeholder).toBe(
      "Archiv durchsuchen — #tag filtert",
    );
    expect(document.documentElement.lang).toBe("de");
  });

  it("lässt Markup ohne Marker unangetastet", () => {
    document.body.innerHTML = `<span>unberührt</span>`;
    applyI18n();
    expect(document.querySelector("span")!.textContent).toBe("unberührt");
  });
});
