//! Projekt-Registry: projects.json (Projekt-ID → Ordner + Pool) unter
//! ~/.config/ai-central. Schlüssel ist die Projekt-UUID aus der
//! .ai-central/config.json des Projekts; die Pool-Zuordnung ist
//! maschinenlokal und lebt deshalb hier, nicht in der syncbaren
//! Projekt-Config.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::domain::paths::{contract_home, expand_home, Paths};

/// Registry-Eintrag eines Projekts: Ordner + Pool-Zuordnung dieser Maschine.
pub(crate) struct RegEntry {
  pub(crate) dir: PathBuf,
  pub(crate) pool: Option<String>,
}

/// Dateiformat: Alt-Einträge sind reine Pfad-Strings, neue Einträge Objekte
/// mit path + pool. Geschrieben wird der String, solange kein Pool gesetzt ist.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RawEntry {
  Path(String),
  Full {
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pool: Option<String>,
  },
}

/// Registry Projekt-ID → Eintrag; ohne projects.json gibt es keine Projekte.
pub(crate) fn load_registry(paths: &Paths) -> Result<BTreeMap<String, RegEntry>, String> {
  let file = paths.projects_file();
  if !file.is_file() {
    return Ok(BTreeMap::new());
  }
  let raw = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
  let map: BTreeMap<String, RawEntry> =
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", file.display()))?;
  Ok(
    map
      .into_iter()
      .map(|(name, e)| {
        let (path, pool) = match e {
          RawEntry::Path(p) => (p, None),
          RawEntry::Full { path, pool } => (path, pool),
        };
        (name, RegEntry { dir: expand_home(paths, &path), pool })
      })
      .collect(),
  )
}

pub(crate) fn save_registry(
  paths: &Paths,
  reg: &BTreeMap<String, RegEntry>,
) -> Result<(), String> {
  let raw_map: BTreeMap<&String, RawEntry> = reg
    .iter()
    .map(|(name, e)| {
      let path = contract_home(paths, &e.dir);
      let raw = match &e.pool {
        None => RawEntry::Path(path),
        Some(pool) => RawEntry::Full { path, pool: Some(pool.clone()) },
      };
      (name, raw)
    })
    .collect();
  let raw = serde_json::to_string_pretty(&raw_map).map_err(|e| e.to_string())?;
  fs::create_dir_all(paths.config_dir())
    .map_err(|e| format!("{}: {e}", paths.config_dir().display()))?;
  let file = paths.projects_file();
  crate::domain::write_atomic(&file, &(raw + "\n"))
}

/// Ordner eines registrierten Projekts.
pub(crate) fn project_dir(paths: &Paths, name: &str) -> Result<PathBuf, String> {
  Ok(
    load_registry(paths)?
      .remove(name)
      .ok_or_else(|| format!("Projekt nicht registriert: {name}"))?
      .dir,
  )
}

/// Pool-Zuordnung eines registrierten Projekts (maschinenlokal).
pub(crate) fn project_pool(paths: &Paths, name: &str) -> Result<Option<String>, String> {
  Ok(
    load_registry(paths)?
      .remove(name)
      .ok_or_else(|| format!("Projekt nicht registriert: {name}"))?
      .pool,
  )
}

/// Setzt bzw. löscht die Pool-Zuordnung eines Projekts in der Registry.
pub(crate) fn set_project_pool(
  paths: &Paths,
  name: &str,
  pool: Option<&str>,
) -> Result<(), String> {
  let mut reg = load_registry(paths)?;
  let entry = reg
    .get_mut(name)
    .ok_or_else(|| format!("Projekt nicht registriert: {name}"))?;
  entry.pool = pool.map(str::to_string);
  save_registry(paths, &reg)
}

/// Nimmt ein Projekt in die Registry auf.
pub(crate) fn register_project(
  paths: &Paths,
  name: &str,
  dir: &std::path::Path,
) -> Result<(), String> {
  let mut reg = load_registry(paths)?;
  if reg.contains_key(name) {
    return Err(format!("Projekt existiert bereits: {name}"));
  }
  reg.insert(name.to_string(), RegEntry { dir: dir.to_path_buf(), pool: None });
  save_registry(paths, &reg)
}

/// Entfernt nur den Registry-Eintrag; der Projektordner bleibt.
pub(crate) fn unregister_project(paths: &Paths, name: &str) -> Result<(), String> {
  let mut reg = load_registry(paths)?;
  if reg.remove(name).is_none() {
    return Err(format!("Projekt nicht registriert: {name}"));
  }
  save_registry(paths, &reg)
}
