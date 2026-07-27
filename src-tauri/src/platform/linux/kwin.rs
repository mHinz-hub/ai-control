//! KWin-Script pro Benutzer aktivieren und KWin neu laden — analog zu
//! `gnome-extensions enable`. Systemweit liegt es aus dem Paket. Nötig, weil
//! unter Wayland nur der Compositor unser Popup positionieren darf.

use std::process::Command;

pub(super) fn enable_script() {
  let _ = Command::new("kwriteconfig6")
    .args([
      "--file",
      "kwinrc",
      "--group",
      "Plugins",
      "--key",
      "ai-central-popupEnabled",
      "true",
    ])
    .status();
  let _ = Command::new("dbus-send")
    .args([
      "--session",
      "--dest=org.kde.KWin",
      "--type=method_call",
      "/KWin",
      "org.kde.KWin.reconfigure",
    ])
    .status();
}
