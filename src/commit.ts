/// Commit-Fenster eines Projekts: links die Repos, daneben ihre geänderten
/// Dateien, rechts der Diff, unten die Nachricht.
///
/// Ein Projekt hat oft mehr als ein Repo (Projektordner + Arbeitsverzeichnisse).
/// Deshalb sind die Repos die oberste Ebene, und Auswahl wie Nachricht gehören
/// zu genau einem von ihnen; committet wird immer nur das aktive.
///
/// Ob ein Push durchgeht, steht vor dem Commit fest: `git push --dry-run` läuft
/// beim Öffnen für jedes Repo nebenläufig und entscheidet, ob der Knopf
/// „Commit und Push" anwählbar ist.

import "./panel-window.css";
import "./commit-window.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { applyTheme, THEMES } from "./themes";
import { applyI18n, t } from "./messages";
import { renderDiff } from "./diff-view";

interface ChangedFile {
  path: string;
  from: string | null;
  status: string;
  staged: boolean;
}
interface Repo {
  path: string;
  name: string;
  branch: string;
  upstream: string | null;
  files: ChangedFile[];
}
interface PushCheck {
  ok: boolean;
  /// Meldung von git; leer, wenn der Branch keinen Upstream hat.
  detail: string;
}
interface CommitDone {
  log: string;
  /// Fehlermeldung des Pushs; leer, wenn er gelang oder nicht verlangt war.
  push_error: string;
}

applyI18n();
const project = new URLSearchParams(location.search).get("project")!;
const win = getCurrentWebviewWindow();

function showError(msg: string) {
  const el = document.createElement("pre");
  el.style.cssText =
    "color:#f38ba8;padding:16px;font:12px Menlo,monospace;white-space:pre-wrap";
  el.textContent = msg;
  document.body.appendChild(el);
  invoke("term_log", { msg: `commit: ${msg}` });
}
window.addEventListener("error", (e) =>
  showError(`error: ${e.message}\n${e.error?.stack ?? ""}`),
);
window.addEventListener("unhandledrejection", (e) =>
  showError(`rejection: ${e.reason}\n${e.reason?.stack ?? ""}`),
);

// Dekorationsloses Fenster (Linux): eigene Knöpfe und Resize-Zonen; macOS
// behält die native Ampel.
const isMac = /Mac|Macintosh/.test(navigator.userAgent);
document.documentElement.dataset.platform = isMac ? "mac" : "other";
document.getElementById("win-close")!.addEventListener("click", () => win.close());
if (!isMac) {
  document.getElementById("win-min")!.addEventListener("click", () => win.minimize());
  document
    .getElementById("win-max")!
    .addEventListener("click", () => win.toggleMaximize());
  for (const g of document.querySelectorAll<HTMLElement>(".grip")) {
    g.addEventListener("mousedown", (e) => {
      e.preventDefault();
      win.startResizeDragging(g.dataset.dir as never);
    });
  }
}

// Das Theme kommt als URL-Parameter vom Öffner — die Projektliste dafür zu
// holen kostete je Projekt ein `pgrep`, nur um einen Namen zu erfahren.
const theme = new URLSearchParams(location.search).get("theme");
applyTheme(THEMES[theme || "mocha"]);

const repoList = document.getElementById("repos")!;
const fileList = document.getElementById("files")!;
const diffBox = document.getElementById("diff")!;
const msgBox = document.getElementById("msg") as HTMLTextAreaElement;
const allBox = document.getElementById("all") as HTMLInputElement;
const statusLine = document.getElementById("status")!;
const commitBtn = document.getElementById("commit") as HTMLButtonElement;
const pushBtn = document.getElementById("commit-push") as HTMLButtonElement;

let repos: Repo[] = [];
/// Ausgewählte Dateien je Repo — beim Laden alles, wie `git add -A`.
const selected = new Map<string, Set<string>>();
/// Nachricht je Repo; der Wechsel in der Repo-Spalte verliert sie nicht.
const messages = new Map<string, string>();
/// Ergebnis der Push-Vorprüfung je Repo; fehlt der Eintrag, läuft sie noch.
const pushState = new Map<string, PushCheck>();
let active = "";
let activeFile = "";

function repo(path: string): Repo {
  return repos.find((r) => r.path === path)!;
}

function say(text: string, bad = false) {
  statusLine.textContent = text;
  statusLine.classList.toggle("bad", bad);
}

// ---------- Repo-Spalte ----------

/// Zeile und Ampel je Repo — gemerkt, damit ein eintreffendes Prüfergebnis
/// nur seinen Punkt umfärbt, statt die ganze Spalte samt Klick-Listenern neu
/// aufzubauen.
const repoRows = new Map<string, { row: HTMLElement; dot: HTMLElement }>();

