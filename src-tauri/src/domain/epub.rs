//! ePub-Viewer: Ein Buch im Archiv ist ein ZIP mit XHTML-Seiten. Zum Anzeigen
//! wird es einmal in den Cache entpackt (`~/.config/ai-central/epub/<key>/`) —
//! die `.epub` im Archiv bleibt unangetastet, und die Invarianten des
//! Notizmodells (ensure_ids, ensure_node_texts) fassen nichts davon an: sie
//! laufen nur über `.md`/`.html` IM Archiv.
//!
//! Entpacken allein reicht nicht. Lesereihenfolge, Inhaltsverzeichnis und
//! Metadaten stehen nicht in den XHTML-Dateien, sondern in den
//! Verwaltungsdateien des Formats:
//!   META-INF/container.xml → Pfad des OPF
//!   OPF: Metadaten, Manifest (id → Datei), Spine (Lesereihenfolge), Layout
//!   Nav-Dokument (EPUB 3) bzw. toc.ncx (EPUB 2) → Inhaltsverzeichnis
//! Genau die drei liest dieses Modul; ausgeliefert wird `Book` — alles, was
//! der Viewer braucht.

use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::domain::paths::Paths;

/// Cache-Wurzel der entpackten Bücher — außerhalb des Archivs.
pub(crate) fn cache_root() -> PathBuf {
  Paths::real().config_dir().join("epub")
}

/// Ein geöffnetes Buch, wie der Viewer es braucht.
#[derive(serde::Serialize)]
pub(crate) struct Book {
  /// Cache-Schlüssel; erstes Segment der `epub://`-Adressen des Viewers.
  pub(crate) key: String,
  pub(crate) title: String,
  pub(crate) creator: Option<String>,
  pub(crate) language: Option<String>,
  /// `pre-paginated` (feste Seiten) oder `reflowable` (fließender Text).
  pub(crate) layout: String,
  /// Lesereihenfolge aus dem Spine — die Dateinamen im ZIP sagen sie nicht.
  pub(crate) spine: Vec<Page>,
  pub(crate) toc: Vec<TocItem>,
}

#[derive(serde::Serialize)]
pub(crate) struct Page {
  /// Pfad relativ zur Buchwurzel im Cache (URL-Pfad ohne führenden Slash).
  pub(crate) href: String,
  /// `left`/`right` aus `page-spread-*` des Spine-Eintrags — Seite einer
  /// Doppelseite.
  pub(crate) spread: Option<String>,
  /// Seitenmaße aus dem `<meta name="viewport">` der Seite; nur bei
  /// pre-paginated gefüllt, dort trägt jede Seite ihre eigene Größe.
  pub(crate) width: Option<u32>,
  pub(crate) height: Option<u32>,
}

#[derive(serde::Serialize)]
pub(crate) struct TocItem {
  pub(crate) title: String,
  /// Ziel relativ zur Buchwurzel, ggf. mit `#fragment`.
  pub(crate) href: String,
  /// Verschachtelungstiefe (0 = oberste Ebene).
  pub(crate) level: usize,
}

/// Öffnet ein Buch: entpacken (einmalig) und Verwaltungsdateien lesen.
pub(crate) fn open(path: &Path) -> Result<Book, String> {
  open_in(&cache_root(), path)
}

fn open_in(root: &Path, path: &Path) -> Result<Book, String> {
  let dir = ensure_unpacked(root, path)?;
  let key = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
  let opf_rel = rootfile(&dir)?;
  let opf = read_text(&dir.join(&opf_rel))?;
  // Alle Pfade im OPF sind relativ zu seinem eigenen Ordner.
  let base = parent_of(&opf_rel);
  let pkg = parse_opf(&opf)?;

  let mut spine = Vec::new();
  for entry in &pkg.spine {
    let item = pkg
      .manifest
      .iter()
      .find(|i| i.id == entry.idref)
      .ok_or_else(|| format!("Spine verweist auf unbekanntes Manifest-Item: {}", entry.idref))?;
    let href = join_rel(&base, &item.href);
    let (width, height) = if pkg.layout == "pre-paginated" {
      viewport(&read_text(&dir.join(&href))?)
    } else {
      (None, None)
    };
    spine.push(Page { href, spread: entry.spread.clone(), width, height });
  }

  let toc = read_toc(&dir, &base, &pkg)?;
  Ok(Book {
    key,
    title: pkg.title,
    creator: pkg.creator,
    language: pkg.language,
    layout: pkg.layout,
    spine,
    toc,
  })
}

