//! Commit-Dialog: Git-Zustand der Repos eines Projekts, Diffs, Push-Vorprüfung
//! und der Commit selbst.
//!
//! Ein Projekt ist nicht ein Repo, sondern mehrere: der Projektordner und die
//! zusätzlichen Arbeitsverzeichnisse aus `permissions.additionalDirectories`.
//! Mehrere Einträge können in dasselbe Repo zeigen (Unterordner) — deshalb
//! wird jeder Pfad erst auf seine Wurzel abgebildet (`rev-parse
//! --show-toplevel`) und die Liste danach entdoppelt.
//!
//! Alles läuft über das `git`-Programm statt über eine Bibliothek: Es liest
//! dieselbe Konfiguration wie der Nutzer im Terminal (Credential-Helper,
//! SSH-Kommando, includeIf-Blöcke), und genau das entscheidet darüber, ob ein
//! Push gelingt.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Ein Repo, wie der Dialog es in der linken Spalte zeigt.
#[derive(serde::Serialize)]
pub(crate) struct Repo {
  /// Wurzel des Arbeitsbaums (absolut) — Adressat aller weiteren Aufrufe.
  pub(crate) path: String,
  /// Ordnername der Wurzel, Anzeigename der Zeile.
  pub(crate) name: String,
  /// Aktueller Branch; leer im Detached-HEAD.
  pub(crate) branch: String,
  /// Upstream als `origin/main`; `None`, wenn der Branch keinen hat — dann
  /// kann der Dialog den Push ohne Netzwerkaufruf als unmöglich anzeigen.
  pub(crate) upstream: Option<String>,
  pub(crate) files: Vec<ChangedFile>,
}

/// Eine geänderte Datei aus `git status`.
#[derive(serde::Serialize, PartialEq, Debug)]
pub(crate) struct ChangedFile {
  /// Pfad relativ zur Repo-Wurzel; bei Umbenennung der neue.
  pub(crate) path: String,
  /// Voriger Pfad einer Umbenennung — die Zeile zeigt `alt → neu`.
  pub(crate) from: Option<String>,
  /// Zusammengefasster Zustand: `M` geändert, `A` neu, `D` gelöscht,
  /// `R` umbenannt, `?` nicht versioniert.
  pub(crate) status: String,
  /// Ist die Änderung (auch) schon im Index? Steuert das Häkchen beim Öffnen.
  pub(crate) staged: bool,
}

/// Ergebnis der Push-Vorprüfung.
#[derive(serde::Serialize)]
pub(crate) struct PushCheck {
  pub(crate) ok: bool,
  /// Meldung von git — im Erfolgsfall die Zusammenfassung des Trockenlaufs,
  /// sonst der Grund (kein Upstream, Auth, Non-Fast-Forward).
  pub(crate) detail: String,
}

/// Ruft `git` in `dir` auf und liefert (Erfolg, stdout, stderr).
fn git(dir: &Path, args: &[&str]) -> Result<(bool, String, String), String> {
  let out = Command::new("git")
    .current_dir(dir)
    .args(args)
    .output()
    .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
  Ok((
    out.status.success(),
    String::from_utf8_lossy(&out.stdout).to_string(),
    String::from_utf8_lossy(&out.stderr).to_string(),
  ))
}

/// Wie `git`, aber ein Fehlschlag ist ein Fehler — für Aufrufe, deren
/// Misslingen nichts Ausgesagtes bedeutet.
fn git_ok(dir: &Path, args: &[&str]) -> Result<String, String> {
  let (ok, stdout, stderr) = git(dir, args)?;
  if !ok {
    return Err(format!("git {}: {}", args.join(" "), stderr.trim()));
  }
  Ok(stdout)
}

/// Wurzel des Repos, in dem `dir` liegt; `None`, wenn dort keins ist.
fn toplevel(dir: &Path) -> Option<PathBuf> {
  let (ok, stdout, _) = git(dir, &["rev-parse", "--show-toplevel"]).ok()?;
  ok.then(|| PathBuf::from(stdout.trim()))
}

