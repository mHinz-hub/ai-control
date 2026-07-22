//! Windows: Alpha-Stand. Implementiert ist der native Tray (wie macOS, Popup
//! über der Taskbar), Fenster-Fokus über Tauri, reveal_path und der
//! Credential Manager über die keyring-Crate (keyring_store.rs). Alles
//! andere sind laut scheiternde Stubs — die offenen Punkte stehen an den
//! Funktionen.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::domain::paths::Paths;
use crate::domain::pool::APIKEY_FILE;
use crate::domain::project::ProjectConfig;
use crate::platform::{Anchor, TrayCallbacks};

// ---------- Prozesse ----------

/// TODO Windows: Prozess-Erkennung (Toolhelp-Snapshot über die Kommandozeile
/// `--terminal <projekt>`). Bis dahin gilt kein Projekt als laufend.
pub(crate) fn terminal_pids(_project: &str) -> Vec<u32> {
  Vec::new()
}

pub(crate) fn kill_terminal(_pid: u32) -> Result<(), String> {
  Err("Windows: Terminal-Prozesse beenden ist nicht implementiert".into())
}

// ---------- Aktivierung / Fokus / Icons ----------

pub(crate) fn yield_activation(_bundle_id: &str) {}

pub(crate) fn activate_self(app: &tauri::AppHandle) {
  use tauri::Manager;
  if let Some(window) = app.webview_windows().values().next() {
    window.set_focus().expect("Terminal-Fenster nicht fokussierbar");
  }
}

/// Windows setzt das Taskbar-Icon über das Fenster-Icon (app.rs), nicht pro
/// Prozess.
pub(crate) fn set_dock_icon(_path: &str) {}

/// TODO Windows: laufendes Terminal-Fenster über die PID nach vorn holen.
pub(crate) fn focus_terminal(_pid: u32) {}

pub(crate) fn set_app_id(_name: &str) {}

/// Selbstinstallation gibt es nur unter macOS — hier installiert der NSIS-Setup.
pub(crate) fn offer_move_to_applications(_paths: &crate::domain::paths::Paths) {}

pub(crate) fn reveal_path(path: &Path) {
  let _ = Command::new("explorer")
    .arg(format!("/select,{}", path.display()))
    .spawn();
}

// ---------- Dateisystem / Shell ----------

pub(crate) fn home_dir() -> PathBuf {
  PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE nicht gesetzt"))
}

/// TODO Windows: PATH-Aufbau/Quoting klären (PowerShell vs. cmd); bis dahin
/// cmd /C mit dem konfigurierten Kommando.
pub(crate) fn shell_command(cmd: &str) -> portable_pty::CommandBuilder {
  let mut c = portable_pty::CommandBuilder::new("cmd");
  c.args(["/C", cmd]);
  c
}

/// TODO Windows: Key-Datei mit restriktiver ACL (Pendant zu 0600).
pub(crate) fn write_secret_file(_path: &Path, _content: &str) -> Result<(), String> {
  Err("Windows: geschützte Key-Datei ist nicht implementiert — Credential Manager verwenden".into())
}

/// TODO Windows: Symlinks brauchen Developer-Mode oder Privileg; Junctions
/// wären die Alternative für Ordner.
pub(crate) fn symlink(_target: &Path, _link: &Path) -> Result<(), String> {
  Err("Windows: Pool-Runtime-Symlinks sind nicht implementiert".into())
}

// ---------- Desktop-Integration (nur Linux substantiell) ----------

pub(crate) fn write_terminal_desktop(_paths: &Paths, _project: &str, _cfg: &ProjectConfig) {}
pub(crate) fn remove_terminal_desktop(_paths: &Paths, _project: &str) {}
pub(crate) fn sync_all_desktops(_paths: &Paths) {}

// ---------- Secrets ----------

/// TODO Windows: Helper-Kommando, das den Key aus dem Credential Manager
/// liest (claude führt es in seiner Shell aus). Bis dahin nur die Key-Datei.
pub(crate) fn apikey_helper_command(dir: &Path, _pool_id: &str) -> String {
  format!("type \"{}\"", dir.join(APIKEY_FILE).display())
}

pub(crate) fn oauth_keychain_exists(_service: &str) -> Result<bool, String> {
  Err("Windows: OAuth-Keychain-Status ist nicht implementiert".into())
}

pub(crate) fn oauth_keychain_delete(_service: &str) -> Result<(), String> {
  Err("Windows: OAuth-Keychain-Reset ist nicht implementiert".into())
}

// ---------- Tray ----------

/// Natives Tray-Icon im System-Tray. Links-Klick meldet das Icon-Rect; die
/// Taskbar liegt unten, das Popup gehört über das Icon.
pub(crate) fn init_tray(app: &tauri::AppHandle, cb: TrayCallbacks) -> Result<(), String> {
  use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
  let icon = tauri::image::Image::from_bytes(include_bytes!("../../icons/32x32.png"))
    .map_err(|e| e.to_string())?;
  TrayIconBuilder::with_id("main")
    .icon(icon)
    .on_tray_icon_event(move |tray, event| {
      if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        rect,
        ..
      } = event
      {
        (cb.show)(tray.app_handle(), Anchor::IconRect { rect, popup_below: false });
      }
    })
    .build(app)
    .map_err(|e| e.to_string())?;
  Ok(())
}
