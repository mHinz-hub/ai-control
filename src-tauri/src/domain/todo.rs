//! Feature: Todoliste. Muster robotunits: Datei im Projekt-Root, per
//! SessionStart-Hook (jq) als additionalContext in jede Session injiziert.

use std::fs;

use crate::domain::check_name;
use crate::domain::paths::Paths;
use crate::domain::project::settings_path;
use crate::domain::registry::project_dir;

pub(crate) const TODO_FILE: &str = "OFFENE-PUNKTE.md";
const TODO_SKELETON: &str = "# Offene Punkte — bei jedem Start prüfen und abhaken\n\nKeine offenen Punkte.\n";

/// Der Hook landet in der `settings.json` des Projekts und wird von Claude Code
/// bei jedem Sessionstart über die Shell ausgeführt. Der Pfad muss darum
/// gequotet werden: Er stammt aus dem Ordnernamen, den der Nutzer im Dialog
/// wählt — bei einem geklonten Fremd-Repo also von außen. Unquotiert genügte
/// ein Ordner `repo$(…)` für dauerhafte Codeausführung, und schon ein
/// Leerzeichen im Pfad hätte den Hook still zerbrochen.
fn todo_hook_command(dir: &std::path::Path) -> String {
  format!(
    "jq -Rs '{{systemMessage: ., hookSpecificOutput:{{hookEventName:\"SessionStart\", additionalContext: .}}}}' {}",
    shell_quote(&dir.join(TODO_FILE).to_string_lossy())
  )
}

/// Ein Argument für `sh -c` in einfache Anführungszeichen setzen. Innerhalb
/// davon ist jedes Zeichen literal; einzig das Apostroph selbst muss die
/// Quotierung verlassen und wieder betreten (`'\''`).
fn shell_quote(s: &str) -> String {
  format!("'{}'", s.replace('\'', r"'\''"))
}

fn hook_is_todo(group: &serde_json::Value) -> bool {
  group["hooks"]
    .as_array()
    .map(|hs| {
      hs.iter().any(|h| {
        h["command"]
          .as_str()
          .map_or(false, |c| c.contains(TODO_FILE))
      })
    })
    .unwrap_or(false)
}

pub(crate) fn todo_state_in(paths: &Paths, name: &str) -> Result<bool, String> {
  check_name(name)?;
  let sp = settings_path(&project_dir(paths, name)?);
  if !sp.is_file() {
    return Ok(false);
  }
  let raw = fs::read_to_string(&sp).map_err(|e| format!("{}: {e}", sp.display()))?;
  let v: serde_json::Value =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?;
  Ok(
    v["hooks"]["SessionStart"]
      .as_array()
      .map(|a| a.iter().any(hook_is_todo))
      .unwrap_or(false),
  )
}

pub(crate) fn set_todo_in(paths: &Paths, name: &str, enabled: bool) -> Result<(), String> {
  check_name(name)?;
  let dir = project_dir(paths, name)?;
  let sp = settings_path(&dir);
  if !sp.is_file() {
    return Err(format!("settings.json fehlt: {}", sp.display()));
  }
  let raw = fs::read_to_string(&sp).map_err(|e| format!("{}: {e}", sp.display()))?;
  let mut v: serde_json::Value =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?;

  let root = v.as_object_mut().ok_or("settings.json ist kein Objekt")?;
  let hooks = root
    .entry("hooks")
    .or_insert_with(|| serde_json::json!({}))
    .as_object_mut()
    .ok_or("hooks ist kein Objekt")?;
  let session_start = hooks
    .entry("SessionStart")
    .or_insert_with(|| serde_json::json!([]))
    .as_array_mut()
    .ok_or("SessionStart ist kein Array")?;

  session_start.retain(|g| !hook_is_todo(g));
  if enabled {
    session_start.insert(
      0,
      serde_json::json!({
        "hooks": [
          { "type": "command", "command": todo_hook_command(&dir) }
        ]
      }),
    );
    let todo_path = dir.join(TODO_FILE);
    if !todo_path.is_file() {
      fs::write(&todo_path, TODO_SKELETON)
        .map_err(|e| format!("{}: {e}", todo_path.display()))?;
    }
  }

  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&sp, &(raw + "\n"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::project::{create_project_full_in, TerminalConfig};
  use crate::domain::testutil::{create_project, tmp_paths};

  /// Der Hook-Befehl geht durch die Shell; ein Pfad mit Metazeichen darf dort
  /// nichts ausführen. Ordnernamen sind bei geklonten Repos Fremdeingabe.
  #[test]
  fn hook_befehl_quotet_den_pfad() {
    let cmd = todo_hook_command(std::path::Path::new("/tmp/repo$(touch /tmp/pwned)"));
    assert!(cmd.ends_with("'/tmp/repo$(touch /tmp/pwned)/OFFENE-PUNKTE.md'"), "{cmd}");

    // Ein Apostroph im Pfad darf die Quotierung nicht aufbrechen.
    let cmd = todo_hook_command(std::path::Path::new("/tmp/o'brien"));
    assert!(cmd.ends_with(r"'/tmp/o'\''brien/OFFENE-PUNKTE.md'"), "{cmd}");
    // Nach dem Zerlegen an den Quotes bleibt kein unquotierter Bereich übrig,
    // in dem eine Shell noch etwas zu interpretieren hätte.
    assert!(!cmd.contains("$("));
  }

  #[test]
  fn todo_zuschalten_und_abschalten() {
    let p = tmp_paths();
    let id =
      create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default(), false)
        .unwrap();
    assert!(!todo_state_in(&p, &id).unwrap());

    set_todo_in(&p, &id, true).unwrap();
    assert!(todo_state_in(&p, &id).unwrap());
    let todo_path = p.projects_dir().join("proj").join(TODO_FILE);
    assert_eq!(fs::read_to_string(&todo_path).unwrap(), TODO_SKELETON);

    // doppelt aktivieren erzeugt keinen zweiten Hook
    set_todo_in(&p, &id, true).unwrap();
    let settings: serde_json::Value = serde_json::from_str(
      &fs::read_to_string(settings_path(&p.projects_dir().join("proj"))).unwrap(),
    )
    .unwrap();
    let groups = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(groups.iter().filter(|g| hook_is_todo(g)).count(), 1);

    // Abschalten: Hook weg, Datei (mit Inhalt) bleibt
    fs::write(&todo_path, "# Offene Punkte\n\n- [ ] wichtig\n").unwrap();
    set_todo_in(&p, &id, false).unwrap();
    assert!(!todo_state_in(&p, &id).unwrap());
    assert!(todo_path.is_file());
    assert!(fs::read_to_string(&todo_path).unwrap().contains("wichtig"));
  }

  #[test]
  fn todo_ohne_settings_scheitert() {
    let p = tmp_paths();
    create_project(&p, "alt").unwrap();
    assert!(set_todo_in(&p, "alt", true).is_err());
    assert!(!todo_state_in(&p, "alt").unwrap());
  }
}