/// Repo-Wurzeln hinter den Verzeichnissen eines Projekts, in der Reihenfolge
/// der Eingabe und ohne Dopplungen. Zugleich die Liste, gegen die die
/// Commands ein hereingereichtes Verzeichnis prüfen.
pub(crate) fn roots(dirs: &[PathBuf]) -> Vec<PathBuf> {
  let mut roots: Vec<PathBuf> = Vec::new();
  for dir in dirs {
    if let Some(root) = toplevel(dir) {
      if !roots.contains(&root) {
        roots.push(root);
      }
    }
  }
  roots
}

/// Repos hinter den Verzeichnissen eines Projekts.
pub(crate) fn repos(dirs: &[PathBuf]) -> Result<Vec<Repo>, String> {
  roots(dirs).iter().map(|r| read_repo(r)).collect()
}

/// Autorisierungsgrenze: gibt `dir` nur zurück, wenn es eine der Repo-Wurzeln
/// hinter `dirs` ist. Die Funktionen darunter schreiben (`commit`) und reden
/// mit dem Netz (`push`) — sie dürfen nur auf geprüften Wurzeln laufen, und
/// die Prüfung gehört neben sie, nicht allein in die eine Aufrufstelle.
pub(crate) fn repo_of(dirs: &[PathBuf], dir: &str) -> Result<PathBuf, String> {
  let want = PathBuf::from(dir);
  roots(dirs)
    .contains(&want)
    .then_some(want)
    .ok_or_else(|| format!("kein Repo dieses Projekts: {dir}"))
}

/// Ein Aufruf statt dreier: `--branch` stellt dem Status eine Kopfzeile
/// `## <branch>...<upstream>` voran, aus der Branch und Upstream mit
/// abfallen. Der eigene `rev-parse`-Aufruf dafür entfiel — er scheiterte
/// zudem in einem Repo ohne ersten Commit und riss damit den ganzen Dialog
/// mit; die Kopfzeile meldet diesen Fall als `No commits yet on <branch>`.
fn read_repo(root: &Path) -> Result<Repo, String> {
  let raw = git_ok(
    root,
    &["status", "--porcelain=v1", "-z", "--branch", "--untracked-files=all"],
  )?;
  let (head, rest) = split_branch_header(&raw);
  let (branch, upstream) = parse_branch_header(head);
  Ok(Repo {
    path: root.display().to_string(),
    name: root
      .file_name()
      .unwrap_or_default()
      .to_string_lossy()
      .to_string(),
    branch,
    upstream,
    files: parse_status(rest),
  })
}

/// Trennt die `--branch`-Kopfzeile vom Rest des Status-Stroms. Sie ist der
/// erste NUL-getrennte Eintrag.
fn split_branch_header(raw: &str) -> (&str, &str) {
  match raw.split_once('\0') {
    Some((head, rest)) => (head, rest),
    None => (raw, ""),
  }
}

/// `## main...origin/main [ahead 1]` → ("main", Some("origin/main")).
/// Ohne Upstream fehlt der `...`-Teil, im Detached-HEAD steht dort
/// `HEAD (no branch)`, vor dem ersten Commit `No commits yet on main`.
fn parse_branch_header(head: &str) -> (String, Option<String>) {
  let head = head.trim_start_matches("## ").trim();
  if head.starts_with("HEAD (no branch)") {
    return (String::new(), None);
  }
  let head = head.strip_prefix("No commits yet on ").unwrap_or(head);
  // Die Klammer trägt nur den Abstand zum Upstream (`[ahead 1]`).
  let head = head.split(" [").next().unwrap_or(head);
  match head.split_once("...") {
    Some((branch, up)) => (branch.to_string(), Some(up.to_string())),
    None => (head.to_string(), None),
  }
}

