//! Projekte: Anlage (Wizard), Registry-Aufnahme, Arbeitsordner, Pool-Zuordnung,
//! Terminal-Einstellungen, Löschen. Der Projektordner trägt .claude/settings.json
//! und .ai-central/ (config.json + Icon — synct mit dem Projekt); die
//! Pool-Zuordnung ist maschinenlokal und liegt in der Registry.
//!
//! Identität: Jedes Projekt hat eine UUID (`id` in der config.json). Sie ist
//! der Registry-Schlüssel und der `project`-Parameter aller Commands; der
//! Anzeigename (`name`) ist reine Darstellung. Registry-Eintrag und Ordner
//! sind damit gegeneinander prüfbar (`verify_project_dir_in`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::domain::check_name;
use crate::domain::paths::{contract_home, expand_home, Paths};
use crate::domain::pool::POOL_FILE;
use crate::domain::registry::{
  load_registry, project_dir, project_pool, register_project, save_registry, set_project_pool,
  RegEntry,
};

/// Projekt-eigener Config-Ordner im Projekt-Root: config.json + Icon.
pub(crate) const PROJECT_CONFIG_DIR: &str = ".ai-central";

/// Projekt-Config im Config-Ordner (Terminal-Einstellungen + Archiv-Home).
pub(crate) const PROJECT_FILE: &str = "config.json";

/// Alt-Layout: einzelne Datei im Projekt-Root; wird beim App-Start migriert.
const LEGACY_PROJECT_FILE: &str = "ai-central.json";

#[derive(Serialize)]
pub(crate) struct Project {
  pub(crate) id: String,
  pub(crate) name: String,
  pub(crate) path: String,
  pub(crate) pool: Option<String>,
  pub(crate) running: bool,
  pub(crate) terminal: TerminalConfig,
  /// Warum die Config dieses Projekts nicht lesbar ist (kaputtes JSON,
  /// stehengebliebene Merge-Marker, fehlende Rechte). Das Projekt bleibt in
  /// der Liste und trägt seinen Fehler sichtbar — die übrigen laufen weiter.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) error: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct ProjectConfig {
  /// Projekt-UUID — Registry-Schlüssel; synct mit dem Projekt und macht den
  /// Registry-Eintrag gegen den Ordner prüfbar.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub(crate) id: Option<String>,
  /// Anzeigename (Projektliste, Fenstertitel-Fallback, .desktop-Name).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub(crate) name: Option<String>,
  #[serde(default, skip_serializing_if = "TerminalConfig::is_empty")]
  pub(crate) terminal: TerminalConfig,
  /// Archiv-Home des Projekts: Zielordner fürs Archivieren von Panel-Entwürfen
  /// (~-relativ gespeichert). Liest den früheren Key `archiveDir` weiterhin ein,
  /// geschrieben wird nur noch `archiveHome`.
  #[serde(
    default,
    rename = "archiveHome",
    alias = "archiveDir",
    skip_serializing_if = "Option::is_none"
  )]
  pub(crate) archive_home: Option<String>,
  /// Modul-Abweichungen vom Default (`"commands": false`); fehlender Key =
  /// Default aus der Registry (domain/modules.rs). Kern-Module ignorieren
  /// den Eintrag.
  #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pub(crate) modules: std::collections::BTreeMap<String, bool>,
  /// Alle Keys, die dieser Build nicht kennt — unverändert durchgereicht.
  /// Ohne das verliert jeder read-modify-write (Pool-Zuweisung, Terminal-
  /// Einstellungen) still die Felder neuerer Versionen: serde verwirft
  /// Unbekanntes beim Deserialisieren, und geschrieben wird die ganze Datei.
  #[serde(flatten)]
  pub(crate) rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct TerminalConfig {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub theme: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub icon: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  /// Unbekannte Keys, wie bei `ProjectConfig` — der verlustbehaftete
  /// Round-Trip gilt hier genauso, weil dieser Block mitgeschrieben wird.
  #[serde(flatten)]
  pub rest: serde_json::Map<String, serde_json::Value>,
}

impl TerminalConfig {
  pub(crate) fn is_empty(&self) -> bool {
    self.theme.is_none() && self.icon.is_none() && self.title.is_none() && self.rest.is_empty()
  }
}

pub(crate) fn settings_path(dir: &std::path::Path) -> PathBuf {
  dir.join(".claude").join("settings.json")
}

pub(crate) fn read_project_config_in(
  paths: &Paths,
  project: &str,
) -> Result<ProjectConfig, String> {
  read_config_at(&project_dir(paths, project)?)
}

/// config.json unter `<dir>/.ai-central` lesen; ohne Datei der Default.
/// Für Fälle, in denen der Ordner vor der Registrierung steht (Import,
/// Migration) — sonst über `read_project_config_in`.
pub(crate) fn read_config_at(dir: &std::path::Path) -> Result<ProjectConfig, String> {
  let cfg_path = dir.join(PROJECT_CONFIG_DIR).join(PROJECT_FILE);
  if !cfg_path.is_file() {
    return Ok(ProjectConfig::default());
  }
  let raw =
    fs::read_to_string(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
  serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg_path.display()))
}

pub(crate) fn write_project_config_in(
  paths: &Paths,
  project: &str,
  cfg: &ProjectConfig,
) -> Result<(), String> {
  let raw = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
  let dir = project_dir(paths, project)?.join(PROJECT_CONFIG_DIR);
  fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  let path = dir.join(PROJECT_FILE);
  crate::domain::write_atomic(&path, &(raw + "\n"))
}

pub(crate) fn is_running(project: &str) -> bool {
  !crate::platform::terminal_pids(project).is_empty()
}

/// Beendet die Terminal-Prozesse eines Projekts per SIGTERM auf die exakte PID.
pub(crate) fn kill_terminals(project: &str) -> Result<(), String> {
  for pid in crate::platform::terminal_pids(project) {
    crate::platform::kill_terminal(pid)?;
  }
  Ok(())
}

/// Projekte, die diesem Pool zugeordnet sind und gerade laufen — dieselbe
/// Lauf-Erkennung wie die Projektliste (`is_running`).
pub(crate) fn running_projects_using_pool(
  paths: &Paths,
  pool: &str,
) -> Result<Vec<String>, String> {
  Ok(projects_using_pool(paths, pool)?
    .into_iter()
    .filter(|p| is_running(p))
    .collect())
}

/// Namen aller Projekte, denen die Registry `pool` zuordnet.
pub(crate) fn projects_using_pool(paths: &Paths, pool: &str) -> Result<Vec<String>, String> {
  Ok(
    load_registry(paths)?
      .into_iter()
      .filter(|(_, e)| e.pool.as_deref() == Some(pool))
      .map(|(name, _)| name)
      .collect(),
  )
}

pub(crate) fn list_projects_in(paths: &Paths) -> Result<Vec<Project>, String> {
  let mut projects = Vec::new();
  for (id, entry) in load_registry(paths)? {
    // Ein Projekt mit kaputter Config verschwindet nicht und nimmt auch nicht
    // die übrigen mit: Es steht mit seiner ID in der Liste und trägt den
    // Fehler. Sichtbar bleiben muss es, damit der Ordner überhaupt auffindbar
    // ist — nur von dort lässt sich der Schaden beheben.
    let (cfg, error) = match read_project_config_in(paths, &id) {
      Ok(cfg) => (cfg, None),
      Err(e) => (ProjectConfig::default(), Some(e)),
    };
    projects.push(Project {
      running: is_running(&id),
      path: contract_home(paths, &entry.dir),
      pool: entry.pool,
      name: cfg.name.unwrap_or_else(|| id.clone()),
      terminal: cfg.terminal,
      error,
      id,
    });
  }
  Ok(projects)
}

/// Anzeigename eines Projekts; Fallback ist die ID (Projekte vor der Migration).
pub(crate) fn display_name_in(paths: &Paths, project: &str) -> Result<String, String> {
  Ok(
    read_project_config_in(paths, project)?
      .name
      .unwrap_or_else(|| project.to_string()),
  )
}

