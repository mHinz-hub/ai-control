//! Dokument- und Ordner-Operationen im Archiv-Baum (Baum-Navigation im
//! Panel): verschieben, umbenennen, löschen. Alle Pfade sind relativ zum
//! Archiv-Home und strikt darauf begrenzt — die relpaths kommen aus dem
//! Webview, ein Traversal-Bestandteil bricht laut ab.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::domain::archive::{
  frontmatter, slugify, strip_frontmatter, strip_stamp, utc_now_iso, ArchiveMeta,
};

/// Relativer Pfad ohne Traversal: nur normale Komponenten, nichts Absolutes.
fn checked_rel(rel: &str) -> Result<(), String> {
  if rel.is_empty() {
    return Err("leerer Pfad".into());
  }
  for c in Path::new(rel).components() {
    match c {
      Component::Normal(_) => {}
      _ => return Err(format!("unzulässiger Pfad: {rel}")),
    }
  }
  Ok(())
}

/// Vorhandenes Archiv-Dokument zum relpath.
pub(crate) fn doc_path(home: &Path, relpath: &str) -> Result<PathBuf, String> {
  checked_rel(relpath)?;
  if !relpath.ends_with(".md") && !relpath.ends_with(".html") {
    return Err(format!("keine Notiz-Datei: {relpath}"));
  }
  let path = home.join(relpath);
  if !path.is_file() {
    return Err(format!("Dokument nicht gefunden: {relpath}"));
  }
  Ok(path)
}

/// Vorhandene Archiv-Datei zum relpath — Notizen und Bücher (`.epub`).
/// Gegenstück zu `doc_path`: was hier durchkommt, ist nicht zwingend
/// bearbeitbar, nur vorhanden.
pub(crate) fn file_path(home: &Path, relpath: &str) -> Result<PathBuf, String> {
  checked_rel(relpath)?;
  let path = home.join(relpath);
  if !path.is_file() {
    return Err(format!("Datei nicht gefunden: {relpath}"));
  }
  Ok(path)
}

/// Ist die Datei eine HTML-Notiz?
fn is_html(path: &Path) -> bool {
  path.extension().is_some_and(|e| e == "html")
}

/// Ziel für rename: darf noch nicht existieren — stilles Überschreiben wäre
/// Datenverlust.
fn fresh_target(target: &Path, home: &Path) -> Result<(), String> {
  if target.exists() {
    return Err(format!(
      "Ziel existiert bereits: {}",
      target.strip_prefix(home).unwrap_or(target).display()
    ));
  }
  Ok(())
}


/// Benennt ein Dokument um: neuer Name als Slug hinter dem (erhaltenen)
/// Zeitstempel. Liefert den neuen relpath.
pub(crate) fn rename_doc(home: &Path, relpath: &str, name: &str) -> Result<String, String> {
  let src = doc_path(home, relpath)?;
  let stem = src.file_stem().unwrap().to_string_lossy().to_string();
  let stamp = &stem[..stem.len() - strip_stamp(&stem).len()];
  let target = src.with_file_name(format!("{stamp}{}.md", slugify(name)));
  if target == src {
    return Ok(relpath.to_string());
  }
  fresh_target(&target, home)?;
  fs::rename(&src, &target).map_err(|e| format!("{}: {e}", src.display()))?;
  Ok(target.strip_prefix(home).unwrap().display().to_string())
}