/// `git status --porcelain=v1 -z` zerlegen.
///
/// Das NUL-Format statt der Zeilenform, weil letztere Pfade mit Sonderzeichen
/// quotet und maskiert — hier kommen sie roh. Ein Eintrag ist `XY <pfad>`;
/// bei Umbenennung und Kopie folgt der vorige Pfad als eigener Eintrag.
pub(crate) fn parse_status(raw: &str) -> Vec<ChangedFile> {
  let mut items = raw.split('\0').filter(|s| !s.is_empty());
  let mut files = Vec::new();
  while let Some(entry) = items.next() {
    let (code, path) = entry.split_at(3.min(entry.len()));
    let x = code.chars().next().unwrap_or(' ');
    let y = code.chars().nth(1).unwrap_or(' ');
    let from = matches!(x, 'R' | 'C').then(|| items.next().unwrap_or_default().to_string());
    // Der Indexstand (X) sagt, was der Commit heute enthielte; steht dort
    // nichts, zählt der Arbeitsbaum (Y).
    let main = if x != ' ' && x != '?' { x } else { y };
    files.push(ChangedFile {
      path: path.to_string(),
      from,
      status: main.to_string(),
      staged: x != ' ' && x != '?',
    });
  }
  files
}

/// Pfad als wörtlicher Pathspec: ohne das Präfix liest git `*`, `?`, `[` und
/// `\` im Dateinamen als Muster — eine ausgewählte `a?b.txt` nähme dann auch
/// `axb.txt` mit, die der Nutzer gerade abgewählt hat.
fn literal(path: &str) -> String {
  format!(":(literal){path}")
}

/// Diff einer Datei gegen HEAD. Nicht versionierte Dateien haben dort keinen
/// Vorgänger — für sie vergleicht `--no-index` gegen `/dev/null`, was den
/// ganzen Inhalt als Zugang zeigt. Beide Wege liefern denselben Unified-Diff.
pub(crate) fn diff(root: &Path, path: &str, untracked: bool) -> Result<String, String> {
  let spec = literal(path);
  let args: Vec<&str> = if untracked {
    // `--no-index` vergleicht zwei Dateien im Dateisystem, nicht zwei
    // Pathspecs — hier steht der rohe Pfad.
    vec!["diff", "--no-index", "--", "/dev/null", path]
  } else {
    vec!["diff", "HEAD", "--", &spec]
  };
  // `--no-index` meldet einen Unterschied per Exit-Code 1; das ist der
  // Normalfall und kein Fehlschlag.
  let (_, stdout, stderr) = git(root, &args)?;
  if stdout.is_empty() && !stderr.trim().is_empty() {
    return Err(stderr.trim().to_string());
  }
  Ok(stdout)
}

/// Push-Vorprüfung: der Trockenlauf macht Verbindung, Authentifizierung und
/// die Fast-Forward-Prüfung gegen den Remote-Stand vollständig durch und
/// überträgt allein die Objekte nicht. Damit steht vor dem Commit fest, ob
/// der Push danach durchgeht.
///
/// Ein Branch ohne Upstream kommt gar nicht erst ans Netz: die Antwort steht
/// lokal fest. `detail` bleibt dann leer — die Oberfläche setzt dafür ihren
/// eigenen Text, während sie sonst die Meldung von git zeigt.
pub(crate) fn push_check(root: &Path) -> Result<PushCheck, String> {
  let (has_upstream, ..) =
    git(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])?;
  if !has_upstream {
    return Ok(PushCheck { ok: false, detail: String::new() });
  }
  let (ok, stdout, stderr) = git(root, &["push", "--dry-run"])?;
  let detail = [stderr.trim(), stdout.trim()]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
  Ok(PushCheck { ok, detail })
}

/// Ergebnis eines Commits: der Commit selbst gelingt oder scheitert, der Push
/// danach ist eine eigene Auskunft — nur so kann die Oberfläche einen
/// misslungenen Push melden, ohne den gelungenen Commit zu verschweigen.
#[derive(serde::Serialize)]
pub(crate) struct CommitDone {
  pub(crate) log: String,
  /// Fehlermeldung des Pushs; leer, wenn er gelang oder nicht verlangt war.
  pub(crate) push_error: String,
}

