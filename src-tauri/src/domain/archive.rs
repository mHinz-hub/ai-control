//! Archiv: Panel-Entwürfe als Markdown-Dateien mit Frontmatter im
//! konfigurierten Archiv-Ordner des Projekts persistieren.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::paths::{contract_home, expand_home, panel_file, Paths};
use crate::domain::project::{
  read_project_config_in, settings_path, write_project_config_in,
};
use crate::domain::registry::project_dir;

/// Konfiguriertes Archiv-Home des Projekts (config.json: archiveHome),
/// Home-expandiert; None, wenn nicht gesetzt.
pub(crate) fn project_archive_home(project: &str) -> Option<PathBuf> {
  let paths = Paths::real();
  let dir = read_project_config_in(&paths, project).ok()?.archive_home?;
  Some(expand_home(&paths, &dir))
}

/// Archiv-Home als Result — die gemeinsame Vorbedingung aller Archiv-Tools.
/// Legt den konfigurierten Ordner an, falls er (noch) nicht existiert.
pub(crate) fn require_archive_home(project: &str) -> Result<PathBuf, String> {
  let home = project_archive_home(project).ok_or("kein Archiv-Ordner gesetzt")?;
  fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
  Ok(home)
}

/// Setzt das Archiv-Home: config.json (~-relativ) und ein Eintrag in
/// permissions.additionalDirectories + Edit der Projekt-settings.json, damit
/// claude das Archiv später lesen/scannen darf.
pub(crate) fn set_project_archive_home(project: &str, dir: &str) -> Result<(), String> {
  let paths = Paths::real();
  let expanded = expand_home(&paths, dir);
  if !expanded.is_absolute() {
    return Err(format!("Archiv-Ordner muss ein absoluter Pfad sein: {}", expanded.display()));
  }
  // Zu weit gefasst? Ein Vorfahre des Home (inkl. Home selbst und Root) würde
  // claude über additionalDirectories weiten Zugriff geben — das lehnen wir ab.
  if paths.home.starts_with(&expanded) {
    return Err(format!(
      "Archiv-Ordner zu weit gefasst: {}. Bitte einen spezifischen Unterordner wählen.",
      expanded.display()
    ));
  }
  fs::create_dir_all(&expanded).map_err(|e| format!("{}: {e}", expanded.display()))?;
  let contracted = contract_home(&paths, &expanded);
  let mut cfg = read_project_config_in(&paths, project)?;
  cfg.archive_home = Some(contracted.clone());
  write_project_config_in(&paths, project, &cfg)?;
  add_archive_permission(&paths, project, &contracted)
}

/// Trägt den Archiv-Ordner idempotent in additionalDirectories + Edit-Allow ein.
/// Auch beim Projekt-Import im Einsatz (mitgebrachtes archiveHome).
pub(crate) fn add_archive_permission(
  paths: &Paths,
  project: &str,
  dir: &str,
) -> Result<(), String> {
  let sp = settings_path(&project_dir(paths, project)?);
  crate::domain::update_settings_permissions(&sp, true, |perms| {
    let dirs = crate::domain::perm_array(perms, "additionalDirectories")?;
    if !dirs.iter().any(|d| d.as_str() == Some(dir)) {
      dirs.push(serde_json::json!(dir));
    }
    let edit = format!("Edit({dir}/**)");
    let allow = crate::domain::perm_array(perms, "allow")?;
    if !allow.iter().any(|p| p.as_str() == Some(&edit)) {
      allow.push(serde_json::json!(edit));
    }
    Ok(())
  })
}

