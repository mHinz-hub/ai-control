//! Tauri-Commands der Verwaltungs-UI (Haupt-App, Popup, Terminal-Header) —
//! dünne Delegation an domain/ und platform/.

use std::time::{Duration, Instant};

use crate::domain::archive::{
  archive_panel_content, project_archive_home, require_archive_home, ArchiveMeta,
};
use crate::domain::credentials::keychain_service;
use crate::domain::paths::Paths;
use crate::domain::pool::{
  create_apikey_pool_in, create_oauth_pool_in, create_reference_pool_in, delete_pool_in,
  ensure_oauth_pool, link_pool_runtime_in, list_pools_in, offered_default_dir, read_pool,
  rename_pool_in, set_apikey_in, PoolInfo,
};
use crate::domain::project::{
  add_project_in, add_work_dir_in, assign_pool_in, create_project_full_in, delete_preview_in,
  delete_project_scoped_in, display_name_in, is_running, kill_terminals, list_projects_in,
  project_work_dirs_in, read_project_config_in, remove_work_dir_in, resolve_icon_path,
  running_projects_using_pool, set_project_dir_in, set_terminal_config_in, unassign_pool_in,
  DeletePreview, Project, TerminalConfig,
};
use crate::domain::settings;
use crate::domain::todo::{set_todo_in, todo_state_in};
use crate::domain::usage::{usage_stats_in, UsageRow};
use crate::platform::KeychainStore;
use crate::terminal;

// ---------- Projekte ----------

#[tauri::command]
pub(crate) fn list_projects() -> Result<Vec<Project>, String> {
  list_projects_in(&Paths::real())
}

#[tauri::command]
pub(crate) fn create_project_full(
  name: String,
  dir: Option<String>,
  pool: Option<String>,
  work_dir: Option<String>,
  create_work_dir: bool,
  terminal: TerminalConfig,
  todo: bool,
) -> Result<String, String> {
  create_project_full_in(
    &Paths::real(),
    &name,
    dir.as_deref(),
    pool.as_deref(),
    work_dir.as_deref(),
    create_work_dir,
    terminal,
    todo,
  )
}

/// Bestehenden Ordner als Projekt aufnehmen (nur Registry-Eintrag).
#[tauri::command]
pub(crate) fn add_project(path: String) -> Result<(), String> {
  add_project_in(&Paths::real(), &path)
}

/// Projektordner neu zuordnen; bei laufender Session gesperrt.
#[tauri::command]
pub(crate) fn set_project_dir(project: String, dir: String) -> Result<(), String> {
  if is_running(&project) {
    let name = display_name_in(&Paths::real(), &project)?;
    return Err(format!("{name} läuft noch — erst beenden"));
  }
  set_project_dir_in(&Paths::real(), &project, &dir)
}

#[tauri::command]
pub(crate) fn add_work_dir(project: String, dir: String) -> Result<(), String> {
  add_work_dir_in(&Paths::real(), &project, &dir)
}

#[tauri::command]
pub(crate) fn remove_work_dir(project: String, dir: String) -> Result<(), String> {
  remove_work_dir_in(&Paths::real(), &project, &dir)
}

/// Artefakt-Vorschau für den Lösch-Dialog.
#[tauri::command]
pub(crate) fn delete_preview(project: String) -> Result<DeletePreview, String> {
  delete_preview_in(&Paths::real(), &project)
}

/// Löschen in drei Stufen: "integration" (nur ai-control-Spuren),
/// "archive" (zusätzlich Archiv), "full" (zusätzlich Projektordner,
/// Arbeitsordner per Flag).
#[tauri::command]
pub(crate) fn delete_project_scoped(
  project: String,
  scope: String,
  delete_work_dirs: bool,
) -> Result<(), String> {
  if is_running(&project) {
    let name = display_name_in(&Paths::real(), &project)?;
    return Err(format!("{name} läuft noch — erst beenden"));
  }
  delete_project_scoped_in(&Paths::real(), &project, &scope, delete_work_dirs)
}

#[tauri::command]
pub(crate) fn project_work_dirs(project: String) -> Result<Vec<String>, String> {
  project_work_dirs_in(&Paths::real(), &project)
}

