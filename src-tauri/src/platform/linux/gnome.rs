//! D-Bus-Relay für die GNOME-Extension: Show()/Hide() steuern das
//! Popup-Fenster. Die Extension ruft Show() beim Panel-Klick und positioniert
//! das Fenster selbst (Anchor::Managed) — unter Wayland darf nur der
//! Compositor fremde Fenster platzieren.

use crate::platform::{Anchor, TrayCallbacks};

struct PopupService {
  app: tauri::AppHandle,
  cb: TrayCallbacks,
}

#[zbus::interface(name = "com.aicentral.Popup1")]
impl PopupService {
  fn show(&self) {
    (self.cb.show)(&self.app, Anchor::Managed);
  }
  fn hide(&self) {
    (self.cb.hide)(&self.app);
  }
}

/// Hält den D-Bus-Namen com.aicentral.Popup, solange die App läuft — die
/// Extension zeigt ihren Panel-Button nur, wenn der Name präsent ist.
pub(super) fn spawn_dbus_service(app: tauri::AppHandle, cb: TrayCallbacks) {
  std::thread::spawn(move || {
    let serve = move || -> zbus::Result<()> {
      let _conn = zbus::blocking::connection::Builder::session()?
        .name("com.aicentral.Popup")?
        .serve_at("/com/aicentral/Popup", PopupService { app, cb })?
        .build()?;
      loop {
        std::thread::park();
      }
    };
    if let Err(e) = serve() {
      eprintln!("D-Bus-Popup-Dienst: {e}");
    }
  });
}
