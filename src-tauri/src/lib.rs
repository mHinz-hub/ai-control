mod terminal;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Feste Dateinamen im Pool-Ordner. Der Pool-Ordner selbst ist das
/// CLAUDE_CONFIG_DIR. oauth-Credentials verwaltet claude selbst (Keychain);
/// die App speichert keine Tokens.
const APIKEY_FILE: &str = "apikey";
const POOL_FILE: &str = "pool.json";
const PROJECT_FILE: &str = "ai-control.json";
const PROJECTS_FILE: &str = "projects.json";

/// Wurzelpfade; in Tests mit temporärem home instanziierbar.
pub(crate) struct Paths {
  home: PathBuf,
}

impl Paths {
  pub(crate) fn real() -> Self {
    Paths {
      home: PathBuf::from(std::env::var("HOME").expect("HOME nicht gesetzt")),
    }
  }

  /// Default-Root: Alt-Layout (Discovery ohne Registry) und Ablageort neuer
  /// Projekte ohne gewählten Zielordner.
  fn projects_dir(&self) -> PathBuf {
    self.home.join("claude-projects")
  }

  fn config_dir(&self) -> PathBuf {
    self.home.join(".config").join("ai-control")
  }

  fn projects_file(&self) -> PathBuf {
    self.config_dir().join(PROJECTS_FILE)
  }

  fn pools_dir(&self) -> PathBuf {
    self.config_dir().join("pools")
  }

  /// Gemeinsames Icons-Verzeichnis aller Projekte — synct mit der App-Config,
  /// unabhängig von Pool-Zuordnung und Quell-Repos.
  fn icons_dir(&self) -> PathBuf {
    self.config_dir().join("icons")
  }

  fn pool_dir(&self, pool: &str) -> PathBuf {
    self.pools_dir().join(pool)
  }
}

// ---------- Projekt-Registry ----------

/// Pfad unterhalb von Home als "~/…" schreiben — Registry-Einträge im
/// Home-Bereich bleiben damit maschinenübergreifend stabil.
fn contract_home(paths: &Paths, p: &std::path::Path) -> String {
  match p.strip_prefix(&paths.home) {
    Ok(rest) => format!("~/{}", rest.display()),
    Err(_) => p.display().to_string(),
  }
}

/// Registry Name → Projektordner; ohne projects.json gibt es keine Projekte.
fn load_registry(paths: &Paths) -> Result<std::collections::BTreeMap<String, PathBuf>, String> {
  let file = paths.projects_file();
  if !file.is_file() {
    return Ok(std::collections::BTreeMap::new());
  }
  let raw = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
  let map: std::collections::BTreeMap<String, String> =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", file.display()))?;
  Ok(
    map
      .into_iter()
      .map(|(name, p)| {
        let dir = expand_home(paths, &p);
        (name, dir)
      })
      .collect(),
  )
}

fn save_registry(
  paths: &Paths,
  reg: &std::collections::BTreeMap<String, PathBuf>,
) -> Result<(), String> {
  let contracted: std::collections::BTreeMap<&String, String> =
    reg.iter().map(|(name, dir)| (name, contract_home(paths, dir))).collect();
  let raw = serde_json::to_string_pretty(&contracted).map_err(|e| e.to_string())?;
  fs::create_dir_all(paths.config_dir()).map_err(|e| e.to_string())?;
  let file = paths.projects_file();
  fs::write(&file, raw + "\n").map_err(|e| format!("{}: {e}", file.display()))
}

/// Ordner eines registrierten Projekts.
pub(crate) fn project_dir(paths: &Paths, name: &str) -> Result<PathBuf, String> {
  load_registry(paths)?
    .remove(name)
    .ok_or_else(|| format!("Projekt nicht registriert: {name}"))
}

/// Nimmt ein Projekt in die Registry auf.
fn register_project(paths: &Paths, name: &str, dir: &std::path::Path) -> Result<(), String> {
  let mut reg = load_registry(paths)?;
  if reg.contains_key(name) {
    return Err(format!("Projekt existiert bereits: {name}"));
  }
  reg.insert(name.to_string(), dir.to_path_buf());
  save_registry(paths, &reg)
}

/// Entfernt nur den Registry-Eintrag; der Projektordner bleibt.
fn unregister_project(paths: &Paths, name: &str) -> Result<(), String> {
  let mut reg = load_registry(paths)?;
  if reg.remove(name).is_none() {
    return Err(format!("Projekt nicht registriert: {name}"));
  }
  save_registry(paths, &reg)
}

fn project_config_path(paths: &Paths, project: &str) -> Result<PathBuf, String> {
  Ok(project_dir(paths, project)?.join(PROJECT_FILE))
}

#[derive(Serialize)]
struct Project {
  name: String,
  path: String,
  pool: Option<String>,
  running: bool,
  terminal: TerminalConfig,
}

#[derive(Serialize, Deserialize)]
struct Pool {
  name: String,
  #[serde(rename = "credentialType")]
  credential_type: String,
}

#[derive(Serialize, Deserialize, Default)]
struct ProjectConfig {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pool: Option<String>,
  #[serde(default, skip_serializing_if = "TerminalConfig::is_empty")]
  terminal: TerminalConfig,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct TerminalConfig {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub theme: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub icon: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
}

impl TerminalConfig {
  fn is_empty(&self) -> bool {
    self.theme.is_none() && self.icon.is_none() && self.title.is_none()
  }
}

fn is_running(project: &str) -> bool {
  !terminal_pids(project).is_empty()
}

/// Projekte, die diesem Pool zugeordnet sind und gerade laufen — dieselbe
/// Lauf-Erkennung wie die Projektliste (`is_running`).
fn running_projects_using_pool(paths: &Paths, pool: &str) -> Result<Vec<String>, String> {
  Ok(projects_using_pool(paths, pool)?
    .into_iter()
    .filter(|p| is_running(p))
    .collect())
}

/// PIDs der eingebauten Terminal-Prozesse (`app --terminal <projekt>`).
fn terminal_pids(project: &str) -> Vec<u32> {
  let out = Command::new("pgrep")
    .args(["-f", "--", &format!("--terminal {project}$")])
    .output();
  match out {
    Ok(o) => String::from_utf8_lossy(&o.stdout)
      .lines()
      .filter_map(|l| l.trim().parse().ok())
      .collect(),
    Err(_) => Vec::new(),
  }
}

/// Tick-Intervall des Session-Watchers.
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// App-eigene settings.json unter ~/.config/ai-control (nicht pool-/projektbezogen).
const APP_SETTINGS_FILE: &str = "settings.json";

/// Opt-in: synct der Watcher bei Session-Ende? Default aus.
fn sync_on_session_end(paths: &Paths) -> bool {
  fs::read_to_string(paths.config_dir().join(APP_SETTINGS_FILE))
    .ok()
    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    .and_then(|v| v["syncOnSessionEnd"].as_bool())
    .unwrap_or(false)
}

/// Kommando, das im Projekt-Terminal startet (settings.json: claudeCommand).
pub(crate) fn claude_command(paths: &Paths) -> String {
  fs::read_to_string(paths.config_dir().join(APP_SETTINGS_FILE))
    .ok()
    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    .and_then(|v| v["claudeCommand"].as_str().map(str::to_string))
    .unwrap_or_else(|| "claude".into())
}

/// Pool-Config-Verzeichnis eines Projekts — wird dem Terminal als
/// CLAUDE_CONFIG_DIR mitgegeben.
pub(crate) fn project_pool_dir(project: &str) -> Result<Option<PathBuf>, String> {
  let paths = Paths::real();
  Ok(read_project_config_in(&paths, project)?.pool.map(|p| paths.pool_dir(&p)))
}

/// Setzt das Opt-in; erhält übrige App-settings.
fn set_sync_on_session_end_in(paths: &Paths, enabled: bool) -> Result<(), String> {
  let path = paths.config_dir().join(APP_SETTINGS_FILE);
  let mut v: serde_json::Value = fs::read_to_string(&path)
    .ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_else(|| serde_json::json!({}));
  v["syncOnSessionEnd"] = serde_json::json!(enabled);
  fs::create_dir_all(paths.config_dir()).map_err(|e| e.to_string())?;
  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  fs::write(&path, raw + "\n").map_err(|e| format!("{}: {e}", path.display()))
}

/// Nächstliegendes Git-Repo (Ordner mit .git) ab `dir` aufwärts.
fn git_root(dir: &std::path::Path) -> Option<PathBuf> {
  dir
    .ancestors()
    .find(|a| a.join(".git").exists())
    .map(|a| a.to_path_buf())
}

/// (Name, läuft) aller Projekte, sortiert — dieselbe Erkennung wie die
/// Projektliste; Grundlage für Session-Watcher und Tray-Menü.
fn project_state(paths: &Paths) -> Vec<(String, bool)> {
  let mut state: Vec<(String, bool)> = list_projects_in(paths)
    .map(|ps| ps.into_iter().map(|p| (p.name, p.running)).collect())
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

/// Session-Watcher im Tray-Prozess: pollt die laufenden Projekte; wechselt eines
/// von „läuft" auf „läuft nicht mehr", ist die Session beendet → Kontext syncen.
/// Erkennung über das Verschwinden des Prozesses, greift daher auch bei
/// HUP/Kill (im sterbenden Prozess liefe kein Hook mehr).
/// Ändert sich Projektliste oder Laufstatus, baut er das Tray-Menü neu auf.
fn spawn_session_watcher(app: tauri::AppHandle) {
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
      // der Tray verlangt kein Repo, er nutzt es nur, falls vorhanden.
      if !ended.is_empty() && sync_on_session_end(&paths) {
        for project in ended {
          if let Ok(dir) = project_dir(&paths, project) {
            if let Some(repo) = git_root(&dir) {
              sync_session_context(&repo, project);
            }
          }
        }
      }
      // Menü-Objekte gehören auf den Main-Thread.
      let handle = app.clone();
      app
        .run_on_main_thread(move || {
          let menu = tray_menu(&handle).expect("Tray-Menü nicht aufbaubar");
          handle
            .tray_by_id("main")
            .expect("Tray-Icon fehlt")
            .set_menu(Some(menu))
            .expect("Tray-Menü nicht setzbar");
        })
        .expect("Main-Thread nicht erreichbar");
      state = current;
    }
  });
}

fn check_name(name: &str) -> Result<(), String> {
  if name.trim().is_empty() || name.contains('/') || name.contains("..") {
    return Err(format!("ungültiger Name: {name}"));
  }
  Ok(())
}

fn read_pool(paths: &Paths, pool: &str) -> Result<Pool, String> {
  let cfg_path = paths.pool_dir(pool).join(POOL_FILE);
  let raw = fs::read_to_string(&cfg_path)
    .map_err(|e| format!("{}: {e}", cfg_path.display()))?;
  serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg_path.display()))
}

// ---------- Projekte ----------

fn list_projects_in(paths: &Paths) -> Result<Vec<Project>, String> {
  let mut projects = Vec::new();
  for (name, dir) in load_registry(paths)? {
    let cfg = read_project_config_in(paths, &name)?;
    projects.push(Project {
      running: is_running(&name),
      path: contract_home(paths, &dir),
      pool: cfg.pool,
      terminal: cfg.terminal,
      name,
    });
  }
  Ok(projects)
}

// ---------- Feature: Todoliste ----------

/// Muster robotunits: Datei im Projekt-Root, per SessionStart-Hook (jq)
/// als additionalContext in jede Session injiziert.
const TODO_FILE: &str = "OFFENE-PUNKTE.md";
const TODO_SKELETON: &str = "# Offene Punkte — bei jedem Start prüfen und abhaken\n\nKeine offenen Punkte.\n";

fn todo_hook_command(dir: &std::path::Path) -> String {
  format!(
    "jq -Rs '{{systemMessage: ., hookSpecificOutput:{{hookEventName:\"SessionStart\", additionalContext: .}}}}' {}",
    dir.join(TODO_FILE).display()
  )
}