pub(crate) fn delete_doc(home: &Path, relpath: &str) -> Result<(), String> {
  let path = file_path(home, relpath)?;
  fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Legt einen (auch leeren) Ordner unterhalb des Archiv-Home an.
pub(crate) fn create_folder(home: &Path, folder: &str) -> Result<(), String> {
  checked_rel(folder)?;
  let dir = home.join(folder);
  fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Legt ein leeres Dokument an: `<slug>.md` im Ordner (leer = Wurzel), das
/// Datum steht in der Frontmatter (`created`), nicht im Dateinamen. Liefert
/// den relpath.
pub(crate) fn create_doc(
  home: &Path,
  folder: &str,
  name: &str,
  project_name: &str,
) -> Result<String, String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Dokumentname fehlt".into());
  }
  if !folder.is_empty() {
    checked_rel(folder)?;
  }
  let dir = home.join(folder);
  fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  let target = dir.join(format!("{}.md", slugify(name)));
  let fm = frontmatter(name, project_name, &utc_now_iso()?, &ArchiveMeta::default());
  let mut file = match fs::OpenOptions::new().write(true).create_new(true).open(&target) {
    Ok(f) => f,
    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
      return Err(format!(
        "Ziel existiert bereits: {}",
        target.strip_prefix(home).unwrap_or(&target).display()
      ))
    }
    Err(e) => return Err(format!("{}: {e}", target.display())),
  };
  std::io::Write::write_all(&mut file, fm.as_bytes())
    .map_err(|e| format!("{}: {e}", target.display()))?;
  Ok(target.strip_prefix(home).unwrap().display().to_string())
}

/// Legt eine leere HTML-Notiz an: `<slug>.html` im Ordner (leer = Wurzel).
/// Liefert den relpath.
pub(crate) fn create_html(
  home: &Path,
  folder: &str,
  name: &str,
  project_name: &str,
) -> Result<String, String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Dokumentname fehlt".into());
  }
  if !folder.is_empty() {
    checked_rel(folder)?;
  }
  let dir = home.join(folder);
  fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  let target = dir.join(format!("{}.html", slugify(name)));
  let doc = crate::domain::archive_html::skeleton(name, project_name, &utc_now_iso()?);
  let mut file = match fs::OpenOptions::new().write(true).create_new(true).open(&target) {
    Ok(f) => f,
    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
      return Err(format!(
        "Ziel existiert bereits: {}",
        target.strip_prefix(home).unwrap_or(&target).display()
      ))
    }
    Err(e) => return Err(format!("{}: {e}", target.display())),
  };
  std::io::Write::write_all(&mut file, doc.as_bytes())
    .map_err(|e| format!("{}: {e}", target.display()))?;
  Ok(target.strip_prefix(home).unwrap().display().to_string())
}

/// Setzt den Anzeige-Titel einer Notiz: `title:` im Frontmatter der Datei.
/// Der Dateiname bleibt — er ist der technische Name (Modellvorgabe: der
/// angezeigte Titel steht im Text). Ohne Frontmatter-Block bekommt die Datei
/// keinen Titel angeheftet, sondern der Aufruf scheitert laut.
pub(crate) fn set_title(path: &Path, title: &str) -> Result<(), String> {
  let title = title.trim();
  if title.is_empty() {
    return Err("Titel fehlt".into());
  }
  let doc = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
  if is_html(path) {
    let out = crate::domain::archive_html::set_title(&doc, title)?;
    return fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()));
  }
  let rest = doc
    .strip_prefix("---\n")
    .ok_or_else(|| format!("kein Frontmatter-Block: {}", path.display()))?;
  let end = rest
    .find("\n---\n")
    .ok_or_else(|| format!("kein Frontmatter-Block: {}", path.display()))?;
  let line = format!("title: \"{}\"", title.replace('"', "'"));
  let mut lines: Vec<String> = rest[..end].lines().map(str::to_string).collect();
  match lines.iter().position(|l| l.trim_start().starts_with("title:")) {
    Some(i) => lines[i] = line,
    None => lines.insert(0, line),
  }
  let out = format!("---\n{}\n{}", lines.join("\n"), &rest[end + 1..]);
  fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// Schreibt den Panel-Body zurück in ein Archiv-Dokument (Quell-Verknüpfung
/// des Dokument-Tabs): der Frontmatter-Block der Datei bleibt, der Rumpf wird
/// ersetzt.
pub(crate) fn write_body(path: &Path, text: &str) -> Result<(), String> {
  let doc = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
  if is_html(path) {
    let out = crate::domain::archive_html::replace_body(&doc, text)?;
    return fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()));
  }
  let head = doc.len() - strip_frontmatter(&doc).len();
  let out = format!("{}{}\n", &doc[..head], text.trim_end());
  fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// Verschiebt bzw. benennt einen ganzen Ordner um (fs::rename des