/// Wechselt das Archiv-Home: neues Home setzen (validieren, anlegen, Rechte,
/// Config), auf Wunsch die Dokumente aus dem alten Home hinüberziehen, dann
/// die Rechte des alten zurücknehmen. Ohne `migrate` bleibt das alte Archiv
/// unverändert liegen — nichts wird implizit verschoben; der Umzug ist eine
/// bewusste Option im Dialog.
pub(crate) fn change_project_archive_home(
  project: &str,
  dir: &str,
  migrate: bool,
) -> Result<(), String> {
  let paths = Paths::real();
  let old = read_project_config_in(&paths, project)?.archive_home;
  set_project_archive_home(project, dir)?;
  let new_home = expand_home(&paths, dir);
  if let Some(old_c) = old {
    let old_home = expand_home(&paths, &old_c);
    if old_home == new_home {
      return Ok(());
    }
    if migrate && old_home.is_dir() {
      move_dir_contents(&old_home, &new_home)?;
    }
    remove_archive_permission(&paths, project, &old_c)?;
  }
  Ok(())
}

/// Zieht alle Einträge von `from` nach `to` um. Gleiche Platte per rename,
/// über Dateisystemgrenzen als Kopie + Löschen. Ein im Ziel schon vorhandener
/// Name bricht laut ab — nichts wird überschrieben.
fn move_dir_contents(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
  for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
    let entry = entry.map_err(|e| format!("{}: {e}", from.display()))?;
    let src = entry.path();
    let dest = to.join(entry.file_name());
    if dest.exists() {
      return Err(format!("existiert schon im neuen Archiv: {}", dest.display()));
    }
    match fs::rename(&src, &dest) {
      Ok(()) => {}
      Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
        copy_recursive(&src, &dest)?;
        if src.is_dir() {
          fs::remove_dir_all(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        } else {
          fs::remove_file(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        }
      }
      Err(e) => return Err(format!("{}: {e}", src.display())),
    }
  }
  Ok(())
}

fn copy_recursive(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
  if src.is_dir() {
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
      let entry = entry.map_err(|e| format!("{}: {e}", src.display()))?;
      copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
    }
  } else {
    fs::copy(src, dest).map_err(|e| format!("{}: {e}", src.display()))?;
  }
  Ok(())
}

/// Wählt das Archiv ab: archiveHome aus der config.json entfernen und die
/// beim Setzen eingetragenen Rechte (additionalDirectories + Edit-Allow) aus
/// der Projekt-settings.json zurücknehmen. Der Ordner selbst bleibt liegen.
pub(crate) fn clear_project_archive_home(project: &str) -> Result<(), String> {
  let paths = Paths::real();
  let mut cfg = read_project_config_in(&paths, project)?;
  let Some(dir) = cfg.archive_home.take() else {
    return Ok(());
  };
  write_project_config_in(&paths, project, &cfg)?;
  remove_archive_permission(&paths, project, &dir)
}

/// Gegenstück zu add_archive_permission; auch der Lösch-Dialog (Stufe „nur
/// Integration") nimmt darüber die Archiv-Rechte zurück.
pub(crate) fn remove_archive_permission(
  paths: &Paths,
  project: &str,
  dir: &str,
) -> Result<(), String> {
  let sp = settings_path(&project_dir(paths, project)?);
  if !sp.is_file() {
    return Ok(());
  }
  crate::domain::update_settings_permissions(&sp, false, |perms| {
    if let Some(dirs) = perms.get_mut("additionalDirectories").and_then(|d| d.as_array_mut()) {
      dirs.retain(|d| d.as_str() != Some(dir));
    }
    let edit = format!("Edit({dir}/**)");
    if let Some(allow) = perms.get_mut("allow").and_then(|a| a.as_array_mut()) {
      allow.retain(|p| p.as_str() != Some(&edit));
    }
    Ok(())
  })
}

/// Metadaten beim Archivieren: Unterordner im Archiv-Home plus Frontmatter-Felder.
#[derive(Default)]
pub(crate) struct ArchiveMeta {
  /// Titel aus dem Archiv-Formular; ohne ihn gilt die erste Überschrift.
  pub(crate) title: Option<String>,
  /// Unterordner relativ zum Archiv-Home (wird angelegt).
  pub(crate) folder: Option<String>,
  /// Einzeiler fürs Frontmatter.
  pub(crate) description: Option<String>,
  /// Schlagwörter fürs Frontmatter.
  pub(crate) tags: Vec<String>,
}

