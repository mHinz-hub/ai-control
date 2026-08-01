//! Archiv-Index: reproduzierbare Sicht über den Archiv-Baum eines Projekts —
//! pro Markdown-Dokument Name, Frontmatter-Metadaten und Wikilinks; Backlinks
//! fallen beim Scan als Nebenprodukt ab. Der Index ist abgeleitete Information
//! und wird bei Bedarf frisch aus dem Baum gebaut (nichts davon wird gesynct).
//! Das Dateiformat (Frontmatter, Zeitstempel-Stem) definiert archive.rs.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::domain::archive::{
  parse_frontmatter, parse_tag_list, slugify, strip_frontmatter, strip_stamp,
};

#[derive(serde::Serialize, Clone)]
pub(crate) struct Doc {
  /// Technische ID aus dem Frontmatter — bleibt über Umbenennen und
  /// Verschieben hinweg gleich; alle Verweise laufen darüber.
  pub(crate) id: String,
  /// Pfad relativ zum Archiv-Home (Eigenschaft, keine Identität).
  pub(crate) relpath: String,
  /// Notiz-Typ: `md` oder `html` — bestimmt Anzeige und Editor.
  pub(crate) kind: &'static str,
  /// Wikilink-Name: Datei-Stem ohne führenden Zeitstempel.
  pub(crate) name: String,
  /// Frontmatter-Titel, sonst der Name.
  pub(crate) title: String,
  pub(crate) description: Option<String>,
  pub(crate) tags: Vec<String>,
  /// Frontmatter-`created` (ISO) — Datumsquelle für Dokumente ohne
  /// Zeitstempel im Dateinamen.
  pub(crate) created: Option<String>,
  /// Letzte Änderung (Datei-mtime, `YYYY-MM-DD`).
  pub(crate) modified: String,
  /// Wikilink-Ziele im Dokumenttext (`[[ziel]]`/`[[ziel|label]]`, nur das Ziel).
  pub(crate) links: Vec<String>,
  /// Namen der Dokumente, die per Wikilink hierher zeigen.
  pub(crate) backlinks: Vec<String>,
}

/// Scannt den Archiv-Baum rekursiv über alle Markdown-Dateien; versteckte
/// Einträge (Punkt-Präfix) bleiben außen vor. Reihenfolge: relpath sortiert;
/// Backlinks sind nach dem Scan gefüllt.
pub(crate) fn scan_archive(home: &Path) -> Result<Vec<Doc>, String> {
  Ok(scan_with_bodies(home)?.into_iter().map(|(doc, _)| doc).collect())
}

/// Wie `scan_archive`, liefert zu jedem Dokument den bereits gelesenen
/// Volltext mit — die Suche indexiert damit ohne zweite Lesung pro Datei.
pub(crate) fn scan_with_bodies(home: &Path) -> Result<Vec<(Doc, String)>, String> {
  let mut docs = Vec::new();
  walk(home, home, &mut docs)?;
  docs.sort_by(|a, b| a.0.relpath.cmp(&b.0.relpath));
  fill_backlinks(&mut docs);
  Ok(docs)
}

/// Backlinks in einem Durchlauf: Slug → Doc-Index einmal aufbauen, jeden Link
/// genau einmal auflösen und zu Backlinks invertieren.
fn fill_backlinks(docs: &mut [(Doc, String)]) {
  let mut lookup: HashMap<String, usize> = HashMap::new();
  for (i, (doc, _)) in docs.iter().enumerate() {
    let stem = Path::new(&doc.relpath).file_stem().unwrap_or_default().to_string_lossy();
    for key in [slugify(&doc.name), slugify(&doc.title), slugify(&stem)] {
      lookup.entry(key).or_insert(i);
    }
  }
  let mut back: Vec<Vec<String>> = vec![Vec::new(); docs.len()];
  for (i, (doc, _)) in docs.iter().enumerate() {
    let mut targets: std::collections::BTreeSet<usize> = docs[i]
      .0
      .links
      .iter()
      .filter_map(|l| lookup.get(&slugify(l)).copied())
      .collect();
    targets.remove(&i);
    for t in targets {
      back[t].push(doc.name.clone());
    }
  }
  for ((doc, _), b) in docs.iter_mut().zip(back) {
    doc.backlinks = b;
  }
}