/// Inhaltsverzeichnis: Nav-Dokument (EPUB 3), sonst `toc.ncx` (EPUB 2).
fn read_toc(dir: &Path, base: &str, pkg: &Package) -> Result<Vec<TocItem>, String> {
  if let Some(nav) = pkg.manifest.iter().find(|i| i.properties.split_whitespace().any(|p| p == "nav"))
  {
    let rel = join_rel(base, &nav.href);
    return Ok(parse_nav(&read_text(&dir.join(&rel))?, &parent_of(&rel)));
  }
  let ncx = pkg
    .manifest
    .iter()
    .find(|i| i.media_type == "application/x-dtbncx+xml");
  match ncx {
    Some(item) => {
      let rel = join_rel(base, &item.href);
      Ok(parse_ncx(&read_text(&dir.join(&rel))?, &parent_of(&rel)))
    }
    None => Ok(Vec::new()),
  }
}

// ---------- Entpacken ----------

/// Buchordner im Cache; entpackt beim ersten Öffnen. Der Schlüssel trägt die
/// Änderungszeit der Datei — ein ersetztes Buch bekommt damit von selbst einen
/// neuen Ordner statt eine veraltete Kopie.
fn ensure_unpacked(root: &Path, src: &Path) -> Result<PathBuf, String> {
  let meta = fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
  let mtime = meta
    .modified()
    .map_err(|e| format!("{}: {e}", src.display()))?
    .duration_since(std::time::UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs();
  let key = format!("{}-{mtime}", digest(&src.to_string_lossy()));
  let dir = root.join(&key);
  if dir.is_dir() {
    return Ok(dir);
  }
  // Erst vollständig daneben entpacken, dann an seinen Platz benennen: ein
  // abgebrochener Lauf hinterlässt so kein halbes Buch, das beim nächsten
  // Öffnen als fertig gälte.
  let tmp = dir.with_extension("part");
  if tmp.exists() {
    fs::remove_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
  }
  fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
  unpack(src, &tmp)?;
  fs::rename(&tmp, &dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  Ok(dir)
}

fn unpack(src: &Path, dest: &Path) -> Result<(), String> {
  let file = fs::File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
  let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", src.display()))?;
  for i in 0..zip.len() {
    let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
    // enclosed_name lehnt absolute Pfade und `..` ab — ein Buch schreibt
    // nur in seinen eigenen Ordner.
    let rel = entry
      .enclosed_name()
      .ok_or_else(|| format!("unzulässiger Pfad im Buch: {}", entry.name()))?;
    let out = dest.join(rel);
    if entry.is_dir() {
      fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
      continue;
    }
    if let Some(parent) = out.parent() {
      fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let mut target = fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    std::io::copy(&mut entry, &mut target).map_err(|e| format!("{}: {e}", out.display()))?;
  }
  Ok(())
}

fn digest(s: &str) -> String {
  use sha2::Digest;
  let mut h = sha2::Sha256::new();
  h.update(s.as_bytes());
  format!("{:x}", h.finalize())[..16].to_string()
}

fn read_text(path: &Path) -> Result<String, String> {
  let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
  Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ---------- Verwaltungsdateien ----------

/// `META-INF/container.xml` → Pfad des OPF im Buch.
fn rootfile(dir: &Path) -> Result<String, String> {
  let text = read_text(&dir.join("META-INF/container.xml"))?;
  let mut reader = reader(&text);
  let mut buf = Vec::new();
  loop {
    match reader.read_event_into(&mut buf).map_err(|e| e.to_string())? {
      Event::Empty(e) | Event::Start(e) if local(e.name().as_ref()) == b"rootfile" => {
        if let Some(p) = attr(&e, b"full-path") {
          return Ok(p);
        }
      }
      Event::Eof => break,
      _ => {}
    }
    buf.clear();
  }
  Err("kein rootfile in META-INF/container.xml".into())
}

struct Item {
  id: String,
  href: String,
  media_type: String,
  properties: String,
}

struct SpineRef {
  idref: String,
  spread: Option<String>,
}

struct Package {
  title: String,
  creator: Option<String>,
  language: Option<String>,
  layout: String,
  manifest: Vec<Item>,
  spine: Vec<SpineRef>,
}

/// OPF: Metadaten, Manifest und Spine — Kern des Formats.
fn parse_opf(text: &str) -> Result<Package, String> {
  let mut reader = reader(text);
  let mut buf = Vec::new();
  let (mut title, mut creator, mut language) = (String::new(), None, None);
  let mut layout = "reflowable".to_string();
  let mut manifest = Vec::new();
  let mut spine = Vec::new();
  // Textinhalt gehört zum zuletzt geöffneten Element (dc:title, meta …).
  let mut open: Vec<u8> = Vec::new();
  let mut property = String::new();
  loop {
    match reader.read_event_into(&mut buf).map_err(|e| e.to_string())? {
      Event::Start(e) => {
        open = local(e.name().as_ref()).to_vec();
        if open == b"meta" {
          property = attr(&e, b"property").unwrap_or_default();
        }
      }
      Event::Empty(e) => match local(e.name().as_ref()) {
        b"item" => manifest.push(Item {
          id: attr(&e, b"id").unwrap_or_default(),
          href: attr(&e, b"href").unwrap_or_default(),
          media_type: attr(&e, b"media-type").unwrap_or_default(),
          properties: attr(&e, b"properties").unwrap_or_default(),
        }),
        b"itemref" => spine.push(SpineRef {
          idref: attr(&e, b"idref").unwrap_or_default(),
          spread: attr(&e, b"properties").and_then(|p| {
            p.split_whitespace()
              .find_map(|x| x.strip_prefix("page-spread-").map(str::to_string))
          }),
        }),
        // EPUB 2 kennt kein Layout-Property; dort steht die feste Seitengröße
        // als `<meta name="fixed-layout" content="true">`.
        b"meta" => {
          if attr(&e, b"name").as_deref() == Some("fixed-layout")
            && attr(&e, b"content").as_deref() == Some("true")
          {
            layout = "pre-paginated".to_string();
          }
        }
        _ => {}
      },
      Event::Text(t) => {
        let value = t.unescape().map_err(|e| e.to_string())?.trim().to_string();
        if value.is_empty() {
          continue;
        }
        match open.as_slice() {
          b"title" if title.is_empty() => title = value,
          b"creator" if creator.is_none() => creator = Some(value),
          b"language" if language.is_none() => language = Some(value),
          b"meta" if property == "rendition:layout" => layout = value,
          _ => {}
        }
      }
      Event::End(_) => open.clear(),
      Event::Eof => break,
      _ => {}
    }
    buf.clear();
  }
  if spine.is_empty() {
    return Err("kein Spine im OPF — keine Lesereihenfolge".into());
  }
  Ok(Package { title, creator, language, layout, manifest, spine })
}

/// Nav-Dokument (EPUB 3): die erste `<ol>`-Verschachtelung im `nav`-Element
/// ist das Inhaltsverzeichnis; die Tiefe der Listen ist die Gliederungsebene.
fn parse_nav(text: &str, base: &str) -> Vec<TocItem> {
  let mut reader = reader(text);
  let mut buf = Vec::new();
  let mut out = Vec::new();
  let mut depth = 0usize;
  let mut href: Option<String> = None;
  let mut label = String::new();
  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(e)) => match local(e.name().as_ref()) {
        b"ol" => depth += 1,
        b"a" => {
          href = attr(&e, b"href");
          label.clear();
        }
        _ => {}
      },
      Ok(Event::Text(t)) => {
        if href.is_some() {
          label.push_str(t.unescape().unwrap_or_default().trim());
        }
      }
      Ok(Event::End(e)) => match local(e.name().as_ref()) {
        b"ol" => depth = depth.saturating_sub(1),
        b"a" => {
          if let Some(h) = href.take() {
            out.push(TocItem {
              title: label.trim().to_string(),
              href: join_rel(base, &h),
              level: depth.saturating_sub(1),
            });
          }
        }
        _ => {}
      },
      Ok(Event::Eof) | Err(_) => break,
      _ => {}
    }
    buf.clear();
  }
  out
}

/// `toc.ncx` (EPUB 2): verschachtelte `navPoint`s mit `navLabel/text` und
/// `content src`.
fn parse_ncx(text: &str, base: &str) -> Vec<TocItem> {
  let mut reader = reader(text);
  let mut buf = Vec::new();
  let mut out = Vec::new();
  let mut depth = 0usize;
  let mut label = String::new();
  let mut in_text = false;
  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(e)) => match local(e.name().as_ref()) {
        b"navPoint" => {
          depth += 1;
          label.clear();
        }
        b"text" => in_text = true,
        _ => {}
      },
      Ok(Event::Text(t)) => {
        if in_text {
          label.push_str(t.unescape().unwrap_or_default().trim());
        }
      }
      Ok(Event::Empty(e)) if local(e.name().as_ref()) == b"content" => {
        if let Some(src) = attr(&e, b"src") {
          out.push(TocItem {
            title: label.trim().to_string(),
            href: join_rel(base, &src),
            level: depth.saturating_sub(1),
          });
        }
      }
      Ok(Event::End(e)) => match local(e.name().as_ref()) {
        b"navPoint" => depth = depth.saturating_sub(1),
        b"text" => in_text = false,
        _ => {}
      },
      Ok(Event::Eof) | Err(_) => break,
      _ => {}
    }
    buf.clear();
  }
  out
}