/// Archiviert den aktuellen Panel-Inhalt des Projekts als Markdown-Datei mit
/// Frontmatter im Archiv-Home. `dir_override` setzt das Home zugleich
/// (Terminal-Fallback ohne Dialog). Liefert den geschriebenen Pfad.
pub(crate) fn archive_panel_content(
  project: &str,
  dir_override: Option<&str>,
  meta: &ArchiveMeta,
) -> Result<PathBuf, String> {
  // Erst prüfen, dann konfigurieren: `set_project_archive_home` legt Ordner an
  // und trägt eine Berechtigung in die settings.json des Projekts ein. Käme das
  // vor der Leer-Prüfung, hinterließe ein Archivieren mit leerem Panel die
  // Meldung „nicht archiviert“ — und ein dauerhaft umgestelltes Archiv-Home.
  let text = fs::read_to_string(panel_file(project)).unwrap_or_default();
  if text.trim().is_empty() {
    return Err("Panel ist leer — nichts zu archivieren".into());
  }
  let folder = meta.folder.as_deref().map(check_folder).transpose()?;
  if let Some(d) = dir_override {
    set_project_archive_home(project, d)?;
  }
  let home = require_archive_home(project)?;
  let dir = match folder {
    Some(f) => home.join(f),
    None => home,
  };
  fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

  let secs = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs();
  let (stamp, iso) = utc_stamp(secs);
  let title = meta.title.clone().unwrap_or_else(|| first_line(&text));
  let (path, mut file) = create_unique(&dir, &stamp, &slugify(&title))?;
  // Frontmatter trägt den Anzeigenamen, nicht die Projekt-ID.
  let name = crate::domain::project::display_name_in(&Paths::real(), project)?;
  let doc = format!("{}{}\n", frontmatter(&title, &name, &iso, meta), text.trim_end());
  std::io::Write::write_all(&mut file, doc.as_bytes())
    .map_err(|e| format!("{}: {e}", path.display()))?;
  Ok(path)
}

/// Archiv-Dokument kollisionsfrei anlegen.
///
/// Der Zeitstempel hat Minutenauflösung; zweimal Archivieren innerhalb einer
/// Minute mit derselben Titelzeile ergäbe denselben Namen — bei Kollision
/// wird `-2`, `-3`, … angehängt. Die Garantie kommt vom Dateisystem
/// (`create_new`), nicht von einem `exists()`-Vorabblick: Beim Archiv-Sync
/// über zwei Maschinen wäre der Vorabblick ein TOCTOU-Fenster, und stiller
/// Datenverlust träfe ausgerechnet die dauerhafte Ablage.
fn create_unique(
  dir: &std::path::Path,
  stamp: &str,
  slug: &str,
) -> Result<(PathBuf, fs::File), String> {
  for n in 1.. {
    let name = if n == 1 {
      format!("{stamp}-{slug}.md")
    } else {
      format!("{stamp}-{slug}-{n}.md")
    };
    let path = dir.join(name);
    match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
      Ok(file) => return Ok((path, file)),
      Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
      Err(e) => return Err(format!("{}: {e}", path.display())),
    }
  }
  unreachable!()
}

/// Unterordner-Pfad: relativ, nur normale Komponenten (kein `..`, kein Root).
fn check_folder(folder: &str) -> Result<&std::path::Path, String> {
  let p = std::path::Path::new(folder);
  let normal = p
    .components()
    .all(|c| matches!(c, std::path::Component::Normal(_)));
  if p.components().next().is_some() && normal {
    Ok(p)
  } else {
    Err(format!("Unterordner muss ein relativer Pfad ohne '..' sein: {folder}"))
  }
}

