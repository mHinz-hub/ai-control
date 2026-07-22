//! Pools: benannte Credential-Sets unter ~/.config/ai-control/pools/<id>.
//! Jeder Pool-Ordner ist ein vollständiges CLAUDE_CONFIG_DIR (settings.json,
//! CLAUDE.md, Panel-Skill, MCP-Registrierung); oauth-Credentials verwaltet
//! claude selbst (Keychain), die App speichert keine Tokens.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::domain::check_name;
use crate::domain::credentials::{ApikeyStore, KEYCHAIN_UNAVAILABLE};
use crate::domain::paths::Paths;
use crate::domain::project::{is_running, projects_using_pool, unassign_pool_in};
use crate::domain::settings::pool_sync_dir;

/// Feste Dateinamen im Pool-Ordner.
pub(crate) const APIKEY_FILE: &str = "apikey";
pub(crate) const POOL_FILE: &str = "pool.json";

#[derive(Serialize, Deserialize)]
pub(crate) struct Pool {
  pub(crate) name: String,
  #[serde(rename = "credentialType")]
  pub(crate) credential_type: String,
  /// Referenzierter Pool: das CLAUDE_CONFIG_DIR liegt außerhalb, der
  /// Pool-Ordner ist dann nur die Hülle für diese pool.json. Home-kontrahiert
  /// abgelegt („~/.claude"), damit der Eintrag maschinenübergreifend trägt.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub(crate) dir: Option<String>,
  /// Unbekannte Keys unverändert durchreichen — beim Umbenennen wird die
  /// ganze pool.json neu geschrieben (dieselbe Fehlerklasse wie ProjectConfig).
  #[serde(flatten)]
  pub(crate) rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize)]
pub(crate) struct PoolInfo {
  /// Ordnername unter pools/ (bei Neuanlagen eine UUID) — stabile ID, an der
  /// Keychain-Suffix, Symlinks und Projekt-Zuordnungen hängen.
  pub(crate) id: String,
  /// Anzeigename aus pool.json, frei umbenennbar.
  pub(crate) name: String,
  #[serde(rename = "credentialType")]
  pub(crate) credential_type: String,
  pub(crate) projects: Vec<String>,
  /// Teilmenge von `projects`, die gerade läuft (dasselbe `is_running` wie die
  /// Projektliste). Der Löschen-Dialog sperrt darauf.
  pub(crate) running: Vec<String>,
  #[serde(rename = "hasCredentials")]
  pub(crate) has_credentials: bool,
  /// Bei referenzierten Pools das fremde Config-Verzeichnis („~/.claude"),
  /// sonst leer. Die UI zeigt daran, dass hier nichts der App gehört.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) dir: Option<String>,
}

/// Das CLAUDE_CONFIG_DIR eines Pools. Normale Pools sind ihr eigenes
/// Config-Verzeichnis; ein referenzierter Pool ist nur eine Hülle mit
/// pool.json und zeigt über `dir` auf ein bestehendes Verzeichnis — typisch
/// claudes Default `~/.claude`, dessen Login damit weiterbenutzt wird.
pub(crate) fn pool_config_dir(paths: &Paths, pool: &str) -> Result<PathBuf, String> {
  check_name(pool)?;
  Ok(config_dir_of(paths, pool, &read_pool(paths, pool)?))
}

fn config_dir_of(paths: &Paths, pool: &str, p: &Pool) -> PathBuf {
  match &p.dir {
    Some(d) => crate::domain::paths::expand_home(paths, d),
    None => paths.pool_dir(pool),
  }
}

/// Referenzierte Pools verwalten fremdes Gut: die App legt dort nichts an,
/// baut nichts um und löscht nichts. Guard für genau diese Operationen.
fn reject_reference(paths: &Paths, pool: &str, was: &str) -> Result<(), String> {
  if let Some(dir) = read_pool(paths, pool)?.dir {
    return Err(format!("{was} geht bei einem verwiesenen Pool nicht ({dir})"));
  }
  Ok(())
}