#[tauri::command]
pub(crate) fn todo_state(project: String) -> Result<bool, String> {
  todo_state_in(&Paths::real(), &project)
}

#[tauri::command]
pub(crate) fn set_todo(project: String, enabled: bool) -> Result<(), String> {
  set_todo_in(&Paths::real(), &project, enabled)
}

#[tauri::command]
pub(crate) fn set_terminal_config(
  project: String,
  theme: Option<String>,
  icon: Option<String>,
  title: Option<String>,
) -> Result<(), String> {
  set_terminal_config_in(
    &Paths::real(),
    &project,
    TerminalConfig { theme, icon, title, ..Default::default() },
  )
}

/// Icon eines Projekts als data-URL für die Übersicht (PNG oder SVG).
#[tauri::command]
pub(crate) fn project_icon(project: String) -> Result<Option<String>, String> {
  use base64::{engine::general_purpose::STANDARD, Engine};
  let paths = Paths::real();
  let Some(icon) = read_project_config_in(&paths, &project)?.terminal.icon else {
    return Ok(None);
  };
  let path = resolve_icon_path(&paths, &project, &icon)?;
  let mime = match path.extension().and_then(|e| e.to_str()) {
    Some(e) if e.eq_ignore_ascii_case("svg") => "image/svg+xml",
    _ => "image/png",
  };
  let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
  Ok(Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes))))
}

// ---------- Pools ----------

#[tauri::command]
pub(crate) fn list_pools() -> Result<Vec<PoolInfo>, String> {
  list_pools_in(&Paths::real(), &KeychainStore)
}

#[tauri::command]
pub(crate) fn create_oauth_pool(name: String) -> Result<String, String> {
  create_oauth_pool_in(&Paths::real(), &name)
}

#[tauri::command]
pub(crate) fn create_apikey_pool(
  name: String,
  key: String,
  allow_file: bool,
) -> Result<String, String> {
  create_apikey_pool_in(&Paths::real(), &KeychainStore, &name, &key, allow_file)
}

#[tauri::command]
pub(crate) fn rename_pool(pool: String, name: String) -> Result<(), String> {
  rename_pool_in(&Paths::real(), &pool, &name)
}

/// Anzeige-Namen der Projekte (für Fehlermeldungen mit Projektliste).
fn display_names(paths: &Paths, ids: &[String]) -> Result<Vec<String>, String> {
  ids.iter().map(|id| display_name_in(paths, id)).collect()
}

#[tauri::command]
pub(crate) fn delete_pool(pool: String) -> Result<(), String> {
  let paths = Paths::real();
  let running = running_projects_using_pool(&paths, &pool)?;
  if !running.is_empty() {
    return Err(format!(
      "Pool wird noch benutzt — läuft: {}",
      display_names(&paths, &running)?.join(", ")
    ));
  }
  delete_pool_in(&paths, &KeychainStore, &pool)
}

#[tauri::command]
pub(crate) fn assign_pool(project: String, pool: String) -> Result<(), String> {
  assign_pool_in(&Paths::real(), &project, &pool)
}

#[tauri::command]
pub(crate) fn unassign_pool(project: String) -> Result<(), String> {
  unassign_pool_in(&Paths::real(), &project)
}

/// Anzeigename eines Pools (pool.json) — die ID ist der Ordnername.
#[tauri::command]
pub(crate) fn pool_label(pool: String) -> Result<String, String> {
  Ok(read_pool(&Paths::real(), &pool)?.name)
}

#[tauri::command]
pub(crate) fn set_apikey(pool: String, key: String, allow_file: bool) -> Result<(), String> {
  set_apikey_in(&Paths::real(), &KeychainStore, &pool, &key, allow_file)
}

#[tauri::command]
pub(crate) fn keychain_status(pool: String) -> Result<bool, String> {
  crate::platform::oauth_keychain_exists(&keychain_service(&Paths::real(), &pool)?)
}

/// Claudes Default-Verzeichnis, sofern vorhanden und noch nicht als Pool
/// eingebunden — die Pool-Anlage bietet es dann als fertigen Pool an.
#[tauri::command]
pub(crate) fn default_config_dir() -> Option<String> {
  offered_default_dir(&Paths::real())
}

