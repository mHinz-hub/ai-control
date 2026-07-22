//! OS-Adapter. Dieses Modul definiert die vollständige Liste der
//! plattformabhängigen Operationen — pro Zielsystem existiert genau eine
//! Implementierung mit identischen Signaturen, aufgelöst zur Compile-Zeit
//! (kein dyn Trait). Wer ein neues OS portiert, implementiert diese API:
//!
//! Prozesse           terminal_pids, kill_terminal
//! Fokus/Aktivierung  focus_terminal, activate_self, yield_activation
//! Fenster/Icons      set_dock_icon, set_app_id
//! Dateisystem        home_dir, write_secret_file, symlink, reveal_path
//! Shell              shell_command
//! Secrets            KeychainStore (impl ApikeyStore), apikey_helper_command,
//!                    oauth_keychain_exists, oauth_keychain_delete
//! Desktop            write_terminal_desktop, remove_terminal_desktop,
//!                    sync_all_desktops (nur Linux substantiell, sonst No-ops)
//! Tray               init_tray
//!
//! Vertrag Tray/Popup: Das Popup ist auf allen Systemen dasselbe HTML-Fenster
//! (app.rs). Abstrahiert wird NUR der Trigger — init_tray zeigt ein Icon und
//! meldet den Klick mit einem `Anchor`, der sagt, wo das Popup hin soll.
//! Menüs gibt es nicht; einzige Ausnahme ist der SNI-Rechtsklick „Beenden"
//! unter Linux, weil StatusNotifierItem ein Kontextmenü verlangt.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::{
  home_dir, kill_terminal, shell_command, symlink, terminal_pids, write_secret_file,
};

#[cfg(not(target_os = "macos"))]
mod keyring_store;
#[cfg(not(target_os = "macos"))]
pub(crate) use keyring_store::KeychainStore;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::*;

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows::*;

/// Wo das gemeinsame Popup-Fenster hin soll — mehr weiß app.rs über das OS
/// nicht. Je Zielsystem wird nur eine Teilmenge der Varianten konstruiert.
#[allow(dead_code)]
pub(crate) enum Anchor {
  /// Nativer Tray liefert das Icon-Rect (macOS, Windows). `popup_below`
  /// entscheidet die Plattform: Menüleiste oben → Popup darunter (macOS),
  /// Taskbar unten → Popup darüber (Windows).
  IconRect { rect: tauri::Rect, popup_below: bool },
  /// SNI/ksni (KDE/XFCE/Cinnamon): Klick-Koordinaten aus Activate. Cinnamon
  /// liefert echte Werte; wo der Host keine mitgibt (0,0), fällt app.rs auf
  /// die Zeigerposition zurück — der Zeiger steht beim Klick auf dem Icon.
  Click { x: i32, y: i32 },
  /// Positionierung übernimmt der Compositor bzw. die Shell-Extension
  /// (GNOME, KDE-Wayland/KWin).
  Managed,
}

/// Klick-Relay des Trays an app.rs: `show` zeigt das Popup am Anchor,
/// `hide` versteckt es (nur der GNOME-Toggle ruft hide).
pub(crate) struct TrayCallbacks {
  pub(crate) show: Box<dyn Fn(&tauri::AppHandle, Anchor) + Send + Sync>,
  #[allow(dead_code)] // macOS/Windows togglen nicht — dort versteckt Fokusverlust.
  pub(crate) hide: Box<dyn Fn(&tauri::AppHandle) + Send + Sync>,
}
