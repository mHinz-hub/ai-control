//! OS-freier Kern, nach Domänen geschnitten. Alles hier ist über `Paths`
//! (injizierbares Home) und den `ApikeyStore`-Trait testbar; OS-Aufrufe
//! laufen ausschließlich über crate::platform.

pub(crate) mod archive;
pub(crate) mod archive_index;
pub(crate) mod archive_html;
pub(crate) mod archive_ops;
pub(crate) mod archive_search;
pub(crate) mod search_index;
pub(crate) mod credentials;
pub(crate) mod epub;
pub(crate) mod git;
pub(crate) mod modules;
pub(crate) mod paths;
pub(crate) mod pool;
pub(crate) mod project;
pub(crate) mod registry;
pub(crate) mod settings;
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

/// Read-modify-write auf dem `permissions`-Objekt einer Claude-settings.json
/// (Projekt wie Pool). `create` behandelt eine fehlende Datei als leeres
/// Objekt — für Einträge, die die Datei erst anlegen; ohne `create` scheitert
/// das Lesen laut. Geschrieben wird nur, wenn `f` den Inhalt tatsächlich
/// verändert hat — die Datei gehört claude, unnötige Writes unterbleiben.
pub(crate) fn update_settings_permissions(
  sp: &std::path::Path,
  create: bool,
  f: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<(), String>,
) -> Result<(), String> {
  let mut v: serde_json::Value = if !sp.is_file() && create {
    serde_json::json!({})
  } else {
    let raw = std::fs::read_to_string(sp).map_err(|e| format!("{}: {e}", sp.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", sp.display()))?
  };
  let before = v.clone();
  {
    let root = v.as_object_mut().ok_or("settings.json ist kein Objekt")?;
    let perms = root
      .entry("permissions")
      .or_insert_with(|| serde_json::json!({}))
      .as_object_mut()
      .ok_or("permissions ist kein Objekt")?;
    f(perms)?;
  }
  // Ein hier erst angelegtes, leer gebliebenes permissions-Objekt wieder
  // entfernen — sonst zählte ein No-op als Änderung.
  if before.get("permissions").is_none()
    && v["permissions"].as_object().is_some_and(|o| o.is_empty())
  {
    v.as_object_mut().unwrap().remove("permissions");
  }
  if v == before {
    return Ok(());
  }
  let parent = sp.parent().ok_or("settings.json ohne Elternordner")?;
  std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
  let raw = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
  write_atomic(sp, &(raw + "\n"))
}

/// Array-Feld im permissions-Objekt, bei Bedarf angelegt.
pub(crate) fn perm_array<'a>(
  perms: &'a mut serde_json::Map<String, serde_json::Value>,
  key: &str,
) -> Result<&'a mut Vec<serde_json::Value>, String> {
  perms
    .entry(key)
    .or_insert_with(|| serde_json::json!([]))
    .as_array_mut()
    .ok_or_else(|| format!("{key} ist kein Array"))
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