function renderRepos() {
  repoList.replaceChildren();
  repoRows.clear();
  for (const r of repos) {
    const row = document.createElement("button");
    row.className = "commit-repo" + (r.path === active ? " active" : "");
    const dot = document.createElement("span");
    const name = document.createElement("span");
    name.className = "commit-reponame";
    name.textContent = r.name;
    const sub = document.createElement("span");
    sub.className = "commit-repometa";
    // Im Detached-HEAD gibt es keinen Branchnamen — der Zustand gehört
    // trotzdem angezeigt, ein Commit dort hängt an keinem Zweig.
    const where = r.branch || t("commit.detached");
    sub.textContent = `${where} · ${t("commit.files", { n: r.files.length })}`;
    const head = document.createElement("span");
    head.className = "commit-repohead";
    head.append(dot, name);
    row.append(head, sub);
    row.addEventListener("click", () => selectRepo(r.path));
    repoList.append(row);
    repoRows.set(r.path, { row, dot });
    paintDot(r.path);
  }
}

/// Ampel eines Repos aus dem Stand seiner Push-Prüfung.
function paintDot(path: string) {
  const dot = repoRows.get(path)?.dot;
  if (!dot) return;
  const push = pushState.get(path);
  dot.className = "commit-dot " + (push ? (push.ok ? "ok" : "bad") : "wait");
  dot.title = push ? pushReason(push) : t("commit.checking");
}

/// Grund, warum ein Push nicht durchgeht: die Meldung von git, und wo git
/// nichts sagt (kein Upstream), der eigene Text.
function pushReason(push: PushCheck): string {
  if (push.ok) return "";
  return firstLine(push.detail) || t("commit.noUpstream");
}

function selectRepo(path: string) {
  active = path;
  activeFile = "";
  msgBox.value = messages.get(path) ?? "";
  diffBox.textContent = t("commit.pick");
  for (const [p, { row }] of repoRows) row.classList.toggle("active", p === path);
  renderFiles();
}

// ---------- Dateiliste ----------

/// Zeile und Häkchen je Datei des aktiven Repos — dieselbe Überlegung wie bei
/// den Repo-Zeilen: markieren und umschalten fassen einzelne Elemente an.
const fileRows = new Map<string, { row: HTMLElement; box: HTMLInputElement }>();

function renderFiles() {
  const r = repo(active);
  const sel = selected.get(active)!;
  fileList.replaceChildren();
  fileRows.clear();
  allBox.checked = r.files.length > 0 && sel.size === r.files.length;
  allBox.disabled = r.files.length === 0;
  if (r.files.length === 0) {
    const empty = document.createElement("div");
    empty.className = "commit-empty";
    empty.textContent = t("commit.clean");
    fileList.append(empty);
    updateActions();
    return;
  }
  const frag = document.createDocumentFragment();
  for (const f of r.files) {
    const row = document.createElement("div");
    row.className = "commit-file" + (f.path === activeFile ? " active" : "");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = sel.has(f.path);
    box.addEventListener("change", () => {
      if (box.checked) sel.add(f.path);
      else sel.delete(f.path);
      allBox.checked = sel.size === r.files.length;
      updateActions();
    });
    const tag = document.createElement("span");
    tag.className = `commit-status-tag s-${f.status}`;
    tag.textContent = f.status;
    const name = document.createElement("span");
    name.className = "commit-path";
    name.textContent = f.from ? `${f.from} → ${f.path}` : f.path;
    name.title = name.textContent;
    name.addEventListener("click", () => showDiff(f));
    row.append(box, tag, name);
    frag.append(row);
    fileRows.set(f.path, { row, box });
  }
  fileList.append(frag);
  updateActions();
}

allBox.addEventListener("change", () => {
  // Ohne Repos gibt es nichts auszuwählen — der Kasten ist dann abgeschaltet,
  // aber ein Klick darf trotzdem nicht ins Leere greifen.
  if (!active) return;
  const sel = selected.get(active)!;
  sel.clear();
  for (const [path, { box }] of fileRows) {
    box.checked = allBox.checked;
    if (allBox.checked) sel.add(path);
  }
  updateActions();
});

// ---------- Diff ----------

async function showDiff(f: ChangedFile) {
  for (const [path, { row }] of fileRows) row.classList.toggle("active", path === f.path);
  activeFile = f.path;
  const text = await invoke<string>("git_diff", {
    project,
    dir: active,
    path: f.path,
    untracked: f.status === "?",
  });
  diffBox.replaceChildren(renderDiff(text));
}

// ---------- Commit ----------

/// Beide Knöpfe stehen immer da: committen geht auch, wenn der Push
/// durchginge. Ob er durchgeht, sagt allein der Zustand des Push-Knopfes —
/// der Grund steht in seinem Tooltip, nicht als Meldung in der Fußzeile.
function updateActions() {
  const sel = selected.get(active);
  const push = pushState.get(active);
  commitBtn.disabled = !sel?.size;
  pushBtn.disabled = !sel?.size || !push?.ok;
  pushBtn.title = push ? pushReason(push) : t("commit.checking");
}