fn walk(home: &Path, dir: &Path, docs: &mut Vec<(Doc, String)>) -> Result<(), String> {
  let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  for entry in entries {
    let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = entry.path();
    let file_name = entry.file_name();
    let fname = file_name.to_string_lossy();
    if fname.starts_with('.') {
      continue;
    }
    if path.is_dir() {
      walk(home, &path, docs)?;
    } else if fname.ends_with(".md") || fname.ends_with(".html") {
      docs.push(read_doc(home, &path)?);
    } else if fname.ends_with(".epub") {
      docs.push(read_book(home, &path)?);
    } else {
      docs.push(read_file_node(home, &path)?);
    }
  }
  Ok(())
}

/// Letzte Änderung als voller ISO-Zeitstempel (`YYYY-MM-DDTHH:MM:SSZ`) —
/// die Anzeige kürzt aufs Datum, sortiert und gehovert wird sekundengenau.
fn modified(path: &Path) -> Result<String, String> {
  let mtime = fs::metadata(path)
    .and_then(|m| m.modified())
    .map_err(|e| format!("{}: {e}", path.display()))?
    .duration_since(std::time::UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs();
  Ok(crate::domain::archive::utc_stamp(mtime).1)
}

/// Ein Buch (`.epub`) im Archiv: Eintrag im Baum, kein Notiz-Inhalt. Seine
/// Identität ist der Pfad — in eine Binärdatei lässt sich kein Frontmatter
/// schreiben, und die Invarianten des Notizmodells (ensure_ids,
/// ensure_node_texts) fassen Bücher darum auch nicht an. Der Titel kommt aus
/// dem Dateinamen; die Metadaten im Buch liest erst der Viewer.
fn read_book(home: &Path, path: &Path) -> Result<(Doc, String), String> {
  let relpath = path
    .strip_prefix(home)
    .map_err(|e| format!("{}: {e}", path.display()))?
    .display()
    .to_string();
  let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
  let name = strip_stamp(&stem).to_string();
  let doc = Doc {
    id: format!("epub:{relpath}"),
    relpath,
    kind: "epub",
    title: name.clone(),
    name,
    description: None,
    tags: Vec::new(),
    created: None,
    modified: modified(path)?,
    links: Vec::new(),
    backlinks: Vec::new(),
  };
  Ok((doc, String::new()))
}

/// Endungen, deren Inhalt Text ist und damit in die Volltextsuche gehört.
/// Alles andere (Bilder, Archive, ePub, Diagramme) bleibt draußen: Bytes in
/// einem Wortindex finden nichts und kosten bei jedem Suchlauf Speicher.
const TEXT_ENDUNGEN: &[&str] = &[
  "txt", "text", "log", "json", "yaml", "yml", "xml", "csv", "toml", "ini", "conf", "cfg",
  "sh", "bash", "py", "rs", "ts", "js", "css", "sql", "env", "gitignore",
];

/// Rohtext einer sonstigen Datei für den Suchindex — leer, wenn die Endung
/// nicht für Text steht oder der Inhalt kein UTF-8 ist (dann ist es trotz
/// Endung keine Textdatei).
fn text_inhalt(path: &Path) -> Result<String, String> {
  let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
  if !TEXT_ENDUNGEN.contains(&ext.as_str()) {
    return Ok(String::new());
  }
  let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
  Ok(String::from_utf8(bytes).unwrap_or_default())
}

/// Eine sonstige Datei im Archiv (JSON, Log, Skript …): Eintrag im Baum nach
/// dem Muster der Bücher — die Identität ist der Pfad, der Titel der volle
/// Dateiname samt Endung. Kein Frontmatter, kein Notizmodell; den Inhalt
/// zeigt die Rohtext-Ansicht, und bei Textformaten geht er in die Suche.
fn read_file_node(home: &Path, path: &Path) -> Result<(Doc, String), String> {
  let relpath = path
    .strip_prefix(home)
    .map_err(|e| format!("{}: {e}", path.display()))?
    .display()
    .to_string();
  let fname = path.file_name().unwrap_or_default().to_string_lossy().to_string();
  let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
  let doc = Doc {
    id: format!("file:{relpath}"),
    relpath,
    kind: "file",
    title: fname,
    name: strip_stamp(&stem).to_string(),
    description: None,
    tags: Vec::new(),
    created: None,
    modified: modified(path)?,
    links: Vec::new(),
    backlinks: Vec::new(),
  };
  Ok((doc, text_inhalt(path)?))
}

fn read_doc(home: &Path, path: &Path) -> Result<(Doc, String), String> {
  let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
  let modified = modified(path)?;
  let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
  let name = strip_stamp(&stem).to_string();
  let html = path.extension().is_some_and(|e| e == "html");
  let kind = if html { "html" } else { "md" };
  // Beide Typen tragen dieselben Angaben, nur an ihrer jeweils üblichen
  // Stelle: Markdown im Frontmatter, HTML in <title>/<meta>.
  let fm = if html {
    crate::domain::archive_html::parse_meta(&text)
  } else {
    parse_frontmatter(&text)
  };
  let relpath = path
    .strip_prefix(home)
    .map_err(|e| format!("{}: {e}", path.display()))?
    .display()
    .to_string();
  let doc = Doc {
    id: fm.get("id").cloned().unwrap_or_default(),
    relpath,
    title: fm.get("title").unwrap_or(&name).clone(),
    description: fm.get("description").cloned(),
    tags: fm
      .get(if html { "keywords" } else { "tags" })
      .map(|t| parse_tag_list(t))
      .unwrap_or_default(),
    created: fm.get("created").cloned(),
    links: wikilinks(&text),
    backlinks: Vec::new(),
    modified,
    kind,
    name,
  };
  // Der Rumpf ist, was die Ansicht zeigt: HTML ohne Markup, Markdown ohne
  // Frontmatter. Sonst gälte ein Treffer im Titel als Text-Treffer, und die
  // Markierung im geöffneten Dokument liefe ins Leere.
  let indexed = if html {
    crate::domain::archive_html::strip_tags(&text)
  } else {
    strip_frontmatter(&text).to_string()
  };
  Ok((doc, indexed))
}

/// Slug-Vergleich eines Wikilink-Ziels gegen Name, Titel und Datei-Stem.
fn matches(doc: &Doc, want: &str) -> bool {
  slugify(&doc.name) == want
    || slugify(&doc.title) == want
    || Path::new(&doc.relpath)
      .file_stem()
      .is_some_and(|s| slugify(&s.to_string_lossy()) == want)
}

/// Übersichts- bzw. Schlagwort-Seite als strukturierte Wiki-Daten: Dokumente
/// nach Ordnern gruppiert (neueste zuerst), Schlagwort-Leiste mit Zählern.
/// Das Panel rendert daraus die Wiki-Ansicht; `kind` unterscheidet im
/// Wiki-Puffer Seite und Dokument.
#[derive(serde::Serialize)]
pub(crate) struct WikiPage {
  pub(crate) kind: &'static str,
  pub(crate) home: String,
  pub(crate) tag: Option<String>,
  pub(crate) total: usize,
  pub(crate) tags: Vec<TagCount>,
  pub(crate) folders: Vec<WikiFolder>,
}

#[derive(serde::Serialize)]
pub(crate) struct TagCount {
  pub(crate) name: String,
  pub(crate) count: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct WikiFolder {
  /// Ordner relativ zum Archiv-Home; leer für die Wurzel.
  pub(crate) name: String,
  pub(crate) docs: Vec<WikiDocEntry>,
}

#[derive(serde::Serialize)]
pub(crate) struct WikiDocEntry {
  /// Technische ID — Adressat aller Aktionen.
  pub(crate) id: String,
  /// Notiz-Typ (`md`/`html`) — Symbol im Baum, Anzeige und Editor.
  pub(crate) kind: &'static str,
  /// Pfad relativ zum Archiv-Home — Sprung ins Dokument und Adressat der
  /// Zeilen-Aktionen (umbenennen, löschen).
  pub(crate) relpath: String,
  pub(crate) name: String,
  pub(crate) title: String,
  pub(crate) description: Option<String>,
  pub(crate) tags: Vec<String>,
  /// Archivierungsdatum aus dem Zeitstempel-Stem (`YYYY-MM-DD`).
  pub(crate) date: Option<String>,
  /// Zahl der Dokumente, die per Wikilink hierher zeigen.
  pub(crate) backlinks: usize,
  /// Letzte Änderung (Datei-mtime, `YYYY-MM-DD`).
  pub(crate) modified: String,
}

pub(crate) fn archive_page(home: &Path, tag: Option<&str>) -> Result<WikiPage, String> {
  let docs = scan_archive(home)?;
  let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
  for doc in &docs {
    for t in &doc.tags {
      *counts.entry(t).or_default() += 1;
    }
  }
  let selected: Vec<&Doc> = match tag {
    Some(t) => docs.iter().filter(|d| d.tags.iter().any(|x| x == t)).collect(),
    None => docs.iter().collect(),
  };
  let mut folders: std::collections::BTreeMap<String, Vec<&Doc>> =
    std::collections::BTreeMap::new();
  // Die Übersicht führt alle Ordner, auch dokumentlose — der Navigator zeigt
  // frisch angelegte Ordner sonst nicht. Schlagwort-Seiten bleiben auf die
  // Treffer-Ordner beschränkt.
  if tag.is_none() {
    let mut dirs = Vec::new();
    folder_paths(home, home, &mut dirs)?;
    for d in dirs {
      folders.entry(d).or_default();
    }
  }
  for doc in &selected {
    let folder = Path::new(&doc.relpath)
      .parent()
      .map(|p| p.display().to_string())
      .unwrap_or_default();
    folders.entry(folder).or_default().push(doc);
  }
  Ok(WikiPage {
    kind: "page",
    home: home.display().to_string(),
    tag: tag.map(str::to_string),
    total: selected.len(),
    tags: counts
      .into_iter()
      .map(|(name, count)| TagCount { name: name.to_string(), count })
      .collect(),
    folders: folders
      .into_iter()
      .map(|(name, mut list)| {
        // Zeitstempel-Stems sortieren chronologisch — absteigend = neueste oben.
        list.sort_by(|a, b| b.relpath.cmp(&a.relpath));
        WikiFolder { name, docs: list.into_iter().map(doc_entry).collect() }
      })
      .collect(),
  })
}

/// Ordner-Knoten für den Zielordner-Baum des Archiv-Dialogs: Pfad plus
/// Anzeige-Titel aus dem Knotentext (`<name>.md`/`.html` daneben) — dieselbe
/// logische Sicht wie die Archiv-Ansicht.
#[derive(serde::Serialize)]
pub(crate) struct FolderNode {
  /// Technische ID des Knotentexts — Adressat der Auswahl.
  pub(crate) id: String,
  /// Pfad relativ zum Archiv-Home (nur Eigenschaft).
  pub(crate) path: String,
  /// Titel des Knotentexts, sonst der Ordnername.
  pub(crate) title: String,
}

pub(crate) fn folder_nodes(home: &Path) -> Result<Vec<FolderNode>, String> {
  let mut paths = Vec::new();
  folder_paths(home, home, &mut paths)?;
  paths.sort();
  let mut out = Vec::new();
  for path in paths {
    let name = Path::new(&path)
      .file_name()
      .unwrap_or_default()
      .to_string_lossy()
      .to_string();
    // Knotentext daneben: erst .md, dann .html.
    let fm = ["md", "html"]
      .iter()
      .map(|ext| home.join(&path).with_extension(ext))
      .find(|p| p.is_file())
      .and_then(|p| Some((p.extension()?.to_string_lossy().to_string(), fs::read_to_string(p).ok()?)))
      .map(|(ext, text)| {
        if ext == "html" {
          crate::domain::archive_html::parse_meta(&text)
        } else {
          parse_frontmatter(&text)
        }
      })
      .unwrap_or_default();
    let title = fm.get("title").cloned().unwrap_or_else(|| name.clone());
    let id = fm.get("id").cloned().unwrap_or_default();
    out.push(FolderNode { id, path, title });
  }
  Ok(out)
}

/// Alle Ordner-Relpaths unterhalb von `dir`, rekursiv; versteckte Einträge
/// (Punkt-Präfix) bleiben außen vor — wie beim Dokument-Scan.
pub(crate) fn folder_paths(home: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
  let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  for entry in entries {
    let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = entry.path();
    if entry.file_name().to_string_lossy().starts_with('.') || !path.is_dir() {
      continue;
    }
    out.push(path.strip_prefix(home).unwrap().display().to_string());
    folder_paths(home, &path, out)?;
  }
  Ok(())
}

fn doc_entry(doc: &Doc) -> WikiDocEntry {
  let stem = Path::new(&doc.relpath)
    .file_stem()
    .unwrap_or_default()
    .to_string_lossy()
    .to_string();
  // Zeitstempel im Dateinamen, sonst der Frontmatter-`created`-Tag —
  // im Navigator angelegte Dokumente tragen das Datum nur dort.
  let date = (stem != doc.name)
    .then(|| stem[..10].to_string())
    .or_else(|| {
      doc
        .created
        .as_ref()
        .filter(|c| c.len() >= 10)
        .map(|c| c[..10].to_string())
    });
  WikiDocEntry {
    id: doc.id.clone(),
    kind: doc.kind,
    relpath: doc.relpath.clone(),
    name: doc.name.clone(),
    title: doc.title.clone(),
    description: doc.description.clone(),
    tags: doc.tags.clone(),
    date,
    backlinks: doc.backlinks.len(),
    modified: doc.modified.clone(),
  }
}

/// Löst eine technische ID auf den aktuellen relpath auf.
pub(crate) fn resolve_id(home: &Path, id: &str) -> Result<String, String> {
  // Eine leere ID fände sonst das erste Dokument, dem noch keine ID ins
  // Frontmatter geschrieben wurde — ein beliebiges statt des gemeinten.
  if id.is_empty() {
    return Err("leere Notiz-ID".into());
  }
  scan_archive(home)?
    .into_iter()
    .find(|d| d.id == id)
    .map(|d| d.relpath)
    .ok_or_else(|| format!("keine Notiz mit ID {id}"))
}

/// Löst ein Wikilink-Ziel (Name, Titel oder Datei-Stem) gegen das Archiv auf
/// und liefert den relpath des Dokuments.
pub(crate) fn resolve_doc(home: &Path, target: &str) -> Result<String, String> {
  let docs = scan_archive(home)?;
  let want = slugify(target);
  docs
    .iter()
    .find(|d| matches(d, &want))
    .map(|d| d.relpath.clone())
    .ok_or_else(|| format!("kein Archiv-Dokument zu „{target}“ gefunden"))
}

/// Alle `[[ziel]]`-Vorkommen im Text, in Dokumentreihenfolge; bei
/// `[[ziel|label]]` zählt nur das Ziel.
fn wikilinks(text: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut rest = text;
  while let Some(start) = rest.find("[[") {
    rest = &rest[start + 2..];
    let Some(end) = rest.find("]]") else {
      break;
    };
    let inner = &rest[..end];
    let target = inner.split('|').next().unwrap_or(inner).trim();
    if !target.is_empty() {
      out.push(target.to_string());
    }
    rest = &rest[end + 2..];
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::tmp_paths;
  use std::path::PathBuf;

  fn write(home: &Path, rel: &str, content: &str) {
    let path = home.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
  }

  fn archiv() -> PathBuf {
    let home = tmp_paths().home.join("archiv");
    fs::create_dir_all(&home).unwrap();
    write(
      &home,
      "2026-07-19_1000-adr-logging.md",
      "---\ntitle: \"ADR Logging\"\ndescription: \"Logging vereinheitlichen\"\ntags: [\"adr\", \"infra\"]\n---\n\nSiehe [[notiz-deploy|die Deploy-Notiz]].\n",
    );
    write(
      &home,
      "konzepte/2026-07-19_1005-notiz-deploy.md",
      "---\ntitle: \"Notiz Deploy\"\n---\n\nText ohne Links.\n",
    );
    write(&home, ".versteckt/ignoriert.md", "unsichtbar");
    home
  }

  #[test]
  fn scan_liest_baum_frontmatter_und_links() {
    let home = archiv();
    let docs = scan_archive(&home).unwrap();
    assert_eq!(docs.len(), 2);
    let adr = &docs[0];
    assert_eq!(adr.name, "adr-logging");
    assert_eq!(adr.title, "ADR Logging");
    assert_eq!(adr.description.as_deref(), Some("Logging vereinheitlichen"));
    assert_eq!(adr.tags, vec!["adr", "infra"]);
    assert_eq!(adr.links, vec!["notiz-deploy"]);
    assert_eq!(docs[1].relpath, "konzepte/2026-07-19_1005-notiz-deploy.md");
  }

  #[test]
  fn sonstige_datei_wird_datei_knoten() {
    let home = archiv();
    write(&home, "konzepte/daten.json", "{\"a\": 1}");
    let docs = scan_archive(&home).unwrap();
    let datei = docs.iter().find(|d| d.kind == "file").unwrap();
    assert_eq!(datei.id, "file:konzepte/daten.json");
    assert_eq!(datei.title, "daten.json");
    assert_eq!(datei.relpath, "konzepte/daten.json");
    assert!(datei.tags.is_empty());
  }

  #[test]
  fn backlinks_aus_wikilinks() {
    let home = archiv();
    let docs = scan_archive(&home).unwrap();
    let deploy = docs.iter().find(|d| d.name == "notiz-deploy").unwrap();
    assert_eq!(deploy.backlinks, vec!["adr-logging"]);
    let adr = docs.iter().find(|d| d.name == "adr-logging").unwrap();
    assert!(adr.backlinks.is_empty());
  }

  #[test]
  fn startseite_gruppiert_und_zaehlt() {
    let home = archiv();
    let page = archive_page(&home, None).unwrap();
    assert_eq!(page.kind, "page");
    assert_eq!(page.tag, None);
    assert_eq!(page.total, 2);
    let tags: Vec<(&str, usize)> =
      page.tags.iter().map(|t| (t.name.as_str(), t.count)).collect();
    assert_eq!(tags, vec![("adr", 1), ("infra", 1)]);
    assert_eq!(page.folders.len(), 2);
    assert_eq!(page.folders[0].name, "");
    let adr = &page.folders[0].docs[0];
    assert_eq!(adr.name, "adr-logging");
    assert_eq!(adr.title, "ADR Logging");
    assert_eq!(adr.description.as_deref(), Some("Logging vereinheitlichen"));
    assert_eq!(adr.date.as_deref(), Some("2026-07-19"));
    assert_eq!(adr.relpath, "2026-07-19_1000-adr-logging.md");
    assert_eq!(page.folders[1].name, "konzepte");
    assert_eq!(page.folders[1].docs[0].name, "notiz-deploy");
    assert_eq!(page.folders[1].docs[0].backlinks, 1);
  }

  #[test]
  fn leere_ordner_und_created_datum() {
    let home = archiv();
    fs::create_dir_all(home.join("leer/unter")).unwrap();
    write(
      &home,
      "notiz-plain.md",
      "---\ntitle: \"Plain\"\ncreated: 2026-07-23T10:00:00Z\n---\n\nText.\n",
    );
    let page = archive_page(&home, None).unwrap();
    let names: Vec<&str> = page.folders.iter().map(|f| f.name.as_str()).collect();
    // Dokumentlose Ordner erscheinen (Navigator), versteckte weiterhin nicht.
    assert!(names.contains(&"leer"));
    assert!(names.contains(&"leer/unter"));
    assert!(!names.iter().any(|n| n.starts_with(".versteckt")));
    // Ohne Zeitstempel im Namen kommt das Datum aus dem Frontmatter-created.
    let plain = page
      .folders
      .iter()
      .find(|f| f.name.is_empty())
      .unwrap()
      .docs
      .iter()
      .find(|d| d.name == "notiz-plain")
      .unwrap();
    assert_eq!(plain.date.as_deref(), Some("2026-07-23"));
  }

  #[test]
  fn tag_seite_filtert() {
    let home = archiv();
    let page = archive_page(&home, Some("adr")).unwrap();
    assert_eq!(page.tag.as_deref(), Some("adr"));
    assert_eq!(page.total, 1);
    assert_eq!(page.folders.len(), 1);
    assert_eq!(page.folders[0].docs[0].name, "adr-logging");
    // Die Schlagwort-Leiste bleibt vollständig — sie ist die Navigation.
    assert_eq!(page.tags.len(), 2);
  }

  #[test]
  fn resolve_ueber_name_titel_und_stem() {
    let home = archiv();
    // Auflösung über Titel; Name und Stem gehen über dieselben Slug-Vergleiche.
    assert_eq!(
      resolve_doc(&home, "Notiz Deploy").unwrap(),
      "konzepte/2026-07-19_1005-notiz-deploy.md"
    );
    assert_eq!(
      resolve_doc(&home, "2026-07-19_1000-adr-logging").unwrap(),
      "2026-07-19_1000-adr-logging.md"
    );
    assert!(resolve_doc(&home, "fehlt").is_err());
  }
}