/// Ausgewählte Dateien committen, auf Wunsch anschließend pushen.
///
/// `git add -A -- <pfade>` bringt auch Löschungen in den Index, `git commit --
/// <pfade>` committet genau diese Pfade — was sonst noch vorgemerkt war,
/// bleibt unangetastet. Der Weg über `git reset` verbot sich: er hätte eine
/// von Hand aufgebaute Staging-Auswahl auch dann verworfen, wenn der Commit
/// danach scheitert (Hook, Signatur, Konflikt).
///
/// Bei einer Umbenennung gehört der vorige Pfad in die Commit-Auswahl — sonst
/// enthielte der Commit die neue Datei und ließe die alte stehen. In die
/// `add`-Auswahl gehört er dagegen NICHT: eine Umbenennung meldet git nur aus
/// dem Index, dort ist die Löschung des alten Pfades also schon vermerkt, und
/// im Arbeitsbaum gibt es ihn nicht mehr — `git add` bräche mit „did not match
/// any files" ab.
pub(crate) fn commit(
  root: &Path,
  files: &[String],
  message: &str,
  push: bool,
) -> Result<CommitDone, String> {
  if files.is_empty() {
    return Err("keine Datei ausgewählt".into());
  }
  if message.trim().is_empty() {
    return Err("leere Commit-Nachricht".into());
  }
  let status = git_ok(root, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
  let renamed: Vec<String> = parse_status(&status)
    .into_iter()
    .filter(|f| files.contains(&f.path))
    .filter_map(|f| f.from)
    .collect();
  let add_specs: Vec<String> = files.iter().map(|p| literal(p)).collect();
  let commit_specs: Vec<String> = files
    .iter()
    .chain(renamed.iter())
    .map(|p| literal(p))
    .collect();
  let mut add: Vec<&str> = vec!["add", "-A", "--"];
  add.extend(add_specs.iter().map(|s| s.as_str()));
  git_ok(root, &add)?;
  let mut done = CommitDone {
    log: {
      let mut c: Vec<&str> = vec!["commit", "-m", message, "--"];
      c.extend(commit_specs.iter().map(|s| s.as_str()));
      git_ok(root, &c)?
    },
    push_error: String::new(),
  };
  if push {
    let (ok, stdout, stderr) = git(root, &["push"])?;
    if ok {
      done.log.push_str(&stdout);
      done.log.push_str(&stderr);
    } else {
      done.push_error = stderr.trim().to_string();
    }
  }
  Ok(done)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn status_liest_zustaende() {
    // XY-Paare wie git sie liefert: Index links, Arbeitsbaum rechts.
    let raw = "M  a.rs\0 M b.rs\0?? neu.txt\0A  c.rs\0 D weg.rs\0";
    let files = parse_status(raw);
    assert_eq!(files.len(), 5);
    assert_eq!(files[0], ChangedFile { path: "a.rs".into(), from: None, status: "M".into(), staged: true });
    assert_eq!(files[1], ChangedFile { path: "b.rs".into(), from: None, status: "M".into(), staged: false });
    assert_eq!(files[2], ChangedFile { path: "neu.txt".into(), from: None, status: "?".into(), staged: false });
    assert_eq!(files[3].status, "A");
    assert_eq!(files[4], ChangedFile { path: "weg.rs".into(), from: None, status: "D".into(), staged: false });
  }

  #[test]
  fn status_liest_umbenennung() {
    // Bei R folgt der vorige Pfad als eigener NUL-Eintrag.
    let raw = "R  neu.rs\0alt.rs\0M  x.rs\0";
    let files = parse_status(raw);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "neu.rs");
    assert_eq!(files[0].from.as_deref(), Some("alt.rs"));
    assert_eq!(files[0].status, "R");
    assert_eq!(files[1].path, "x.rs");
  }

  #[test]
  fn status_behaelt_sonderzeichen() {
    // Ohne -z stünde hier ein gequoteter, maskierter Pfad.
    let raw = " M ordner/mit leer\"zeichen.md\0";
    let files = parse_status(raw);
    assert_eq!(files[0].path, "ordner/mit leer\"zeichen.md");
  }

  #[test]
  fn commit_verlangt_auswahl_und_nachricht() {
    let dir = std::env::temp_dir();
    assert!(commit(&dir, &[], "x", false).is_err());
    assert!(commit(&dir, &["a".into()], "  ", false).is_err());
  }

  #[test]
  fn branch_kopfzeile_liest_branch_und_upstream() {
    assert_eq!(
      parse_branch_header("## main...origin/main"),
      ("main".into(), Some("origin/main".into()))
    );
    // Abstandsangabe hinter dem Upstream gehört nicht zum Namen.
    assert_eq!(
      parse_branch_header("## main...origin/main [ahead 2, behind 1]"),
      ("main".into(), Some("origin/main".into()))
    );
    // Ohne Upstream fehlt der ...-Teil.
    assert_eq!(parse_branch_header("## wip"), ("wip".into(), None));
    // Detached HEAD hat keinen Branch.
    assert_eq!(parse_branch_header("## HEAD (no branch)"), (String::new(), None));
    // Vor dem ersten Commit — der Branch steht hinter dem Hinweis.
    assert_eq!(
      parse_branch_header("## No commits yet on main"),
      ("main".into(), None)
    );
  }

  #[test]
  fn status_ohne_kopfzeile_bleibt_ganz() {
    let (head, rest) = split_branch_header("## main\0M  a.rs\0");
    assert_eq!(head, "## main");
    assert_eq!(rest, "M  a.rs\0");
    // Ein leerer Status trägt nur die Kopfzeile.
    assert_eq!(split_branch_header("## main"), ("## main", ""));
  }

  #[test]
  fn pathspec_ist_woertlich() {
    assert_eq!(literal("a?b.txt"), ":(literal)a?b.txt");
  }

  /// Wegwerf-Repo mit einem Commit; `git` selbst ist der Prüfstein — die
  /// beiden Fehler an dieser Stelle (halbe Umbenennung, dann ein `add`, das
  /// den alten Pfad nicht findet) waren beide nur echt gegen echtes git zu
  /// sehen.
  fn repo(name: &str) -> std::path::PathBuf {
    // Name je Test: die Testfaelle laufen nebenlaeufig im selben Tempordner.
    let dir = std::env::temp_dir().join(format!("git-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for args in [
      vec!["init", "--quiet"],
      vec!["config", "user.email", "t@example.invalid"],
      vec!["config", "user.name", "Test"],
      vec!["config", "commit.gpgsign", "false"],
    ] {
      git_ok(&dir, &args).unwrap();
    }
    std::fs::write(dir.join("alt.txt"), "inhalt\n").unwrap();
    git_ok(&dir, &["add", "-A"]).unwrap();
    git_ok(&dir, &["commit", "-m", "erst"]).unwrap();
    dir
  }

  #[test]
  fn umbenennung_kommt_ganz_in_den_commit() {
    let dir = repo("rename");
    git_ok(&dir, &["mv", "alt.txt", "neu.txt"]).unwrap();
    // Die Oberfläche kennt nur den neuen Pfad — der alte muss von selbst
    // mitgehen, sonst bliebe alt.txt in HEAD stehen.
    commit(&dir, &["neu.txt".into()], "umbenannt", false).unwrap();
    let tracked = git_ok(&dir, &["ls-files"]).unwrap();
    assert_eq!(tracked.trim(), "neu.txt");
    assert!(git_ok(&dir, &["status", "--porcelain"]).unwrap().trim().is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn abgewaehltes_bleibt_draussen_und_der_index_unberuehrt() {
    let dir = repo("auswahl");
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    // b.txt ist von Hand vorgemerkt, aber nicht ausgewählt: der Commit darf
    // sie nicht mitnehmen und die Vormerkung nicht verwerfen.
    git_ok(&dir, &["add", "b.txt"]).unwrap();
    commit(&dir, &["a.txt".into()], "nur a", false).unwrap();
    let head = git_ok(&dir, &["show", "--name-only", "--format=", "HEAD"]).unwrap();
    assert_eq!(head.trim(), "a.txt");
    let staged = git_ok(&dir, &["diff", "--cached", "--name-only"]).unwrap();
    assert_eq!(staged.trim(), "b.txt");
    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn repo_of_laesst_nur_projekt_repos_durch() {
    // Ohne Repos hinter den Verzeichnissen gibt es nichts freizugeben.
    let tmp = std::env::temp_dir();
    assert!(repo_of(&[], &tmp.display().to_string()).is_err());
    assert!(repo_of(&[tmp.clone()], "/etc").is_err());
  }
}