/// YAML-Frontmatter des Archiv-Dokuments inklusive optionaler
/// description/tags aus den Metadaten. Gegenstück: `parse_frontmatter` unten —
/// Schreiber und Leser des Formats leben bewusst im selben Modul.
pub(crate) fn frontmatter(
  title: &str,
  project: &str,
  iso: &str,
  meta: &ArchiveMeta,
) -> String {
  let mut fm = format!(
    "---\nid: {}\ntitle: \"{}\"\nproject: {project}\ncreated: {iso}\nsource: ai-central\n",
    uuid::Uuid::new_v4(),
    title.replace('"', "'"),
  );
  if let Some(d) = &meta.description {
    fm.push_str(&format!("description: \"{}\"\n", d.replace('"', "'")));
  }
  if !meta.tags.is_empty() {
    let quoted: Vec<String> =
      meta.tags.iter().map(|t| format!("\"{}\"", t.replace('"', "'"))).collect();
    fm.push_str(&format!("tags: [{}]\n", quoted.join(", ")));
  }
  fm.push_str("---\n\n");
  fm
}

/// Minimaler Frontmatter-Parser für die selbst geschriebenen Dokumente:
/// `key: value`-Zeilen zwischen den beiden `---`-Markern, Anführungszeichen
/// um Werte werden entfernt.
pub(crate) fn parse_frontmatter(text: &str) -> std::collections::HashMap<String, String> {
  let mut map = std::collections::HashMap::new();
  let Some(rest) = text.strip_prefix("---\n") else {
    return map;
  };
  let Some(end) = rest.find("\n---") else {
    return map;
  };
  for line in rest[..end].lines() {
    let Some((key, value)) = line.split_once(':') else {
      continue;
    };
    map.insert(key.trim().to_string(), unquote(value.trim()).to_string());
  }
  map
}

/// Dokument-Rumpf ohne den Frontmatter-Block; führende Leerzeilen entfernt.
pub(crate) fn strip_frontmatter(text: &str) -> &str {
  let Some(rest) = text.strip_prefix("---\n") else {
    return text;
  };
  match rest.find("\n---\n") {
    Some(end) => rest[end + 5..].trim_start_matches('\n'),
    None => text,
  }
}

fn unquote(s: &str) -> &str {
  s.strip_prefix('"')
    .and_then(|s| s.strip_suffix('"'))
    .unwrap_or(s)
}

/// Inline-Liste `["a", "b"]` bzw. `[a, b]` in Einzel-Tags zerlegen.
pub(crate) fn parse_tag_list(raw: &str) -> Vec<String> {
  raw
    .trim_start_matches('[')
    .trim_end_matches(']')
    .split(',')
    .map(|t| unquote(t.trim()).to_string())
    .filter(|t| !t.is_empty())
    .collect()
}

/// Datei-Stem ohne führenden `YYYY-MM-DD_HHMM-`-Zeitstempel — das Gegenstück
/// zum Dateinamen aus `utc_stamp` + `slugify`.
pub(crate) fn strip_stamp(stem: &str) -> &str {
  let bytes = stem.as_bytes();
  let stamped = bytes.len() > 16
    && bytes[..16].iter().enumerate().all(|(i, b)| match i {
      4 | 7 => *b == b'-',
      10 => *b == b'_',
      15 => *b == b'-',
      _ => b.is_ascii_digit(),
    });
  if stamped {
    &stem[16..]
  } else {
    stem
  }
}

/// Titelzeile: erste Überschrift (## …) oder sonst erste nicht-leere Zeile.
/// Titel des aktuellen Panel-Entwurfs (erste Überschrift bzw. erste Zeile) —
/// Vorbelegung für das Titel-Feld im Archiv-Formular.
pub(crate) fn panel_title(project: &str) -> String {
  first_line(&fs::read_to_string(panel_file(project)).unwrap_or_default())
}

fn first_line(text: &str) -> String {
  let mut fallback: Option<&str> = None;
  for line in text.lines() {
    let t = line.trim();
    if t.is_empty() {
      continue;
    }
    let h = t.trim_start_matches('#').trim();
    if t.starts_with('#') && !h.is_empty() {
      return h.to_string();
    }
    fallback.get_or_insert(t);
  }
  fallback.unwrap_or("entwurf").to_string()
}

