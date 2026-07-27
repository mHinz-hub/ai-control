/// Unified-Diff als Zeilenraster: alte Zeilennummer, neue Zeilennummer, Text.
/// Eigene Datei, weil das die einzige Stelle des Commit-Fensters ist, die ohne
/// Fenster und ohne Tauri auskommt — und damit die einzige, die sich prüfen
/// lässt.

/// Zeilenart eines Unified-Diffs; `inHunk` sagt, ob der erste `@@`-Kopf schon
/// gelesen ist.
///
/// Vor dem ersten Hunk sind `+++`/`---` Dateiangaben, danach ist eine Zeile,
/// die mit `---` beginnt, entfernter Inhalt — eine gelöschte YAML-Trennlinie
/// etwa. Zählte sie als Kopfzeile, verlöre sie ihre Nummer und verschöbe alle
/// folgenden Nummern des Abschnitts.
export function diffClass(line: string, inHunk: boolean): string {
  if (line.startsWith("@@")) return "d-hunk";
  if (!inHunk) {
    if (line.startsWith("+++") || line.startsWith("---")) return "d-meta";
    if (line.startsWith("diff ") || line.startsWith("index ")) return "d-meta";
    if (line.startsWith("new file") || line.startsWith("deleted file")) return "d-meta";
    if (line.startsWith("similarity index") || line.startsWith("rename ")) return "d-meta";
  }
  if (line.startsWith("\\ No newline")) return "d-meta";
  if (line.startsWith("+")) return "d-add";
  if (line.startsWith("-")) return "d-del";
  return "";
}

/// Baut die Zeilen des Diffs. Die Nummern kommen aus den Hunk-Köpfen
/// (`@@ -a,b +c,d @@`) und laufen von dort mit; ohne sie wäre eine Fundstelle
/// im Diff nicht auffindbar. Ein Fragment statt einzelner Einhängungen: ein
/// großer Diff sind sonst Tausende Mutationen am gerenderten Baum.
export function renderDiff(text: string): DocumentFragment {
  const frag = document.createDocumentFragment();
  let oldNo = 0;
  let newNo = 0;
  let inHunk = false;
  for (const line of text.split("\n")) {
    const kind = diffClass(line, inHunk);
    const row = document.createElement("span");
    row.className = "dl " + kind;
    const left = document.createElement("i");
    const right = document.createElement("i");
    left.className = "dln";
    right.className = "dln";
    if (kind === "d-hunk") {
      const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(line);
      if (m) {
        oldNo = Number(m[1]);
        newNo = Number(m[2]);
        inHunk = true;
      }
    } else if (kind === "d-add") {
      right.textContent = String(newNo++);
    } else if (kind === "d-del") {
      left.textContent = String(oldNo++);
    } else if (kind === "") {
      left.textContent = String(oldNo++);
      right.textContent = String(newNo++);
    }
    const body = document.createElement("code");
    body.textContent = line;
    row.append(left, right, body);
    frag.append(row);
  }
  return frag;
}
