//! Cinnamon-Extension pro Benutzer aktivieren — analog zu
//! `gnome-extensions enable`. Systemweit liegt sie aus dem Paket; Cinnamon
//! lädt sie sofort, sobald die UUID in enabled-extensions auftaucht.

use std::process::Command;

const UUID: &str = "ai-central-popup@local";

pub(super) fn enable_extension() {
  let Ok(out) = Command::new("gsettings")
    .args(["get", "org.cinnamon", "enabled-extensions"])
    .output()
  else {
    return;
  };
  let list = String::from_utf8_lossy(&out.stdout);
  if list.contains(UUID) {
    return;
  }
  // GVariant-Liste erweitern: "['a']" → "['a', 'ai-central-popup@local']",
  // leer ("[]" bzw. typannotiert "@as []") → "['ai-central-popup@local']".
  let inner = list
    .trim()
    .trim_start_matches("@as")
    .trim()
    .trim_start_matches('[')
    .trim_end_matches(']')
    .trim();
  let new = if inner.is_empty() {
    format!("['{UUID}']")
  } else {
    format!("[{inner}, '{UUID}']")
  };
  let _ = Command::new("gsettings")
    .args(["set", "org.cinnamon", "enabled-extensions", &new])
    .status();
}