/// Prüft Registry-Eintrag gegen den Ordner: die config.json unter dem
/// registrierten Pfad muss die Projekt-ID tragen. Liefert den Ordner.
pub(crate) fn verify_project_dir_in(paths: &Paths, project: &str) -> Result<PathBuf, String> {
  let dir = project_dir(paths, project)?;
  let cfg = read_project_config_in(paths, project)?;
  if cfg.id.as_deref() != Some(project) {
    return Err(format!(
      "Projektordner passt nicht zur Registry: {} trägt die ID {}, registriert ist {project}",
      dir.display(),
      cfg.id.as_deref().unwrap_or("(keine)"),
    ));
  }
  Ok(dir)
}

/// Fehlerkennung „Projekt hat keinen Pool". Der Tray-/Popup-Start fängt genau
/// diese ab und öffnet die Pool-Auswahl. Eine Kennung statt Klartext, damit die
/// Unterscheidung nicht an einer übersetzbaren Meldung hängt.
pub(crate) const NO_POOL: &str = "no-pool";

/// Pool-Config-Verzeichnis eines Projekts — wird dem Terminal als
/// CLAUDE_CONFIG_DIR mitgegeben.
///
/// Ohne zugewiesenen Pool ist das ein Fehler, kein `Ok(None)`: Sonst startete
/// die Session ohne CLAUDE_CONFIG_DIR und übernähme damit stillschweigend
/// claudes Default-Verzeichnis samt dessen Login — ein Verzeichnis, das der
/// Nutzer nie gewählt hat. `Ok(None)` bleibt allein dem Referenzpool auf
/// `~/.claude` vorbehalten: dort ist der Default die ausdrückliche Wahl.
///
/// Der Pool-Name wird geprüft, obwohl `assign_pool_in` das beim Zuweisen schon
/// tut: Die Migration übernimmt Pool-Einträge aus der alten, versionierten
/// ai-central.json eines geklonten Repos. Ungeprüft bestimmte ein `../…`
/// daraus ein beliebiges Verzeichnis als CLAUDE_CONFIG_DIR — und dessen
/// `settings.json` trägt `apiKeyHelper`, ein Kommando, das beim Sessionstart
/// ausgeführt wird.
pub(crate) fn project_pool_dir(project: &str) -> Result<Option<PathBuf>, String> {
  project_pool_dir_in(&Paths::real(), project)
}

pub(crate) fn project_pool_dir_in(
  paths: &Paths,
  project: &str,
) -> Result<Option<PathBuf>, String> {
  let Some(pool) = project_pool(paths, project)? else {
    return Err(NO_POOL.to_string());
  };
  check_name(&pool).map_err(|_| format!("ungültiger Pool in der Registry: {pool}"))?;
  let dir = crate::domain::pool::pool_config_dir(paths, &pool)?;
  // Zeigt der Pool auf claudes Default-Verzeichnis, bleibt CLAUDE_CONFIG_DIR
  // ungesetzt: claude nimmt dann von sich aus dieses Verzeichnis — und den
  // unsuffixierten Keychain-Eintrag, an dem ein bestehender Login hängt.
  if dir == paths.default_claude_dir() {
    return Ok(None);
  }
  Ok(Some(dir))
}

/// Projekt-Config für den Terminal-Prozess (Terminal-Einstellungen + Name).
pub(crate) fn project_config(project: &str) -> Result<ProjectConfig, String> {
  read_project_config_in(&Paths::real(), project)
}

/// Legt ein Projekt nach dem Muster der bestehenden an: memory/, .gitignore
/// (Sentinel), .claude/settings.json (autoMemoryDirectory, Permissions,
/// Berechtigungen), Registry-Eintrag mit Pool, .ai-central/config.json mit
/// ID, Name und Terminal-Config. Ohne Zielordner landet das Projekt unter
/// ~/claude-projects/<name>. Liefert die neue Projekt-ID.
pub(crate) fn create_project_full_in(
  paths: &Paths,
  name: &str,
  dir: Option<&str>,
  pool: Option<&str>,
  work_dir: Option<&str>,
  create_work_dir: bool,
  terminal: TerminalConfig,
) -> Result<String, String> {
  check_name(name)?;
  let dir = match dir {
    Some(d) => expand_home(paths, d),
    None => paths.projects_dir().join(name),
  };
  let mut reg = load_registry(paths)?;
  if list_projects_in(paths)?.iter().any(|p| p.name == name) {
    return Err(format!("Projekt existiert bereits: {name}"));
  }
  if dir.exists() {
    return Err(format!("Ordner existiert bereits: {}", dir.display()));
  }
  if let Some(pool) = pool {
    if !paths.pool_dir(pool).join(POOL_FILE).is_file() {
      return Err(format!("Pool existiert nicht: {pool}"));
    }
  }
  // Arbeitsverzeichnis zuerst prüfen/anlegen — scheitert das, entsteht kein
  // halbes Projekt.
  if let Some(wd) = work_dir {
    let wd_path = expand_home(paths, wd);
    if create_work_dir {
      fs::create_dir_all(&wd_path).map_err(|e| format!("{}: {e}", wd_path.display()))?;
    } else if !wd_path.is_dir() {
      return Err(format!("Arbeitsverzeichnis fehlt: {}", wd_path.display()));
    }
  }

  fs::create_dir_all(dir.join(".claude"))
    .map_err(|e| format!("{}: {e}", dir.join(".claude").display()))?;
  fs::create_dir_all(dir.join("memory"))
    .map_err(|e| format!("{}: {e}", dir.join("memory").display()))?;
  fs::write(dir.join(".gitignore"), ".ai-central-running\n")
    .map_err(|e| format!("{}: {e}", dir.join(".gitignore").display()))?;

  let contracted = contract_home(paths, &dir);
  let mut allow = vec![format!("Edit({contracted}/**)")];
  let mut additional: Vec<String> = Vec::new();
  if let Some(wd) = work_dir {
    allow.insert(0, format!("Edit({wd}/**)"));
    additional.push(wd.to_string());
  }
  let settings = serde_json::json!({
    "autoMemoryDirectory": format!("{contracted}/memory"),
    "permissions": {
      "allow": allow,
      "additionalDirectories": additional,
    },
  });
  let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&dir.join(".claude").join("settings.json"), &(raw + "\n"))?;

  let id = uuid::Uuid::new_v4().to_string();
  reg.insert(id.clone(), RegEntry { dir: dir.clone(), pool: pool.map(str::to_string) });
  save_registry(paths, &reg)?;
  let cfg = ProjectConfig {
    id: Some(id.clone()),
    name: Some(name.to_string()),
    terminal,
    archive_home: None,
    modules: Default::default(),
    rest: Default::default(),
  };
  write_project_config_in(paths, &id, &cfg)?;
  crate::platform::write_terminal_desktop(paths, &id, &cfg);
  Ok(id)
}

/// Verlegt den Projektordner: Registry-Eintrag auf den neuen Pfad; Verweise
/// auf den alten Root in der settings.json (autoMemoryDirectory,
/// Edit-Permission) ziehen mit. Verschoben wird nichts — der neue Ordner
/// muss existieren.
pub(crate) fn set_project_dir_in(paths: &Paths, name: &str, dir: &str) -> Result<(), String> {
  check_name(name)?;
  let new_dir = expand_home(paths, dir);
  if !new_dir.is_dir() {
    return Err(format!("Ordner nicht gefunden: {}", new_dir.display()));
  }
  let mut reg = load_registry(paths)?;
  let entry = reg
    .get_mut(name)
    .ok_or_else(|| format!("Projekt nicht registriert: {name}"))?;
  let old_dir = entry.dir.clone();
  if old_dir == new_dir {
    return Ok(());
  }
  entry.dir = new_dir.clone();
  save_registry(paths, &reg)?;

  let sp = settings_path(&new_dir);
  if !sp.is_file() {
    return Ok(()); // importierte Projekte ohne settings.json
  }
  let old_c = contract_home(paths, &old_dir);
  let new_c = contract_home(paths, &new_dir);
  let raw = fs::read_to_string(&sp).map_err(|e| format!("{}: {e}", sp.display()))?;
  let mut v: serde_json::Value =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?;
  if let Some(mem) = v["autoMemoryDirectory"].as_str() {
    if let Some(rest) = mem.strip_prefix(&old_c) {
      v["autoMemoryDirectory"] = serde_json::json!(format!("{new_c}{rest}"));
    }
  }
  if let Some(allow) = v["permissions"]["allow"].as_array_mut() {
    let old_edit = format!("Edit({old_c}/**)");
    for e in allow.iter_mut() {
      if e.as_str() == Some(&old_edit) {
        *e = serde_json::json!(format!("Edit({new_c}/**)"));
      }
    }
  }
  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&sp, &(raw + "\n"))
}