/// Seitenmaße einer Fixed-Layout-Seite: `<meta name="viewport"
/// content="width=1200, height=1600">`.
fn viewport(text: &str) -> (Option<u32>, Option<u32>) {
  let mut meta = crate::domain::archive_html::parse_meta(text);
  let Some(content) = meta.remove("viewport") else {
    return (None, None);
  };
  let value = |key: &str| {
    content
      .split(',')
      .filter_map(|p| p.trim().split_once('='))
      .find(|(k, _)| k.trim() == key)
      .and_then(|(_, v)| v.trim().parse::<u32>().ok())
  };
  (value("width"), value("height"))
}

// ---------- XML- und Pfad-Handwerk ----------

/// XML-Leser für die Verwaltungsdateien: nachlässig bei nicht geschlossenen
/// Elementen — gelesen wird aus fremden Büchern, nicht aus eigenen Dateien.
fn reader(text: &str) -> Reader<&[u8]> {
  let mut reader = Reader::from_str(text);
  reader.config_mut().check_end_names = false;
  reader
}

/// Elementname ohne Namensraum-Präfix (`dc:title` → `title`).
fn local(name: &[u8]) -> &[u8] {
  match name.iter().rposition(|b| *b == b':') {
    Some(i) => &name[i + 1..],
    None => name,
  }
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
  e.attributes().flatten().find(|a| local(a.key.as_ref()) == key).map(|a| {
    String::from_utf8_lossy(&a.value).into_owned()
  })
}