/// Verzeichnisses); Eltern des Ziels entstehen bei Bedarf. Die Textdatei des
/// Knotens (gleichnamiges Dokument daneben) wandert mit.
pub(crate) fn move_folder(home: &Path, folder: &str, to: &str) -> Result<(), String> {
  checked_rel(folder)?;
  checked_rel(to)?;
  let src = home.join(folder);
  if !src.is_dir() {
    return Err(format!("Ordner nicht gefunden: {folder}"));
  }
  let target = home.join(to);
  if target == src {
    return Ok(());
  }
  fresh_target(&target, home)?;
  if let Some(parent) = target.parent() {
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
  }
  fs::rename(&src, &target).map_err(|e| format!("{}: {e}", src.display()))?;
  let src_text = src.with_extension("md");
  if src_text.is_file() {
    let target_text = target.with_extension("md");
    fresh_target(&target_text, home)?;
    fs::rename(&src_text, &target_text).map_err(|e| format!("{}: {e}", src_text.display()))?;
  }
  Ok(())
}

/// Zweite Invariante des Notizmodells: JEDES Dokument trägt eine technische
/// ID im Frontmatter (`id:`). Sie entsteht einmal und bleibt — Titel,
/// Dateiname und Ordner dürfen sich danach beliebig ändern, Verweise laufen
/// über die ID. Dateien ohne ID (von Hand angelegt, Altbestand) bekommen hier
/// eine.
pub(crate) fn ensure_ids(home: &Path) -> Result<(), String> {
  for path in note_files(home)? {
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if is_html(&path) {
      if crate::domain::archive_html::parse_meta(&text).contains_key("id") {
        continue;
      }
      let id = uuid::Uuid::new_v4().to_string();
      let out = crate::domain::archive_html::set_meta(&text, "id", &id)?;
      fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
      continue;
    }
    if crate::domain::archive::parse_frontmatter(&text).contains_key("id") {
      continue;
    }
    let line = format!("id: {}", uuid::Uuid::new_v4());
    let out = match text.strip_prefix("---\n") {
      // Frontmatter vorhanden: ID als erste Zeile einfügen.
      Some(rest) => format!("---\n{line}\n{rest}"),
      // Ohne Frontmatter: einen Block davor setzen, Rumpf bleibt.
      None => format!("---\n{line}\n---\n\n{text}"),
    };
    fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
  }
  Ok(())
}

/// Alle Notiz-Dateien (`.md`, `.html`) unterhalb von `dir`, rekursiv;
/// versteckte Einträge bleiben außen vor.
fn note_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
  let mut out = Vec::new();
  for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
    let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = entry.file_name().to_string_lossy().to_string();
    if name.starts_with('.') {
      continue;
    }
    let path = entry.path();
    if path.is_dir() {
      out.extend(note_files(&path)?);
    } else if name.ends_with(".md") || name.ends_with(".html") {
      out.push(path);
    }
  }
  Ok(out)
}

/// Invariante des Notizmodells: JEDER Knoten besitzt eine Textdatei — der
/// Ordnername ist nur technische Verwaltung, Titel und Inhalt stehen im
/// gleichnamigen Dokument daneben (Wurzel: `index.md`). Läuft vor jedem
/// Seitenaufbau und ergänzt fehlende Texte (Titel = Ordnername) — auch für
/// von Hand oder per Tool angelegte Ordner.
pub(crate) fn ensure_node_texts(
  home: &Path,
  project_name: &str,
) -> Result<(), String> {
  ensure_dir_texts(home, project_name)?;
  let stems = md_stems(home)?;
  if !stems.contains("index") {
    write_node_text(&home.join("index.md"), "Archiv", project_name)?;
  }
  Ok(())
}

