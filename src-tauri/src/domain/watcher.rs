//! Session-Watcher: pollt die laufenden Projekte; wechselt eines von „läuft"
//! auf „läuft nicht mehr", ist die Session beendet → Kontext syncen (Opt-in).

use std::process::Command;
use std::time::Duration;

use crate::domain::paths::Paths;
use crate::domain::project::{display_name_in, list_projects_in};
use crate::domain::registry::project_dir;
use crate::domain::settings::sync_on_session_end;

/// Tick-Intervall des Session-Watchers.
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// Nächstliegendes Git-Repo (Ordner mit .git) ab `dir` aufwärts.
fn git_root(dir: &std::path::Path) -> Option<std::path::PathBuf> {
  dir
    .ancestors()
    .find(|a| a.join(".git").exists())
    .map(|a| a.to_path_buf())
}

/// (Projekt-ID, läuft) aller Projekte, sortiert — dieselbe Erkennung wie die
/// Projektliste; Grundlage für Session-Watcher und Tray-Menü.
fn project_state(paths: &Paths) -> Vec<(String, bool)> {
  let mut state: Vec<(String, bool)> = list_projects_in(paths)
    .map(|ps| ps.into_iter().map(|p| (p.id, p.running)).collect())
    .unwrap_or_default();
  state.sort();
  state
}

/// Committet/pusht das Repo des Projekts nach einem Session-Ende:
/// add -A → commit → pull --rebase → push. Nichts zu committen beendet still;
/// jeder andere Fehler bricht die Kette mit Meldung ab.
fn sync_session_context(repo: &std::path::Path, project: &str) {
  let run = |args: &[&str]| {
    Command::new("git")
      .arg("-C")
      .arg(repo)
      .args(args)
      .output()
      .map_err(|e| format!("git {}: {e}", args.join(" ")))
  };
  let msg = format!("session-end sync: {project}");
  let steps: [&[&str]; 4] = [
    &["add", "-A"],
    &["commit", "-m", &msg],
    &["pull", "--rebase"],
    &["push"],
  ];
  for (i, args) in steps.iter().enumerate() {
    match run(args) {
      Ok(o) if o.status.success() => {}
      // commit ohne Änderungen: nichts zu syncen, kein Fehler.
      Ok(_) if i == 1 && run(&["diff", "--cached", "--quiet"]).is_ok_and(|d| d.status.success()) => {
        return;
      }
      Ok(o) => {
        eprintln!(
          "session-end sync {project}: git {} — {}",
          args.join(" "),
          String::from_utf8_lossy(&o.stderr).trim()
        );
        return;
      }
      Err(e) => {
        eprintln!("session-end sync {project}: {e}");
        return;
      }
    }
  }
}

/// Erkennung über das Verschwinden des Prozesses, greift daher auch bei
/// HUP/Kill (im sterbenden Prozess liefe kein Hook mehr).
/// Das Popup zeigt den Laufstatus selbst über sein 2-s-Polling.
pub(crate) fn spawn_session_watcher() {
  std::thread::spawn(move || {
    let paths = Paths::real();
    let mut state = project_state(&paths);
    loop {
      std::thread::sleep(WATCH_INTERVAL);
      let current = project_state(&paths);
      if current == state {
        continue;
      }
      let ended: Vec<&String> = state
        .iter()
        .filter(|(name, running)| {
          *running && !current.iter().any(|(n, r)| n == name && *r)
        })
        .map(|(name, _)| name)
        .collect();
      // Nur syncen, wenn zugestimmt UND das Projekt in einem Git-Repo liegt —
      // der Watcher verlangt kein Repo, er nutzt es nur, falls vorhanden.
      if !ended.is_empty() && sync_on_session_end(&paths) {
        for project in ended {
          if let Ok(dir) = project_dir(&paths, project) {
            if let Some(repo) = git_root(&dir) {
              let name =
                display_name_in(&paths, project).unwrap_or_else(|_| project.clone());
              sync_session_context(&repo, &name);
            }
          }
        }
      }
      state = current;
    }
  });
}