/// Ordner eines Pfads („OEBPS/content.opf" → „OEBPS"); leer für die Wurzel.
fn parent_of(rel: &str) -> String {
  match rel.rsplit_once('/') {
    Some((head, _)) => head.to_string(),
    None => String::new(),
  }
}

/// Hängt ein relatives href an seinen Basisordner und löst `.`/`..` auf; das
/// Fragment (`#kapitel`) bleibt erhalten. Ergebnis ist ein Pfad relativ zur
/// Buchwurzel.
fn join_rel(base: &str, href: &str) -> String {
  let (path, fragment) = match href.split_once('#') {
    Some((p, f)) => (p, Some(f)),
    None => (href, None),
  };
  let mut parts: Vec<&str> = Vec::new();
  for part in base.split('/').chain(path.split('/')) {
    match part {
      "" | "." => {}
      ".." => {
        parts.pop();
      }
      p => parts.push(p),
    }
  }
  let joined = parts.join("/");
  match fragment {
    Some(f) => format!("{joined}#{f}"),
    None => joined,
  }
}

// ---------- Auslieferung an den Viewer ----------

/// Antwort auf eine `epub://`-Anfrage: der URL-Pfad (`/<key>/<datei>`) zeigt
/// in den Cache. Die Segmente bleiben echte Pfadsegmente — nur so lösen die
/// relativen Verweise der Seiten (`../images/a.png`, Stylesheets, Schriften)
/// im Webview auf.
pub(crate) fn serve(url_path: &str) -> Result<(Vec<u8>, &'static str), String> {
  serve_in(&cache_root(), url_path)
}