fn ensure_dir_texts(dir: &Path, project_name: &str) -> Result<(), String> {
  let stems = md_stems(dir)?;
  for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
    let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = entry.file_name().to_string_lossy().to_string();
    if name.starts_with('.') || !entry.path().is_dir() {
      continue;
    }
    // Auch gestempelte Dokumente zählen als Knotentext (Name = Stem ohne
    // Stempel) — kein Duplikat anlegen.
    if !stems.contains(name.as_str()) {
      write_node_text(&dir.join(format!("{name}.md")), &name, project_name)?;
    }
    ensure_dir_texts(&entry.path(), project_name)?;
  }
  Ok(())
}

/// Namen (Stem ohne Zeitstempel) aller Notiz-Dateien direkt im Ordner.
fn md_stems(dir: &Path) -> Result<std::collections::HashSet<String>, String> {
  let mut out = std::collections::HashSet::new();
  for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
    let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = entry.file_name().to_string_lossy().to_string();
    for ext in [".md", ".html"] {
      if let Some(stem) = name.strip_suffix(ext) {
        out.insert(strip_stamp(stem).to_string());
      }
    }
  }
  Ok(out)
}

fn write_node_text(path: &Path, title: &str, project_name: &str) -> Result<(), String> {
  let fm = frontmatter(title, project_name, &utc_now_iso()?, &ArchiveMeta::default());
  fs::write(path, fm).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::tmp_paths;

  fn archiv() -> PathBuf {
    let home = tmp_paths().home.join("archiv");
    fs::create_dir_all(home.join("konzepte")).unwrap();
    fs::write(home.join("2026-07-19_1000-adr-logging.md"), "a").unwrap();
    fs::write(home.join("konzepte/2026-07-19_1005-notiz-deploy.md"), "b").unwrap();
    home
  }



  #[test]
  fn traversal_und_fremde_pfade_brechen_ab() {
    let home = archiv();
    assert!(delete_doc(&home, "../2026-07-19_1000-adr-logging.md").is_err());
    assert!(delete_doc(&home, "/etc/passwd.md").is_err());
    assert!(delete_doc(&home, "konzepte").is_err()); // kein .md
    assert!(move_folder(&home, "konzepte", "../raus").is_err());
  }

  #[test]
  fn umbenennen_erhaelt_stempel_und_sluggt() {
    let home = archiv();
    let rel =
      rename_doc(&home, "konzepte/2026-07-19_1005-notiz-deploy.md", "Deploy Nötiz!").unwrap();
    assert_eq!(rel, "konzepte/2026-07-19_1005-deploy-noetiz.md");
    assert!(home.join(&rel).is_file());

    // Ohne Stempel bleibt der Name pur.
    fs::write(home.join("plain.md"), "c").unwrap();
    assert_eq!(rename_doc(&home, "plain.md", "Neu").unwrap(), "neu.md");
  }

  #[test]
  fn anlegen_ordner_und_dokument() {
    let home = archiv();
    create_folder(&home, "notizen/2026").unwrap();
    assert!(home.join("notizen/2026").is_dir());
    assert!(create_folder(&home, "../raus").is_err());

    let rel = create_doc(&home, "notizen/2026", "Deploy Nötiz!", "proj").unwrap();
    assert_eq!(rel, "notizen/2026/deploy-noetiz.md");
    let text = fs::read_to_string(home.join(&rel)).unwrap();
    assert!(text.contains("\ntitle: \"Deploy Nötiz!\"\n"));
    assert!(text.contains("created: "));

    // In der Wurzel, gleicher Name kollidiert laut, leerer Name bricht ab.
    assert_eq!(create_doc(&home, "", "Plain", "proj").unwrap(), "plain.md");
    let err = create_doc(&home, "notizen/2026", "Deploy Nötiz!", "proj").unwrap_err();
    assert!(err.contains("existiert bereits"));
    assert!(create_doc(&home, "", "  ", "proj").is_err());
  }

  #[test]
  fn titel_setzen_ersetzt_frontmatter_zeile() {
    let home = archiv();
    let path = home.join("t.md");
    fs::write(&path, "---\ntitle: \"Alt\"\nproject: p\n---\n\nRumpf\n").unwrap();
    set_title(&path, "Neuer \"Titel\"").unwrap();
    assert_eq!(
      fs::read_to_string(&path).unwrap(),
      "---\ntitle: \"Neuer 'Titel'\"\nproject: p\n---\n\nRumpf\n"
    );

    // Ohne title-Zeile kommt sie oben dazu; der Rumpf bleibt.
    let plain = home.join("ohne.md");
    fs::write(&plain, "---\nproject: p\n---\n\nRumpf\n").unwrap();
    set_title(&plain, "T").unwrap();
    assert_eq!(
      fs::read_to_string(&plain).unwrap(),
      "---\ntitle: \"T\"\nproject: p\n---\n\nRumpf\n"
    );

    // Leerer Titel und Datei ohne Frontmatter scheitern laut.
    assert!(set_title(&path, "  ").is_err());
    let bare = home.join("bare.md");
    fs::write(&bare, "nur Text\n").unwrap();
    assert!(set_title(&bare, "T").is_err());
  }

  #[test]
  fn write_body_ersetzt_rumpf_und_erhaelt_frontmatter() {
    let home = archiv();
    let path = home.join("mit-fm.md");
    fs::write(&path, "---\ntitle: \"T\"\n---\n\nalter Text\n").unwrap();
    write_body(&path, "neuer Text").unwrap();
    assert_eq!(
      fs::read_to_string(&path).unwrap(),
      "---\ntitle: \"T\"\n---\n\nneuer Text\n"
    );

    // Ohne Frontmatter wird die Datei komplett ersetzt.
    let plain = home.join("ohne-fm.md");
    fs::write(&plain, "alt").unwrap();
    write_body(&plain, "neu").unwrap();
    assert_eq!(fs::read_to_string(&plain).unwrap(), "neu\n");

    assert!(write_body(&home.join("fehlt.md"), "x").is_err());
  }

  #[test]
  fn jeder_knoten_bekommt_text() {
    let home = archiv();
    fs::create_dir_all(home.join("leer/unter")).unwrap();
    ensure_node_texts(&home, "proj").unwrap();
    // Wurzel und alle Ordner haben danach ihre Textdatei.
    assert!(home.join("index.md").is_file());
    assert!(home.join("leer.md").is_file());
    assert!(home.join("leer/unter.md").is_file());
    let text = fs::read_to_string(home.join("leer.md")).unwrap();
    assert!(text.contains("\ntitle: \"leer\"\n"));
    // Gestempelte Zwillinge zählen als Knotentext — kein Duplikat: konzepte/
    // hat keins, bekommt eines; ein zweiter Lauf legt nichts Neues an.
    assert!(home.join("konzepte.md").is_file());
    let before = fs::read_to_string(home.join("konzepte.md")).unwrap();
    ensure_node_texts(&home, "proj").unwrap();
    assert_eq!(fs::read_to_string(home.join("konzepte.md")).unwrap(), before);
  }

  #[test]
  fn ordner_verschieben_nimmt_knotentext_mit() {
    let home = archiv();
    fs::write(home.join("konzepte.md"), "k").unwrap();
    move_folder(&home, "konzepte", "notizen").unwrap();
    assert!(home.join("notizen").is_dir());
    assert_eq!(fs::read_to_string(home.join("notizen.md")).unwrap(), "k");
    assert!(!home.join("konzepte.md").exists());
  }

  #[test]
  fn loeschen_und_ordner_verschieben() {
    let home = archiv();
    delete_doc(&home, "2026-07-19_1000-adr-logging.md").unwrap();
    assert!(!home.join("2026-07-19_1000-adr-logging.md").exists());

    move_folder(&home, "konzepte", "notizen/2026").unwrap();
    assert!(home.join("notizen/2026/2026-07-19_1005-notiz-deploy.md").is_file());
    assert!(!home.join("konzepte").exists());

    let err = move_folder(&home, "fehlt", "x").unwrap_err();
    assert!(err.contains("nicht gefunden"));
  }
}