/// Pool, der auf ein bestehendes Config-Verzeichnis verweist (statt eines neu
/// angelegten). Der vorhandene Login dort bleibt gültig.
#[tauri::command]
pub(crate) fn create_reference_pool(name: String, dir: String) -> Result<String, String> {
  create_reference_pool_in(&Paths::real(), &name, &dir)
}

/// Setzt einen oauth-Pool zurück: löscht den suffixierten Keychain-Eintrag,
/// damit claude beim nächsten Start des Pools erneut `/login` verlangt. Nur
/// bei ungenutztem Pool. Der Login selbst passiert in der Session, nicht hier.
#[tauri::command]
pub(crate) fn oauth_login(pool: String) -> Result<(), String> {
  let paths = Paths::real();
  ensure_oauth_pool(&paths, &pool)?;
  // Bei einem verwiesenen Pool hinge daran der Login des fremden
  // Verzeichnisses — den löscht die App nicht.
  if let Some(dir) = read_pool(&paths, &pool)?.dir {
    return Err(format!(
      "Neuanmeldung geht bei einem verwiesenen Pool nicht ({dir}) — dort meldet claude selbst an"
    ));
  }

  let running = running_projects_using_pool(&paths, &pool)?;
  if !running.is_empty() {
    return Err(format!(
      "Neuanmeldung nur bei ungenutztem Pool möglich — läuft: {}",
      display_names(&paths, &running)?.join(", ")
    ));
  }

  let service = keychain_service(&paths, &pool)?;
  if crate::platform::oauth_keychain_exists(&service)? {
    crate::platform::oauth_keychain_delete(&service)?;
  }
  Ok(())
}

/// Verlinkt die synced Runtime eines bestehenden Pools. Nur bei idlem Pool —
/// sonst würde das Transkript der laufenden Session ersetzt.
#[tauri::command]
pub(crate) fn link_pool_runtime(pool: String) -> Result<(), String> {
  let paths = Paths::real();
  if !running_projects_using_pool(&paths, &pool)?.is_empty() {
    return Err(format!("{pool} wird benutzt — erst Session beenden"));
  }
  link_pool_runtime_in(&paths, &pool)
}

// ---------- Sessions ----------

#[tauri::command]
pub(crate) fn stop_project(project: String) -> Result<(), String> {
  if crate::platform::terminal_pids(&project).is_empty() {
    let name = display_name_in(&Paths::real(), &project)?;
    return Err(format!("{name} läuft nicht"));
  }
  kill_terminals(&project)
}

/// Beendet die laufenden Terminal-Prozesse, wartet auf ihr Ende und öffnet das
/// interne Terminal neu.
#[tauri::command]
pub(crate) fn restart_project(app: tauri::AppHandle, project: String) -> Result<(), String> {
  kill_terminals(&project)?;
  let deadline = Instant::now() + Duration::from_secs(30);
  while is_running(&project) {
    if Instant::now() > deadline {
      let name = display_name_in(&Paths::real(), &project)?;
      return Err(format!("{name} hat sich nach 30 s nicht beendet"));
    }
    std::thread::sleep(Duration::from_millis(250));
  }
  terminal::open_terminal(app, project)
}

/// Tray-Klick auf ein Projekt: läuft es, kommt das Terminal-Fenster nach vorn,
/// sonst startet es.
fn start_or_focus(app: &tauri::AppHandle, project: &str) {
  match crate::platform::terminal_pids(project).first() {
    Some(pid) => crate::platform::focus_terminal(*pid),
    None => {
      if let Err(e) = terminal::open_terminal(app.clone(), project.to_string()) {
        eprintln!("{project} starten: {e}");
      }
    }
  }
}

/// Tray-/Popup-Klick: Projekt starten oder das laufende Terminal fokussieren.
#[tauri::command]
pub(crate) fn start_or_focus_cmd(app: tauri::AppHandle, project: String) {
  start_or_focus(&app, &project);
}

// ---------- Popup-Fußzeile ----------

/// Popup-Fußzeile „Öffnen": das Hauptfenster zeigen.
#[tauri::command]
pub(crate) fn open_main_window(app: tauri::AppHandle) {
  use tauri::Manager;
  if let Some(w) = app.get_webview_window("main") {
    let _ = w.show();
    let _ = w.set_focus();
  }
}