fn serve_in(root: &Path, url_path: &str) -> Result<(Vec<u8>, &'static str), String> {
  let rel = decode(url_path.split(['?', '#']).next().unwrap_or_default().trim_start_matches('/'));
  let root = root
    .canonicalize()
    .map_err(|e| format!("{}: {e}", root.display()))?;
  let path = root
    .join(&rel)
    .canonicalize()
    .map_err(|e| format!("{rel}: {e}"))?;
  // Das Buch liest aus seinem Cache-Ordner, sonst nirgendwoher.
  if !path.starts_with(&root) {
    return Err(format!("außerhalb des Buch-Caches: {rel}"));
  }
  let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
  Ok((bytes, mime(&path)))
}

/// Prozent-Dekodierung des URL-Pfads (Leerzeichen, Umlaute in Dateinamen).
fn decode(s: &str) -> String {
  let bytes = s.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = (i + 2 < bytes.len())
      .then(|| std::str::from_utf8(&bytes[i + 1..i + 3]).ok())
      .flatten()
      .filter(|_| bytes[i] == b'%')
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match hex {
      Some(b) => {
        out.push(b);
        i += 3;
      }
      None => {
        out.push(bytes[i]);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

/// Inhaltstyp für die Auslieferung über das `epub://`-Protokoll.
pub(crate) fn mime(path: &Path) -> &'static str {
  match path.extension().and_then(|e| e.to_str()).unwrap_or_default() {
    "xhtml" | "html" | "htm" => "application/xhtml+xml",
    "css" => "text/css",
    "js" => "text/javascript",
    "png" => "image/png",
    "jpg" | "jpeg" => "image/jpeg",
    "gif" => "image/gif",
    "svg" => "image/svg+xml",
    "webp" => "image/webp",
    "woff" => "font/woff",
    "woff2" => "font/woff2",
    "ttf" => "font/ttf",
    "otf" => "font/otf",
    "xml" | "ncx" | "opf" => "application/xml",
    _ => "application/octet-stream",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Tractatus</dc:title>
    <dc:creator>Wittgenstein</dc:creator>
    <dc:language>de</dc:language>
    <meta property="rendition:layout">pre-paginated</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="k1" href="text/kap1.xhtml" media-type="application/xhtml+xml"/>
    <item id="k2" href="text/kap2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="k1" properties="page-spread-left"/>
    <itemref idref="k2"/>
  </spine>
</package>"#;

  #[test]
  fn opf_metadaten_manifest_und_spine() {
    let pkg = parse_opf(OPF).unwrap();
    assert_eq!(pkg.title, "Tractatus");
    assert_eq!(pkg.creator.as_deref(), Some("Wittgenstein"));
    assert_eq!(pkg.language.as_deref(), Some("de"));
    assert_eq!(pkg.layout, "pre-paginated");
    assert_eq!(pkg.manifest.len(), 3);
    assert_eq!(pkg.spine.len(), 2);
    assert_eq!(pkg.spine[0].idref, "k1");
    assert_eq!(pkg.spine[0].spread.as_deref(), Some("left"));
    assert_eq!(pkg.spine[1].spread, None);
  }

  /// Ohne Spine gibt es keine Lesereihenfolge — das bricht laut ab, statt ein
  /// Buch mit willkürlicher Dateireihenfolge zu zeigen.
  #[test]
  fn opf_ohne_spine_scheitert() {
    let text = OPF.replace("<itemref idref=\"k1\" properties=\"page-spread-left\"/>", "")
      .replace("<itemref idref=\"k2\"/>", "");
    assert!(parse_opf(&text).is_err());
  }

  #[test]
  fn nav_liest_verschachtelung() {
    let nav = r#"<html><body><nav epub:type="toc"><ol>
      <li><a href="text/kap1.xhtml">Erstes</a>
        <ol><li><a href="text/kap1.xhtml#a">Abschnitt</a></li></ol>
      </li>
      <li><a href="text/kap2.xhtml">Zweites</a></li>
    </ol></nav></body></html>"#;
    let toc = parse_nav(nav, "");
    assert_eq!(toc.len(), 3);
    assert_eq!(toc[0].title, "Erstes");
    assert_eq!(toc[0].level, 0);
    assert_eq!(toc[1].title, "Abschnitt");
    assert_eq!(toc[1].level, 1);
    assert_eq!(toc[1].href, "text/kap1.xhtml#a");
    assert_eq!(toc[2].level, 0);
  }

  #[test]
  fn ncx_liest_verschachtelung() {
    let ncx = r#"<ncx><navMap>
      <navPoint><navLabel><text>Erstes</text></navLabel><content src="kap1.html"/>
        <navPoint><navLabel><text>Abschnitt</text></navLabel><content src="kap1.html#a"/></navPoint>
      </navPoint>
    </navMap></ncx>"#;
    let toc = parse_ncx(ncx, "OEBPS");
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Erstes");
    assert_eq!(toc[0].href, "OEBPS/kap1.html");
    assert_eq!(toc[0].level, 0);
    assert_eq!(toc[1].level, 1);
    assert_eq!(toc[1].href, "OEBPS/kap1.html#a");
  }

  #[test]
  fn join_rel_loest_punkte_auf() {
    assert_eq!(join_rel("OEBPS", "text/kap1.xhtml"), "OEBPS/text/kap1.xhtml");
    assert_eq!(join_rel("OEBPS/text", "../images/a.png"), "OEBPS/images/a.png");
    assert_eq!(join_rel("", "content.opf"), "content.opf");
    assert_eq!(join_rel("OEBPS", "./kap.xhtml#z"), "OEBPS/kap.xhtml#z");
  }

  #[test]
  fn viewport_aus_meta() {
    let page = r#"<html><head><meta name="viewport" content="width=1200, height=1600"><title>x</title></head><body/></html>"#;
    assert_eq!(viewport(page), (Some(1200), Some(1600)));
    assert_eq!(viewport("<html><head><title>x</title></head></html>"), (None, None));
  }

  /// Ganzer Weg an einem gebauten Buch: ZIP entpacken, container.xml → OPF,
  /// Spine in Lesereihenfolge, Nav als Inhaltsverzeichnis, Seitenmaße aus dem
  /// Viewport der Seite.
  #[test]
  fn buch_oeffnen_von_zip_bis_seiten() {
    let home = crate::domain::testutil::tmp_paths().home;
    let book = home.join("tractatus.epub");
    fs::create_dir_all(&home).unwrap();
    let seite = |n: u32| {
      format!(
        "<html><head><meta name=\"viewport\" content=\"width=800, height=1200\">\
         <title>Seite {n}</title></head><body>Seite {n}</body></html>"
      )
    };
    write_zip(
      &book,
      &[
        (
          "META-INF/container.xml",
          "<container><rootfiles><rootfile full-path=\"OEBPS/content.opf\"/></rootfiles></container>"
            .to_string(),
        ),
        ("OEBPS/content.opf", OPF.to_string()),
        (
          "OEBPS/nav.xhtml",
          "<html><body><nav><ol><li><a href=\"text/kap2.xhtml\">Zweites</a></li></ol></nav></body></html>"
            .to_string(),
        ),
        ("OEBPS/text/kap1.xhtml", seite(1)),
        ("OEBPS/text/kap2.xhtml", seite(2)),
      ],
    );

    let root = home.join("cache");
    let b = open_in(&root, &book).unwrap();
    assert_eq!(b.title, "Tractatus");
    assert_eq!(b.layout, "pre-paginated");
    // Reihenfolge kommt aus dem Spine, nicht aus dem ZIP.
    assert_eq!(b.spine.len(), 2);
    assert_eq!(b.spine[0].href, "OEBPS/text/kap1.xhtml");
    assert_eq!(b.spine[0].spread.as_deref(), Some("left"));
    assert_eq!(b.spine[1].href, "OEBPS/text/kap2.xhtml");
    // Feste Seiten tragen ihre Maße selbst.
    assert_eq!((b.spine[0].width, b.spine[0].height), (Some(800), Some(1200)));
    assert_eq!(b.toc.len(), 1);
    assert_eq!(b.toc[0].title, "Zweites");
    assert_eq!(b.toc[0].href, "OEBPS/text/kap2.xhtml");
    // Entpackt liegt es im Cache, die Seiten sind lesbar.
    let dir = root.join(&b.key);
    assert!(dir.join("OEBPS/text/kap1.xhtml").is_file());
    // Zweites Öffnen nimmt denselben Ordner.
    assert_eq!(open_in(&root, &book).unwrap().key, b.key);
    // Auslieferung: Pfad unter dem Cache wird gelesen, Ausbruch scheitert.
    let (bytes, mime) = serve_in(&root, &format!("/{}/OEBPS/text/kap1.xhtml", b.key)).unwrap();
    assert_eq!(mime, "application/xhtml+xml");
    assert!(String::from_utf8_lossy(&bytes).contains("Seite 1"));
    assert!(serve_in(&root, &format!("/{}/../../etc/hosts", b.key)).is_err());
  }

  fn write_zip(path: &Path, entries: &[(&str, String)]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<()> =
      zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in entries {
      zip.start_file(*name, opts).unwrap();
      std::io::Write::write_all(&mut zip, content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
  }

  #[test]
  fn decode_prozent_sequenzen() {
    assert_eq!(decode("text/kap%201.xhtml"), "text/kap 1.xhtml");
    assert_eq!(decode("a/%C3%BCber.xhtml"), "a/über.xhtml");
    assert_eq!(decode("a/b.xhtml"), "a/b.xhtml");
    // Angebrochene Sequenz am Ende bleibt stehen, statt zu verschlucken.
    assert_eq!(decode("a%"), "a%");
  }

  #[test]
  fn mime_nach_endung() {
    assert_eq!(mime(Path::new("a/b.xhtml")), "application/xhtml+xml");
    assert_eq!(mime(Path::new("a/b.PNG")), "application/octet-stream");
    assert_eq!(mime(Path::new("a/b.png")), "image/png");
  }
}