/// Nimmt einen bestehenden Ordner als Projekt auf. Eine mitgebrachte
/// .ai-central/config.json liefert ID und Name (Sync-Fall: dasselbe Projekt
/// behält seine ID auf jeder Maschine); ohne Config entsteht eine neue ID mit
/// dem Ordnernamen als Name. Mitgebrachte Pfade werden geprüft und angelegt:
/// Arbeitsordner aus der settings.json sowie das Archiv-Home samt
/// Permissions-Eintrag.
pub(crate) fn add_project_in(paths: &Paths, path: &str) -> Result<(), String> {
  let dir = expand_home(paths, path);
  if !dir.is_dir() {
    return Err(format!("Ordner nicht gefunden: {}", dir.display()));
  }
  let dirname = dir
    .file_name()
    .ok_or_else(|| format!("kein Ordnername: {}", dir.display()))?
    .to_string_lossy()
    .into_owned();
  check_name(&dirname)?;

  let mut cfg = read_config_at(&dir)?;
  if let Some(id) = cfg.id.as_deref() {
    if load_registry(paths)?.contains_key(id) {
      return Err(format!("Projekt ist schon registriert: {id}"));
    }
  }
  let id = cfg.id.get_or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone();
  cfg.name.get_or_insert(dirname);
  register_project(paths, &id, &dir)?;
  write_project_config_in(paths, &id, &cfg)?;

  for wd in project_work_dirs_in(paths, &id)? {
    let wd_path = expand_home(paths, &wd);
    fs::create_dir_all(&wd_path).map_err(|e| format!("{}: {e}", wd_path.display()))?;
  }
  if let Some(archive) = cfg.archive_home.as_deref() {
    let expanded = expand_home(paths, archive);
    fs::create_dir_all(&expanded).map_err(|e| format!("{}: {e}", expanded.display()))?;
    crate::domain::archive::add_archive_permission(paths, &id, archive)?;
  }
  crate::platform::write_terminal_desktop(paths, &id, &cfg);
  Ok(())
}

/// Trägt einen Arbeitsordner in die Projekt-settings.json ein — wie im
/// Wizard: permissions.additionalDirectories + Edit-Permission. Legt die
/// Datei an, wenn sie fehlt (importierte Projekte).
pub(crate) fn add_work_dir_in(paths: &Paths, name: &str, dir: &str) -> Result<(), String> {
  check_name(name)?;
  let wd_path = expand_home(paths, dir);
  if !wd_path.is_dir() {
    return Err(format!("Arbeitsverzeichnis fehlt: {}", wd_path.display()));
  }
  let dir = contract_home(paths, &wd_path);
  let sp = settings_path(&project_dir(paths, name)?);
  crate::domain::update_settings_permissions(&sp, true, |perms| {
    let dirs = crate::domain::perm_array(perms, "additionalDirectories")?;
    if dirs.iter().any(|d| d.as_str() == Some(&dir)) {
      return Err(format!("schon eingetragen: {dir}"));
    }
    dirs.push(serde_json::json!(dir));
    let allow = crate::domain::perm_array(perms, "allow")?;
    allow.insert(0, serde_json::json!(format!("Edit({dir}/**)")));
    Ok(())
  })
}

/// Nimmt einen Arbeitsordner wieder raus: additionalDirectories-Eintrag und
/// zugehörige Edit-Permission. Der Ordner selbst bleibt.
pub(crate) fn remove_work_dir_in(paths: &Paths, name: &str, dir: &str) -> Result<(), String> {
  check_name(name)?;
  let sp = settings_path(&project_dir(paths, name)?);
  crate::domain::update_settings_permissions(&sp, false, |perms| {
    let dirs = crate::domain::perm_array(perms, "additionalDirectories")?;
    let before = dirs.len();
    dirs.retain(|d| d.as_str() != Some(dir));
    if dirs.len() == before {
      return Err(format!("nicht eingetragen: {dir}"));
    }
    if let Some(allow) = perms.get_mut("allow").and_then(|a| a.as_array_mut()) {
      allow.retain(|p| p.as_str() != Some(&format!("Edit({dir}/**)")));
    }
    Ok(())
  })
}

/// Arbeitsordner des Projekts: additionalDirectories aus der Projekt-settings.json.
pub(crate) fn project_work_dirs_in(paths: &Paths, name: &str) -> Result<Vec<String>, String> {
  check_name(name)?;
  let sp = settings_path(&project_dir(paths, name)?);
  if !sp.is_file() {
    return Ok(Vec::new());
  }
  let raw = fs::read_to_string(&sp).map_err(|e| format!("{}: {e}", sp.display()))?;
  let v: serde_json::Value =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?;
  Ok(
    v["permissions"]["additionalDirectories"]
      .as_array()
      .map(|a| {
        a.iter()
          .filter_map(|x| x.as_str().map(String::from))
          .collect()
      })
      .unwrap_or_default(),
  )
}

/// Vorschau für den Lösch-Dialog: welche Artefakte die drei Stufen
/// (Integration / Integration+Archiv / komplett) konkret treffen würden.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeletePreview {
  pub(crate) name: String,
  pub(crate) project_dir: String,
  pub(crate) ai_central_dir: bool,
  pub(crate) archive_permission: bool,
  pub(crate) panel_files: usize,
  pub(crate) archive_home: Option<String>,
  pub(crate) archive_docs: usize,
  pub(crate) work_dirs: Vec<String>,
}

pub(crate) fn delete_preview_in(paths: &Paths, project: &str) -> Result<DeletePreview, String> {
  check_name(project)?;
  let dir = project_dir(paths, project)?;
  let cfg = read_project_config_in(paths, project)?;
  let archive_docs = match cfg.archive_home.as_deref() {
    Some(a) => {
      let home = expand_home(paths, a);
      if home.is_dir() {
        crate::domain::archive_index::scan_archive(&home)?.len()
      } else {
        0
      }
    }
    None => 0,
  };
  Ok(DeletePreview {
    name: cfg.name.clone().unwrap_or_else(|| project.to_string()),
    project_dir: contract_home(paths, &dir),
    ai_central_dir: dir.join(PROJECT_CONFIG_DIR).is_dir(),
    archive_permission: cfg.archive_home.is_some(),
    panel_files: session_files(paths, project).iter().filter(|f| f.is_file()).count(),
    archive_home: cfg.archive_home.clone(),
    archive_docs,
    work_dirs: project_work_dirs_in(paths, project)?,
  })
}

/// Flüchtige Panel-Kanaldateien des Projekts unter ~/.config/ai-central/panels.
/// Panel-Kanaldateien des Projekts — aus der Modul-Registry, damit neue
/// Puffer automatisch in Löschvorschau und Löschung landen.
fn session_files(paths: &Paths, project: &str) -> Vec<PathBuf> {
  let panels = paths.config_dir().join("panels");
  crate::domain::modules::MODULES
    .iter()
    .flat_map(|m| m.buffers)
    // Puffer im Projekt sind keine Panel-Kanäle: sie liegen im Punktordner
    // und fallen mit ihm, statt einzeln gelöscht zu werden.
    .filter(|b| !b.in_project)
    .map(|b| panels.join(format!("{project}.{}", b.suffix)))
    .collect()
}

