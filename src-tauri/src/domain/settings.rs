//! App-eigene settings.json unter ~/.config/ai-control (nicht pool-/projektbezogen):
//! claudeCommand, syncOnSessionEnd, poolSyncDir, terminalFontSize, spellcheckLang.

use std::fs;
use std::path::PathBuf;

use crate::domain::paths::{expand_home, Paths};

pub(crate) const APP_SETTINGS_FILE: &str = "settings.json";

fn read_app_settings(paths: &Paths) -> Option<serde_json::Value> {
  let raw = fs::read_to_string(paths.config_dir().join(APP_SETTINGS_FILE)).ok()?;
  serde_json::from_str(&raw).ok()
}

/// Opt-in: synct der Watcher bei Session-Ende? Default aus.
pub(crate) fn sync_on_session_end(paths: &Paths) -> bool {
  read_app_settings(paths)
    .and_then(|v| v["syncOnSessionEnd"].as_bool())
    .unwrap_or(false)
}

/// Setzt das Opt-in; erhält übrige App-settings.
pub(crate) fn set_sync_on_session_end_in(paths: &Paths, enabled: bool) -> Result<(), String> {
  write_app_setting(paths, "syncOnSessionEnd", serde_json::json!(enabled))
}

/// Kommando, das im Projekt-Terminal startet (settings.json: claudeCommand).
pub(crate) fn claude_command(paths: &Paths) -> String {
  read_app_settings(paths)
    .and_then(|v| v["claudeCommand"].as_str().map(str::to_string))
    .unwrap_or_else(|| "claude".into())
}

/// Sync-Ziel für Pool-Laufzeitdaten (settings.json: poolSyncDir).
/// Ungesetzt = Feature aus, alle Daten bleiben lokal im Pool-Ordner.
pub(crate) fn pool_sync_dir(paths: &Paths) -> Option<PathBuf> {
  read_app_settings(paths)
    .and_then(|v| v["poolSyncDir"].as_str().map(|d| expand_home(paths, d)))
}

/// Terminal-Schriftgröße (settings.json: terminalFontSize), ein Wert für alle
/// Terminals. Default 13.
pub(crate) fn terminal_font_size(paths: &Paths) -> u32 {
  read_app_settings(paths)
    .and_then(|v| v["terminalFontSize"].as_u64())
    .map(|n| n as u32)
    .unwrap_or(13)
}

/// Setzt die Schriftgröße; erhält übrige App-settings.
pub(crate) fn set_terminal_font_size_in(paths: &Paths, size: u32) -> Result<(), String> {
  write_app_setting(paths, "terminalFontSize", serde_json::json!(size))
}

/// Standard-Sprache der Rechtschreibprüfung (settings.json: spellcheckLang),
/// Default „de". Pro Text im Panel überschreibbar.
pub(crate) fn spellcheck_lang(paths: &Paths) -> String {
  read_app_settings(paths)
    .and_then(|v| v["spellcheckLang"].as_str().map(str::to_string))
    .unwrap_or_else(|| "de".to_string())
}

/// Hat der Nutzer das Angebot, die App nach ~/Applications zu holen, schon
/// abgelehnt? (settings.json: moveOfferDismissed, nur macOS)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn move_offer_dismissed(paths: &Paths) -> bool {
  read_app_settings(paths)
    .and_then(|v| v["moveOfferDismissed"].as_bool())
    .unwrap_or(false)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn set_move_offer_dismissed_in(paths: &Paths, dismissed: bool) -> Result<(), String> {
  write_app_setting(paths, "moveOfferDismissed", serde_json::json!(dismissed))
}

/// Einen Schlüssel setzen; übrige App-settings bleiben erhalten.
///
/// Nur eine *fehlende* Datei rechtfertigt ein frisches Objekt. Unlesbar oder
/// kaputtes JSON bricht ab — sonst würde ein einziges Verstellen der
/// Schriftgröße claudeCommand, poolSyncDir & Co. endgültig verwerfen
/// (read_app_settings wirft für die Lese-Getter alles in denselben None).
fn write_app_setting(
  paths: &Paths,
  key: &str,
  value: serde_json::Value,
) -> Result<(), String> {
  let path = paths.config_dir().join(APP_SETTINGS_FILE);
  let mut v: serde_json::Value = match fs::read_to_string(&path) {
    Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
    Err(e) => return Err(format!("{}: {e}", path.display())),
  };
  v[key] = value;
  fs::create_dir_all(paths.config_dir())
    .map_err(|e| format!("{}: {e}", paths.config_dir().display()))?;
  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  crate::domain::write_atomic(&path, &(raw + "\n"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::tmp_paths;

  #[test]
  fn sync_optin_default_aus_und_umschaltbar() {
    let p = tmp_paths();
    assert!(!sync_on_session_end(&p)); // default: kein Sync ohne Zustimmung
    set_sync_on_session_end_in(&p, true).unwrap();
    assert!(sync_on_session_end(&p));
    set_sync_on_session_end_in(&p, false).unwrap();
    assert!(!sync_on_session_end(&p));
  }
}
