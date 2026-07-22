//! Linux: Desktop-Erkennung, Secret-Service-Helper, D-Bus-/SNI-Tray.
//! Der Klick-Weg unterscheidet sich je Desktop (GNOME-Extension vs.
//! StatusNotifierItem), das Ergebnis ist überall dasselbe Popup-Fenster.

mod cinnamon;
mod desktop;
mod gnome;
mod kwin;
mod tray_sni;

pub(crate) use desktop::{remove_terminal_desktop, sync_all_desktops, write_terminal_desktop};

use std::path::Path;
use std::process::Command;

use crate::domain::credentials::APIKEY_SERVICE;
use crate::domain::pool::APIKEY_FILE;
use crate::platform::TrayCallbacks;

// ---------- Aktivierung / Fokus / Icons ----------

/// Linux kennt keine kooperative Aktivierungsabtretung.
pub(crate) fn yield_activation(_bundle_id: &str) {}

/// Fenster ohne NSApplication über die Tauri-API fokussieren.
pub(crate) fn activate_self(app: &tauri::AppHandle) {
  use tauri::Manager;
  if let Some(window) = app.webview_windows().values().next() {
    window.set_focus().expect("Terminal-Fenster nicht fokussierbar");
  }
}

/// Linux kennt kein Dock-Icon pro Prozess — das Icon kommt über die
/// .desktop-Datei (StartupWMClass/app_id, siehe desktop.rs).
pub(crate) fn set_dock_icon(_path: &str) {}

/// Fremdprozess-Fokus über die PID steht ohne NSRunningApplication nicht zur
/// Verfügung.
pub(crate) fn focus_terminal(_pid: u32) {}

/// Wayland-app_id des Prozesses (muss vor dem GTK-Init gesetzt sein) —
/// darüber ordnen GNOME/dash-to-dock dem Fenster die .desktop-Datei zu.
pub(crate) fn set_app_id(name: &str) {
  glib::set_prgname(Some(name));
}

pub(crate) fn reveal_path(path: &Path) {
  let target = path.parent().unwrap_or(path);
  let _ = Command::new("xdg-open").arg(target).spawn();
}

/// Selbstinstallation gibt es nur unter macOS — hier installieren deb/rpm.
pub(crate) fn offer_move_to_applications(_paths: &crate::domain::paths::Paths) {}

// ---------- Desktop-Erkennung ----------

/// GNOME-Session? (XDG_CURRENT_DESKTOP enthält „GNOME"). Steuert
/// AppImage-Sperre und Extension-Aktivierung.
pub(crate) fn is_gnome() -> bool {
  std::env::var("XDG_CURRENT_DESKTOP")
    .map(|d| d.to_uppercase().contains("GNOME"))
    .unwrap_or(false)
}

fn is_kde() -> bool {
  std::env::var("XDG_CURRENT_DESKTOP")
    .map(|d| d.to_uppercase().contains("KDE"))
    .unwrap_or(false)
}

/// Cinnamon meldet sich je nach Session als „X-Cinnamon" oder „Cinnamon".
fn is_cinnamon() -> bool {
  std::env::var("XDG_CURRENT_DESKTOP")
    .map(|d| d.to_uppercase().contains("CINNAMON"))
    .unwrap_or(false)
}

// ---------- Secrets ----------

/// apiKeyHelper-Kommando eines apikey-Pools: liest den Key aus dem Keyring
/// (dieselben service/username-Attribute, die die keyring-Crate anlegt),
/// bei fehlendem Eintrag aus der Key-Datei.
pub(crate) fn apikey_helper_command(dir: &Path, pool_id: &str) -> String {
  format!(
    "secret-tool lookup service {APIKEY_SERVICE} username {pool_id} 2>/dev/null || cat '{}'",
    dir.join(APIKEY_FILE).display()
  )
}

/// claudes OAuth-Credentials liegen unter Linux nicht in einem per
/// security-CLI prüfbaren Keychain — Status/Reset sind hier nicht verfügbar.
pub(crate) fn oauth_keychain_exists(_service: &str) -> Result<bool, String> {
  Err("OAuth-Keychain-Status unter Linux nicht verfügbar".into())
}

pub(crate) fn oauth_keychain_delete(_service: &str) -> Result<(), String> {
  Err("OAuth-Keychain-Reset unter Linux nicht verfügbar".into())
}

// ---------- Tray ----------

/// Überall dasselbe Ergebnis — Klick aufs Tray-Icon zeigt das Popup, nie ein
/// Menü. Nur der Weg zum Klick unterscheidet sich: GNOME hat kein Tray, daher
/// die Shell-Extension (D-Bus-Relay); KDE/XFCE/Cinnamon haben ein
/// Standard-Tray (StatusNotifierItem), dessen Activate direkt gemeldet wird.
pub(crate) fn init_tray(app: &tauri::AppHandle, cb: TrayCallbacks) -> Result<(), String> {
  if is_gnome() {
    // Die Extension liegt systemweit (aus dem Paket), aktiviert wird sie pro
    // Benutzer in dconf — das erledigt die im Benutzerkontext laufende App.
    let _ = Command::new("gnome-extensions")
      .args(["enable", "ai-control-popup@local"])
      .status();
    gnome::spawn_dbus_service(app.clone(), cb);
  } else {
    tray_sni::spawn(app.clone(), cb);
    // KDE-Wayland: das Popup-Positionieren übernimmt das KWin-Script.
    if is_kde() {
      kwin::enable_script();
    } else if is_cinnamon() {
      // Wayland-Platzierung + X11-Fokus kommen aus der Cinnamon-Extension;
      // sie liegt systemweit aus dem Paket, aktiviert wird pro Benutzer.
      cinnamon::enable_extension();
    }
  }
  Ok(())
}