function firstLine(text: string): string {
  return text.split("\n").find((l) => l.trim()) ?? "";
}

/// Committet genau das aktive Repo — nie mehrere auf einmal: jedes Repo hat
/// seine eigene Auswahl und seine eigene Nachricht.
///
/// Ein misslungener Push macht den Commit nicht rückgängig; deshalb wird der
/// Stand in beiden Fällen neu geladen und der Push-Fehler daneben gemeldet.
/// Sonst zeigte die Liste Dateien, die längst committet sind.
async function runCommit(push: boolean) {
  const dir = active;
  const name = repo(dir).name;
  commitBtn.disabled = true;
  pushBtn.disabled = true;
  let done: CommitDone;
  try {
    done = await invoke<CommitDone>("git_commit", {
      project,
      dir,
      files: [...selected.get(dir)!],
      message: msgBox.value,
      push,
    });
  } catch (e) {
    say(`${name}: ${e}`, true);
    updateActions();
    return;
  }
  messages.delete(dir);
  await reload(dir);
  if (done.push_error) say(t("commit.pushFailed", { name, grund: firstLine(done.push_error) }), true);
  else say(t("commit.done", { name }));
}

// Das Feld ist die eine Quelle für die Nachricht des aktiven Repos; jeder
// Anschlag hinterlegt sie, damit ein Repo-Wechsel sie nicht verliert.
msgBox.addEventListener("input", () => {
  if (active) messages.set(active, msgBox.value);
});
commitBtn.addEventListener("click", () => void runCommit(false));
pushBtn.addEventListener("click", () => void runCommit(true));
// Cmd/Ctrl+Enter committet aus dem Nachrichtenfeld heraus, ohne Push.
msgBox.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !commitBtn.disabled) {
    void runCommit(false);
  }
});

// ---------- Laden ----------

async function load() {
  repos = await invoke<Repo[]>("git_repos", { project });
  selected.clear();
  pushState.clear();
  for (const r of repos) selected.set(r.path, new Set(r.files.map((f) => f.path)));
  if (repos.length === 0) {
    repoList.replaceChildren();
    fileList.replaceChildren();
    diffBox.textContent = t("commit.noRepos");
    active = "";
    allBox.disabled = true;
    updateActions();
    say("");
    return;
  }
  active = repos.some((r) => r.path === active) ? active : repos[0].path;
  msgBox.value = messages.get(active) ?? "";
  diffBox.textContent = t("commit.pick");
  renderRepos();
  renderFiles();
  for (const r of repos) void check(r.path);
}

/// Push-Vorprüfung eines Repos; sie läuft je Repo nebenläufig, weil sie ans
/// Netz geht. Ohne Upstream antwortet das Backend ohne Roundtrip.
async function check(dir: string) {
  pushState.set(dir, await invoke<PushCheck>("git_push_check", { project, dir }));
  paintDot(dir);
  if (dir === active) updateActions();
}

/// Nach einem Commit hat sich nur dieses eine Repo verändert — die übrigen
/// erneut zu lesen und ihre Push-Prüfung übers Netz zu wiederholen, wäre
/// verschenkte Zeit.
async function reload(dir: string) {
  const fresh = await invoke<Repo[]>("git_repos", { project });
  const updated = fresh.find((r) => r.path === dir);
  if (!updated) return load(); // Repo verschwunden — ganz neu aufbauen.
  repos = repos.map((r) => (r.path === dir ? updated : r));
  selected.set(dir, new Set(updated.files.map((f) => f.path)));
  if (dir === active) {
    msgBox.value = messages.get(dir) ?? "";
    diffBox.textContent = t("commit.pick");
    activeFile = "";
  }
  renderRepos();
  renderFiles();
  await check(dir);
}

await load();

/// Nachrichten-Vorschläge aus `show_commit`: sie liegen als JSON in der
/// Signaldatei, die das Fenster aufgemacht hat — je Repo einer, adressiert
/// über den Ordnernamen der Repo-Wurzel. Beim Start wird die Datei gelesen
/// (das Öffnen-Event lief da schon), danach kommt jeder weitere Aufruf als
/// `commit-open` an. Ein bereits getippter Text bleibt stehen.
function suggest(raw: string) {
  if (!raw.trim()) return;
  const proposals = JSON.parse(raw) as { repo: string; message: string }[];
  for (const p of proposals) {
    const target = repos.find((r) => r.name === p.repo);
    if (!target || messages.get(target.path)?.trim()) continue;
    messages.set(target.path, p.message);
    if (target.path === active) msgBox.value = p.message;
  }
}
suggest(await invoke<string>("buffer_read", { project, buffer: "commit" }));
await listen<string>("commit-open", (e) => suggest(e.payload));