pub(crate) fn read_pool(paths: &Paths, pool: &str) -> Result<Pool, String> {
  let cfg_path = paths.pool_dir(pool).join(POOL_FILE);
  let raw = fs::read_to_string(&cfg_path)
    .map_err(|e| format!("{}: {e}", cfg_path.display()))?;
  serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg_path.display()))
}

pub(crate) fn list_pools_in(
  paths: &Paths,
  store: &dyn ApikeyStore,
) -> Result<Vec<PoolInfo>, String> {
  let mut pools = Vec::new();
  if !paths.pools_dir().is_dir() {
    return Ok(pools);
  }
  let entries =
    fs::read_dir(paths.pools_dir()).map_err(|e| format!("{}: {e}", paths.pools_dir().display()))?;
  for entry in entries {
    let entry = entry.map_err(|e| format!("{}: {e}", paths.pools_dir().display()))?;
    let cfg_path = entry.path().join(POOL_FILE);
    if !cfg_path.is_file() {
      continue;
    }
    let raw = fs::read_to_string(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
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
    // Anzeige-Namen fürs Frontend; die Lauf-Erkennung braucht die IDs.
    let mut projects = Vec::new();
    let mut running = Vec::new();
    for project in projects_using_pool(paths, &id)? {
      let name = crate::domain::project::display_name_in(paths, &project)?;
      if is_running(&project) {
        running.push(name.clone());
      }
      projects.push(name);
    }
    pools.push(PoolInfo {
      id,
      projects,
      running,
      name: pool.name,
      credential_type: pool.credential_type,
      has_credentials,
      dir: pool.dir,
    });
  }
  pools.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(pools)
}

/// (ID, Anzeigename) aller Pools. Beim ersten Pool existiert pools/ noch
/// nicht — dann ist die Liste leer.
pub(crate) fn pool_names(paths: &Paths) -> Result<Vec<(String, String)>, String> {
  let mut out = Vec::new();
  if !paths.pools_dir().is_dir() {
    return Ok(out);
  }
  for entry in
    fs::read_dir(paths.pools_dir()).map_err(|e| format!("{}: {e}", paths.pools_dir().display()))?
  {
    let entry = entry.map_err(|e| format!("{}: {e}", paths.pools_dir().display()))?;
    let cfg_path = entry.path().join(POOL_FILE);
    if !cfg_path.is_file() {
      continue;
    }
    let raw = fs::read_to_string(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
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

fn write_pool_json(dir: &PathBuf, pool: &Pool) -> Result<(), String> {
  fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  let raw = serde_json::to_string_pretty(pool).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&dir.join(POOL_FILE), &(raw + "\n"))
}

/// Grundausstattung eines Pool-Ordners (= CLAUDE_CONFIG_DIR): settings.json
/// (aufgeräumte UI-Defaults + `extra`) und eine CLAUDE.md, die claude als
/// User-Scope liest. CLAUDE.md wird nur angelegt, wenn sie fehlt.
/// Die Prompt-Vorschläge/Rückkehr-Zusammenfassung werden abgeschaltet — sonst
/// erscheint oben die Vorschlagstabelle.
pub(crate) fn init_pool_config(
  dir: &PathBuf,
  extra: serde_json::Value,
) -> Result<(), String> {
  let mut settings = serde_json::json!({
    "promptSuggestionEnabled": false,
    "awaySummaryEnabled": false,
    "permissions": { "allow": PANEL_PERMISSIONS },
  });
  let base = settings.as_object_mut().unwrap();
  if let Some(obj) = extra.as_object() {
    for (k, v) in obj {
      base.insert(k.clone(), v.clone());
    }
  }
  fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&dir.join("settings.json"), &(raw + "\n"))?;
  let claude_md = dir.join("CLAUDE.md");
  if !claude_md.is_file() {
    fs::write(&claude_md, "").map_err(|e| format!("{}: {e}", claude_md.display()))?;
  }
  install_panel_skill(dir);
  register_mcp_server(dir);
  Ok(())
}

/// Skill, der das MCP-Tool `write_panel` für Entwürfe anweist. Liegt im
/// Pool-Ordner (= CLAUDE_CONFIG_DIR), damit claude ihn als User-Skill findet.
const PANEL_SKILL: &str = include_str!("../../resources/panel-skill.md");

/// MCP-Server-Key — zugleich das Label, das claude im Tool-Call anzeigt
/// („text panel"). Bestimmt den Tool-Namespace `mcp__<key>__<tool>`.
const PANEL_MCP_SERVER: &str = "text-panel";

/// Freigaben für die Panel-MCP-Tools — damit die Aufrufe ohne Rückfrage
/// laufen. Namensschema: `mcp__<server>__<tool>`.
const PANEL_PERMISSIONS: [&str; 2] = [
  "mcp__text-panel__write_panel",
  "mcp__text-panel__write_commands",
];

/// Schreibt/aktualisiert die Panel-Skill-Datei in einem Pool (überschreibt eine
/// evtl. ältere, tee-basierte Fassung).
fn install_panel_skill(pool_dir: &std::path::Path) {
  let skill_dir = pool_dir.join("skills").join("panel");
  if fs::create_dir_all(&skill_dir).is_ok() {
    let _ = fs::write(skill_dir.join("SKILL.md"), PANEL_SKILL);
  }
}

/// Frühere Panel-Freigaben (Bash- und alter MCP-Key), die aus den Pool-Settings
/// entfernt werden.
const STALE_PANEL_PERMISSIONS: [&str; 3] =
  ["Bash(tee:*)", "Bash(cat:*)", "mcp__aicontrol__write_panel"];

/// Trägt die MCP-Freigabe in die settings.json eines Pools ein und entfernt
/// die alten Bash-Freigaben — idempotent, ohne sonstige Einträge zu verändern.
fn ensure_panel_permission(pool_dir: &std::path::Path) {
  let sp = pool_dir.join("settings.json");
  let Ok(raw) = fs::read_to_string(&sp) else {
    return;
  };
  let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) else {
    return;
  };
  let Some(obj) = v.as_object_mut() else { return };
  let Some(perms) = obj
    .entry("permissions")
    .or_insert_with(|| serde_json::json!({}))
    .as_object_mut()
  else {
    return;
  };
  let Some(allow) = perms
    .entry("allow")
    .or_insert_with(|| serde_json::json!([]))
    .as_array_mut()
  else {
    return;
  };
  let before = allow.clone();
  allow.retain(|e| !e.as_str().is_some_and(|s| STALE_PANEL_PERMISSIONS.contains(&s)));
  for perm in PANEL_PERMISSIONS {
    if !allow.iter().any(|e| e.as_str() == Some(perm)) {
      allow.push(serde_json::json!(perm));
    }
  }
  if *allow == before {
    return; // nichts geändert
  }
  if let Ok(out) = serde_json::to_string_pretty(&v) {
    let _ = crate::domain::write_atomic(&sp, &(out + "\n"));
  }
}

/// Registriert den MCP-Server (dieses Binary mit `--mcp-panel`) in der
/// `.claude.json` des Pools (= CLAUDE_CONFIG_DIR, User-Scope für alle
/// Projekte des Pools). Vorhandene .claude.json wird gemergt, nicht ersetzt.
fn register_mcp_server(pool_dir: &std::path::Path) {
  let Ok(exe) = std::env::current_exe() else {
    return;
  };
  // Tool nicht deferren, sonst sieht das Modell write_panel nicht und schreibt
  // den Entwurf in den Chat statt ins Panel.
  let desired = serde_json::json!({
    "type": "stdio",
    "command": exe.to_string_lossy(),
    "args": ["--mcp-panel"],
    "alwaysLoad": true,
  });
  let cfg = pool_dir.join(".claude.json");
  // Nur eine fehlende Datei rechtfertigt ein frisches Objekt. Ist sie da, aber
  // unlesbar oder kaputtes JSON (abgebrochener Schreibvorgang, volle Platte),
  // wird sie in Ruhe gelassen: Sie ist claudes Live-State mit Projekt-Zustand
  // und MCP-Einträgen Dritter. Sie hier zu ersetzen macht aus einem
  // reparierbaren Schaden einen endgültigen.
  let mut v = match fs::read_to_string(&cfg) {
    Ok(s) => {
      let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else { return };
      v
    }
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
    Err(_) => return,
  };
  let Some(obj) = v.as_object_mut() else { return };
  let Some(servers) = obj
    .entry("mcpServers")
    .or_insert_with(|| serde_json::json!({}))
    .as_object_mut()
  else {
    return;
  };
  // Schon korrekt und kein Altkey -> nichts schreiben. `.claude.json` ist
  // claudes Live-State; wir fassen sie nur an, wenn der Eintrag fehlt/abweicht.
  if servers.get("aicontrol").is_none() && servers.get(PANEL_MCP_SERVER) == Some(&desired) {
    return;
  }
  servers.remove("aicontrol"); // alter Key vor Umbenennung
  servers.insert(PANEL_MCP_SERVER.into(), desired);
  if let Ok(out) = serde_json::to_string_pretty(&v) {
    let _ = crate::domain::write_atomic(&cfg, &(out + "\n"));
  }
}

/// Panel-Skill, MCP-Server-Registrierung und Tool-Freigabe in einen Pool
/// bringen. install_panel_skill überschreibt eine evtl. alte tee-Fassung.
fn provision_pool(pool_dir: &std::path::Path) {
  install_panel_skill(pool_dir);
  register_mcp_server(pool_dir);
  ensure_panel_permission(pool_dir);
}

/// Panel-MCP in alle vorhandenen Pools bringen (Migration beim App-Start).
/// Bei referenzierten Pools trifft das deren Zielverzeichnis — die drei
/// Schritte sind additiv (Skill-Datei, MCP-Eintrag, Freigabe-Ergänzung).
pub(crate) fn provision_pools_for_panel(paths: &Paths) {
  if let Ok(pools) = pool_names(paths) {
    for (id, _) in pools {
      if let Ok(dir) = pool_config_dir(paths, &id) {
        provision_pool(&dir);
      }
    }
  }
}

/// Runtime, die pro Pool ins synced Repo gelinkt wird: Transkripte, Todos,
/// Prompt-Historie. (name, ist_ordner)
pub(crate) const SYNCED_RUNTIME: [(&str, bool); 3] =
  [("projects", true), ("todos", true), ("history.jsonl", false)];

/// Zielort der synced Runtime-Daten eines Pools unterhalb von poolSyncDir.
pub(crate) fn pool_data_dir(paths: &Paths, pool: &str) -> Result<PathBuf, String> {
  pool_sync_dir(paths)
    .map(|d| d.join(pool))
    .ok_or_else(|| "poolSyncDir ist nicht konfiguriert".to_string())
}

/// Ersetzt im Pool-Ordner projects/todos/history.jsonl durch Symlinks auf den
/// konfigurierten Sync-Ordner. Die Symlinks sind maschinenlokal, die Daten
/// reisen über den Sync des Zielordners (z. B. git).
/// Vorhandene echte Inhalte werden verworfen (kein History-Erhalt — bewusst).
pub(crate) fn link_pool_runtime_in(paths: &Paths, pool: &str) -> Result<(), String> {
  check_name(pool)?;
  // Das Umbauen verwirft vorhandene Transkripte/Todos — in einem fremden
  // Verzeichnis kommt das nicht in Frage.
  reject_reference(paths, pool, "Runtime verlinken")?;
  let src = paths.pool_dir(pool);
  let data = pool_data_dir(paths, pool)?;
  for (name, is_dir) in SYNCED_RUNTIME {
    let target = data.join(name);
    if is_dir {
      fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    } else {
      fs::create_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
      if !target.exists() {
        fs::write(&target, "").map_err(|e| format!("{}: {e}", target.display()))?;
      }
    }
    let link = src.join(name);
    if link.is_dir() && !link.is_symlink() {
      fs::remove_dir_all(&link).map_err(|e| format!("{}: {e}", link.display()))?;
    } else if link.is_symlink() || link.exists() {
      fs::remove_file(&link).map_err(|e| format!("{}: {e}", link.display()))?;
    }
    crate::platform::symlink(&target, &link)?;
  }
  Ok(())
}

/// Legt einen apikey-Pool an: Key in den Keychain/Keyring (Datei 0600 nur mit
/// allow_file), settings.json mit apiKeyHelper-Kette, CLAUDE.md, pool.json.
/// Ohne Store und ohne allow_file bricht die Anlage ab, bevor etwas entsteht.
/// Liefert die Pool-ID.
pub(crate) fn create_apikey_pool_in(
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
    crate::platform::write_secret_file(&dir.join(APIKEY_FILE), &format!("{key}\n"))?;
  }
  init_pool_config(
    &dir,
    serde_json::json!({ "apiKeyHelper": crate::platform::apikey_helper_command(&dir, &id) }),
  )?;
  write_pool_json(&dir, &Pool { name: name.to_string(), credential_type: "apikey".into(), dir: None, rest: Default::default() })?;
  if pool_sync_dir(paths).is_some() {
    link_pool_runtime_in(paths, &id)?;
  }
  Ok(id)
}