/// Dateinamen-tauglicher Slug (ascii, klein, Bindestriche, max. 60 Zeichen).
pub(crate) fn slugify(s: &str) -> String {
  let mut out = String::new();
  for c in s.chars() {
    // Umlaute/ß transliterieren statt verschlucken („für's" → fuer-s).
    match c {
      'ä' | 'Ä' => out.push_str("ae"),
      'ö' | 'Ö' => out.push_str("oe"),
      'ü' | 'Ü' => out.push_str("ue"),
      'ß' => out.push_str("ss"),
      _ if c.is_ascii_alphanumeric() => out.push(c.to_ascii_lowercase()),
      _ if !out.ends_with('-') => out.push('-'),
      _ => {}
    }
  }
  let out: String = out.trim_matches('-').chars().take(60).collect();
  if out.is_empty() {
    "entwurf".into()
  } else {
    out
  }
}

/// Aktuelle UTC-Zeit als ISO-String — Frontmatter-`created` neu angelegter
/// Dokumente.
pub(crate) fn utc_now_iso() -> Result<String, String> {
  let secs = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs();
  Ok(utc_stamp(secs).1)
}

/// UTC aus Epoch-Sekunden: (Dateistempel `YYYY-MM-DD_HHMM`, ISO `…Z`).
pub(crate) fn utc_stamp(secs: u64) -> (String, String) {
  let (h, mi, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
  let (y, m, d) = civil_from_days((secs / 86400) as i64);
  (
    format!("{y:04}-{m:02}-{d:02}_{h:02}{mi:02}"),
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z"),
  )
}

/// Kalenderdatum aus Tagen seit 1970-01-01 (Howard-Hinnant-Algorithmus).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
  let z = z + 719468;
  let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
  let doe = z - era * 146097;
  let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
  let y = yoe + era * 400;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
  let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
  (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn utc_stamp_bekannte_zeit() {
    // 2026-07-11 (20645 Tage seit Epoch) um 11:14:15 UTC.
    let secs = 20_645u64 * 86400 + 11 * 3600 + 14 * 60 + 15;
    let (stamp, iso) = utc_stamp(secs);
    assert_eq!(stamp, "2026-07-11_1114");
    assert_eq!(iso, "2026-07-11T11:14:15Z");
  }

  /// Zweimal Archivieren in derselben Minute mit gleichem Titel darf die erste
  /// Datei nicht überschreiben — der Stempel hat nur Minutenauflösung, und
  /// die Datei entsteht bereits beim Anlegen (create_new), nicht erst beim
  /// Schreiben.
  #[test]
  fn gleicher_stempel_und_titel_kollidiert_nicht() {
    let dir = crate::domain::testutil::tmp_paths().home.join("archiv");
    fs::create_dir_all(&dir).unwrap();
    let (erste, mut f1) = create_unique(&dir, "2026-07-19_2118", "notiz").unwrap();
    assert_eq!(erste.file_name().unwrap(), "2026-07-19_2118-notiz.md");
    std::io::Write::write_all(&mut f1, b"alt").unwrap();

    let (zweite, _f2) = create_unique(&dir, "2026-07-19_2118", "notiz").unwrap();
    assert_eq!(zweite.file_name().unwrap(), "2026-07-19_2118-notiz-2.md");

    assert_eq!(fs::read_to_string(&erste).unwrap(), "alt");
    let (dritte, _f3) = create_unique(&dir, "2026-07-19_2118", "notiz").unwrap();
    assert_eq!(dritte.file_name().unwrap(), "2026-07-19_2118-notiz-3.md");
  }

  #[test]
  fn civil_from_days_referenz() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_645), (2026, 7, 11));
  }

  #[test]
  fn slugify_grenzen() {
    assert_eq!(slugify("ADR: Logging vereinheitlichen"), "adr-logging-vereinheitlichen");
    assert_eq!(slugify("   "), "entwurf");
    assert_eq!(slugify("###"), "entwurf");
    assert_eq!(slugify("Ein Test für's Archiv"), "ein-test-fuer-s-archiv");
    assert_eq!(slugify("Größe ÄÖÜ"), "groesse-aeoeue");
  }

  #[test]
  fn stamp_und_strip_roundtrip() {
    let (stamp, _) = utc_stamp(20_645u64 * 86400);
    assert_eq!(strip_stamp(&format!("{stamp}-adr-logging")), "adr-logging");
    assert_eq!(strip_stamp("adr-logging"), "adr-logging");
    assert_eq!(strip_stamp("2026-07-19-adr"), "2026-07-19-adr");
  }

  #[test]
  fn frontmatter_und_parser_roundtrip() {
    let meta = ArchiveMeta {
      title: None,
      folder: None,
      description: Some("Kurz".into()),
      tags: vec!["adr".into(), "infra".into()],
    };
    let fm = frontmatter("Titel", "proj", "2026-07-19T10:00:00Z", &meta);
    let map = parse_frontmatter(&fm);
    assert_eq!(map.get("title").map(String::as_str), Some("Titel"));
    assert_eq!(map.get("description").map(String::as_str), Some("Kurz"));
    assert_eq!(parse_tag_list(&map["tags"]), vec!["adr", "infra"]);
  }

  /// Umzug: Einträge wandern komplett, Namenskollision bricht laut ab.
  #[test]
  fn archiv_umzug_verschiebt_und_kollidiert_laut() {
    let home = crate::domain::testutil::tmp_paths().home;
    let (alt, neu) = (home.join("alt"), home.join("neu"));
    fs::create_dir_all(alt.join("ordner")).unwrap();
    fs::write(alt.join("doc.md"), "inhalt").unwrap();
    fs::write(alt.join("ordner/tief.md"), "tief").unwrap();
    fs::create_dir_all(&neu).unwrap();

    move_dir_contents(&alt, &neu).unwrap();
    assert_eq!(fs::read_to_string(neu.join("doc.md")).unwrap(), "inhalt");
    assert_eq!(fs::read_to_string(neu.join("ordner/tief.md")).unwrap(), "tief");
    assert!(fs::read_dir(&alt).unwrap().next().is_none());

    // Kollision: gleicher Name im Ziel → Fehler, nichts überschrieben
    fs::write(alt.join("doc.md"), "neuer").unwrap();
    let err = move_dir_contents(&alt, &neu).unwrap_err();
    assert!(err.contains("existiert schon"));
    assert_eq!(fs::read_to_string(neu.join("doc.md")).unwrap(), "inhalt");
  }

  #[test]
  fn check_folder_relativ_ohne_punktpunkt() {
    assert!(check_folder("konzepte/panel").is_ok());
    assert!(check_folder("../raus").is_err());
    assert!(check_folder("a/../b").is_err());
    assert!(check_folder("/absolut").is_err());
    assert!(check_folder("").is_err());
  }

  #[test]
  fn frontmatter_mit_und_ohne_meta() {
    let leer = ArchiveMeta::default();
    let fm = frontmatter("Titel", "proj", "2026-07-19T10:00:00Z", &leer);
    // Erste Zeile ist die technische ID, dann der Titel.
    assert!(fm.starts_with("---\nid: "));
    assert!(fm.contains("\ntitle: \"Titel\"\n"));
    assert!(!fm.contains("description:"));
    assert!(!fm.contains("tags:"));

    let voll = ArchiveMeta {
      title: None,
      folder: None,
      description: Some("Kurz \"zitiert\"".into()),
      tags: vec!["archiv".into(), "wiki".into()],
    };
    let fm = frontmatter("Titel", "proj", "2026-07-19T10:00:00Z", &voll);
    assert!(fm.contains("description: \"Kurz 'zitiert'\"\n"));
    assert!(fm.contains("tags: [\"archiv\", \"wiki\"]\n"));
  }

  #[test]
  fn first_line_ueberschrift_oder_erste_zeile() {
    assert_eq!(first_line("\n\n# Titel\n\nText"), "Titel");
    assert_eq!(first_line("Kein Heading\nzweite"), "Kein Heading");
    assert_eq!(first_line("   \n"), "entwurf");
  }
}
