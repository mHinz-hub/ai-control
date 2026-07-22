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
  /// Pfad relativ zum Archiv-Home.
  pub(crate) relpath: String,
  /// Wikilink-Name: Datei-Stem ohne führenden Zeitstempel.
  pub(crate) name: String,
  /// Frontmatter-Titel, sonst der Name.
  pub(crate) title: String,
  pub(crate) description: Option<String>,
  pub(crate) tags: Vec<String>,
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
    } else if fname.ends_with(".md") {
      docs.push(read_doc(home, &path)?);
    }
  }
  Ok(())
}

fn read_doc(home: &Path, path: &Path) -> Result<(Doc, String), String> {
  let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
  let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
  let name = strip_stamp(&stem).to_string();
  let fm = parse_frontmatter(&text);
  let relpath = path
    .strip_prefix(home)
    .map_err(|e| format!("{}: {e}", path.display()))?
    .display()
    .to_string();
  let doc = Doc {
    relpath,
    title: fm.get("title").unwrap_or(&name).clone(),
    description: fm.get("description").cloned(),
    tags: fm.get("tags").map(|t| parse_tag_list(t)).unwrap_or_default(),
    links: wikilinks(&text),
    backlinks: Vec::new(),
    name,
  };
  Ok((doc, text))
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
  /// Die neuesten Dokumente über alle Ordner; leer, wenn es nur einen
  /// Ordner gibt (dann wäre die Sektion eine Dublette der Liste).
  pub(crate) recent: Vec<WikiDocEntry>,
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
  pub(crate) name: String,
  pub(crate) title: String,
  pub(crate) description: Option<String>,
  pub(crate) tags: Vec<String>,
  /// Archivierungsdatum aus dem Zeitstempel-Stem (`YYYY-MM-DD`).
  pub(crate) date: Option<String>,
  /// Zahl der Dokumente, die per Wikilink hierher zeigen.
  pub(crate) backlinks: usize,
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
  for doc in &selected {
    let folder = Path::new(&doc.relpath)
      .parent()
      .map(|p| p.display().to_string())
      .unwrap_or_default();
    folders.entry(folder).or_default().push(doc);
  }
  let recent = if folders.len() > 1 {
    // Neueste zuerst über alle Ordner — der Zeitstempel-Stem sortiert
    // chronologisch, verglichen wird der Dateiname.
    let mut all = selected.clone();
    all.sort_by(|a, b| {
      Path::new(&b.relpath).file_name().cmp(&Path::new(&a.relpath).file_name())
    });
    all.into_iter().take(5).map(doc_entry).collect()
  } else {
    Vec::new()
  };
  Ok(WikiPage {
    kind: "page",
    home: home.display().to_string(),
    tag: tag.map(str::to_string),
    total: selected.len(),
    tags: counts
      .into_iter()
      .map(|(name, count)| TagCount { name: name.to_string(), count })
      .collect(),
    recent,
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

fn doc_entry(doc: &Doc) -> WikiDocEntry {
  let stem = Path::new(&doc.relpath)
    .file_stem()
    .unwrap_or_default()
    .to_string_lossy()
    .to_string();
  let date = (stem != doc.name).then(|| stem[..10].to_string());
  WikiDocEntry {
    name: doc.name.clone(),
    title: doc.title.clone(),
    description: doc.description.clone(),
    tags: doc.tags.clone(),
    date,
    backlinks: doc.backlinks.len(),
  }
}

/// Ein Archiv-Dokument als Wiki-Ansicht: Markdown-Rumpf ohne Frontmatter plus
/// Metadaten und Backlinks aus dem Scan.
#[derive(serde::Serialize)]
pub(crate) struct WikiDocPage {
  pub(crate) kind: &'static str,
  pub(crate) home: String,
  pub(crate) relpath: String,
  pub(crate) name: String,
  pub(crate) title: String,
  pub(crate) tags: Vec<String>,
  pub(crate) backlinks: Vec<String>,
  pub(crate) markdown: String,
}

pub(crate) fn wiki_doc(home: &Path, target: &str) -> Result<WikiDocPage, String> {
  let pairs = scan_with_bodies(home)?;
  let want = slugify(target);
  let (doc, body) = pairs
    .iter()
    .find(|(d, _)| matches(d, &want))
    .ok_or_else(|| format!("kein Archiv-Dokument zu „{target}“ gefunden"))?;
  Ok(WikiDocPage {
    kind: "doc",
    home: home.display().to_string(),
    relpath: doc.relpath.clone(),
    name: doc.name.clone(),
    title: doc.title.clone(),
    tags: doc.tags.clone(),
    backlinks: doc.backlinks.clone(),
    markdown: strip_frontmatter(body).to_string(),
  })
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
    assert_eq!(page.folders[1].name, "konzepte");
    assert_eq!(page.folders[1].docs[0].name, "notiz-deploy");
    // Zwei Ordner → Zuletzt-Sektion, neuestes Dokument zuerst, mit
    // Backlink-Zähler aus dem Scan.
    let recent: Vec<&str> = page.recent.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(recent, vec!["notiz-deploy", "adr-logging"]);
    assert_eq!(page.recent[0].backlinks, 1);
    assert_eq!(page.recent[1].backlinks, 0);
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
  fn wiki_doc_mit_rumpf_und_backlinks() {
    let home = archiv();
    // Auflösung über Titel; Name und Stem gehen über dieselben Slug-Vergleiche.
    let doc = wiki_doc(&home, "Notiz Deploy").unwrap();
    assert_eq!(doc.kind, "doc");
    assert_eq!(doc.name, "notiz-deploy");
    assert_eq!(
      wiki_doc(&home, "2026-07-19_1000-adr-logging").unwrap().name,
      "adr-logging"
    );
    assert_eq!(doc.title, "Notiz Deploy");
    assert_eq!(doc.relpath, "konzepte/2026-07-19_1005-notiz-deploy.md");
    assert_eq!(doc.backlinks, vec!["adr-logging"]);
    assert_eq!(doc.markdown, "Text ohne Links.\n");
    assert!(wiki_doc(&home, "fehlt").is_err());
  }
}