/// Legt einen oauth-Pool an: Grundausstattung (leere settings.json + CLAUDE.md)
/// + pool.json. Die Anmeldung macht claude selbst beim ersten Start des Pools
/// (`/login`) und legt den Keychain-Eintrag an — die App speichert keine Tokens.
/// Liefert die Pool-ID.
pub(crate) fn create_oauth_pool_in(paths: &Paths, name: &str) -> Result<String, String> {
  let dir = check_new_pool(paths, name)?;
  init_pool_config(&dir, serde_json::json!({}))?;
  write_pool_json(&dir, &Pool { name: name.to_string(), credential_type: "oauth".into(), dir: None, rest: Default::default() })?;
  let id = dir.file_name().unwrap().to_string_lossy().into_owned();
  if pool_sync_dir(paths).is_some() {
    link_pool_runtime_in(paths, &id)?;
  }
  Ok(id)
}

/// Legt einen Pool an, der auf ein bestehendes Config-Verzeichnis verweist —
/// gedacht für claudes Default `~/.claude`, damit ein vorhandener Login ohne
/// erneutes `/login` weiterläuft. Angelegt wird nur die Hülle
/// `pools/<UUID>/pool.json`; im Zielverzeichnis entsteht ausschließlich das,
/// was das Panel braucht (Skill, MCP-Eintrag, Tool-Freigabe) — keine
/// settings.json-Grundausstattung, die fremde Einstellungen überschriebe.
/// Liefert die Pool-ID.
pub(crate) fn create_reference_pool_in(
  paths: &Paths,
  name: &str,
  target: &str,
) -> Result<String, String> {
  let hull = check_new_pool(paths, name)?;
  let target_dir = crate::domain::paths::expand_home(paths, target);
  if !target_dir.is_dir() {
    return Err(format!("Verzeichnis gibt es nicht: {}", target_dir.display()));
  }
  // Ein Ziel innerhalb von pools/ wäre ein Pool im Pool: Löschen und
  // Runtime-Symlinks des einen griffen in den anderen.
  if target_dir.starts_with(paths.pools_dir()) {
    return Err("Verzeichnis liegt in der Pool-Verwaltung".into());
  }
  if let Some(other) = pool_names(paths)?.iter().find_map(|(id, n)| {
    let p = read_pool(paths, id).ok()?;
    (config_dir_of(paths, id, &p) == target_dir).then_some(n.clone())
  }) {
    return Err(format!("Verzeichnis wird schon von Pool {other} benutzt"));
  }
  let stored = crate::domain::paths::contract_home(paths, &target_dir);
  write_pool_json(
    &hull,
    &Pool {
      name: name.to_string(),
      credential_type: "oauth".into(),
      dir: Some(stored),
      rest: Default::default(),
    },
  )?;
  provision_pool(&target_dir);
  Ok(hull.file_name().unwrap().to_string_lossy().into_owned())
}

