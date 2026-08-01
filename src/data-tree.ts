/// Faltbare Baum-Ansicht für strukturierte Archiv-Dateien: JSON und YAML
/// über ihren geparsten Wert, XML über den DOM. Gefaltet wird mit
/// <details>/<summary> — Auf- und Zuklappen macht der Browser. Bis zur
/// Tiefe 2 offen, darunter zu: tiefe Bäume bleiben so überschaubar.

const OFFEN_BIS = 2;

function zeile(cls: string, key: string | null, wert: string): HTMLElement {
  const div = document.createElement("div");
  div.className = "dt-leaf";
  if (key !== null) {
    const k = document.createElement("span");
    k.className = "dt-key";
    k.textContent = key;
    div.append(k, ": ");
  }
  const v = document.createElement("span");
  v.className = cls;
  v.textContent = wert;
  div.append(v);
  return div;
}

function skalar(key: string | null, value: unknown): HTMLElement {
  if (value === null || value === undefined) return zeile("dt-null", key, "null");
  if (typeof value === "string") return zeile("dt-str", key, JSON.stringify(value));
  return zeile(typeof value === "number" ? "dt-num" : "dt-bool", key, String(value));
}

function ast(key: string | null, value: unknown, tiefe: number): HTMLElement {
  const kinder: [string, unknown][] = Array.isArray(value)
    ? value.map((v, i) => [String(i), v])
    : value && typeof value === "object"
      ? Object.entries(value as Record<string, unknown>)
      : [];
  if (!kinder.length && (typeof value !== "object" || value === null)) {
    return skalar(key, value);
  }
  const det = document.createElement("details");
  det.className = "dt-branch";
  det.open = tiefe < OFFEN_BIS;
  const sum = document.createElement("summary");
  const klammer = Array.isArray(value) ? `[${kinder.length}]` : `{${kinder.length}}`;
  if (key !== null) {
    const k = document.createElement("span");
    k.className = "dt-key";
    k.textContent = key;
    sum.append(k, " ");
  }
  const hint = document.createElement("span");
  hint.className = "dt-hint";
  hint.textContent = klammer;
  sum.append(hint);
  det.append(sum);
  const box = document.createElement("div");
  box.className = "dt-children";
  for (const [k, v] of kinder) box.append(ast(k, v, tiefe + 1));
  det.append(box);
  return det;
}

/// Baum eines geparsten JSON-/YAML-Werts.
export function dataTree(value: unknown): HTMLElement {
  const root = document.createElement("div");
  root.className = "dt-root";
  root.append(ast(null, value, 0));
  return root;
}

function xmlAst(el: Element, tiefe: number): HTMLElement {
  const attrs = [...el.attributes]
    .map((a) => `${a.name}="${a.value}"`)
    .join(" ");
  const kopf = attrs ? `<${el.tagName} ${attrs}>` : `<${el.tagName}>`;
  const kinder = [...el.children];
  const text = [...el.childNodes]
    .filter((n) => n.nodeType === Node.TEXT_NODE)
    .map((n) => n.textContent?.trim() ?? "")
    .join(" ")
    .trim();
  if (!kinder.length) {
    const div = document.createElement("div");
    div.className = "dt-leaf";
    const tag = document.createElement("span");
    tag.className = "dt-tag";
    tag.textContent = kopf;
    div.append(tag);
    if (text) {
      const v = document.createElement("span");
      v.className = "dt-str";
      v.textContent = ` ${text}`;
      div.append(v);
    }
    return div;
  }
  const det = document.createElement("details");
  det.className = "dt-branch";
  det.open = tiefe < OFFEN_BIS;
  const sum = document.createElement("summary");
  const tag = document.createElement("span");
  tag.className = "dt-tag";
  tag.textContent = kopf;
  sum.append(tag);
  const hint = document.createElement("span");
  hint.className = "dt-hint";
  hint.textContent = ` (${kinder.length})`;
  sum.append(hint);
  det.append(sum);
  const box = document.createElement("div");
  box.className = "dt-children";
  if (text) box.append(zeile("dt-str", null, text));
  for (const k of kinder) box.append(xmlAst(k, tiefe + 1));
  det.append(box);
  return det;
}

/// Baum eines XML-Dokuments (Wurzelelement).
export function xmlTree(root: Element): HTMLElement {
  const box = document.createElement("div");
  box.className = "dt-root";
  box.append(xmlAst(root, 0));
  return box;
}