/// Popup-Fußzeile „Beenden": App verlassen.
#[tauri::command]
pub(crate) fn quit_app(app: tauri::AppHandle) {
  app.exit(0);
}

// ---------- App-Settings ----------

#[tauri::command]
pub(crate) fn sync_setting() -> Result<bool, String> {
  Ok(settings::sync_on_session_end(&Paths::real()))
}

#[tauri::command]
pub(crate) fn set_sync_setting(enabled: bool) -> Result<(), String> {
  settings::set_sync_on_session_end_in(&Paths::real(), enabled)
}

#[tauri::command]
pub(crate) fn terminal_font_size() -> u32 {
  settings::terminal_font_size(&Paths::real())
}

#[tauri::command]
pub(crate) fn set_terminal_font_size(size: u32) -> Result<(), String> {
  settings::set_terminal_font_size_in(&Paths::real(), size)
}

#[tauri::command]
pub(crate) fn spellcheck_lang() -> String {
  settings::spellcheck_lang(&Paths::real())
}

// ---------- Verbrauch ----------

#[tauri::command]
pub(crate) fn usage_stats(days: u32) -> Result<Vec<UsageRow>, String> {
  usage_stats_in(&Paths::real(), days)
}

// ---------- Panel-Archiv ----------

/// Konfiguriertes Archiv-Home des Projekts (für den Panel-Button: entscheidet,
/// ob der Ordner-Dialog nötig ist).
#[tauri::command]
pub(crate) fn panel_archive_dir_cmd(project: String) -> Option<String> {
  project_archive_home(&project).map(|p| p.display().to_string())
}

/// Setzt das Archiv-Home eines Projekts (Einstellungsdialog) — inklusive
/// Permissions-Eintrag in der Projekt-settings.json.
#[tauri::command]
pub(crate) fn set_archive_home_cmd(project: String, dir: String) -> Result<(), String> {
  crate::domain::archive::set_project_archive_home(&project, &dir)
}

/// Wechselt das Archiv-Home (Einstellungsdialog, wenn schon eins gesetzt
/// ist): optional ziehen die Dokumente mit um; die Rechte des alten Ordners
/// werden zurückgenommen.
#[tauri::command]
pub(crate) fn change_archive_home_cmd(
  project: String,
  dir: String,
  migrate: bool,
) -> Result<(), String> {
  crate::domain::archive::change_project_archive_home(&project, &dir, migrate)
}

/// Wählt das Archiv ab (Einstellungsdialog): Config-Eintrag und Permissions
/// weg, der Ordner bleibt liegen.
#[tauri::command]
pub(crate) fn clear_archive_home_cmd(project: String) -> Result<(), String> {
  crate::domain::archive::clear_project_archive_home(&project)
}

/// Archiviert den aktuellen Panel-Entwurf. `dir` (aus dem Ordner-Dialog) setzt
/// das Archiv-Home zugleich; ohne `dir` muss es konfiguriert sein. Ordner,
/// Beschreibung und Schlagwörter kommen aus dem Archiv-Formular im Panel.
#[tauri::command]
pub(crate) fn panel_archive_cmd(
  project: String,
  dir: Option<String>,
  folder: Option<String>,
  description: Option<String>,
  tags: Option<Vec<String>>,
) -> Result<String, String> {
  let meta = ArchiveMeta {
    folder: folder.filter(|f| !f.trim().is_empty()),
    description: description.filter(|d| !d.trim().is_empty()),
    tags: tags.unwrap_or_default(),
  };
  archive_panel_content(&project, dir.as_deref(), &meta).map(|p| p.display().to_string())
}

/// Dokumentliste des Archiv-Index (frisch gescannt) fürs Panel.
#[tauri::command]
pub(crate) fn archive_docs_cmd(
  project: String,
) -> Result<Vec<crate::domain::archive_index::Doc>, String> {
  let home = require_archive_home(&project)?;
  crate::domain::archive_index::scan_archive(&home)
}

/// Zeigt einen Pfad im Dateimanager (nach dem Archivieren „im Finder zeigen").
#[tauri::command]
pub(crate) fn reveal_path_cmd(path: String) {
  crate::platform::reveal_path(std::path::Path::new(&path));
}