/// Claudes Default-Verzeichnis, wenn es existiert und noch kein Pool darauf
/// zeigt — die UI bietet es dann beim Anlegen als fertigen Pool an.
pub(crate) fn offered_default_dir(paths: &Paths) -> Option<String> {
  let dir = paths.default_claude_dir();
  if !dir.is_dir() {
    return None;
  }
  let taken = pool_names(paths).ok()?.iter().any(|(id, _)| {
    read_pool(paths, id).is_ok_and(|p| config_dir_of(paths, id, &p) == dir)
  });
  (!taken).then(|| crate::domain::paths::contract_home(paths, &dir))
}

/// Setzt den Anzeigenamen eines Pools — reines pool.json-Update, ID/Ordner
/// (und damit Keychain-Suffix, Symlinks, Zuordnungen) bleiben unverändert.
pub(crate) fn rename_pool_in(paths: &Paths, pool: &str, name: &str) -> Result<(), String> {
  check_name(name)?;
  let mut current = read_pool(paths, pool)?;
  if pool_names(paths)?.iter().any(|(id, n)| id != pool && n == name) {
    return Err(format!("Pool existiert bereits: {name}"));
  }
  current.name = name.to_string();
  write_pool_json(&paths.pool_dir(pool), &current)
}

/// Löscht einen Pool samt Ordner (inkl. Credentials, bei apikey auch den
/// Keychain-Eintrag). Zugeordnete Projekte verlieren die Zuordnung
/// (Terminal-Einstellungen bleiben erhalten). Den Schutz gegen laufende
/// Sessions setzt der delete_pool-Command davor.
/// Bei referenzierten Pools fällt nur die Hülle: das verwiesene Verzeichnis
/// und der Login darin gehören der App nicht.
pub(crate) fn delete_pool_in(
  paths: &Paths,
  store: &dyn ApikeyStore,
  name: &str,
) -> Result<(), String> {
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
  fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Schreibt den API-Key eines apikey-Pools neu: in den Keychain/Keyring, die
/// Fallback-Datei wird dabei entfernt (migriert Datei-Pools beim Key-Ändern).
/// Ohne verfügbaren Store: mit allow_file in die Datei (0600), sonst Abbruch
/// ohne Änderung. Der apiKeyHelper wird auf die aktuelle Kette gehoben.
pub(crate) fn set_apikey_in(
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
      fs::remove_file(&key_path).map_err(|e| format!("{}: {e}", key_path.display()))?;
    }
  } else {
    if !allow_file {
      return Err(KEYCHAIN_UNAVAILABLE.into());
    }
    crate::platform::write_secret_file(&key_path, &format!("{key}\n"))?;
  }
  let settings_path = dir.join("settings.json");
  let raw = fs::read_to_string(&settings_path)
    .map_err(|e| format!("{}: {e}", settings_path.display()))?;
  let mut settings: serde_json::Value = serde_json::from_str(&raw)
    .map_err(|e| format!("{}: {e}", settings_path.display()))?;
  settings["apiKeyHelper"] =
    serde_json::json!(crate::platform::apikey_helper_command(&dir, pool));
  let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&settings_path, &(raw + "\n"))
}

