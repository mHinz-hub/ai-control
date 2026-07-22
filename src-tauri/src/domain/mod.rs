//! OS-freier Kern, nach Domänen geschnitten. Alles hier ist über `Paths`
//! (injizierbares Home) und den `ApikeyStore`-Trait testbar; OS-Aufrufe
//! laufen ausschließlich über crate::platform.

pub(crate) mod archive;
pub(crate) mod archive_index;
pub(crate) mod archive_search;
pub(crate) mod credentials;
pub(crate) mod paths;
pub(crate) mod pool;
pub(crate) mod project;
pub(crate) mod registry;
pub(crate) mod settings;
pub(crate) mod todo;
pub(crate) mod usage;
pub(crate) mod watcher;

#[cfg(test)]
pub(crate) mod testutil;

/// Datei atomar ersetzen: temp-Datei im Zielverzeichnis, dann rename. Ein
/// Absturz oder eine volle Platte hinterlässt nie eine abgeschnittene
/// Zieldatei — wichtig für Dateien, die der App nicht allein gehören
/// (.claude/settings.json der Projekte, claudes .claude.json, Registry).
pub(crate) fn write_atomic(path: &std::path::Path, content: &str) -> Result<(), String> {
  let dir = path
    .parent()
    .ok_or_else(|| format!("{}: kein Elternordner", path.display()))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .ok_or_else(|| format!("{}: kein Dateiname", path.display()))?;
  let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
  std::fs::write(&tmp, content).map_err(|e| format!("{}: {e}", tmp.display()))?;
  std::fs::rename(&tmp, path).map_err(|e| {
    let _ = std::fs::remove_file(&tmp);
    format!("{}: {e}", path.display())
  })
}

/// Namensprüfung für Projekte und Pool-Anzeigenamen.
///
/// Der Name wird als Pfadsegment verwendet (Pool-Verzeichnis, Panel-Dateien)
/// und landet zugleich in Dateien mit zeilenbasiertem Format — vor allem in der
/// `.desktop`-Datei unter Linux. Darum reicht das Verbot von `/` und `..`
/// nicht: Ein Zeilenumbruch im Namen hängt dort eigene Schlüssel an, und
/// `Exec=` braucht keinen Schrägstrich. Projektnamen stammen aus
/// `dir.file_name()`, bei einem geklonten Fremd-Repo also von außen.
pub(crate) fn check_name(name: &str) -> Result<(), String> {
  let ungueltig = name.trim().is_empty()
    || name.contains('/')
    || name.contains('\\')
    || name.contains("..")
    || name.starts_with('.')
    || name.chars().any(|c| c.is_control());
  if ungueltig {
    return Err(format!("ungültiger Name: {name}"));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::check_name;

  #[test]
  fn namen_pruefung() {
    for ok in ["projekt", "mein-projekt", "Projekt 2", "äöü"] {
      assert!(check_name(ok).is_ok(), "{ok} sollte gültig sein");
    }
    for bad in [
      "",
      "  ",
      "a/b",
      "a\\b",
      "..",
      "../x",
      ".versteckt",
      "boo\nExec=bash -c pwn", // .desktop-Injektion
      "a\tb",
      "a\u{7f}b",
    ] {
      assert!(check_name(bad).is_err(), "{bad:?} sollte abgelehnt werden");
    }
  }
}
