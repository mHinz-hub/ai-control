//! Pro-Projekt-.desktop-Dateien: dash-to-dock/GNOME ordnen dem Fenster
//! (app_id `aicontrol-<projekt-id>`, via set_prgname) darüber Icon und
//! Anzeigename des Projekts zu.

use std::fs;
use std::path::PathBuf;

use crate::domain::paths::Paths;
use crate::domain::project::{read_project_config_in, resolve_icon_path, ProjectConfig};
use crate::domain::registry::load_registry;

/// ~/.local/share/applications — Ziel der pro-Terminal-.desktop-Dateien.
/// paths-abhängig (nicht $HOME direkt), damit Tests in ihr tmp-home schreiben.
fn applications_dir(paths: &Paths) -> PathBuf {
  paths.home.join(".local/share/applications")
}

/// Schreibt/aktualisiert die .desktop eines Projekts. NoDisplay=true hält den
/// App-Starter sauber.
pub(crate) fn write_terminal_desktop(paths: &Paths, project: &str, cfg: &ProjectConfig) {
  let dir = applications_dir(paths);
  if fs::create_dir_all(&dir).is_err() {
    return;
  }
  let exec = std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(str::to_string))
    .unwrap_or_else(|| "ai-control".into());
  let icon_line = cfg
    .terminal
    .icon
    .as_deref()
    .and_then(|i| resolve_icon_path(paths, project, i).ok())
    .filter(|p| p.exists())
    .and_then(|p| p.to_str().map(str::to_string))
    .map(|p| format!("Icon={p}\n"))
    .unwrap_or_default();
  let name = cfg.name.as_deref().unwrap_or(project);
  let content = format!(
    "[Desktop Entry]\nType=Application\nName={name}\nExec={exec} --terminal {project}\n\
     {icon_line}StartupWMClass=aicontrol-{project}\nNoDisplay=true\n"
  );
  let _ = fs::write(dir.join(format!("aicontrol-{project}.desktop")), content);
}

/// Entfernt die .desktop eines Projekts (beim Löschen).
pub(crate) fn remove_terminal_desktop(paths: &Paths, project: &str) {
  let _ = fs::remove_file(applications_dir(paths).join(format!("aicontrol-{project}.desktop")));
}

/// Beim App-Start: für jedes registrierte Projekt die .desktop neu schreiben und
/// verwaiste (kein registriertes Projekt mehr) entfernen. Dadurch sind sie immer
/// vorhanden und aktuell, bevor überhaupt ein Terminal startet.
pub(crate) fn sync_all_desktops(paths: &Paths) {
  let Ok(reg) = load_registry(paths) else {
    return;
  };
  for project in reg.keys() {
    let cfg = read_project_config_in(paths, project).unwrap_or_default();
    write_terminal_desktop(paths, project, &cfg);
  }
  let dir = applications_dir(paths);
  if let Ok(entries) = fs::read_dir(&dir) {
    for e in entries.flatten() {
      let name = e.file_name();
      let Some(name) = name.to_str() else { continue };
      if let Some(project) = name
        .strip_prefix("aicontrol-")
        .and_then(|n| n.strip_suffix(".desktop"))
      {
        if !reg.contains_key(project) {
          let _ = fs::remove_file(e.path());
        }
      }
    }
  }
}