pub(crate) fn ensure_oauth_pool(paths: &Paths, pool: &str) -> Result<(), String> {
  let p = read_pool(paths, pool)?;
  if p.credential_type != "oauth" {
    return Err(format!("Pool {pool} ist kein oauth-Pool"));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::project::{
    assign_pool_in, read_project_config_in, set_terminal_config_in, TerminalConfig,
  };
  use crate::domain::registry::project_pool;
  use crate::domain::settings::APP_SETTINGS_FILE;
  use crate::domain::testutil::{
    create_project, make_apikey_pool, make_oauth_pool, map_store, mode, tmp_paths, FailStore,
  };

  /// Ein verwiesener Pool legt nur die Hülle an; im Zielverzeichnis entsteht
  /// die Panel-Ausstattung, aber keine settings.json-Grundausstattung — eine
  /// vorhandene settings.json bleibt inhaltlich unangetastet.
  #[test]
  fn referenzpool_laesst_das_zielverzeichnis_stehen() {
    let p = tmp_paths();
    let ziel = p.default_claude_dir();
    fs::create_dir_all(&ziel).unwrap();
    fs::write(ziel.join("settings.json"), "{\"model\":\"opus\"}\n").unwrap();

    let id = create_reference_pool_in(&p, "System", "~/.claude").unwrap();

    // Hülle trägt nur pool.json mit dem Verweis.
    let huelle = p.pool_dir(&id);
    assert!(huelle.join(POOL_FILE).is_file());
    assert!(!huelle.join("settings.json").exists());
    assert_eq!(read_pool(&p, &id).unwrap().dir.as_deref(), Some("~/.claude"));
    assert_eq!(pool_config_dir(&p, &id).unwrap(), ziel);

    // Im Ziel: Panel-Ausstattung dazu, eigene Einstellungen erhalten.
    let settings: serde_json::Value =
      serde_json::from_str(&fs::read_to_string(ziel.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["model"], "opus");
    assert!(settings["permissions"]["allow"]
      .as_array()
      .unwrap()
      .iter()
      .any(|v| v == "mcp__text-panel__write_panel"));
    assert!(ziel.join("skills").join("panel").join("SKILL.md").is_file());
  }

  /// Zeigt der Pool auf claudes Default-Verzeichnis, bleibt CLAUDE_CONFIG_DIR
  /// ungesetzt — nur so greift der unsuffixierte Keychain-Eintrag, an dem ein
  /// bestehender Login hängt.
  #[test]
  fn referenzpool_auf_default_setzt_kein_config_dir() {
    let p = tmp_paths();
    fs::create_dir_all(p.default_claude_dir()).unwrap();
    let id = create_reference_pool_in(&p, "System", "~/.claude").unwrap();
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &id).unwrap();

    assert_eq!(
      crate::domain::project::project_pool_dir_in(&p, "proj").unwrap(),
      None
    );
  }

  /// Ein Verweis auf ein anderes Verzeichnis setzt CLAUDE_CONFIG_DIR wie
  /// gewohnt — dort hängt der Login am suffixierten Eintrag.
  #[test]
  fn referenzpool_auf_fremdes_verzeichnis_setzt_config_dir() {
    let p = tmp_paths();
    let ziel = p.home.join("woanders");
    fs::create_dir_all(&ziel).unwrap();
    let id = create_reference_pool_in(&p, "Extern", "~/woanders").unwrap();
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &id).unwrap();

    assert_eq!(
      crate::domain::project::project_pool_dir_in(&p, "proj").unwrap(),
      Some(ziel)
    );
  }

  /// Löschen trifft die Hülle, nie das verwiesene Verzeichnis.
  #[test]
  fn referenzpool_loeschen_laesst_das_ziel_unberuehrt() {
    let p = tmp_paths();
    let ziel = p.default_claude_dir();
    fs::create_dir_all(&ziel).unwrap();
    fs::write(ziel.join("settings.json"), "{}\n").unwrap();
    let id = create_reference_pool_in(&p, "System", "~/.claude").unwrap();

    delete_pool_in(&p, &map_store(), &id).unwrap();
    assert!(!p.pool_dir(&id).exists());
    assert!(ziel.join("settings.json").is_file());
  }

  /// Runtime-Symlinks bauen das Verzeichnis um und verwerfen Transkripte —
  /// in fremdem Verzeichnis abgelehnt.
  #[test]
  fn referenzpool_kein_runtime_umbau() {
    let p = tmp_paths();
    fs::create_dir_all(p.default_claude_dir()).unwrap();
    fs::create_dir_all(p.config_dir()).unwrap();
    fs::write(
      p.config_dir().join(APP_SETTINGS_FILE),
      "{\"poolSyncDir\":\"~/sync\"}\n",
    )
    .unwrap();
    let id = create_reference_pool_in(&p, "System", "~/.claude").unwrap();

    let err = link_pool_runtime_in(&p, &id).unwrap_err();
    assert!(err.contains("~/.claude"), "{err}");
  }

  /// Zwei Pools auf dasselbe Verzeichnis wären zwei Namen für einen Login.
  #[test]
  fn referenzpool_nur_einmal_pro_verzeichnis() {
    let p = tmp_paths();
    fs::create_dir_all(p.default_claude_dir()).unwrap();
    create_reference_pool_in(&p, "System", "~/.claude").unwrap();

    assert!(offered_default_dir(&p).is_none());
    let err = create_reference_pool_in(&p, "Nochmal", "~/.claude").unwrap_err();
    assert!(err.contains("System"), "{err}");
  }

  /// Ohne Verzeichnis kein Angebot und kein Pool.
  #[test]
  fn referenzpool_braucht_ein_vorhandenes_verzeichnis() {
    let p = tmp_paths();
    assert_eq!(offered_default_dir(&p), None);
    assert!(create_reference_pool_in(&p, "System", "~/.claude").is_err());

    fs::create_dir_all(p.default_claude_dir()).unwrap();
    assert_eq!(offered_default_dir(&p).as_deref(), Some("~/.claude"));
  }

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
      crate::platform::apikey_helper_command(&dir, &id)
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
      crate::platform::apikey_helper_command(&dir, &id)
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
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &id).unwrap();

    rename_pool_in(&p, &id, "kunde-neu").unwrap();

    // Nur der Anzeigename ändert sich — Ordner, Typ und Zuordnung bleiben.
    let pool = read_pool(&p, &id).unwrap();
    assert_eq!(pool.name, "kunde-neu");
    assert_eq!(pool.credential_type, "apikey");
    assert!(p.pool_dir(&id).is_dir());
    assert_eq!(project_pool(&p, "proj").unwrap().as_deref(), Some(id.as_str()));
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
      crate::platform::apikey_helper_command(&dir, &id)
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
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();
    delete_pool_in(&p, &store, &kunde).unwrap();
    assert!(!p.pool_dir(&kunde).exists());
    assert_eq!(project_pool(&p, "proj").unwrap(), None);
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
  fn pool_loeschen_erhaelt_terminal_config() {
    let p = tmp_paths();
    let store = map_store();
    let kunde = make_apikey_pool(&p, &store, "kunde", "sk-1");
    create_project(&p, "proj").unwrap();
    assign_pool_in(&p, "proj", &kunde).unwrap();
    set_terminal_config_in(
      &p,
      "proj",
      TerminalConfig { theme: Some("dracula".into()), ..Default::default() },
    )
    .unwrap();
    delete_pool_in(&p, &store, &kunde).unwrap();
    assert_eq!(project_pool(&p, "proj").unwrap(), None);
    let cfg = read_project_config_in(&p, "proj").unwrap();
    assert_eq!(cfg.terminal.theme.as_deref(), Some("dracula"));
  }

  #[test]
  fn pool_loeschen_unbekannt_scheitert() {
    let p = tmp_paths();
    assert!(delete_pool_in(&p, &map_store(), "gibtsnicht").is_err());
  }
}
