//! macOS: Aktivierung/Fokus über AppKit (objc2), Dock-Icons pro
//! Terminal-Prozess, Keychain über das security-CLI, nativer Tray.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::domain::credentials::{ApikeyStore, APIKEY_SERVICE};
use crate::domain::paths::Paths;
use crate::domain::pool::APIKEY_FILE;
use crate::domain::project::ProjectConfig;
use crate::platform::{Anchor, TrayCallbacks};

// ---------- Aktivierung / Fokus / Icons ----------

/// Tritt die Aktivierung an den nächsten startenden Prozess mit dieser
/// Bundle-ID ab (der Terminal-Prozess läuft unter derselben). Aktivierung ist
/// seit macOS 14 kooperativ — ohne yield öffnet das Terminal-Fenster hinter
/// der aktiven App.
pub(crate) fn yield_activation(bundle_id: &str) {
  use objc2::MainThreadMarker;
  use objc2_app_kit::NSApplication;
  use objc2_foundation::NSString;
  let mtm = MainThreadMarker::new().expect("yield_activation läuft nicht auf dem Main-Thread");
  NSApplication::sharedApplication(mtm)
    .yieldActivationToApplicationWithBundleIdentifier(&NSString::from_str(bundle_id));
}

/// Selbst-Aktivierung des frisch gestarteten Terminal-Prozesses (Ready-Event);
/// die Gegenseite hat vorher per yield abgetreten.
pub(crate) fn activate_self(app: &tauri::AppHandle) {
  use objc2::MainThreadMarker;
  use objc2_app_kit::NSApplication;
  use tauri::Manager;
  let mtm = MainThreadMarker::new().expect("activate_self läuft nicht auf dem Main-Thread");
  NSApplication::sharedApplication(mtm).activate();
  if let Some(window) = app.webview_windows().values().next() {
    window.set_focus().expect("Terminal-Fenster nicht fokussierbar");
  }
}

/// Setzt das Dock-Icon dieses Terminal-Prozesses aus einer PNG/ICNS-Datei.
pub(crate) fn set_dock_icon(path: &str) {
  use objc2::{AnyThread, MainThreadMarker};
  use objc2_app_kit::{NSApplication, NSImage};
  use objc2_foundation::NSString;

  let mtm = MainThreadMarker::new().expect("set_dock_icon läuft nicht auf dem Main-Thread");
  // Nicht ladbar (Datei weg, TCC-geschützter Ordner wie ~/Downloads): Terminal
  // ohne eigenes Dock-Icon starten statt den Prozess zu beenden.
  let Some(img) = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))
  else {
    eprintln!("Dock-Icon nicht ladbar, Terminal startet ohne: {path}");
    return;
  };
  // unsafe laut objc2-Signatur; das NSImage stammt aus einer Datei und ist gültig.
  unsafe {
    NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img));
  }
}

/// Holt das Terminal-Fenster eines laufenden Projekts in den Vordergrund:
/// aktiviert den Terminal-Prozess über seine PID. Aktivierung ist seit
/// macOS 14 kooperativ — die Tray-App tritt sie vorher ab.
pub(crate) fn focus_terminal(pid: u32) {
  use objc2::MainThreadMarker;
  use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};
  let Some(term) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
  else {
    eprintln!("Terminal-Prozess {pid} nicht gefunden");
    return;
  };
  let mtm = MainThreadMarker::new().expect("focus_terminal läuft nicht auf dem Main-Thread");
  NSApplication::sharedApplication(mtm).yieldActivationToApplication(&term);
  term.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
}

/// Wayland-app_id gibt es nur unter Linux.
pub(crate) fn set_app_id(_name: &str) {}

pub(crate) fn reveal_path(path: &Path) {
  let _ = Command::new("open").arg("-R").arg(path).spawn();
}

// ---------- Selbstinstallation ----------

/// App-Bundle zum Programmpfad: …/ai-central.app/Contents/MacOS/ai-central.
/// None, wenn das Binary nicht aus einem .app läuft (cargo run, CLI-Rollen).
fn bundle_dir(exe: &Path) -> Option<&Path> {
  let dir = exe.parent()?.parent()?.parent()?;
  (dir.extension()? == "app").then_some(dir)
}