fn settings_path(dir: &std::path::Path) -> PathBuf {
  dir.join(".claude").join("settings.json")
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

fn todo_state_in(paths: &Paths, name: &str) -> Result<bool, String> {
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

fn set_todo_in(paths: &Paths, name: &str, enabled: bool) -> Result<(), String> {
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
  fs::write(&sp, raw + "\n").map_err(|e| format!("{}: {e}", sp.display()))
}

/// "~/x" relativ zum Home auflösen; alles andere unverändert.
fn expand_home(paths: &Paths, p: &str) -> PathBuf {
  match p.strip_prefix("~/") {
    Some(rest) => paths.home.join(rest),
    None => PathBuf::from(p),
  }
}

/// Legt ein Projekt nach dem Muster der bestehenden an: memory/, .gitignore
/// (Sentinel), .claude/settings.json (autoMemoryDirectory, Permissions,
/// Berechtigungen), ai-control.json mit Pool/Terminal-Config, Registry-Eintrag.
/// Ohne Zielordner landet das Projekt unter ~/claude-projects/<name>.
fn create_project_full_in(
  paths: &Paths,
  name: &str,
  dir: Option<&str>,
  pool: Option<&str>,
  work_dir: Option<&str>,
  create_work_dir: bool,
  terminal: TerminalConfig,
  todo: bool,
) -> Result<(), String> {
  check_name(name)?;
  let dir = match dir {
    Some(d) => expand_home(paths, d),
    None => paths.projects_dir().join(name),
  };
  let mut reg = load_registry(paths)?;
  if reg.contains_key(name) {
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

  fs::create_dir_all(dir.join(".claude")).map_err(|e| e.to_string())?;
  fs::create_dir_all(dir.join("memory")).map_err(|e| e.to_string())?;
  fs::write(dir.join(".gitignore"), ".ai-control-running\n").map_err(|e| e.to_string())?;

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
  fs::write(dir.join(".claude").join("settings.json"), raw + "\n").map_err(|e| e.to_string())?;

  reg.insert(name.to_string(), dir.clone());
  save_registry(paths, &reg)?;
  let cfg = ProjectConfig { pool: pool.map(str::to_string), terminal };
  if cfg.pool.is_some() || !cfg.terminal.is_empty() {
    write_project_config_in(paths, name, &cfg)?;
  }
  if todo {
    set_todo_in(paths, name, true)?;
  }
  Ok(())
}

/// Verlegt den Projektordner: Registry-Eintrag auf den neuen Pfad; Verweise
/// auf den alten Root in der settings.json (autoMemoryDirectory,
/// Edit-Permission) ziehen mit. Verschoben wird nichts — der neue Ordner
/// muss existieren.
fn set_project_dir_in(paths: &Paths, name: &str, dir: &str) -> Result<(), String> {
  check_name(name)?;
  let new_dir = expand_home(paths, dir);
  if !new_dir.is_dir() {
    return Err(format!("Ordner nicht gefunden: {}", new_dir.display()));
  }
  let mut reg = load_registry(paths)?;
  let old_dir = reg
    .get(name)
    .cloned()
    .ok_or_else(|| format!("Projekt nicht registriert: {name}"))?;
  if old_dir == new_dir {
    return Ok(());
  }
  reg.insert(name.to_string(), new_dir.clone());
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
  fs::write(&sp, raw + "\n").map_err(|e| format!("{}: {e}", sp.display()))
}

/// Nimmt einen bestehenden Ordner als Projekt auf; der Name ist der Ordnername.
fn add_project_in(paths: &Paths, path: &str) -> Result<(), String> {
  let dir = expand_home(paths, path);
  if !dir.is_dir() {
    return Err(format!("Ordner nicht gefunden: {}", dir.display()));
  }
  let name = dir
    .file_name()
    .ok_or_else(|| format!("kein Ordnername: {}", dir.display()))?
    .to_string_lossy()
    .into_owned();
  check_name(&name)?;
  register_project(paths, &name, &dir)
}

#[cfg(test)]
fn create_project_in(paths: &Paths, name: &str) -> Result<(), String> {
  check_name(name)?;
  let dir = paths.projects_dir().join(name);
  if dir.exists() {
    return Err(format!("Projekt existiert bereits: {name}"));
  }
  fs::create_dir_all(dir.join(".claude")).map_err(|e| e.to_string())?;
  register_project(paths, name, &dir)
}

/// Trägt einen Arbeitsordner in die Projekt-settings.json ein — wie im
/// Wizard: permissions.additionalDirectories + Edit-Permission. Legt die
/// Datei an, wenn sie fehlt (importierte Projekte).
fn add_work_dir_in(paths: &Paths, name: &str, dir: &str) -> Result<(), String> {
  check_name(name)?;
  let wd_path = expand_home(paths, dir);
  if !wd_path.is_dir() {
    return Err(format!("Arbeitsverzeichnis fehlt: {}", wd_path.display()));
  }
  let dir = contract_home(paths, &wd_path);
  let sp = settings_path(&project_dir(paths, name)?);
  let mut v: serde_json::Value = if sp.is_file() {
    let raw = fs::read_to_string(&sp).map_err(|e| format!("{}: {e}", sp.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?
  } else {
    serde_json::json!({})
  };
  let root = v.as_object_mut().ok_or("settings.json ist kein Objekt")?;
  let perms = root
    .entry("permissions")
    .or_insert_with(|| serde_json::json!({}))
    .as_object_mut()
    .ok_or("permissions ist kein Objekt")?;
  let dirs = perms
    .entry("additionalDirectories")
    .or_insert_with(|| serde_json::json!([]))
    .as_array_mut()
    .ok_or("additionalDirectories ist kein Array")?;
  if dirs.iter().any(|d| d.as_str() == Some(&dir)) {
    return Err(format!("schon eingetragen: {dir}"));
  }
  dirs.push(serde_json::json!(dir));
  let allow = perms
    .entry("allow")
    .or_insert_with(|| serde_json::json!([]))
    .as_array_mut()
    .ok_or("allow ist kein Array")?;
  allow.insert(0, serde_json::json!(format!("Edit({dir}/**)")));
  let parent = sp.parent().ok_or("settings.json ohne Elternordner")?;
  fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  fs::write(&sp, raw + "\n").map_err(|e| format!("{}: {e}", sp.display()))
}

/// Nimmt einen Arbeitsordner wieder raus: additionalDirectories-Eintrag und
/// zugehörige Edit-Permission. Der Ordner selbst bleibt.
fn remove_work_dir_in(paths: &Paths, name: &str, dir: &str) -> Result<(), String> {
  check_name(name)?;
  let sp = settings_path(&project_dir(paths, name)?);
  let raw = fs::read_to_string(&sp).map_err(|e| format!("{}: {e}", sp.display()))?;
  let mut v: serde_json::Value =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?;
  let perms = v["permissions"]
    .as_object_mut()
    .ok_or("permissions ist kein Objekt")?;
  let dirs = perms["additionalDirectories"]
    .as_array_mut()
    .ok_or("additionalDirectories ist kein Array")?;
  let before = dirs.len();
  dirs.retain(|d| d.as_str() != Some(dir));
  if dirs.len() == before {
    return Err(format!("nicht eingetragen: {dir}"));
  }
  if let Some(allow) = perms.get_mut("allow").and_then(|a| a.as_array_mut()) {
    allow.retain(|p| p.as_str() != Some(&format!("Edit({dir}/**)")));
  }
  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  fs::write(&sp, raw + "\n").map_err(|e| format!("{}: {e}", sp.display()))
}

/// Arbeitsordner des Projekts: additionalDirectories aus der Projekt-settings.json.
fn project_work_dirs_in(paths: &Paths, name: &str) -> Result<Vec<String>, String> {
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

/// Löscht den Projektordner und den Registry-Eintrag; optional auch die
/// verknüpften Arbeitsordner. Bei laufender Session wird abgebrochen.
fn delete_project_in(paths: &Paths, name: &str, delete_work_dirs: bool) -> Result<(), String> {
  check_name(name)?;
  let dir = project_dir(paths, name)?;
  if !dir.is_dir() {
    return Err(format!("Projekt nicht gefunden: {name}"));
  }
  if delete_work_dirs {
    for wd in project_work_dirs_in(paths, name)? {
      let wd_path = expand_home(paths, &wd);
      fs::remove_dir_all(&wd_path).map_err(|e| format!("{}: {e}", wd_path.display()))?;
    }
  }
  fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
  unregister_project(paths, name)
}

fn read_project_config_in(paths: &Paths, project: &str) -> Result<ProjectConfig, String> {
  let cfg_path = project_config_path(paths, project)?;
  if !cfg_path.is_file() {
    return Ok(ProjectConfig::default());
  }
  let raw =
    fs::read_to_string(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
  serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg_path.display()))
}

fn write_project_config_in(
  paths: &Paths,
  project: &str,
  cfg: &ProjectConfig,
) -> Result<(), String> {
  let raw = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
  fs::write(project_config_path(paths, project)?, raw + "\n").map_err(|e| e.to_string())
}

fn assign_pool_in(paths: &Paths, project: &str, pool: &str) -> Result<(), String> {
  if !paths.pool_dir(pool).join(POOL_FILE).is_file() {
    return Err(format!("Pool existiert nicht: {pool}"));
  }
  let mut cfg = read_project_config_in(paths, project)?;
  cfg.pool = Some(pool.to_string());
  write_project_config_in(paths, project, &cfg)
}

/// Nimmt den Pool raus; die Config-Datei bleibt nur, wenn sie noch
/// Terminal-Einstellungen trägt.
fn unassign_pool_in(paths: &Paths, project: &str) -> Result<(), String> {
  let mut cfg = read_project_config_in(paths, project)?;
  cfg.pool = None;
  if cfg.terminal.is_empty() {
    let cfg_path = project_config_path(paths, project)?;
    return fs::remove_file(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()));
  }
  write_project_config_in(paths, project, &cfg)
}

fn set_terminal_config_in(
  paths: &Paths,
  project: &str,
  mut terminal: TerminalConfig,
) -> Result<(), String> {
  let mut cfg = read_project_config_in(paths, project)?;
  // Absolut gewählte Icons ins gemeinsame Icons-Verzeichnis kopieren und als
  // Dateiname speichern — Icons gehören zur App-Config, nicht ins Quell-Repo.
  if let Some(icon) = terminal.icon.as_deref() {
    if icon.starts_with('/') {
      let src = PathBuf::from(icon);
      let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
      let name = format!("{project}.{ext}");
      let icons = paths.icons_dir();
      fs::create_dir_all(&icons).map_err(|e| format!("{}: {e}", icons.display()))?;
      let dest = icons.join(&name);
      if src != dest {
        fs::copy(&src, &dest).map_err(|e| format!("{}: {e}", src.display()))?;
      }
      terminal.icon = Some(name);
    }
  }
  cfg.terminal = terminal;
  write_project_config_in(paths, project, &cfg)
}

/// Icon-Pfad einer Projekt-Config auflösen: relative Namen liegen im
/// gemeinsamen Icons-Verzeichnis der App.
fn resolve_icon_path(paths: &Paths, icon: &str) -> PathBuf {
  if icon.starts_with('/') {
    PathBuf::from(icon)
  } else {
    paths.icons_dir().join(icon)
  }
}

/// Icon eines Projekts als PNG-data-URL für die Übersicht; ICNS wird per
/// sips nach PNG konvertiert, weil der Browser ICNS nicht rendert.
#[tauri::command]
fn project_icon(project: String) -> Result<Option<String>, String> {
  use base64::{engine::general_purpose::STANDARD, Engine};
  let paths = Paths::real();
  let Some(icon) = read_project_config_in(&paths, &project)?.terminal.icon else {
    return Ok(None);
  };
  let path = resolve_icon_path(&paths, &icon);
  let is_icns = path
    .extension()
    .and_then(|e| e.to_str())
    .is_some_and(|e| e.eq_ignore_ascii_case("icns"));
  let png = if is_icns {
    let tmp = std::env::temp_dir().join(format!("ai-control-icon-{project}.png"));
    let out = std::process::Command::new("sips")
      .args(["-s", "format", "png"])
      .arg(&path)
      .arg("--out")
      .arg(&tmp)
      .output()
      .map_err(|e| format!("sips: {e}"))?;
    if !out.status.success() {
      return Err(format!("sips: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let bytes = fs::read(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let _ = fs::remove_file(&tmp);
    bytes
  } else {
    fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?
  };
  Ok(Some(format!("data:image/png;base64,{}", STANDARD.encode(png))))
}

/// Menü-Icon fürs Tray: ganz links der Status-Punkt (grün = läuft), daneben
/// das Projekt-Icon (sips konvertiert, auch ICNS). Ein RGBA-Bild 56×36 px;
/// muda rendert Menü-Icons mit 18 pt Höhe und proportionaler Breite.
fn menu_icon(
  paths: &Paths,
  project: &str,
  running: bool,
) -> Result<tauri::image::Image<'static>, String> {
  const H: usize = 36;
  const DOT_W: usize = 20;
  const W: usize = DOT_W + H;
  let mut canvas = vec![0u8; W * H * 4];
  if running {
    // macOS-Systemgrün, weicher Rand über Alpha.
    let (cx, cy, r) = (10.0f32, 18.0f32, 7.0f32);
    for y in 0..H {
      for x in 0..DOT_W {
        let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
        let a = (r - d + 0.5).clamp(0.0, 1.0);
        if a > 0.0 {
          let i = (y * W + x) * 4;
          canvas[i..i + 4].copy_from_slice(&[52, 199, 89, (a * 255.0) as u8]);
        }
      }
    }
  }
  if let Some(icon) = read_project_config_in(paths, project)?.terminal.icon {
    // Nicht ladbares Icon blockiert den Tray nicht (Muster set_dock_icon):
    // Meldung, Eintrag erscheint ohne Projekt-Icon.
    match project_icon_rgba_36(paths, project, &icon) {
      Ok(img) => {
        let (iw, ih) = (img.width() as usize, img.height() as usize);
        let rgba = img.rgba();
        for y in 0..ih.min(H) {
          for x in 0..iw.min(H) {
            let src_i = (y * iw + x) * 4;
            let dst_i = (y * W + DOT_W + x) * 4;
            canvas[dst_i..dst_i + 4].copy_from_slice(&rgba[src_i..src_i + 4]);
          }
        }
      }
      Err(e) => eprintln!("Tray-Icon {project}: {e}"),
    }
  }
  Ok(tauri::image::Image::new_owned(canvas, W as u32, H as u32))
}

/// Projekt-Icon als 36×36-RGBA (sips konvertiert, auch ICNS).
fn project_icon_rgba_36(
  paths: &Paths,
  project: &str,
  icon: &str,
) -> Result<tauri::image::Image<'static>, String> {
  let src = resolve_icon_path(paths, icon);
  let tmp = std::env::temp_dir().join(format!("ai-control-tray-icon-{project}.png"));
  let out = Command::new("sips")
    .args(["-s", "format", "png", "-z", "36", "36"])
    .arg(&src)
    .arg("--out")
    .arg(&tmp)
    .output()
    .map_err(|e| format!("sips: {e}"))?;
  if !out.status.success() {
    return Err(format!("sips: {}", String::from_utf8_lossy(&out.stderr)));
  }
  let bytes = fs::read(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
  let _ = fs::remove_file(&tmp);
  tauri::image::Image::from_bytes(&bytes).map_err(|e| e.to_string())
}

/// Terminal-Einstellungen eines Projekts, für den Terminal-Prozess.
pub(crate) fn terminal_config(project: &str) -> Result<TerminalConfig, String> {
  Ok(read_project_config_in(&Paths::real(), project)?.terminal)
}

/// Namen aller Projekte, die `pool` zugeordnet haben.
fn projects_using_pool(paths: &Paths, pool: &str) -> Result<Vec<String>, String> {
  let mut users = Vec::new();
  for (name, dir) in load_registry(paths)? {
    let cfg_path = dir.join(PROJECT_FILE);
    if !cfg_path.is_file() {
      continue;
    }
    let raw = fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
    let cfg: ProjectConfig = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if cfg.pool.as_deref() == Some(pool) {
      users.push(name);
    }
  }
  Ok(users)
}

// ---------- Pools ----------

/// Ablage der API-Keys: macOS-Keychain / Linux-Secret-Service (keyring-Crate).
/// Die Key-Datei im Pool-Ordner bleibt Fallback, wenn der Store beim Schreiben
/// nicht verfügbar ist.
trait ApikeyStore {
  fn set(&self, pool: &str, key: &str) -> Result<(), String>;
  fn has(&self, pool: &str) -> Result<bool, String>;
  fn delete(&self, pool: &str) -> Result<(), String>;
}

/// Service-Name der Einträge; Account ist die Pool-ID. Unter Linux legt die
/// keyring-Crate die Attribute service/username an — der apiKeyHelper liest
/// mit denselben Attributen über secret-tool.
const APIKEY_SERVICE: &str = "ai-control-apikey";

/// Fehler-Sentinel an die UI: Store nicht verfügbar und Datei-Ablage (noch)
/// nicht erlaubt — die UI fragt dann nach und wiederholt mit allow_file.
const KEYCHAIN_UNAVAILABLE: &str = "keychain-unavailable";

struct KeychainStore;

/// macOS über das security-CLI: dessen Einträge tragen /usr/bin/security in
/// der ACL, der apiKeyHelper (liest ebenfalls per security-CLI beim
/// claude-Start) kommt dadurch ohne Keychain-Prompt an den Key. Über das
/// Security-Framework angelegte Einträge (keyring-Crate) würden beim Lesen
/// durchs CLI prompten.
#[cfg(target_os = "macos")]
impl ApikeyStore for KeychainStore {
  fn set(&self, pool: &str, key: &str) -> Result<(), String> {
    let out = Command::new("security")
      .args(["add-generic-password", "-U", "-s", APIKEY_SERVICE, "-a", pool, "-w", key])
      .output()
      .map_err(|e| e.to_string())?;
    if out.status.success() {
      Ok(())
    } else {
      Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
  }

  fn has(&self, pool: &str) -> Result<bool, String> {
    // Ohne -w: nur Attribute, kein Secret — kein ACL-Prompt möglich.
    let out = Command::new("security")
      .args(["find-generic-password", "-s", APIKEY_SERVICE, "-a", pool])
      .output()
      .map_err(|e| e.to_string())?;
    Ok(out.status.success())
  }

  fn delete(&self, pool: &str) -> Result<(), String> {
    // Fehlender Eintrag ist kein Fehler — gelöscht ist gelöscht.
    Command::new("security")
      .args(["delete-generic-password", "-s", APIKEY_SERVICE, "-a", pool])
      .output()
      .map_err(|e| e.to_string())?;
    Ok(())
  }
}

#[cfg(not(target_os = "macos"))]
impl KeychainStore {
  fn entry(pool: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(APIKEY_SERVICE, pool).map_err(|e| e.to_string())
  }
}

#[cfg(not(target_os = "macos"))]
impl ApikeyStore for KeychainStore {
  fn set(&self, pool: &str, key: &str) -> Result<(), String> {
    Self::entry(pool)?.set_password(key).map_err(|e| e.to_string())
  }

  fn has(&self, pool: &str) -> Result<bool, String> {
    match Self::entry(pool)?.get_password() {
      Ok(_) => Ok(true),
      // Kein Eintrag oder kein Store verfügbar → die Key-Datei entscheidet.
      Err(
        keyring::Error::NoEntry
        | keyring::Error::PlatformFailure(_)
        | keyring::Error::NoStorageAccess(_),
      ) => Ok(false),
      Err(e) => Err(e.to_string()),
    }
  }

  fn delete(&self, pool: &str) -> Result<(), String> {
    match Self::entry(pool)?.delete_credential() {
      Ok(())
      | Err(
        keyring::Error::NoEntry
        | keyring::Error::PlatformFailure(_)
        | keyring::Error::NoStorageAccess(_),
      ) => Ok(()),
      Err(e) => Err(e.to_string()),
    }
  }
}

/// apiKeyHelper-Kommando eines apikey-Pools: liest den Key aus dem
/// Keychain/Keyring, bei fehlendem Eintrag aus der Key-Datei.
fn apikey_helper_command(dir: &std::path::Path, pool_id: &str) -> String {
  let file = dir.join(APIKEY_FILE);
  if cfg!(target_os = "macos") {
    format!(
      "security find-generic-password -w -s {APIKEY_SERVICE} -a {pool_id} 2>/dev/null || cat '{}'",
      file.display()
    )
  } else {
    format!(
      "secret-tool lookup service {APIKEY_SERVICE} username {pool_id} 2>/dev/null || cat '{}'",
      file.display()
    )
  }
}

#[derive(Serialize)]
struct PoolInfo {
  /// Ordnername unter pools/ (bei Neuanlagen eine UUID) — stabile ID, an der
  /// Keychain-Suffix, Symlinks und Projekt-Zuordnungen hängen.
  id: String,
  /// Anzeigename aus pool.json, frei umbenennbar.
  name: String,
  #[serde(rename = "credentialType")]
  credential_type: String,
  projects: Vec<String>,
  /// Teilmenge von `projects`, die gerade läuft (dasselbe `is_running` wie die
  /// Projektliste). Der Löschen-Dialog sperrt darauf.
  running: Vec<String>,
  #[serde(rename = "hasCredentials")]
  has_credentials: bool,
}

fn list_pools_in(paths: &Paths, store: &dyn ApikeyStore) -> Result<Vec<PoolInfo>, String> {
  let mut pools = Vec::new();
  let entries = fs::read_dir(paths.pools_dir()).map_err(|e| e.to_string())?;
  for entry in entries {
    let entry = entry.map_err(|e| e.to_string())?;
    let cfg_path = entry.path().join(POOL_FILE);
    if !cfg_path.is_file() {
      continue;
    }
    let raw = fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
    let pool: Pool =
      serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
    let id = entry.file_name().to_string_lossy().into_owned();
    // oauth: Credentials liegen in claudes eigenem Keychain-Eintrag, dessen
    // Prüfung wäre ein security-Aufruf pro Pool im 3-s-Polling — immer true.
    // apikey: Store-Eintrag (nativer API-Call) oder Fallback-Datei.
    let has_credentials = match pool.credential_type.as_str() {
      "apikey" => {
        store.has(&id)?
          || fs::read_to_string(entry.path().join(APIKEY_FILE))
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
      }
      _ => true,
    };
    let projects = projects_using_pool(paths, &id)?;
    let running = projects.iter().filter(|p| is_running(p)).cloned().collect();
    pools.push(PoolInfo {
      id,
      projects,
      running,
      name: pool.name,
      credential_type: pool.credential_type,
      has_credentials,
    });
  }
  pools.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(pools)
}

/// (ID, Anzeigename) aller Pools. Beim ersten Pool existiert pools/ noch
/// nicht — dann ist die Liste leer.
fn pool_names(paths: &Paths) -> Result<Vec<(String, String)>, String> {
  let mut out = Vec::new();
  if !paths.pools_dir().is_dir() {
    return Ok(out);
  }
  for entry in fs::read_dir(paths.pools_dir()).map_err(|e| e.to_string())? {
    let entry = entry.map_err(|e| e.to_string())?;
    let cfg_path = entry.path().join(POOL_FILE);
    if !cfg_path.is_file() {
      continue;
    }
    let raw = fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
    let pool: Pool =
      serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
    out.push((entry.file_name().to_string_lossy().into_owned(), pool.name));
  }
  Ok(out)
}

/// Prüft den Anzeigenamen (gültig + noch nicht vergeben) und liefert den
/// Ordner für einen neuen Pool: pools/<UUID v4>.
fn check_new_pool(paths: &Paths, name: &str) -> Result<PathBuf, String> {
  check_name(name)?;
  if pool_names(paths)?.iter().any(|(_, n)| n == name) {
    return Err(format!("Pool existiert bereits: {name}"));
  }
  Ok(paths.pool_dir(&uuid::Uuid::new_v4().to_string()))
}

fn write_pool_json(dir: &PathBuf, name: &str, credential_type: &str) -> Result<(), String> {
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let pool = Pool {
    name: name.to_string(),
    credential_type: credential_type.to_string(),
  };
  let raw = serde_json::to_string_pretty(&pool).map_err(|e| e.to_string())?;
  fs::write(dir.join(POOL_FILE), raw + "\n").map_err(|e| e.to_string())
}

fn write_secret_file(path: &PathBuf, content: &str) -> Result<(), String> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  fs::write(path, content).map_err(|e| e.to_string())?;
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
}

/// Grundausstattung eines Pool-Ordners (= CLAUDE_CONFIG_DIR): settings.json
/// (aufgeräumte UI-Defaults + `extra`) und eine CLAUDE.md, die claude als
/// User-Scope liest. CLAUDE.md wird nur angelegt, wenn sie fehlt.
/// Die Prompt-Vorschläge/Rückkehr-Zusammenfassung werden abgeschaltet — sonst
/// erscheint oben die Vorschlagstabelle.
fn init_pool_config(
  dir: &PathBuf,
  extra: serde_json::Value,
) -> Result<(), String> {
  let mut settings = serde_json::json!({
    "promptSuggestionEnabled": false,
    "awaySummaryEnabled": false,
  });
  let base = settings.as_object_mut().unwrap();
  if let Some(obj) = extra.as_object() {
    for (k, v) in obj {
      base.insert(k.clone(), v.clone());
    }
  }
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
  fs::write(dir.join("settings.json"), raw + "\n").map_err(|e| e.to_string())?;
  let claude_md = dir.join("CLAUDE.md");
  if !claude_md.is_file() {
    fs::write(&claude_md, "").map_err(|e| e.to_string())?;
  }
  Ok(())
}

/// Runtime, die pro Pool ins synced Repo gelinkt wird: Transkripte, Todos,
/// Prompt-Historie. (name, ist_ordner)
const SYNCED_RUNTIME: [(&str, bool); 3] =
  [("projects", true), ("todos", true), ("history.jsonl", false)];

/// Sync-Ziel für Pool-Laufzeitdaten (settings.json: poolSyncDir).
/// Ungesetzt = Feature aus, alle Daten bleiben lokal im Pool-Ordner.
fn pool_sync_dir(paths: &Paths) -> Option<PathBuf> {
  fs::read_to_string(paths.config_dir().join(APP_SETTINGS_FILE))
    .ok()
    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    .and_then(|v| v["poolSyncDir"].as_str().map(|d| expand_home(paths, d)))
}

/// Zielort der synced Runtime-Daten eines Pools unterhalb von poolSyncDir.
fn pool_data_dir(paths: &Paths, pool: &str) -> Result<PathBuf, String> {
  pool_sync_dir(paths)
    .map(|d| d.join(pool))
    .ok_or_else(|| "poolSyncDir ist nicht konfiguriert".to_string())
}

/// Ersetzt im Pool-Ordner projects/todos/history.jsonl durch Symlinks auf den
/// konfigurierten Sync-Ordner. Die Symlinks sind maschinenlokal, die Daten
/// reisen über den Sync des Zielordners (z. B. git).
/// Vorhandene echte Inhalte werden verworfen (kein History-Erhalt — bewusst).
fn link_pool_runtime_in(paths: &Paths, pool: &str) -> Result<(), String> {
  check_name(pool)?;
  let src = paths.pool_dir(pool);
  let data = pool_data_dir(paths, pool)?;
  for (name, is_dir) in SYNCED_RUNTIME {
    let target = data.join(name);
    if is_dir {
      fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    } else {
      fs::create_dir_all(&data).map_err(|e| e.to_string())?;
      if !target.exists() {
        fs::write(&target, "").map_err(|e| e.to_string())?;
      }
    }
    let link = src.join(name);
    if link.is_dir() && !link.is_symlink() {
      fs::remove_dir_all(&link).map_err(|e| e.to_string())?;
    } else if link.is_symlink() || link.exists() {
      fs::remove_file(&link).map_err(|e| e.to_string())?;
    }
    std::os::unix::fs::symlink(&target, &link).map_err(|e| e.to_string())?;
  }
  Ok(())
}

/// Legt einen apikey-Pool an: Key in den Keychain/Keyring (Datei 0600 nur mit
/// allow_file), settings.json mit apiKeyHelper-Kette, CLAUDE.md, pool.json.
/// Ohne Store und ohne allow_file bricht die Anlage ab, bevor etwas entsteht.
/// Liefert die Pool-ID.
fn create_apikey_pool_in(
  paths: &Paths,
  store: &dyn ApikeyStore,
  name: &str,
  key: &str,
  allow_file: bool,
) -> Result<String, String> {
  let dir = check_new_pool(paths, name)?;
  let key = key.trim();
  if key.is_empty() {
    return Err("leerer API-Key".into());
  }
  let id = dir.file_name().unwrap().to_string_lossy().into_owned();
  if store.set(&id, key).is_err() {
    if !allow_file {
      return Err(KEYCHAIN_UNAVAILABLE.into());
    }
    write_secret_file(&dir.join(APIKEY_FILE), &format!("{key}\n"))?;
  }
  init_pool_config(
    &dir,
    serde_json::json!({ "apiKeyHelper": apikey_helper_command(&dir, &id) }),
  )?;
  write_pool_json(&dir, name, "apikey")?;
  if pool_sync_dir(paths).is_some() {
    link_pool_runtime_in(paths, &id)?;
  }
  Ok(id)
}

/// Legt einen oauth-Pool an: Grundausstattung (leere settings.json + CLAUDE.md)
/// + pool.json. Die Anmeldung macht claude selbst beim ersten Start des Pools
/// (`/login`) und legt den Keychain-Eintrag an — die App speichert keine Tokens.
/// Liefert die Pool-ID.
fn create_oauth_pool_in(paths: &Paths, name: &str) -> Result<String, String> {
  let dir = check_new_pool(paths, name)?;
  init_pool_config(&dir, serde_json::json!({}))?;
  write_pool_json(&dir, name, "oauth")?;
  let id = dir.file_name().unwrap().to_string_lossy().into_owned();
  if pool_sync_dir(paths).is_some() {
    link_pool_runtime_in(paths, &id)?;
  }
  Ok(id)
}

/// Setzt den Anzeigenamen eines Pools — reines pool.json-Update, ID/Ordner
/// (und damit Keychain-Suffix, Symlinks, Zuordnungen) bleiben unverändert.
fn rename_pool_in(paths: &Paths, pool: &str, name: &str) -> Result<(), String> {
  check_name(name)?;
  let current = read_pool(paths, pool)?;
  if pool_names(paths)?.iter().any(|(id, n)| id != pool && n == name) {
    return Err(format!("Pool existiert bereits: {name}"));
  }
  write_pool_json(&paths.pool_dir(pool), name, &current.credential_type)
}

/// Löscht einen Pool samt Ordner (inkl. Credentials, bei apikey auch den
/// Keychain-Eintrag). Zugeordnete Projekte verlieren die Zuordnung
/// (Terminal-Einstellungen bleiben erhalten). Den Schutz gegen laufende
/// Sessions setzt der delete_pool-Command davor.
fn delete_pool_in(paths: &Paths, store: &dyn ApikeyStore, name: &str) -> Result<(), String> {
  let dir = paths.pool_dir(name);
  if !dir.join(POOL_FILE).is_file() {
    return Err(format!("Pool nicht gefunden: {name}"));
  }
  if read_pool(paths, name)?.credential_type == "apikey" {
    store.delete(name)?;
  }
  for project in projects_using_pool(paths, name)? {
    unassign_pool_in(paths, &project)?;
  }
  fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Schreibt den API-Key eines apikey-Pools neu: in den Keychain/Keyring, die
/// Fallback-Datei wird dabei entfernt (migriert Datei-Pools beim Key-Ändern).
/// Ohne verfügbaren Store: mit allow_file in die Datei (0600), sonst Abbruch
/// ohne Änderung. Der apiKeyHelper wird auf die aktuelle Kette gehoben.
fn set_apikey_in(
  paths: &Paths,
  store: &dyn ApikeyStore,
  pool: &str,
  key: &str,
  allow_file: bool,
) -> Result<(), String> {
  let p = read_pool(paths, pool)?;
  if p.credential_type != "apikey" {
    return Err(format!("Pool {pool} ist kein apikey-Pool"));
  }
  let key = key.trim();
  if key.is_empty() {
    return Err("leerer API-Key".into());
  }
  let dir = paths.pool_dir(pool);
  let key_path = dir.join(APIKEY_FILE);
  if store.set(pool, key).is_ok() {
    if key_path.is_file() {
      fs::remove_file(&key_path).map_err(|e| e.to_string())?;
    }
  } else {
    if !allow_file {
      return Err(KEYCHAIN_UNAVAILABLE.into());
    }
    write_secret_file(&key_path, &format!("{key}\n"))?;
  }
  let settings_path = dir.join("settings.json");
  let raw = fs::read_to_string(&settings_path)
    .map_err(|e| format!("{}: {e}", settings_path.display()))?;
  let mut settings: serde_json::Value = serde_json::from_str(&raw)
    .map_err(|e| format!("{}: {e}", settings_path.display()))?;
  settings["apiKeyHelper"] = serde_json::json!(apikey_helper_command(&dir, pool));
  let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
  fs::write(&settings_path, raw + "\n").map_err(|e| e.to_string())
}

fn ensure_oauth_pool(paths: &Paths, pool: &str) -> Result<(), String> {
  let p = read_pool(paths, pool)?;
  if p.credential_type != "oauth" {
    return Err(format!("Pool {pool} ist kein oauth-Pool"));
  }
  Ok(())
}

// ---------- Verbrauch ----------

#[derive(Serialize, Default)]
struct UsageTotals {
  #[serde(rename = "inputTokens")]
  input_tokens: u64,
  #[serde(rename = "outputTokens")]
  output_tokens: u64,
  #[serde(rename = "cacheCreationTokens")]
  cache_creation_tokens: u64,
  #[serde(rename = "cacheReadTokens")]
  cache_read_tokens: u64,
  #[serde(rename = "costUsd")]
  cost_usd: f64,
}

#[derive(Serialize)]
struct UsageRow {
  pool: String,
  project: String,
  #[serde(flatten)]
  totals: UsageTotals,
}

/// USD pro MTok (input, output, cache_write, cache_read).
/// Cache: write 1.25× / read 0.1× des Input-Preises.
fn model_rates(model: &str) -> (f64, f64, f64, f64) {
  let (input, output) = if model.contains("fable") || model.contains("mythos") {
    (10.0, 50.0)
  } else if model.contains("opus") {
    (5.0, 25.0)
  } else if model.contains("haiku") {
    (1.0, 5.0)
  } else {
    // sonnet und Unbekanntes
    (3.0, 15.0)
  };
  (input, output, input * 1.25, input * 0.1)
}

/// Tage seit Unix-Epoche für ein Kalenderdatum (Howard Hinnant, days_from_civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
  let y = if m <= 2 { y - 1 } else { y };
  let era = if y >= 0 { y } else { y - 399 } / 400;
  let yoe = y - era * 400;
  let mp = (m + 9) % 12;
  let doy = (153 * mp + 2) / 5 + d - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era * 146097 + doe - 719468
}

/// "YYYY-MM-DDTHH:MM:SS(.mmm)Z" (UTC) → Unix-Sekunden.
fn parse_ts(ts: &str) -> Option<i64> {
  if ts.len() < 19 {
    return None;
  }
  let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
  let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
  let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
  Some(days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + s)
}

/// Projektpfad so kodieren, wie claude die Transcript-Ordner benennt
/// (alles außer [a-zA-Z0-9] wird '-').
fn encode_project_path(path: &str) -> String {
  path
    .chars()
    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
    .collect()
}

/// Eine Transcript-Zeile auswerten: nur Zeilen mit message.usage zählen,
/// Zeitfilter, Dedup über message.id+requestId (Retries doppeln sonst).
fn add_usage_line(
  line: &str,
  cutoff: i64,
  seen: &mut HashSet<String>,
  totals: &mut UsageTotals,
) {
  let v: serde_json::Value = match serde_json::from_str(line) {
    Ok(v) => v,
    // Nicht-JSON (z. B. angeschnittene letzte Zeile einer laufenden Session)
    Err(_) => return,
  };
  let usage = &v["message"]["usage"];
  if !usage.is_object() {
    return;
  }
  match v["timestamp"].as_str().and_then(parse_ts) {
    Some(t) if t >= cutoff => {}
    _ => return,
  }
  if let Some(id) = v["message"]["id"].as_str() {
    let key = format!("{id}:{}", v["requestId"].as_str().unwrap_or(""));
    if !seen.insert(key) {
      return;
    }
  }
  let model = v["message"]["model"].as_str().unwrap_or("");
  if model == "<synthetic>" {
    return;
  }
  let input = usage["input_tokens"].as_u64().unwrap_or(0);
  let output = usage["output_tokens"].as_u64().unwrap_or(0);
  let cache_write = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
  let cache_read = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
  let (ri, ro, rw, rr) = model_rates(model);
  totals.input_tokens += input;
  totals.output_tokens += output;
  totals.cache_creation_tokens += cache_write;
  totals.cache_read_tokens += cache_read;
  totals.cost_usd += input as f64 / 1e6 * ri
    + output as f64 / 1e6 * ro
    + cache_write as f64 / 1e6 * rw
    + cache_read as f64 / 1e6 * rr;
}

/// Aggregiert message.usage aus den Transcript-JSONLs aller Pools:
/// <pool>/projects/<kodierter-projektpfad>/*.jsonl → Pool × Projekt.
/// Sichtfenster ist, was claude noch nicht weggeräumt hat (cleanupPeriodDays).
fn usage_stats_in(paths: &Paths, days: u32) -> Result<Vec<UsageRow>, String> {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs() as i64;
  let cutoff = now - days as i64 * 86400;

  // kodierter Projektpfad → Projektname
  let mut names = HashMap::new();
  for (name, dir) in load_registry(paths)? {
    names.insert(encode_project_path(&dir.to_string_lossy()), name);
  }

  let mut seen = HashSet::new();
  let mut rows: HashMap<(String, String), UsageTotals> = HashMap::new();

  for pool_entry in fs::read_dir(paths.pools_dir()).map_err(|e| e.to_string())? {
    let pool_entry = pool_entry.map_err(|e| e.to_string())?;
    let id = pool_entry.file_name().to_string_lossy().into_owned();
    // Anzeigename aus pool.json; Ordner ohne pool.json unter der ID ausweisen.
    let pool = if pool_entry.path().join(POOL_FILE).is_file() {
      read_pool(paths, &id)?.name
    } else {
      id
    };
    let projects_root = pool_entry.path().join("projects");
    if !projects_root.is_dir() {
      continue;
    }
    for proj_entry in fs::read_dir(&projects_root).map_err(|e| e.to_string())? {
      let proj_entry = proj_entry.map_err(|e| e.to_string())?;
      if !proj_entry.path().is_dir() {
        continue;
      }
      let encoded = proj_entry.file_name().to_string_lossy().into_owned();
      // Unbekannte Ordner (Sessions außerhalb der Projekte) unter dem
      // kodierten Namen ausweisen statt verschlucken.
      let project = names.get(&encoded).cloned().unwrap_or(encoded);
      let totals = rows.entry((pool.clone(), project)).or_default();
      for file in fs::read_dir(proj_entry.path()).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
          continue;
        }
        let f = fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for line in BufReader::new(f).lines() {
          let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
          add_usage_line(&line, cutoff, &mut seen, totals);
        }
      }
    }
  }

  let mut out: Vec<UsageRow> = rows
    .into_iter()
    .map(|((pool, project), totals)| UsageRow { pool, project, totals })
    .collect();
  out.sort_by(|a, b| {
    a.pool
      .cmp(&b.pool)
      .then(b.totals.cost_usd.total_cmp(&a.totals.cost_usd))
  });
  Ok(out)
}

// ---------- Tauri-Commands ----------

#[tauri::command]
fn list_projects() -> Result<Vec<Project>, String> {
  list_projects_in(&Paths::real())
}

#[tauri::command]
fn create_project_full(
  name: String,
  dir: Option<String>,
  pool: Option<String>,
  work_dir: Option<String>,
  create_work_dir: bool,
  terminal: TerminalConfig,
  todo: bool,
) -> Result<(), String> {
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
fn add_project(path: String) -> Result<(), String> {
  add_project_in(&Paths::real(), &path)
}

/// Projekt aus der Registry nehmen; der Ordner bleibt unangetastet.
#[tauri::command]
fn remove_project(name: String) -> Result<(), String> {
  if is_running(&name) {
    return Err(format!("{name} läuft noch — erst beenden"));
  }
  unregister_project(&Paths::real(), &name)
}

/// Projektordner neu zuordnen; bei laufender Session gesperrt.
#[tauri::command]
fn set_project_dir(project: String, dir: String) -> Result<(), String> {
  if is_running(&project) {
    return Err(format!("{project} läuft noch — erst beenden"));
  }
  set_project_dir_in(&Paths::real(), &project, &dir)
}

#[tauri::command]
fn add_work_dir(project: String, dir: String) -> Result<(), String> {
  add_work_dir_in(&Paths::real(), &project, &dir)
}

#[tauri::command]
fn remove_work_dir(project: String, dir: String) -> Result<(), String> {
  remove_work_dir_in(&Paths::real(), &project, &dir)
}

#[tauri::command]
fn todo_state(project: String) -> Result<bool, String> {
  todo_state_in(&Paths::real(), &project)
}

#[tauri::command]
fn set_todo(project: String, enabled: bool) -> Result<(), String> {
  set_todo_in(&Paths::real(), &project, enabled)
}

#[tauri::command]
fn delete_project(name: String, delete_work_dirs: bool) -> Result<(), String> {
  if is_running(&name) {
    return Err(format!("{name} läuft noch — erst beenden"));
  }
  delete_project_in(&Paths::real(), &name, delete_work_dirs)
}

#[tauri::command]
fn project_work_dirs(project: String) -> Result<Vec<String>, String> {
  project_work_dirs_in(&Paths::real(), &project)
}

#[tauri::command]
fn list_pools() -> Result<Vec<PoolInfo>, String> {
  list_pools_in(&Paths::real(), &KeychainStore)
}

#[tauri::command]
fn create_oauth_pool(name: String) -> Result<String, String> {
  create_oauth_pool_in(&Paths::real(), &name)
}

#[tauri::command]
fn create_apikey_pool(name: String, key: String, allow_file: bool) -> Result<String, String> {
  create_apikey_pool_in(&Paths::real(), &KeychainStore, &name, &key, allow_file)
}

#[tauri::command]
fn rename_pool(pool: String, name: String) -> Result<(), String> {
  rename_pool_in(&Paths::real(), &pool, &name)
}

#[tauri::command]
fn delete_pool(pool: String) -> Result<(), String> {
  let paths = Paths::real();
  let running = running_projects_using_pool(&paths, &pool)?;
  if !running.is_empty() {
    return Err(format!(
      "Pool wird noch benutzt — läuft: {}",
      running.join(", ")
    ));
  }
  delete_pool_in(&paths, &KeychainStore, &pool)
}

#[tauri::command]
fn assign_pool(project: String, pool: String) -> Result<(), String> {
  assign_pool_in(&Paths::real(), &project, &pool)
}

#[tauri::command]
fn unassign_pool(project: String) -> Result<(), String> {
  unassign_pool_in(&Paths::real(), &project)
}

#[tauri::command]
fn usage_stats(days: u32) -> Result<Vec<UsageRow>, String> {
  usage_stats_in(&Paths::real(), days)
}

#[tauri::command]
fn set_terminal_config(
  project: String,
  theme: Option<String>,
  icon: Option<String>,
  title: Option<String>,
) -> Result<(), String> {
  set_terminal_config_in(&Paths::real(), &project, TerminalConfig { theme, icon, title })
}

#[tauri::command]
fn set_apikey(pool: String, key: String, allow_file: bool) -> Result<(), String> {
  set_apikey_in(&Paths::real(), &KeychainStore, &pool, &key, allow_file)
}

/// Keychain-Service-Name des Pools: claude legt pro CLAUDE_CONFIG_DIR einen
/// suffixierten Eintrag an, Suffix = erste 8 Hex-Zeichen von SHA-256 über
/// den Pool-Pfad.
fn keychain_service(paths: &Paths, pool: &str) -> String {
  let hash = Sha256::digest(paths.pool_dir(pool).to_string_lossy().as_bytes());
  let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
  format!("Claude Code-credentials-{}", &hex[..8])
}

fn keychain_entry_exists(service: &str) -> Result<bool, String> {
  let out = Command::new("security")
    .args(["find-generic-password", "-s", service])
    .output()
    .map_err(|e| e.to_string())?;
  Ok(out.status.success())
}

#[tauri::command]
fn keychain_status(pool: String) -> Result<bool, String> {
  keychain_entry_exists(&keychain_service(&Paths::real(), &pool))
}

/// Setzt einen oauth-Pool zurück: löscht den suffixierten Keychain-Eintrag,
/// damit claude beim nächsten Start des Pools erneut `/login` verlangt. Nur
/// bei ungenutztem Pool. Der Login selbst passiert in der Session, nicht hier.
#[tauri::command]
fn oauth_login(pool: String) -> Result<(), String> {
  let paths = Paths::real();
  ensure_oauth_pool(&paths, &pool)?;

  let running = running_projects_using_pool(&paths, &pool)?;
  if !running.is_empty() {
    return Err(format!(
      "Neuanmeldung nur bei ungenutztem Pool möglich — läuft: {}",
      running.join(", ")
    ));
  }

  let service = keychain_service(&paths, &pool);
  if keychain_entry_exists(&service)? {
    let out = Command::new("security")
      .args(["delete-generic-password", "-s", &service])
      .output()
      .map_err(|e| e.to_string())?;
    if !out.status.success() {
      return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
  }
  Ok(())
}

/// Beendet die Terminal-Prozesse eines Projekts per SIGTERM auf die exakte
/// PID. Der Prozesstod schließt den PTY-Master, claude bekommt HUP und endet —
/// wie beim Schließen des Fensters.
fn kill_terminals(project: &str) -> Result<(), String> {
  for pid in terminal_pids(project) {
    let out = Command::new("kill")
      .arg(pid.to_string())
      .output()
      .map_err(|e| e.to_string())?;
    if !out.status.success() {
      return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
  }
  Ok(())
}

#[tauri::command]
fn stop_project(project: String) -> Result<(), String> {
  if terminal_pids(&project).is_empty() {
    return Err(format!("{project} läuft nicht"));
  }
  kill_terminals(&project)
}

/// Beendet die laufenden Terminal-Prozesse, wartet auf ihr Ende und öffnet das
/// interne Terminal neu.
#[tauri::command]
fn restart_project(app: tauri::AppHandle, project: String) -> Result<(), String> {
  kill_terminals(&project)?;
  let deadline = Instant::now() + Duration::from_secs(30);
  while is_running(&project) {
    if Instant::now() > deadline {
      return Err(format!("{project} hat sich nach 30 s nicht beendet"));
    }
    std::thread::sleep(Duration::from_millis(250));
  }
  terminal::open_terminal(app, project)
}

#[tauri::command]
fn sync_setting() -> Result<bool, String> {
  Ok(sync_on_session_end(&Paths::real()))
}

#[tauri::command]
fn set_sync_setting(enabled: bool) -> Result<(), String> {
  set_sync_on_session_end_in(&Paths::real(), enabled)
}

/// Verlinkt die synced Runtime eines bestehenden Pools. Nur bei idlem Pool —
/// sonst würde das Transkript der laufenden Session ersetzt.
#[tauri::command]
fn link_pool_runtime(pool: String) -> Result<(), String> {
  let paths = Paths::real();
  if !running_projects_using_pool(&paths, &pool)?.is_empty() {
    return Err(format!("{pool} wird benutzt — erst Session beenden"));
  }
  link_pool_runtime_in(&paths, &pool)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // generate_context! darf pro Crate nur einmal expandieren (_EMBED_INFO_PLIST).
  let context = tauri::generate_context!();
  let mut args = std::env::args().skip(1);
  match args.next().as_deref() {
    // `app --terminal <projekt>`: eigener Prozess pro Terminal-Fenster,
    // damit jedes Terminal ein eigenes Dock-Icon bekommt.
    Some("--terminal") => {
      let project = args.next().expect("--terminal braucht einen Projektnamen");
      let icon = terminal_config(&project)
        .expect("Projekt-Config nicht lesbar")
        .icon
        .map(|i| {
          resolve_icon_path(&Paths::real(), &i)
            .to_string_lossy()
            .into_owned()
        });
      terminal_builder(project)
        .build(context)
        .expect("error while building tauri application")
        // Das Dock-Icon erst nach dem App-Start setzen: in setup() gesetzt
        // überschreibt macOS es beim Anlegen des Dock-Tiles wieder.
        .run(move |app, event| {
          if let tauri::RunEvent::Ready = event {
            if let Some(icon) = icon.as_deref() {
              terminal::set_dock_icon(icon);
            }
            terminal::activate_self(app);
          }
        });
    }
    _ => main_builder()
      .run(context)
      .expect("error while running tauri application"),
  }
}

/// Tray-Menü: pro Projekt ein Eintrag — Status-Punkt, Projekt-Icon, Name.
/// Klick startet das Projekt bzw. holt das laufende Terminal nach vorn.
fn tray_menu(
  app: &tauri::AppHandle,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
  use tauri::menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem};
  let paths = Paths::real();
  let menu = Menu::new(app)?;
  menu.append(&MenuItem::with_id(app, "open", "Öffnen", true, None::<&str>)?)?;
  menu.append(&PredefinedMenuItem::separator(app)?)?;
  let mut projects = list_projects_in(&paths)?;
  projects.sort_by(|a, b| a.name.cmp(&b.name));
  for p in projects {
    let icon = menu_icon(&paths, &p.name, p.running)?;
    menu.append(&IconMenuItem::with_id(
      app,
      format!("project:{}", p.name),
      &p.name,
      true,
      Some(icon),
      None::<&str>,
    )?)?;
  }
  menu.append(&PredefinedMenuItem::separator(app)?)?;
  menu.append(&MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?)?;
  Ok(menu)
}

/// Tray-Klick auf ein Projekt: läuft es, kommt das Terminal-Fenster nach vorn,
/// sonst startet es.
fn start_or_focus(app: &tauri::AppHandle, project: &str) {
  match terminal_pids(project).first() {
    Some(pid) => terminal::focus_terminal(*pid),
    None => {
      if let Err(e) = terminal::open_terminal(app.clone(), project.to_string()) {
        eprintln!("{project} starten: {e}");
      }
    }
  }
}

/// Haupt-App: reine Tray-App ohne Dock-Eintrag.
fn main_builder() -> tauri::Builder<tauri::Wry> {
  use tauri::Manager;

  tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // Nur Tray-Icon in der Menüleiste, kein Dock-Eintrag.
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);

      // Session-Watcher: synct bei Session-Ende (Prozess verschwindet) und
      // hält das Tray-Menü aktuell.
      spawn_session_watcher(app.handle().clone());

      // Fenster im Code statt in tauri.conf.json, damit der Terminal-Prozess
      // (gleiches Binary, gleiche Config) kein main-Fenster anlegt.
      tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
        .title("ai-control")
        .inner_size(800.0, 600.0)
        .visible(false)
        .build()?;

      let menu = tray_menu(app.handle())?;
      tauri::tray::TrayIconBuilder::with_id("main")
        // include_bytes! statt default_window_icon: cargo trackt die Datei,
        // neu generierte Icons landen damit sicher im nächsten Build.
        .icon(tauri::image::Image::from_bytes(include_bytes!(
          "../icons/trayTemplate.png"
        ))?)
        // Template-Icon: macOS färbt es passend zur Menüleiste ein.
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
          "open" => {
            let w = app.get_webview_window("main").unwrap();
            w.show().unwrap();
            w.set_focus().unwrap();
          }
          "quit" => app.exit(0),
          id => {
            if let Some(project) = id.strip_prefix("project:") {
              start_or_focus(app, project);
            }
          }
        })
        .build(app)?;

      Ok(())
    })
    // Hauptfenster schließen versteckt nur; Beenden geht übers Tray-Menü.
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        window.hide().unwrap();
      }
    })
    .invoke_handler(tauri::generate_handler![
      list_projects,
      create_project_full,
      add_project,
      remove_project,
      delete_project,
      project_work_dirs,
      set_project_dir,
      add_work_dir,
      remove_work_dir,
      list_pools,
      create_oauth_pool,
      create_apikey_pool,
      rename_pool,
      delete_pool,
      assign_pool,
      unassign_pool,
      set_terminal_config,
      project_icon,
      todo_state,
      set_todo,
      usage_stats,
      stop_project,
      restart_project,
      sync_setting,
      set_sync_setting,
      link_pool_runtime,
      oauth_login,
      keychain_status,
      set_apikey,
      terminal::open_terminal
    ])
}