/// Holt eine ToDo-Liste vom alten Ablageort (maschinenlokal unter panels/) in
/// den Projekt-Punktordner. Bis 0.4.0 lag sie neben den flüchtigen Kanälen und
/// fehlte damit auf jedem anderen Rechner; ab jetzt reist sie mit dem Repo.
///
/// Läuft beim Session-Start, einmal je Projekt: liegt am Ziel schon eine
/// Liste, bleibt sie — die alte ist dann der ältere Stand und wird nicht
/// darübergeschrieben.
pub(crate) fn migrate_todos_into_project(paths: &Paths, project: &str) -> Result<(), String> {
  let alt = crate::domain::paths::legacy_todos_file(paths, project);
  if !alt.is_file() {
    return Ok(());
  }
  let neu = crate::domain::paths::todos_file_in(paths, project);
  if neu.is_file() {
    return Ok(());
  }
  if let Some(parent) = neu.parent() {
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
  }
  fs::rename(&alt, &neu).map_err(|e| format!("{} -> {}: {e}", alt.display(), neu.display()))
}

/// Löscht ein Projekt in drei Stufen (Eskalationsleiter, jede schließt die
/// vorige ein):
/// - "integration": nur die ai-central-Spuren — Registry, .ai-central/,
///   Archiv-Rechte in der settings.json, Panel-Dateien, .desktop. Ordner,
///   memory/ und das Claude-Code-Grundgerüst der settings.json bleiben.
/// - "archive": zusätzlich der Archiv-Ordner samt Dokumenten.
/// - "full": zusätzlich der Projektordner; Arbeitsordner per Flag.
pub(crate) fn delete_project_scoped_in(
  paths: &Paths,
  project: &str,
  scope: &str,
  delete_work_dirs: bool,
) -> Result<(), String> {
  check_name(project)?;
  let dir = project_dir(paths, project)?;
  if !dir.is_dir() {
    return Err(format!("Projektordner nicht gefunden: {}", dir.display()));
  }
  let cfg = read_project_config_in(paths, project)?;

  // Archiv vor der Integration räumen — Stufe 1 löscht die Config, die den
  // Pfad kennt.
  if scope == "archive" || scope == "full" {
    if let Some(a) = cfg.archive_home.as_deref() {
      let home = expand_home(paths, a);
      if home.is_dir() {
        fs::remove_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
      }
    }
  }

  match scope {
    "integration" | "archive" => {
      if let Some(a) = cfg.archive_home.as_deref() {
        crate::domain::archive::remove_archive_permission(paths, project, a)?;
      }
      let ac = dir.join(PROJECT_CONFIG_DIR);
      if ac.is_dir() {
        fs::remove_dir_all(&ac).map_err(|e| format!("{}: {e}", ac.display()))?;
      }
    }
    "full" => {
      if delete_work_dirs {
        for wd in project_work_dirs_in(paths, project)? {
          let wd_path = expand_home(paths, &wd);
          fs::remove_dir_all(&wd_path).map_err(|e| format!("{}: {e}", wd_path.display()))?;
        }
      }
      fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    other => return Err(format!("unbekannter Lösch-Umfang: {other}")),
  }

  for f in session_files(paths, project) {
    if f.is_file() {
      fs::remove_file(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    }
  }
  crate::domain::registry::unregister_project(paths, project)?;
  crate::platform::remove_terminal_desktop(paths, project);
  Ok(())
}

pub(crate) fn assign_pool_in(paths: &Paths, project: &str, pool: &str) -> Result<(), String> {
  if !paths.pool_dir(pool).join(POOL_FILE).is_file() {
    return Err(format!("Pool existiert nicht: {pool}"));
  }
  set_project_pool(paths, project, Some(pool))
}

/// Nimmt die Pool-Zuordnung aus der Registry.
pub(crate) fn unassign_pool_in(paths: &Paths, project: &str) -> Result<(), String> {
  if project_pool(paths, project)?.is_none() {
    return Err(format!("kein Pool zugeordnet: {project}"));
  }
  set_project_pool(paths, project, None)
}

pub(crate) fn set_terminal_config_in(
  paths: &Paths,
  project: &str,
  mut terminal: TerminalConfig,
) -> Result<(), String> {
  let mut cfg = read_project_config_in(paths, project)?;
  // Absolut gewählte Icons in den Projekt-Config-Ordner kopieren und als
  // Dateiname speichern — das Icon synct damit mit dem Projekt.
  if let Some(icon) = terminal.icon.as_deref() {
    if icon.starts_with('/') {
      let cfg_dir = project_dir(paths, project)?.join(PROJECT_CONFIG_DIR);
      terminal.icon = Some(adopt_icon(std::path::Path::new(icon), &cfg_dir)?);
    }
  }
  // Die Terminal-Config kommt aus der Oberfläche und kennt nur die drei Felder;
  // unbekannte Keys der bestehenden Datei würden sonst hier verloren gehen.
  terminal.rest = std::mem::take(&mut cfg.terminal.rest);
  cfg.terminal = terminal;
  write_project_config_in(paths, project, &cfg)?;
  crate::platform::write_terminal_desktop(paths, project, &cfg);
  Ok(())
}

/// Icon-Datei als `icon.<ext>` (kleingeschrieben, Default png) in den
/// Projekt-Config-Ordner kopieren; liefert den gespeicherten Dateinamen.
fn adopt_icon(src: &std::path::Path, cfg_dir: &std::path::Path) -> Result<String, String> {
  let ext = src
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or("png")
    .to_lowercase();
  let name = format!("icon.{ext}");
  fs::create_dir_all(cfg_dir).map_err(|e| format!("{}: {e}", cfg_dir.display()))?;
  let dest = cfg_dir.join(&name);
  if src != dest {
    fs::copy(src, &dest).map_err(|e| format!("{}: {e}", src.display()))?;
  }
  Ok(name)
}

/// Icon-Pfad einer Projekt-Config auflösen: relative Namen liegen im
/// Projekt-Config-Ordner (.ai-central) des Projekts.
pub(crate) fn resolve_icon_path(
  paths: &Paths,
  project: &str,
  icon: &str,
) -> Result<PathBuf, String> {
  if icon.starts_with('/') {
    Ok(PathBuf::from(icon))
  } else {
    Ok(project_dir(paths, project)?.join(PROJECT_CONFIG_DIR).join(icon))
  }
}

/// Die Ordner, die einem Projekt gehören: sein Projektordner und die
/// Arbeitsverzeichnisse aus `permissions.additionalDirectories` (mit `~`
/// aufgelöst). Eine Quelle für alle, die diese Grenze ziehen — der
/// Panel-Lesezugriff ebenso wie der Commit-Dialog.
pub(crate) fn project_roots_in(paths: &Paths, project: &str) -> Result<Vec<PathBuf>, String> {
  let mut roots = vec![project_dir(paths, project)?];
  for d in project_work_dirs_in(paths, project)? {
    roots.push(expand_home(paths, &d));
  }
  Ok(roots)
}

/// Liest eine Datei für `write_panel(path)` — den promptfreien MCP-Weg.
/// Erlaubt ist genau, was die Session über ihre Permissions ohnehin promptfrei
/// liest: Projektordner, Arbeitsordner und Archiv-Home. Geprüft wird nach
/// `canonicalize` (symlink-fest), nur reguläre Dateien bis 2 MB — sonst
/// umginge das Tool das Read-Permission-Modell von Claude Code, und ein
/// Device wie /dev/zero hinge den Prozess.
pub(crate) fn read_for_panel_in(
  paths: &Paths,
  project: &str,
  src: &str,
) -> Result<String, String> {
  const MAX: u64 = 2 * 1024 * 1024;
  let canon = fs::canonicalize(expand_home(paths, src)).map_err(|e| format!("{src}: {e}"))?;
  let mut roots = project_roots_in(paths, project)?;
  if let Some(a) = read_project_config_in(paths, project)?.archive_home {
    roots.push(expand_home(paths, &a));
  }
  let allowed = roots
    .iter()
    .filter_map(|r| fs::canonicalize(r).ok())
    .any(|r| canon.starts_with(&r));
  if !allowed {
    return Err(format!(
      "{src}: liegt außerhalb von Projekt-, Arbeits- und Archiv-Ordnern"
    ));
  }
  let f = fs::File::open(&canon).map_err(|e| format!("{src}: {e}"))?;
  let meta = f.metadata().map_err(|e| format!("{src}: {e}"))?;
  if !meta.is_file() {
    return Err(format!("{src}: keine reguläre Datei"));
  }
  if meta.len() > MAX {
    return Err(format!("{src}: größer als 2 MB"));
  }
  let mut text = String::new();
  std::io::Read::read_to_string(&mut std::io::Read::take(f, MAX), &mut text)
    .map_err(|e| format!("{src}: {e}"))?;
  Ok(text)
}

/// Migration beim App-Start (idempotent): ai-central.json aus dem Projekt-Root
/// nach .ai-central/config.json, die Pool-Zuordnung daraus in die Registry,
/// Projekt-Icons aus dem zentralen Icons-Verzeichnis in den Projekt-Config-Ordner.
/// Namens-Schlüssel der Registry werden auf die Projekt-UUID umgestellt; fehlt
/// die in der config.json, entsteht sie hier (Name = bisheriger Schlüssel).
pub(crate) fn migrate_layout_in(paths: &Paths) -> Result<(), String> {
  let reg = load_registry(paths)?;
  let mut new_reg: BTreeMap<String, RegEntry> = BTreeMap::new();
  let mut reg_dirty = false;
  for (key, mut entry) in reg {
    let legacy = entry.dir.join(LEGACY_PROJECT_FILE);
    if legacy.is_file() {
      let raw = fs::read_to_string(&legacy).map_err(|e| format!("{}: {e}", legacy.display()))?;
      let mut v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", legacy.display()))?;
      let obj = v.as_object_mut().ok_or_else(|| format!("{}: kein Objekt", legacy.display()))?;
      if let Some(pool) = obj.remove("pool").and_then(|p| p.as_str().map(str::to_string)) {
        if entry.pool.is_none() {
          entry.pool = Some(pool);
          reg_dirty = true;
        }
      }
      let cfg_dir = entry.dir.join(PROJECT_CONFIG_DIR);
      let cfg_path = cfg_dir.join(PROJECT_FILE);
      if !cfg_path.exists() && !obj.is_empty() {
        fs::create_dir_all(&cfg_dir).map_err(|e| format!("{}: {e}", cfg_dir.display()))?;
        let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        crate::domain::write_atomic(&cfg_path, &(raw + "\n"))?;
      }
      fs::remove_file(&legacy).map_err(|e| format!("{}: {e}", legacy.display()))?;
    }

    // Config lesen; ID und Name sicherstellen. Der bisherige Registry-Schlüssel
    // war der Anzeigename.
    let cfg_dir = entry.dir.join(PROJECT_CONFIG_DIR);
    let cfg_path = cfg_dir.join(PROJECT_FILE);
    // Der Zustand eines einzelnen Projektordners darf den Start nicht
    // aufhalten: Die Migration läuft im Tauri-setup, ein Fehler hier ließe die
    // App ohne Fenster und ohne Meldung enden. Ein Projekt mit unlesbarer
    // Config bleibt unverändert in der Registry und wird übersprungen; sichtbar
    // wird es in der Projektliste (list_projects_in).
    let mut cfg = match read_config_at(&entry.dir) {
      Ok(cfg) => cfg,
      Err(e) => {
        log::warn!("Projekt {key} übersprungen: {e}");
        new_reg.insert(key, entry);
        continue;
      }
    };
    let mut cfg_dirty = cfg.id.is_none() || cfg.name.is_none();
    let id = cfg.id.get_or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone();
    cfg.name.get_or_insert_with(|| key.clone());

    // Icon aus ~/.config/ai-central/icons/<name> in den Projekt-Config-Ordner
    // (Kopie + Löschen statt rename — Icons-Verzeichnis und Projekt können auf
    // verschiedenen Dateisystemen liegen).
    if let Some(icon) = cfg.terminal.icon.clone() {
      if !icon.starts_with('/') {
        let old = paths.icons_dir().join(&icon);
        if old.is_file() {
          cfg.terminal.icon = Some(adopt_icon(&old, &cfg_dir)?);
          fs::remove_file(&old).map_err(|e| format!("{}: {e}", old.display()))?;
          cfg_dirty = true;
        }
      }
    }

    if cfg_dirty {
      fs::create_dir_all(&cfg_dir).map_err(|e| format!("{}: {e}", cfg_dir.display()))?;
      let raw = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
      crate::domain::write_atomic(&cfg_path, &(raw + "\n"))?;
    }
    if id != key {
      reg_dirty = true;
    }
    new_reg.insert(id, entry);
  }
  if reg_dirty {
    save_registry(paths, &new_reg)?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::{create_project, make_apikey_pool, make_oauth_pool, map_store, tmp_paths};

  fn project_config_path(paths: &Paths, project: &str) -> Result<PathBuf, String> {
    Ok(project_dir(paths, project)?.join(PROJECT_CONFIG_DIR).join(PROJECT_FILE))
  }

  /// Ein Pool-Eintrag mit Pfad-Bestandteilen — etwa per Migration aus der
  /// mitgeklonten Alt-Datei in die Registry gelangt — darf kein beliebiges
  /// Verzeichnis zum CLAUDE_CONFIG_DIR machen.
  #[test]
  fn pool_mit_traversal_wird_abgelehnt() {
    let p = tmp_paths();
    create_project(&p, "fremd").unwrap();
    crate::domain::registry::set_project_pool(&p, "fremd", Some("../../../../tmp/x")).unwrap();

    let err = project_pool_dir_in(&p, "fremd").unwrap_err();
    assert!(err.contains("ungültiger Pool"));
  }

  /// Ein Projekt mit kaputter Config (stehengebliebene Merge-Marker) darf
  /// weder die Migration noch die Projektliste kippen. Die Migration läuft im
  /// Tauri-setup: ein Fehler dort beendet die App, bevor ein Fenster da ist —
  /// ohne jede Meldung, weil es noch keine Oberfläche gibt.
  #[test]
  fn kaputte_projekt_config_haelt_start_und_liste_nicht_auf() {
    let p = tmp_paths();
    create_project(&p, "heil").unwrap();
    create_project(&p, "kaputt").unwrap();
    let cfg = p
      .projects_dir()
      .join("kaputt")
      .join(PROJECT_CONFIG_DIR)
      .join(PROJECT_FILE);
    fs::write(
      &cfg,
      "<<<<<<< HEAD\n{\"id\": \"kaputt\", \"name\": \"A\"}\n=======\n{\"id\": \"kaputt\", \"name\": \"B\"}\n>>>>>>> other\n",
    )
    .unwrap();

    // Die Migration läuft durch und lässt das kaputte Projekt in Ruhe.
    migrate_layout_in(&p).unwrap();
    assert_eq!(
      fs::read_to_string(&cfg).unwrap().lines().next().unwrap(),
      "<<<<<<< HEAD"
    );

    // Beide Projekte stehen in der Liste; das kaputte trägt seinen Fehler und
    // die ID als Namen.
    let liste = list_projects_in(&p).unwrap();
    assert_eq!(liste.len(), 2);
    let heil = liste.iter().find(|x| x.id == "heil").unwrap();
    assert!(heil.error.is_none());
    let kaputt = liste.iter().find(|x| x.id == "kaputt").unwrap();
    assert_eq!(kaputt.name, "kaputt");
    assert!(kaputt.error.as_ref().unwrap().contains("config.json"));
  }

  /// Ohne zugewiesenen Pool gibt es kein CLAUDE_CONFIG_DIR und damit auch
  /// keine Session: Der Fehler trägt die Kennung, an der der Tray-Start die
  /// Pool-Auswahl öffnet. Ein stilles `Ok(None)` würde claudes
  /// Default-Verzeichnis ungefragt übernehmen.
  #[test]
  fn projekt_ohne_pool_liefert_fehler() {
    let p = tmp_paths();
    create_project(&p, "ohne").unwrap();

    assert_eq!(project_pool_dir_in(&p, "ohne").unwrap_err(), NO_POOL);
  }

  /// Das Setzen der Terminal-Config kommt aus der Oberfläche und kennt nur
  /// theme/icon/title — unbekannte Keys müssen trotzdem stehen bleiben.
  #[test]
  fn terminal_config_setzen_erhaelt_fremde_keys() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let cfg_path = project_config_path(&p, "proj").unwrap();
    fs::write(&cfg_path, r#"{"terminal":{"theme":"monokai","fontSize":13}}"#).unwrap();

    set_terminal_config_in(
      &p,
      "proj",
      TerminalConfig { theme: Some("dracula".into()), ..Default::default() },
    )
    .unwrap();

    let v: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    assert_eq!(v["terminal"]["theme"], "dracula");
    assert_eq!(v["terminal"]["fontSize"], 13);
  }

  #[test]
  fn projekt_wizard_scaffold() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    let id = create_project_full_in(
      &p,
      "neu",
      None,
      Some(&kunde),
      Some("~/projects/neu"),
      true,
      TerminalConfig {
        theme: Some("dracula".into()),
        title: Some("Neu".into()),
        ..Default::default()
      },
    )
    .unwrap();

    let dir = p.projects_dir().join("neu");
    assert!(dir.join("memory").is_dir());
    assert_eq!(
      fs::read_to_string(dir.join(".gitignore")).unwrap(),
      ".ai-central-running\n"
    );
    assert!(p.home.join("projects").join("neu").is_dir());

    // ID ist UUID und Registry-Schlüssel; Pool liegt in der Registry.
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_eq!(project_dir(&p, &id).unwrap(), dir);
    assert_eq!(project_pool(&p, &id).unwrap().as_deref(), Some(kunde.as_str()));
    let cfg = read_project_config_in(&p, &id).unwrap();
    assert_eq!(cfg.id.as_deref(), Some(id.as_str()));
    assert_eq!(cfg.name.as_deref(), Some("neu"));
    assert_eq!(cfg.terminal.title.as_deref(), Some("Neu"));
    assert_eq!(cfg.terminal.theme.as_deref(), Some("dracula"));
    // Registry-Eintrag und Ordner passen zusammen.
    assert_eq!(verify_project_dir_in(&p, &id).unwrap(), dir);

    let settings: serde_json::Value = serde_json::from_str(
      &fs::read_to_string(dir.join(".claude").join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["autoMemoryDirectory"], "~/claude-projects/neu/memory");
    assert_eq!(settings["permissions"]["allow"][0], "Edit(~/projects/neu/**)");
    assert_eq!(
      settings["permissions"]["allow"][1],
      "Edit(~/claude-projects/neu/**)"
    );
    assert_eq!(settings["permissions"]["additionalDirectories"][0], "~/projects/neu");
    assert!(settings.get("hooks").is_none());
  }

  #[test]
  fn projekt_wizard_minimal_ohne_pool_und_workdir() {
    let p = tmp_paths();
    let id =
      create_project_full_in(&p, "neu", None, None, None, false, TerminalConfig::default())
        .unwrap();
    let dir = p.projects_dir().join("neu");
    assert!(dir.join(".claude").join("settings.json").is_file());
    // config.json entsteht immer — sie trägt die Projekt-Identität.
    let v: serde_json::Value = serde_json::from_str(
      &fs::read_to_string(project_config_path(&p, &id).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(v["id"], id.as_str());
    assert_eq!(v["name"], "neu");
    assert!(v.get("terminal").is_none());
    let settings: serde_json::Value = serde_json::from_str(
      &fs::read_to_string(dir.join(".claude").join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["permissions"]["allow"][0], "Edit(~/claude-projects/neu/**)");
    assert_eq!(settings["permissions"]["additionalDirectories"], serde_json::json!([]));
  }

  #[test]
  fn projekt_wizard_doppelter_name_scheitert() {
    let p = tmp_paths();
    create_project(&p, "neu").unwrap();
    let err =
      create_project_full_in(&p, "neu", None, None, None, false, TerminalConfig::default())
        .unwrap_err();
    assert!(err.contains("existiert bereits"));
  }

  #[test]
  fn projekt_wizard_fehlendes_workdir_scheitert() {
    let p = tmp_paths();
    let err = create_project_full_in(
      &p,
      "neu",
      None,
      None,
      Some("~/projects/gibtsnicht"),
      false,
      TerminalConfig::default(),
    )
    .unwrap_err();
    assert!(err.contains("Arbeitsverzeichnis fehlt"));
    // kein halbes Projekt zurückgeblieben
    assert!(!p.projects_dir().join("neu").exists());
  }

  // -- Projekt zuordnen / rausnehmen / wechseln --

  #[test]
  fn projekt_zuordnen() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();

    assert_eq!(project_pool(&p, "proj").unwrap().as_deref(), Some(kunde.as_str()));
    // die Zuordnung ist maschinenlokal — die Projekt-Config bleibt ohne pool-Key
    let raw = fs::read_to_string(project_config_path(&p, "proj").unwrap()).unwrap();
    assert!(!raw.contains("pool"));

    let pools =
      crate::domain::pool::list_pools_in(&p, &crate::domain::testutil::FailStore).unwrap();
    assert_eq!(pools[0].projects, vec!["proj"]);
  }

  #[test]
  fn projekt_zuordnen_pool_fehlt_scheitert() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert!(assign_pool_in(&p, "proj", "gibtsnicht").is_err());
  }

  #[test]
  fn projekt_rausnehmen() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();
    unassign_pool_in(&p, "proj").unwrap();
    assert_eq!(project_pool(&p, "proj").unwrap(), None);
  }

  #[test]
  fn projekt_rausnehmen_ohne_zuordnung_scheitert() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert!(unassign_pool_in(&p, "proj").is_err());
  }

  /// Wechsel oauth → apikey: das Projekt sieht nur den Pool-Namen,
  /// der Credential-Typ ist ihm egal.
  #[test]
  fn pool_wechseln_typ_ist_projekt_egal() {
    let p = tmp_paths();
    let privat = make_oauth_pool(&p, "privat");
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project(&p, "proj").unwrap();

    assign_pool_in(&p, "proj", &privat).unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();

    assert_eq!(project_pool(&p, "proj").unwrap().as_deref(), Some(kunde.as_str()));
  }

  // -- Projekt-Config: Identität, Round-Trip, Icon --

  #[test]
  fn verify_erkennt_fremden_ordner() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert!(verify_project_dir_in(&p, "proj").is_ok());
    // Ordner „ersetzt": config.json trägt eine andere ID.
    fs::write(
      p.projects_dir().join("proj").join(PROJECT_CONFIG_DIR).join(PROJECT_FILE),
      r#"{"id": "anderes-projekt", "name": "proj"}"#,
    )
    .unwrap();
    let err = verify_project_dir_in(&p, "proj").unwrap_err();
    assert!(err.contains("passt nicht zur Registry"));
  }

  /// Der Config-Round-Trip (lesen, Feld ändern, neu schreiben) reicht Keys
  /// durch, die diese Build-Version nicht kennt.
  #[test]
  fn config_roundtrip_erhaelt_fremde_keys() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let cfg_dir = p.projects_dir().join("proj").join(PROJECT_CONFIG_DIR);
    fs::write(
      cfg_dir.join(PROJECT_FILE),
      r#"{"id": "proj", "name": "proj", "archiveHome": "~/archiv", "kuenftig": {"x": 1}}"#,
    )
    .unwrap();

    set_terminal_config_in(
      &p,
      "proj",
      TerminalConfig { theme: Some("dracula".into()), ..Default::default() },
    )
    .unwrap();

    let v: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(cfg_dir.join(PROJECT_FILE)).unwrap()).unwrap();
    assert_eq!(v["kuenftig"]["x"], 1);
    assert_eq!(v["archiveHome"], "~/archiv");
    assert_eq!(v["terminal"]["theme"], "dracula");
    assert_eq!(v["id"], "proj");
  }

  #[test]
  fn icon_wandert_in_den_projekt_config_ordner() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let src = p.home.join("bild.PNG");
    fs::write(&src, b"png").unwrap();

    set_terminal_config_in(
      &p,
      "proj",
      TerminalConfig { icon: Some(src.display().to_string()), ..Default::default() },
    )
    .unwrap();

    let cfg = read_project_config_in(&p, "proj").unwrap();
    assert_eq!(cfg.terminal.icon.as_deref(), Some("icon.png"));
    let dest = p.projects_dir().join("proj").join(PROJECT_CONFIG_DIR).join("icon.png");
    assert!(dest.is_file());
    assert_eq!(resolve_icon_path(&p, "proj", "icon.png").unwrap(), dest);
  }

  // -- Migration Alt-Layout --

  /// Projekt im Alt-Layout: Registry-Schlüssel = Name, ai-central.json im
  /// Projekt-Root, Icon im zentralen Icons-Verzeichnis, keine .ai-central/.
  fn create_legacy_project(p: &Paths, name: &str) -> PathBuf {
    let dir = p.projects_dir().join(name);
    fs::create_dir_all(dir.join(".claude")).unwrap();
    register_project(p, name, &dir).unwrap();
    dir
  }

  #[test]
  fn migration_altes_layout() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    let dir = create_legacy_project(&p, "proj");
    fs::write(
      dir.join(LEGACY_PROJECT_FILE),
      format!(
        r#"{{"pool": "{kunde}", "terminal": {{"icon": "proj.png"}}, "archiveHome": "~/archiv"}}"#
      ),
    )
    .unwrap();
    fs::create_dir_all(p.icons_dir()).unwrap();
    fs::write(p.icons_dir().join("proj.png"), b"png").unwrap();

    migrate_layout_in(&p).unwrap();

    // Registry-Schlüssel ist jetzt die UUID aus der config.json.
    let reg = load_registry(&p).unwrap();
    assert_eq!(reg.len(), 1);
    let id = reg.keys().next().unwrap().clone();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_eq!(project_pool(&p, &id).unwrap().as_deref(), Some(kunde.as_str()));
    assert!(!dir.join(LEGACY_PROJECT_FILE).exists());

    let cfg = read_project_config_in(&p, &id).unwrap();
    assert_eq!(cfg.id.as_deref(), Some(id.as_str()));
    assert_eq!(cfg.name.as_deref(), Some("proj"));
    assert_eq!(cfg.archive_home.as_deref(), Some("~/archiv"));
    // Icon liegt jetzt im Projekt, das zentrale Verzeichnis ist geräumt.
    assert_eq!(cfg.terminal.icon.as_deref(), Some("icon.png"));
    assert!(dir.join(PROJECT_CONFIG_DIR).join("icon.png").is_file());
    assert!(!p.icons_dir().join("proj.png").exists());
    assert_eq!(verify_project_dir_in(&p, &id).unwrap(), dir);

    // Zweiter Lauf ändert nichts mehr.
    migrate_layout_in(&p).unwrap();
    let reg2 = load_registry(&p).unwrap();
    assert_eq!(reg2.keys().collect::<Vec<_>>(), vec![&id]);
  }

  #[test]
  fn migration_vergibt_id_auch_ohne_altdatei() {
    let p = tmp_paths();
    let dir = create_legacy_project(&p, "proj");
    migrate_layout_in(&p).unwrap();
    let reg = load_registry(&p).unwrap();
    let id = reg.keys().next().unwrap().clone();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    let cfg = read_project_config_in(&p, &id).unwrap();
    assert_eq!(cfg.name.as_deref(), Some("proj"));
    assert_eq!(verify_project_dir_in(&p, &id).unwrap(), dir);
  }

  #[test]
  fn migration_migriertes_projekt_bleibt_unveraendert() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let before = fs::read_to_string(project_config_path(&p, "proj").unwrap()).unwrap();
    migrate_layout_in(&p).unwrap();
    assert_eq!(
      fs::read_to_string(project_config_path(&p, "proj").unwrap()).unwrap(),
      before
    );
    assert!(load_registry(&p).unwrap().contains_key("proj"));
  }

  // -- Projekt anlegen / importieren / löschen --

  #[test]
  fn projekt_anlegen() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert!(p.projects_dir().join("proj").join(".claude").is_dir());

    let projects = list_projects_in(&p).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "proj");
    assert_eq!(projects[0].name, "proj");
    assert_eq!(projects[0].pool, None);
  }

  /// Import eines Ordners mit mitgebrachter Config: die ID wird übernommen
  /// (Sync-Fall), Arbeitsordner und Archiv-Home werden angelegt, die
  /// Archiv-Permission nachgezogen.
  #[test]
  fn projekt_import_uebernimmt_id_und_legt_pfade_an() {
    let p = tmp_paths();
    let dir = p.home.join("import").join("proj");
    fs::create_dir_all(dir.join(PROJECT_CONFIG_DIR)).unwrap();
    fs::write(
      dir.join(PROJECT_CONFIG_DIR).join(PROJECT_FILE),
      r#"{"id": "sync-id", "name": "Mein Projekt", "archiveHome": "~/archiv/proj"}"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join(".claude")).unwrap();
    fs::write(
      settings_path(&dir),
      r#"{"permissions": {"additionalDirectories": ["~/work/proj"]}}"#,
    )
    .unwrap();

    add_project_in(&p, &dir.display().to_string()).unwrap();

    assert_eq!(project_dir(&p, "sync-id").unwrap(), dir);
    assert_eq!(display_name_in(&p, "sync-id").unwrap(), "Mein Projekt");
    assert!(p.home.join("work/proj").is_dir());
    assert!(p.home.join("archiv/proj").is_dir());
    let raw = fs::read_to_string(settings_path(&dir)).unwrap();
    assert!(raw.contains("~/archiv/proj"));
    assert!(raw.contains("Edit(~/archiv/proj/**)"));

    // Doppelt aufnehmen scheitert an der ID.
    let err = add_project_in(&p, &dir.display().to_string()).unwrap_err();
    assert!(err.contains("schon registriert"));
  }

  /// Import ohne mitgebrachte Config: neue UUID, Ordnername wird Anzeigename.
  #[test]
  fn projekt_import_ohne_config_erzeugt_id() {
    let p = tmp_paths();
    let dir = p.home.join("import").join("roh");
    fs::create_dir_all(&dir).unwrap();

    add_project_in(&p, &dir.display().to_string()).unwrap();

    let reg = load_registry(&p).unwrap();
    let id = reg.keys().next().unwrap().clone();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_eq!(display_name_in(&p, &id).unwrap(), "roh");
    assert_eq!(verify_project_dir_in(&p, &id).unwrap(), dir);
  }

  #[test]
  fn projekt_doppelt_anlegen_scheitert() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert!(create_project(&p, "proj").is_err());
  }

  #[test]
  fn projekt_loeschen() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    delete_project_scoped_in(&p, "proj", "full", false).unwrap();
    assert!(!p.projects_dir().join("proj").exists());
  }

  #[test]
  fn projekt_loeschen_laesst_arbeitsordner() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default())
      .unwrap();
    delete_project_scoped_in(&p, &id, "full", false).unwrap();
    assert!(!p.projects_dir().join("proj").exists());
    assert!(p.home.join("projects").join("proj").is_dir());
  }

  #[test]
  fn projekt_loeschen_mit_arbeitsordner() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default())
      .unwrap();
    delete_project_scoped_in(&p, &id, "full", true).unwrap();
    assert!(!p.projects_dir().join("proj").exists());
    assert!(!p.home.join("projects").join("proj").exists());
  }

  #[test]
  fn projekt_loeschen_fehlender_arbeitsordner_scheitert() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default())
      .unwrap();
    fs::remove_dir_all(p.home.join("projects").join("proj")).unwrap();
    assert!(delete_project_scoped_in(&p, &id, "full", true).is_err());
  }

  #[test]
  fn projekt_arbeitsordner_auslesen() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default())
      .unwrap();
    assert_eq!(project_work_dirs_in(&p, &id).unwrap(), vec!["~/projects/proj"]);
    // Projekt ohne settings.json → leer
    let dir = p.home.join("ohne-settings");
    fs::create_dir_all(&dir).unwrap();
    add_project_in(&p, &dir.display().to_string()).unwrap();
    let alt = list_projects_in(&p)
      .unwrap()
      .into_iter()
      .find(|pr| pr.name == "ohne-settings")
      .unwrap();
    assert!(project_work_dirs_in(&p, &alt.id).unwrap().is_empty());
  }

  #[test]
  fn arbeitsordner_nachtraeglich_erfassen_und_entfernen() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default())
      .unwrap();
    fs::create_dir_all(p.home.join("projects/extra")).unwrap();
    let picked = p.home.join("projects/extra").to_string_lossy().into_owned();
    add_work_dir_in(&p, &id, &picked).unwrap();
    // gespeichert wird ~-kontrahiert, in beiden permissions-Feldern
    assert_eq!(project_work_dirs_in(&p, &id).unwrap(), vec!["~/projects/extra"]);
    let raw = fs::read_to_string(settings_path(&p.projects_dir().join("proj"))).unwrap();
    assert!(raw.contains("Edit(~/projects/extra/**)"));
    assert!(add_work_dir_in(&p, &id, &picked).is_err()); // doppelt

    remove_work_dir_in(&p, &id, "~/projects/extra").unwrap();
    assert!(project_work_dirs_in(&p, &id).unwrap().is_empty());
    let raw = fs::read_to_string(settings_path(&p.projects_dir().join("proj"))).unwrap();
    assert!(!raw.contains("~/projects/extra"));
    // Ordner selbst bleibt
    assert!(p.home.join("projects/extra").is_dir());
  }

  #[test]
  fn projektordner_verlegen() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default())
      .unwrap();
    // Nutzer hat den Ordner selbst verschoben; die App ordnet nur neu zu.
    let new_dir = p.home.join("elsewhere").join("proj");
    fs::create_dir_all(p.home.join("elsewhere")).unwrap();
    fs::rename(p.projects_dir().join("proj"), &new_dir).unwrap();
    set_project_dir_in(&p, &id, &new_dir.to_string_lossy()).unwrap();

    assert_eq!(project_dir(&p, &id).unwrap(), new_dir);
    // Die config.json zieht mit — Verifikation greift am neuen Ort.
    assert_eq!(verify_project_dir_in(&p, &id).unwrap(), new_dir);
    let raw = fs::read_to_string(settings_path(&new_dir)).unwrap();
    assert!(raw.contains("~/elsewhere/proj/memory"));
    assert!(raw.contains("Edit(~/elsewhere/proj/**)"));
    assert!(!raw.contains("claude-projects/proj"));
  }

  // -- write_panel(path): promptfreier Lese-Weg --

  #[test]
  fn panel_lesen_nur_aus_projekt_arbeits_und_archivordnern() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let dir = p.projects_dir().join("proj");
    fs::write(dir.join("notiz.md"), "inhalt").unwrap();
    assert_eq!(read_for_panel_in(&p, "proj", &dir.join("notiz.md").display().to_string()).unwrap(), "inhalt");

    // außerhalb: abgelehnt
    fs::write(p.home.join("geheim.txt"), "x").unwrap();
    let err = read_for_panel_in(&p, "proj", &p.home.join("geheim.txt").display().to_string())
      .unwrap_err();
    assert!(err.contains("außerhalb"));

    // Symlink aus dem Projekt hinaus: canonicalize entlarvt das Ziel
    std::os::unix::fs::symlink(p.home.join("geheim.txt"), dir.join("link.txt")).unwrap();
    assert!(read_for_panel_in(&p, "proj", &dir.join("link.txt").display().to_string()).is_err());
  }

  #[test]
  fn panel_lesen_groessenlimit_und_geraetedateien() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let dir = p.projects_dir().join("proj");
    fs::write(dir.join("gross.bin"), vec![b'x'; 2 * 1024 * 1024 + 1]).unwrap();
    let err = read_for_panel_in(&p, "proj", &dir.join("gross.bin").display().to_string())
      .unwrap_err();
    assert!(err.contains("2 MB"));
    // Geräte-Datei liegt eh außerhalb der Wurzeln — der Ablehnungsgrund davor
    assert!(read_for_panel_in(&p, "proj", "/dev/zero").is_err());
  }

  /// Alte ToDo-Liste unter panels/ zieht beim Session-Start ins Projekt; ein
  /// dort schon vorhandener (neuerer) Stand bleibt unangetastet.
  #[test]
  fn todos_ziehen_ins_projekt() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let alt = crate::domain::paths::legacy_todos_file(&p, "proj");
    fs::create_dir_all(alt.parent().unwrap()).unwrap();
    fs::write(&alt, "{\"text\":\"alt\"}\n").unwrap();

    migrate_todos_into_project(&p, "proj").unwrap();

    let neu = crate::domain::paths::todos_file_in(&p, "proj");
    assert!(!alt.exists(), "alte Liste bleibt liegen");
    assert_eq!(fs::read_to_string(&neu).unwrap(), "{\"text\":\"alt\"}\n");

    // Zweiter Lauf mit erneut aufgetauchter Altdatei: das Projekt gewinnt.
    fs::write(&alt, "{\"text\":\"ueberholt\"}\n").unwrap();
    migrate_todos_into_project(&p, "proj").unwrap();
    assert_eq!(fs::read_to_string(&neu).unwrap(), "{\"text\":\"alt\"}\n");
  }

  /// Stufe „nur Integration": ai-central-Spuren weg, Ordner und
  /// Claude-Code-Bestand bleiben.
  #[test]
  fn loeschen_nur_integration() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default())
      .unwrap();
    let dir = p.projects_dir().join("proj");
    // Archiv-Home samt Permission wie über die UI gesetzt
    let mut cfg = read_project_config_in(&p, &id).unwrap();
    cfg.archive_home = Some("~/archiv/proj".into());
    write_project_config_in(&p, &id, &cfg).unwrap();
    crate::domain::archive::add_archive_permission(&p, &id, "~/archiv/proj").unwrap();
    fs::create_dir_all(p.home.join("archiv/proj")).unwrap();
    fs::write(p.home.join("archiv/proj/doc.md"), "inhalt").unwrap();
    // flüchtige Panel-Datei
    fs::create_dir_all(p.config_dir().join("panels")).unwrap();
    fs::write(p.config_dir().join("panels").join(format!("{id}.md")), "x").unwrap();

    let preview = delete_preview_in(&p, &id).unwrap();
    assert!(preview.ai_central_dir && preview.archive_permission);
    assert_eq!(preview.panel_files, 1);
    assert_eq!(preview.archive_docs, 1);

    delete_project_scoped_in(&p, &id, "integration", false).unwrap();

    assert!(dir.is_dir());
    assert!(!dir.join(PROJECT_CONFIG_DIR).exists());
    let sp = fs::read_to_string(settings_path(&dir)).unwrap();
    assert!(!sp.contains("archiv/proj"));
    assert!(sp.contains("autoMemoryDirectory")); // Claude-Code-Bestand bleibt
    assert!(p.home.join("archiv/proj/doc.md").is_file()); // Archiv bleibt
    assert!(!p.config_dir().join("panels").join(format!("{id}.md")).exists());
    assert!(load_registry(&p).unwrap().is_empty());
  }

  /// Stufe „Integration & Archiv": zusätzlich stirbt der Archiv-Ordner.
  #[test]
  fn loeschen_integration_und_archiv() {
    let p = tmp_paths();
    let id = create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default())
      .unwrap();
    let mut cfg = read_project_config_in(&p, &id).unwrap();
    cfg.archive_home = Some("~/archiv/proj".into());
    write_project_config_in(&p, &id, &cfg).unwrap();
    fs::create_dir_all(p.home.join("archiv/proj")).unwrap();

    delete_project_scoped_in(&p, &id, "archive", false).unwrap();

    assert!(p.projects_dir().join("proj").is_dir());
    assert!(!p.home.join("archiv/proj").exists());
    assert!(load_registry(&p).unwrap().is_empty());
  }

  #[test]
  fn loeschen_unbekannter_umfang_scheitert() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert!(delete_project_scoped_in(&p, "proj", "halb", false).is_err());
  }

  #[test]
  fn projekt_loeschen_unbekannt_scheitert() {
    let p = tmp_paths();
    assert!(delete_project_scoped_in(&p, "gibtsnicht", "full", false).is_err());
  }
}