/// Wohin die App sich holen würde: nach ~/Applications, sofern sie noch in
/// keinem Programme-Ordner liegt und dort nichts gleichnamiges steht. Ein
/// Bundle im DMG (`/Volumes/…`) oder in Downloads fällt darunter.
fn move_target(exe: &Path, home: &Path) -> Option<(PathBuf, PathBuf)> {
  let bundle = bundle_dir(exe)?;
  let parent = bundle.parent()?;
  let user_apps = home.join("Applications");
  if parent == user_apps || parent == Path::new("/Applications") {
    return None;
  }
  let target = user_apps.join(bundle.file_name()?);
  (!target.exists()).then(|| (bundle.to_path_buf(), target))
}

/// Einmalig anbieten, sich nach ~/Applications zu holen — der Weg dorthin
/// braucht kein Administrator-Passwort, anders als /Applications. Läuft die
/// App schon aus einem Programme-Ordner oder hat der Nutzer abgelehnt,
/// passiert nichts. Im Debug-Build nie (dort liegt das Bundle im target/).
pub(crate) fn offer_move_to_applications(paths: &Paths) {
  if cfg!(debug_assertions) || crate::domain::settings::move_offer_dismissed(paths) {
    return;
  }
  let Ok(exe) = std::env::current_exe() else { return };
  let Some((bundle, target)) = move_target(&exe, &paths.home) else {
    return;
  };

  let deutsch = std::env::var("LANG").unwrap_or_default().starts_with("de");
  let (title, body, yes, no) = if deutsch {
    (
      "aiCentral installieren?",
      "Die App läuft noch nicht aus einem Programme-Ordner. Soll sie sich nach „Applications\" im Benutzerordner holen und von dort neu starten?",
      "Installieren",
      "Später",
    )
  } else {
    (
      "Install aiCentral?",
      "The app is not running from an applications folder yet. Move it to \"Applications\" in your home folder and restart from there?",
      "Install",
      "Not now",
    )
  };
  let antwort = rfd::MessageDialog::new()
    .set_level(rfd::MessageLevel::Info)
    .set_title(title)
    .set_description(body)
    .set_buttons(rfd::MessageButtons::OkCancelCustom(yes.into(), no.into()))
    .show();
  if antwort != rfd::MessageDialogResult::Custom(yes.into()) {
    let _ = crate::domain::settings::set_move_offer_dismissed_in(paths, true);
    return;
  }

  // ditto statt fs::copy: überträgt Rechte und erweiterte Attribute des
  // Bundles am Stück. Scheitert es, bleibt alles wie es war.
  if std::fs::create_dir_all(target.parent().unwrap()).is_err()
    || !Command::new("/usr/bin/ditto")
      .args([&bundle, &target])
      .status()
      .is_ok_and(|s| s.success())
  {
    return;
  }
  // Die Kopie erbt die Quarantäne des Downloads; ohne sie zu lösen käme beim
  // Neustart erneut die Gatekeeper-Warnung für dieselbe, gerade laufende App.
  let _ = Command::new("/usr/bin/xattr")
    .args(["-dr", "com.apple.quarantine"])
    .arg(&target)
    .status();
  let _ = Command::new(LSREGISTER).arg("-f").arg(&target).status();
  let _ = Command::new("open").arg("-n").arg(&target).status();
  std::process::exit(0);
}

const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

#[cfg(test)]
mod install_tests {
  use super::*;

  fn exe_in(dir: &str) -> PathBuf {
    Path::new(dir).join("ai-central.app/Contents/MacOS/ai-central")
  }

  #[test]
  fn angebot_aus_dmg_und_downloads() {
    let home = Path::new("/Users/t");
    let (bundle, target) = move_target(&exe_in("/Volumes/ai-central"), home).unwrap();
    assert_eq!(bundle, Path::new("/Volumes/ai-central/ai-central.app"));
    assert_eq!(target, Path::new("/Users/t/Applications/ai-central.app"));
    assert!(move_target(&exe_in("/Users/t/Downloads"), home).is_some());
  }

  /// Aus einem Programme-Ordner heraus gibt es nichts anzubieten.
  #[test]
  fn kein_angebot_aus_programme_ordnern() {
    let home = Path::new("/Users/t");
    assert!(move_target(&exe_in("/Users/t/Applications"), home).is_none());
    assert!(move_target(&exe_in("/Applications"), home).is_none());
  }