/// Terminal-Prozess: ein Fenster mit eigener PTY; Activation-Policy bleibt
/// Regular, dadurch Dock-Icon und Cmd-Tab-Eintrag pro Terminal.
fn terminal_builder(project: String) -> tauri::Builder<tauri::Wry> {
  tauri::Builder::default()
    .manage(terminal::Terminals::default())
    .setup(move |app| {
      let cfg = terminal_config(&project)?;
      terminal::build_window(app.handle(), &project, &cfg)?;
      Ok(())
    })
    // Fenster zu → PTY-Session abräumen; danach endet der Prozess.
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::Destroyed = event {
        terminal::close(window);
      }
    })
    .invoke_handler(tauri::generate_handler![
      terminal::term_start,
      terminal::term_log,
      terminal::term_write,
      terminal::term_resize,
      // Pool-Chip im Terminal-Header braucht die Projektliste auch hier.
      list_projects
    ])
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicU32, Ordering};

  static N: AtomicU32 = AtomicU32::new(0);

  /// Frisches Pseudo-Home pro Test; claude-projects existiert wie auf dem
  /// echten System.
  fn tmp_paths() -> Paths {
    let dir = std::env::temp_dir().join(format!(
      "ai-control-test-{}-{}",
      std::process::id(),
      N.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(dir.join("claude-projects")).unwrap();
    Paths { home: dir }
  }

  /// In-Memory-ApikeyStore statt echtem Keychain.
  struct MapStore(std::sync::Mutex<HashMap<String, String>>);

  fn map_store() -> MapStore {
    MapStore(std::sync::Mutex::new(HashMap::new()))
  }

  impl ApikeyStore for MapStore {
    fn set(&self, pool: &str, key: &str) -> Result<(), String> {
      self.0.lock().unwrap().insert(pool.into(), key.into());
      Ok(())
    }
    fn has(&self, pool: &str) -> Result<bool, String> {
      Ok(self.0.lock().unwrap().contains_key(pool))
    }
    fn delete(&self, pool: &str) -> Result<(), String> {
      self.0.lock().unwrap().remove(pool);
      Ok(())
    }
  }

  /// Store ohne Keychain/Keyring — Schreiben scheitert, Lesen findet nichts.
  struct FailStore;

  impl ApikeyStore for FailStore {
    fn set(&self, _pool: &str, _key: &str) -> Result<(), String> {
      Err("kein Keychain".into())
    }
    fn has(&self, _pool: &str) -> Result<bool, String> {
      Ok(false)
    }
    fn delete(&self, _pool: &str) -> Result<(), String> {
      Ok(())
    }
  }

  /// Liefert die Pool-ID (UUID-Ordnername).
  fn make_oauth_pool(p: &Paths, name: &str) -> String {
    create_oauth_pool_in(p, name).unwrap()
  }

  fn make_apikey_pool(p: &Paths, store: &dyn ApikeyStore, name: &str, key: &str) -> String {
    create_apikey_pool_in(p, store, name, key, true).unwrap()
  }

  fn mode(path: &PathBuf) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
  }

  // -- Pool anlegen --

  #[test]
  fn apikey_pool_anlegen() {
    let p = tmp_paths();
    let store = map_store();
    let id = make_apikey_pool(&p, &store, "kunde", "sk-test-123");
    // Ordnername ist eine UUID, der Anzeigename steht nur in pool.json.
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    let dir = p.pool_dir(&id);

    let pool: Pool =
      serde_json::from_str(&fs::read_to_string(dir.join(POOL_FILE)).unwrap()).unwrap();
    assert_eq!(pool.name, "kunde");
    assert_eq!(pool.credential_type, "apikey");

    // Key liegt im Store, keine Datei im Pool-Ordner.
    assert_eq!(store.0.lock().unwrap().get(&id).unwrap(), "sk-test-123");
    assert!(!dir.join(APIKEY_FILE).exists());

    let settings: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap())
        .unwrap();
    assert_eq!(
      settings["apiKeyHelper"].as_str().unwrap(),
      apikey_helper_command(&dir, &id)
    );
  }

  #[test]
  fn apikey_pool_anlegen_ohne_store_faellt_auf_datei_zurueck() {
    let p = tmp_paths();
    let id = make_apikey_pool(&p, &FailStore, "kunde", "sk-test-123");
    let dir = p.pool_dir(&id);

    let key_path = dir.join(APIKEY_FILE);
    assert_eq!(fs::read_to_string(&key_path).unwrap(), "sk-test-123\n");
    assert_eq!(mode(&key_path), 0o600);

    // Der Helper ist dieselbe Kette — der Store-Teil findet nichts,
    // cat liefert die Datei.
    let settings: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap())
        .unwrap();
    assert_eq!(
      settings["apiKeyHelper"].as_str().unwrap(),
      apikey_helper_command(&dir, &id)
    );
  }

  /// Ohne Store und ohne allow_file bricht die Anlage ab — es entsteht nichts.
  #[test]
  fn apikey_anlegen_abbruch_ohne_store() {
    let p = tmp_paths();
    let err = create_apikey_pool_in(&p, &FailStore, "kunde", "sk-1", false).unwrap_err();
    assert_eq!(err, KEYCHAIN_UNAVAILABLE);
    assert!(pool_names(&p).unwrap().is_empty());
    assert!(!p.pools_dir().exists());
  }

  /// Key-Ändern ohne Store und ohne allow_file lässt alles unangetastet.
  #[test]
  fn apikey_aendern_abbruch_ohne_store() {
    let p = tmp_paths();
    let id = make_apikey_pool(&p, &FailStore, "kunde", "sk-alt");
    let dir = p.pool_dir(&id);
    let settings_before = fs::read_to_string(dir.join("settings.json")).unwrap();

    let err = set_apikey_in(&p, &FailStore, &id, "sk-neu", false).unwrap_err();
    assert_eq!(err, KEYCHAIN_UNAVAILABLE);
    assert_eq!(fs::read_to_string(dir.join(APIKEY_FILE)).unwrap(), "sk-alt\n");
    assert_eq!(fs::read_to_string(dir.join("settings.json")).unwrap(), settings_before);
  }

  #[test]
  fn apikey_helper_kette_referenz() {
    let dir = PathBuf::from("/pools/abc");
    let cmd = apikey_helper_command(&dir, "abc");
    assert!(cmd.ends_with("|| cat '/pools/abc/apikey'"));
    #[cfg(target_os = "macos")]
    assert!(cmd.starts_with(
      "security find-generic-password -w -s ai-control-apikey -a abc 2>/dev/null"
    ));
    #[cfg(not(target_os = "macos"))]
    assert!(cmd.starts_with(
      "secret-tool lookup service ai-control-apikey username abc 2>/dev/null"
    ));
  }

  #[test]
  fn oauth_pool_anlegen() {
    let p = tmp_paths();
    let id = make_oauth_pool(&p, "privat");
    let dir = p.pool_dir(&id);

    let pool: Pool =
      serde_json::from_str(&fs::read_to_string(dir.join(POOL_FILE)).unwrap()).unwrap();
    assert_eq!(pool.credential_type, "oauth");

    // Kein Credentials-File — den Login macht claude selbst beim ersten Start.
    assert!(!dir.join(".credentials.json").exists());
    // Grundausstattung: settings.json mit aufgeräumten UI-Defaults + CLAUDE.md,
    // damit claude nicht ins Onboarding fällt und keine Vorschlagstabelle zeigt.
    let settings: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap())
        .unwrap();
    assert_eq!(settings["promptSuggestionEnabled"], serde_json::json!(false));
    assert_eq!(settings["awaySummaryEnabled"], serde_json::json!(false));
    assert!(dir.join("CLAUDE.md").is_file());
  }

  #[test]
  fn pool_init_erhaelt_bestehende_claude_md() {
    let p = tmp_paths();
    let dir = p.pool_dir("privat");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("CLAUDE.md"), "meine Vorgaben\n").unwrap();
    // Anlegen darf die vorhandene CLAUDE.md nicht überschreiben.
    // (check_new_pool erlaubt keinen bestehenden Ordner → direkt init testen)
    init_pool_config(&dir, serde_json::json!({})).unwrap();
    assert_eq!(
      fs::read_to_string(dir.join("CLAUDE.md")).unwrap(),
      "meine Vorgaben\n"
    );
  }

  #[test]
  fn pool_doppelt_anlegen_scheitert() {
    let p = tmp_paths();
    let store = map_store();
    make_apikey_pool(&p, &store, "kunde", "sk-1");
    let err = create_apikey_pool_in(&p, &store, "kunde", "sk-2", true).unwrap_err();
    assert!(err.contains("existiert bereits"));
  }

  // -- Pool umbenennen --

  #[test]
  fn pool_umbenennen() {
    let p = tmp_paths();
    let id = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &id).unwrap();

    rename_pool_in(&p, &id, "kunde-neu").unwrap();

    // Nur der Anzeigename ändert sich — Ordner, Typ und Zuordnung bleiben.
    let pool = read_pool(&p, &id).unwrap();
    assert_eq!(pool.name, "kunde-neu");
    assert_eq!(pool.credential_type, "apikey");
    assert!(p.pool_dir(&id).is_dir());
    let cfg = read_project_config_in(&p, "proj").unwrap();
    assert_eq!(cfg.pool.as_deref(), Some(id.as_str()));
  }

  #[test]
  fn pool_umbenennen_auf_vergebenen_namen_scheitert() {
    let p = tmp_paths();
    let id = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    make_oauth_pool(&p, "privat");
    let err = rename_pool_in(&p, &id, "privat").unwrap_err();
    assert!(err.contains("existiert bereits"));
    // Umbenennen auf den eigenen Namen ist erlaubt (No-op).
    rename_pool_in(&p, &id, "kunde").unwrap();
  }

  #[test]
  fn pool_leerer_key_scheitert() {
    let p = tmp_paths();
    assert!(create_apikey_pool_in(&p, &map_store(), "kunde", "  ", true).is_err());
  }

  #[test]
  fn pool_name_mit_slash_scheitert() {
    let p = tmp_paths();
    assert!(create_apikey_pool_in(&p, &map_store(), "a/b", "sk-1", true).is_err());
  }

  // -- Pool ändern --

  #[test]
  fn apikey_aendern() {
    let p = tmp_paths();
    let store = map_store();
    let id = make_apikey_pool(&p, &store, "kunde", "sk-alt");
    set_apikey_in(&p, &store, &id, "sk-neu", false).unwrap();
    assert_eq!(store.0.lock().unwrap().get(&id).unwrap(), "sk-neu");
    assert!(!p.pool_dir(&id).join(APIKEY_FILE).exists());
  }

  /// Datei-Pool (Anlage ohne Store, z. B. Bestand vor der Keychain-Ablage):
  /// Key-Ändern hebt ihn in den Store, Datei und alter cat-Helper verschwinden.
  #[test]
  fn apikey_aendern_migriert_datei_in_store() {
    let p = tmp_paths();
    let id = make_apikey_pool(&p, &FailStore, "kunde", "sk-alt");
    let dir = p.pool_dir(&id);
    // Bestand simulieren: Helper wie vor der Keychain-Ablage.
    let old = serde_json::json!({ "apiKeyHelper": format!("cat '{}'", dir.join(APIKEY_FILE).display()) });
    fs::write(dir.join("settings.json"), old.to_string()).unwrap();

    let store = map_store();
    set_apikey_in(&p, &store, &id, "sk-neu", false).unwrap();

    assert_eq!(store.0.lock().unwrap().get(&id).unwrap(), "sk-neu");
    assert!(!dir.join(APIKEY_FILE).exists());
    let settings: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap())
        .unwrap();
    assert_eq!(
      settings["apiKeyHelper"].as_str().unwrap(),
      apikey_helper_command(&dir, &id)
    );
  }

  #[test]
  fn apikey_aendern_ohne_store_schreibt_datei() {
    let p = tmp_paths();
    let id = make_apikey_pool(&p, &FailStore, "kunde", "sk-alt");
    set_apikey_in(&p, &FailStore, &id, "sk-neu", true).unwrap();
    let key_path = p.pool_dir(&id).join(APIKEY_FILE);
    assert_eq!(fs::read_to_string(&key_path).unwrap(), "sk-neu\n");
    assert_eq!(mode(&key_path), 0o600);
  }

  #[test]
  fn apikey_aendern_auf_oauth_pool_scheitert() {
    let p = tmp_paths();
    let id = make_oauth_pool(&p, "privat");
    assert!(set_apikey_in(&p, &map_store(), &id, "sk-1", true).is_err());
  }

  #[test]
  fn ensure_oauth_pool_auf_apikey_scheitert() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    assert!(ensure_oauth_pool(&p, &kunde).is_err());
    let privat = make_oauth_pool(&p, "privat");
    assert!(ensure_oauth_pool(&p, &privat).is_ok());
  }

  // -- Pool löschen --

  #[test]
  fn pool_loeschen() {
    let p = tmp_paths();
    let store = map_store();
    let id = make_apikey_pool(&p, &store, "kunde", "sk-1");
    delete_pool_in(&p, &store, &id).unwrap();
    assert!(!p.pool_dir(&id).exists());
    // Der Keychain-Eintrag geht mit.
    assert!(!store.has(&id).unwrap());
  }

  // -- Projekt-Wizard --

  #[test]
  fn projekt_wizard_scaffold() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project_full_in(
      &p,
      "neu",
      None,
      Some(&kunde),
      Some("~/projects/neu"),
      true,
      TerminalConfig {
        theme: Some("dracula".into()),
        icon: None,
        title: Some("Neu".into()),
      },
      true,
    )
    .unwrap();

    let dir = p.projects_dir().join("neu");
    assert!(dir.join("memory").is_dir());
    assert_eq!(
      fs::read_to_string(dir.join(".gitignore")).unwrap(),
      ".ai-control-running\n"
    );
    assert!(p.home.join("projects").join("neu").is_dir());

    let cfg = read_project_config_in(&p, "neu").unwrap();
    assert_eq!(cfg.pool.as_deref(), Some(kunde.as_str()));
    assert_eq!(cfg.terminal.title.as_deref(), Some("Neu"));
    assert_eq!(cfg.terminal.theme.as_deref(), Some("dracula"));

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
    // todo=true: einziger SessionStart-Hook (kein pool-guard mehr), Datei da
    assert!(dir.join(TODO_FILE).is_file());
    let groups = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert!(groups[0]["hooks"][0]["command"]
      .as_str()
      .unwrap()
      .contains(TODO_FILE));
  }

  #[test]
  fn todo_zuschalten_und_abschalten() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default(), false)
      .unwrap();
    assert!(!todo_state_in(&p, "proj").unwrap());

    set_todo_in(&p, "proj", true).unwrap();
    assert!(todo_state_in(&p, "proj").unwrap());
    let todo_path = p.projects_dir().join("proj").join(TODO_FILE);
    assert_eq!(fs::read_to_string(&todo_path).unwrap(), TODO_SKELETON);

    // doppelt aktivieren erzeugt keinen zweiten Hook
    set_todo_in(&p, "proj", true).unwrap();
    let settings: serde_json::Value = serde_json::from_str(
      &fs::read_to_string(settings_path(&p.projects_dir().join("proj"))).unwrap(),
    )
    .unwrap();
    let groups = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(groups.iter().filter(|g| hook_is_todo(g)).count(), 1);

    // Abschalten: Hook weg, Datei (mit Inhalt) bleibt
    fs::write(&todo_path, "# Offene Punkte\n\n- [ ] wichtig\n").unwrap();
    set_todo_in(&p, "proj", false).unwrap();
    assert!(!todo_state_in(&p, "proj").unwrap());
    assert!(todo_path.is_file());
    assert!(fs::read_to_string(&todo_path).unwrap().contains("wichtig"));
  }

  #[test]
  fn todo_ohne_settings_scheitert() {
    let p = tmp_paths();
    create_project_in(&p, "alt").unwrap();
    assert!(set_todo_in(&p, "alt", true).is_err());
    assert!(!todo_state_in(&p, "alt").unwrap());
  }

  #[test]
  fn projekt_wizard_minimal_ohne_pool_und_workdir() {
    let p = tmp_paths();
    create_project_full_in(&p, "neu", None, None, None, false, TerminalConfig::default(), false).unwrap();
    let dir = p.projects_dir().join("neu");
    assert!(dir.join(".claude").join("settings.json").is_file());
    // ohne Pool und Terminal-Config entsteht keine ai-control.json
    assert!(!project_config_path(&p, "neu").unwrap().is_file());
    let settings: serde_json::Value = serde_json::from_str(
      &fs::read_to_string(dir.join(".claude").join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["permissions"]["allow"][0], "Edit(~/claude-projects/neu/**)");
    assert_eq!(settings["permissions"]["additionalDirectories"], serde_json::json!([]));
  }

  #[test]
  fn projekt_wizard_doppelt_scheitert() {
    let p = tmp_paths();
    create_project_in(&p, "neu").unwrap();
    let err =
      create_project_full_in(&p, "neu", None, None, None, false, TerminalConfig::default(), false)
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
      false,
    )
    .unwrap_err();
    assert!(err.contains("Arbeitsverzeichnis fehlt"));
    // kein halbes Projekt zurückgeblieben
    assert!(!p.projects_dir().join("neu").exists());
  }

  // -- Verbrauch --

  fn usage_line(id: &str, req: &str, ts: &str, model: &str, input: u64, output: u64) -> String {
    serde_json::json!({
      "type": "assistant",
      "timestamp": ts,
      "requestId": req,
      "message": {
        "id": id,
        "model": model,
        "usage": {
          "input_tokens": input,
          "output_tokens": output,
          "cache_creation_input_tokens": 1_000_000,
          "cache_read_input_tokens": 2_000_000
        }
      }
    })
    .to_string()
  }

  #[test]
  fn verbrauch_aggregation_dedup_und_zeitfilter() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();

    let proj_path = p.projects_dir().join("proj");
    let encoded = encode_project_path(&proj_path.to_string_lossy());
    let dir = p.pool_dir(&kunde).join("projects").join(&encoded);
    fs::create_dir_all(&dir).unwrap();

    let lines = [
      // Sonnet: 1M in ($3) + 1M out ($15) + 1M cache-write ($3.75) + 2M cache-read ($0.60)
      usage_line("msg_1", "req_1", "2099-01-01T10:00:00.000Z", "claude-sonnet-5", 1_000_000, 1_000_000),
      // Retry-Duplikat: gleiche message.id + requestId → zählt nicht
      usage_line("msg_1", "req_1", "2099-01-01T10:00:01.000Z", "claude-sonnet-5", 1_000_000, 1_000_000),
      // außerhalb des Zeitfensters → zählt nicht
      usage_line("msg_2", "req_2", "2020-01-01T00:00:00.000Z", "claude-sonnet-5", 5, 5),
      // kein usage-Objekt → ignoriert
      r#"{"type":"user","timestamp":"2099-01-01T10:00:02.000Z"}"#.to_string(),
      // kaputte Zeile (laufende Session) → ignoriert
      r#"{"type":"assist"#.to_string(),
    ];
    fs::write(dir.join("session.jsonl"), lines.join("\n")).unwrap();

    // Cutoff = jetzt − 365 Tage: schließt 2020 aus, 2099 ein — datumsunabhängig
    let rows = usage_stats_in(&p, 365).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.pool, "kunde");
    assert_eq!(r.project, "proj");
    assert_eq!(r.totals.input_tokens, 1_000_000);
    assert_eq!(r.totals.output_tokens, 1_000_000);
    assert_eq!(r.totals.cache_creation_tokens, 1_000_000);
    assert_eq!(r.totals.cache_read_tokens, 2_000_000);
    assert!((r.totals.cost_usd - 22.35).abs() < 1e-9);
  }

  #[test]
  fn verbrauch_leer_ohne_transcripts() {
    let p = tmp_paths();
    make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    assert!(usage_stats_in(&p, 30).unwrap().is_empty());
  }

  #[test]
  fn parse_ts_referenz() {
    // 2026-01-01 = 1767225600; +181 Tage bis 01.07. +10 h = 1782900000
    assert_eq!(parse_ts("2026-07-01T10:00:00.000Z"), Some(1_782_900_000));
    assert_eq!(parse_ts("kein datum"), None);
  }

  /// Referenzwert vom echten System: privateDefault → 096c4ef9
  /// (verifiziert 2026-07-03 gegen den von claude angelegten Eintrag).
  #[test]
  fn keychain_service_suffix() {
    let p = Paths { home: PathBuf::from("/Users/marcus.hinz") };
    assert_eq!(
      keychain_service(&p, "privateDefault"),
      "Claude Code-credentials-096c4ef9"
    );
  }

  #[test]
  fn pool_key_status() {
    let p = tmp_paths();
    // Datei-Pool (ohne Store angelegt): Datei entscheidet.
    let kunde = make_apikey_pool(&p, &FailStore, "kunde", "sk-1");
    make_oauth_pool(&p, "privat");

    let pools = list_pools_in(&p, &FailStore).unwrap();
    assert!(pools.iter().find(|x| x.name == "kunde").unwrap().has_credentials);
    // oauth: Keychain wird im Listing bewusst nicht geprüft → immer true.
    assert!(pools.iter().find(|x| x.name == "privat").unwrap().has_credentials);

    fs::write(p.pool_dir(&kunde).join(APIKEY_FILE), "\n").unwrap();
    let pools = list_pools_in(&p, &FailStore).unwrap();
    assert!(!pools.iter().find(|x| x.name == "kunde").unwrap().has_credentials);

    fs::remove_file(p.pool_dir(&kunde).join(APIKEY_FILE)).unwrap();
    let pools = list_pools_in(&p, &FailStore).unwrap();
    assert!(!pools.iter().find(|x| x.name == "kunde").unwrap().has_credentials);
  }

  /// Store-Pool: hasCredentials kommt aus dem Keychain-Eintrag, ohne Datei.
  #[test]
  fn pool_key_status_aus_store() {
    let p = tmp_paths();
    let store = map_store();
    let kunde = make_apikey_pool(&p, &store, "kunde", "sk-1");
    assert!(!p.pool_dir(&kunde).join(APIKEY_FILE).exists());
    let pools = list_pools_in(&p, &store).unwrap();
    assert!(pools.iter().find(|x| x.name == "kunde").unwrap().has_credentials);
    store.delete(&kunde).unwrap();
    let pools = list_pools_in(&p, &store).unwrap();
    assert!(!pools.iter().find(|x| x.name == "kunde").unwrap().has_credentials);
  }

  #[test]
  fn pool_loeschen_loest_zuordnungen() {
    let p = tmp_paths();
    let store = map_store();
    let kunde = make_apikey_pool(&p, &store, "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();
    delete_pool_in(&p, &store, &kunde).unwrap();
    assert!(!p.pool_dir(&kunde).exists());
    assert!(!project_config_path(&p, "proj").unwrap().exists());
  }

  #[test]
  fn pool_anlegen_verlinkt_synced_runtime() {
    let p = tmp_paths();
    fs::create_dir_all(p.config_dir()).unwrap();
    fs::write(
      p.config_dir().join(APP_SETTINGS_FILE),
      "{ \"poolSyncDir\": \"~/claude-projects/pool\" }\n",
    )
    .unwrap();
    let id = make_oauth_pool(&p, "privat");
    let pooldir = p.pool_dir(&id);
    for (name, is_dir) in SYNCED_RUNTIME {
      let link = pooldir.join(name);
      assert!(link.is_symlink(), "{name} sollte Symlink sein");
      assert_eq!(fs::read_link(&link).unwrap(), pool_data_dir(&p, &id).unwrap().join(name));
      let target = pool_data_dir(&p, &id).unwrap().join(name);
      assert_eq!(target.is_dir(), is_dir);
      assert!(target.exists());
    }
    // Zieldaten liegen unter dem konfigurierten poolSyncDir/<id>/
    assert!(p.home.join("claude-projects/pool").join(&id).is_dir());
  }

  /// Ohne poolSyncDir bleibt alles lokal: keine Symlinks, Verlinken scheitert laut.
  #[test]
  fn pool_anlegen_ohne_sync_dir_bleibt_lokal() {
    let p = tmp_paths();
    let id = make_oauth_pool(&p, "privat");
    for (name, _) in SYNCED_RUNTIME {
      assert!(!p.pool_dir(&id).join(name).is_symlink(), "{name} darf kein Symlink sein");
    }
    let err = link_pool_runtime_in(&p, &id).unwrap_err();
    assert!(err.contains("poolSyncDir"));
  }

  #[test]
  fn sync_optin_default_aus_und_umschaltbar() {
    let p = tmp_paths();
    assert!(!sync_on_session_end(&p)); // default: kein Sync ohne Zustimmung
    set_sync_on_session_end_in(&p, true).unwrap();
    assert!(sync_on_session_end(&p));
    set_sync_on_session_end_in(&p, false).unwrap();
    assert!(!sync_on_session_end(&p));
  }

  #[test]
  fn pool_loeschen_erhaelt_terminal_config() {
    let p = tmp_paths();
    let store = map_store();
    let kunde = make_apikey_pool(&p, &store, "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();
    set_terminal_config_in(
      &p,
      "proj",
      TerminalConfig { theme: Some("dracula".into()), icon: None, title: None },
    )
    .unwrap();
    delete_pool_in(&p, &store, &kunde).unwrap();
    let cfg = read_project_config_in(&p, "proj").unwrap();
    assert_eq!(cfg.pool, None);
    assert_eq!(cfg.terminal.theme.as_deref(), Some("dracula"));
  }

  #[test]
  fn pool_loeschen_unbekannt_scheitert() {
    let p = tmp_paths();
    assert!(delete_pool_in(&p, &map_store(), "gibtsnicht").is_err());
  }

  // -- Projekt zuordnen / rausnehmen / wechseln --

  #[test]
  fn projekt_zuordnen() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();

    let cfg: ProjectConfig =
      serde_json::from_str(&fs::read_to_string(project_config_path(&p, "proj").unwrap()).unwrap())
        .unwrap();
    assert_eq!(cfg.pool.as_deref(), Some(kunde.as_str()));

    let pools = list_pools_in(&p, &FailStore).unwrap();
    assert_eq!(pools[0].projects, vec!["proj"]);
  }

  #[test]
  fn projekt_zuordnen_pool_fehlt_scheitert() {
    let p = tmp_paths();
    create_project_in(&p, "proj").unwrap();
    assert!(assign_pool_in(&p, "proj", "gibtsnicht").is_err());
  }

  #[test]
  fn projekt_rausnehmen() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();
    unassign_pool_in(&p, "proj").unwrap();
    assert!(!project_config_path(&p, "proj").unwrap().exists());
  }

  #[test]
  fn projekt_rausnehmen_ohne_zuordnung_scheitert() {
    let p = tmp_paths();
    create_project_in(&p, "proj").unwrap();
    assert!(unassign_pool_in(&p, "proj").is_err());
  }

  /// Wechsel oauth → apikey: das Projekt sieht nur den Pool-Namen,
  /// der Credential-Typ ist ihm egal.
  #[test]
  fn pool_wechseln_typ_ist_projekt_egal() {
    let p = tmp_paths();
    let privat = make_oauth_pool(&p, "privat");
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project_in(&p, "proj").unwrap();

    assign_pool_in(&p, "proj", &privat).unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();

    let raw = fs::read_to_string(project_config_path(&p, "proj").unwrap()).unwrap();
    let cfg: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(cfg, serde_json::json!({ "pool": kunde }));
  }

  // -- Projekt anlegen / löschen --

  #[test]
  fn projekt_anlegen() {
    let p = tmp_paths();
    create_project_in(&p, "proj").unwrap();
    assert!(p.projects_dir().join("proj").join(".claude").is_dir());

    let projects = list_projects_in(&p).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "proj");
    assert_eq!(projects[0].pool, None);
  }

  #[test]
  fn projekt_doppelt_anlegen_scheitert() {
    let p = tmp_paths();
    create_project_in(&p, "proj").unwrap();
    assert!(create_project_in(&p, "proj").is_err());
  }

  #[test]
  fn projekt_loeschen() {
    let p = tmp_paths();
    create_project_in(&p, "proj").unwrap();
    delete_project_in(&p, "proj", false).unwrap();
    assert!(!p.projects_dir().join("proj").exists());
  }

  #[test]
  fn projekt_loeschen_laesst_arbeitsordner() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default(), false)
      .unwrap();
    delete_project_in(&p, "proj", false).unwrap();
    assert!(!p.projects_dir().join("proj").exists());
    assert!(p.home.join("projects").join("proj").is_dir());
  }

  #[test]
  fn projekt_loeschen_mit_arbeitsordner() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default(), false)
      .unwrap();
    delete_project_in(&p, "proj", true).unwrap();
    assert!(!p.projects_dir().join("proj").exists());
    assert!(!p.home.join("projects").join("proj").exists());
  }

  #[test]
  fn projekt_loeschen_fehlender_arbeitsordner_scheitert() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default(), false)
      .unwrap();
    fs::remove_dir_all(p.home.join("projects").join("proj")).unwrap();
    assert!(delete_project_in(&p, "proj", true).is_err());
  }

  #[test]
  fn projekt_arbeitsordner_auslesen() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, Some("~/projects/proj"), true, TerminalConfig::default(), false)
      .unwrap();
    assert_eq!(project_work_dirs_in(&p, "proj").unwrap(), vec!["~/projects/proj"]);
    // Projekt ohne settings.json → leer
    create_project_in(&p, "alt").unwrap();
    assert!(project_work_dirs_in(&p, "alt").unwrap().is_empty());
  }

  #[test]
  fn arbeitsordner_nachtraeglich_erfassen_und_entfernen() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default(), false)
      .unwrap();
    fs::create_dir_all(p.home.join("projects/extra")).unwrap();
    let picked = p.home.join("projects/extra").to_string_lossy().into_owned();
    add_work_dir_in(&p, "proj", &picked).unwrap();
    // gespeichert wird ~-kontrahiert, in beiden permissions-Feldern
    assert_eq!(project_work_dirs_in(&p, "proj").unwrap(), vec!["~/projects/extra"]);
    let raw = fs::read_to_string(settings_path(&p.projects_dir().join("proj"))).unwrap();
    assert!(raw.contains("Edit(~/projects/extra/**)"));
    assert!(add_work_dir_in(&p, "proj", &picked).is_err()); // doppelt

    remove_work_dir_in(&p, "proj", "~/projects/extra").unwrap();
    assert!(project_work_dirs_in(&p, "proj").unwrap().is_empty());
    let raw = fs::read_to_string(settings_path(&p.projects_dir().join("proj"))).unwrap();
    assert!(!raw.contains("~/projects/extra"));
    // Ordner selbst bleibt
    assert!(p.home.join("projects/extra").is_dir());
  }

  #[test]
  fn projektordner_verlegen() {
    let p = tmp_paths();
    create_project_full_in(&p, "proj", None, None, None, false, TerminalConfig::default(), false)
      .unwrap();
    // Nutzer hat den Ordner selbst verschoben; die App ordnet nur neu zu.
    let new_dir = p.home.join("elsewhere").join("proj");
    fs::create_dir_all(p.home.join("elsewhere")).unwrap();
    fs::rename(p.projects_dir().join("proj"), &new_dir).unwrap();
    set_project_dir_in(&p, "proj", &new_dir.to_string_lossy()).unwrap();

    assert_eq!(project_dir(&p, "proj").unwrap(), new_dir);
    let raw = fs::read_to_string(settings_path(&new_dir)).unwrap();
    assert!(raw.contains("~/elsewhere/proj/memory"));
    assert!(raw.contains("Edit(~/elsewhere/proj/**)"));
    assert!(!raw.contains("claude-projects/proj"));
  }

  #[test]
  fn projekt_loeschen_unbekannt_scheitert() {
    let p = tmp_paths();
    assert!(delete_project_in(&p, "gibtsnicht", false).is_err());
  }
}
