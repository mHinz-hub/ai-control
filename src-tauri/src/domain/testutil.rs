//! Gemeinsame Test-Helfer der Domänen-Module: Pseudo-Home, In-Memory-Store,
//! Kurzformen für Pool-/Projekt-Anlage.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::domain::credentials::ApikeyStore;
use crate::domain::paths::Paths;
use crate::domain::pool::{create_apikey_pool_in, create_oauth_pool_in};
use crate::domain::registry::register_project;

static N: AtomicU32 = AtomicU32::new(0);

/// Frisches Pseudo-Home pro Test; claude-projects existiert wie auf dem
/// echten System.
pub(crate) fn tmp_paths() -> Paths {
  let dir = std::env::temp_dir().join(format!(
    "ai-control-test-{}-{}",
    std::process::id(),
    N.fetch_add(1, Ordering::SeqCst)
  ));
  fs::create_dir_all(dir.join("claude-projects")).unwrap();
  Paths { home: dir }
}

/// In-Memory-ApikeyStore statt echtem Keychain.
pub(crate) struct MapStore(pub(crate) std::sync::Mutex<HashMap<String, String>>);

pub(crate) fn map_store() -> MapStore {
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
pub(crate) struct FailStore;

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
pub(crate) fn make_oauth_pool(p: &Paths, name: &str) -> String {
  create_oauth_pool_in(p, name).unwrap()
}

pub(crate) fn make_apikey_pool(
  p: &Paths,
  store: &dyn ApikeyStore,
  name: &str,
  key: &str,
) -> String {
  create_apikey_pool_in(p, store, name, key, true).unwrap()
}

/// Minimal-Projekt: Ordner mit .claude/, .ai-control/config.json und
/// Registry-Eintrag. Die Projekt-ID ist der Name — Tests adressieren das
/// Projekt damit direkt, ohne UUID-Variablen durchzureichen.
pub(crate) fn create_project(paths: &Paths, name: &str) -> Result<(), String> {
  crate::domain::check_name(name)?;
  let dir = paths.projects_dir().join(name);
  if dir.exists() {
    return Err(format!("Projekt existiert bereits: {name}"));
  }
  fs::create_dir_all(dir.join(".claude"))
    .map_err(|e| format!("{}: {e}", dir.join(".claude").display()))?;
  fs::create_dir_all(dir.join(".ai-control"))
    .map_err(|e| format!("{}: {e}", dir.join(".ai-control").display()))?;
  fs::write(
    dir.join(".ai-control").join("config.json"),
    format!("{{\"id\": \"{name}\", \"name\": \"{name}\"}}\n"),
  )
  .map_err(|e| e.to_string())?;
  register_project(paths, name, &dir)
}

pub(crate) fn mode(path: &PathBuf) -> u32 {
  use std::os::unix::fs::PermissionsExt;
  fs::metadata(path).unwrap().permissions().mode() & 0o777
}