  /// Ohne .app-Bundle (cargo run, MCP-Rolle) kein Angebot.
  #[test]
  fn kein_angebot_ohne_bundle() {
    assert!(move_target(Path::new("/w/target/release/ai-central"), Path::new("/Users/t")).is_none());
  }
}

// ---------- Desktop-Integration (nur Linux substantiell) ----------

pub(crate) fn write_terminal_desktop(_paths: &Paths, _project: &str, _cfg: &ProjectConfig) {}
pub(crate) fn remove_terminal_desktop(_paths: &Paths, _project: &str) {}
pub(crate) fn sync_all_desktops(_paths: &Paths) {}

// ---------- Secrets ----------

/// macOS über das security-CLI: dessen Einträge tragen /usr/bin/security in
/// der ACL, der apiKeyHelper (liest ebenfalls per security-CLI beim
/// claude-Start) kommt dadurch ohne Keychain-Prompt an den Key. Über das
/// Security-Framework angelegte Einträge (keyring-Crate) würden beim Lesen
/// durchs CLI prompten.
pub(crate) struct KeychainStore;

impl ApikeyStore for KeychainStore {
  fn set(&self, pool: &str, key: &str) -> Result<(), String> {
    // -w ohne Wert: security liest das Secret über stdin, zweimal (Eingabe und
    // Bestätigung). Als Argument übergeben stünde der Key für die Dauer des
    // Aufrufs in der Prozessliste und wäre für jeden `ps` lesbar.
    let mut child = Command::new("security")
      .args(["add-generic-password", "-U", "-s", APIKEY_SERVICE, "-a", pool, "-w"])
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .map_err(|e| e.to_string())?;
    child
      .stdin
      .take()
      .ok_or("security: kein stdin")?
      .write_all(format!("{key}\n{key}\n").as_bytes())
      .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
      Ok(())
    } else {
      // Die Eingabeaufforderungen landen auf stderr und stehen sonst vor der
      // eigentlichen Fehlermeldung.
      let err = String::from_utf8_lossy(&out.stderr)
        .replace("password data for new item: ", "")
        .replace("retype password for new item: ", "");
      Err(err.trim().to_string())
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

/// apiKeyHelper-Kommando eines apikey-Pools: liest den Key aus dem Keychain,
/// bei fehlendem Eintrag aus der Key-Datei.
pub(crate) fn apikey_helper_command(dir: &Path, pool_id: &str) -> String {
  format!(
    "security find-generic-password -w -s {APIKEY_SERVICE} -a {pool_id} 2>/dev/null || cat '{}'",
    dir.join(APIKEY_FILE).display()
  )
}

/// Existiert claudes suffixierter OAuth-Keychain-Eintrag?
pub(crate) fn oauth_keychain_exists(service: &str) -> Result<bool, String> {
  let out = Command::new("security")
    .args(["find-generic-password", "-s", service])
    .output()
    .map_err(|e| e.to_string())?;
  Ok(out.status.success())
}

/// Löscht claudes suffixierten OAuth-Keychain-Eintrag (Pool-Reset).
pub(crate) fn oauth_keychain_delete(service: &str) -> Result<(), String> {
  let out = Command::new("security")
    .args(["delete-generic-password", "-s", service])
    .output()
    .map_err(|e| e.to_string())?;
  if out.status.success() {
    Ok(())
  } else {
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
  }
}

// ---------- Tray ----------

/// Natives Tray-Icon in der Menüleiste (Template-Icon). Links-Klick meldet
/// das Icon-Rect; das Popup gehört unter das Menüleisten-Icon.
pub(crate) fn init_tray(app: &tauri::AppHandle, cb: TrayCallbacks) -> Result<(), String> {
  use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
  let icon = tauri::image::Image::from_bytes(include_bytes!("../../icons/trayTemplate.png"))
    .map_err(|e| e.to_string())?;
  TrayIconBuilder::with_id("main")
    .icon(icon)
    .icon_as_template(true)
    .on_tray_icon_event(move |tray, event| {
      if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        rect,
        ..
      } = event
      {
        (cb.show)(tray.app_handle(), Anchor::IconRect { rect, popup_below: true });
      }
    })
    .build(app)
    .map_err(|e| e.to_string())?;
  Ok(())
}
