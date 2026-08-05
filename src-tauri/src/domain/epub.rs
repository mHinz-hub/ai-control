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
  /// Alle Angaben des Titelblatts in der Reihenfolge des OPF — Verlag, Jahr,
  /// ISBN, Herausgeber, Übersetzer. Was ein Zitat braucht, steht hier.
  pub(crate) meta: Vec<(String, String)>,
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

/// Ein Kapitel für den Suchindex: seine Adresse im Buch und sein Lesetext.
pub(crate) struct Kapitel {
  /// Href relativ zur Buchwurzel — Adresse des Sprungs.
  pub(crate) href: String,
  /// Überschrift aus dem Inhaltsverzeichnis, sonst der Buchtitel.
  pub(crate) titel: String,
  pub(crate) text: String,
  /// Druckseiten im Text: Zeichenposition → Seitenangabe, aufsteigend.
  /// Grundlage der Zitierfähigkeit — eine Fundstelle liegt zwischen zwei
  /// Marken und bekommt daraus ihre Seite.
  pub(crate) seiten: Vec<(usize, String)>,
}

/// Text einer Kapitelseite ohne Markup, dazu die Positionen der
/// Druckseitenmarken (`epub:type="pagebreak"`). `strip_tags` allein wirft sie
/// weg — mit ihnen fiele der Seitenbezug jedes Zitats.
fn text_mit_seiten(html: &str) -> (String, Vec<(usize, String)>) {
  let mut out = String::with_capacity(html.len() / 2);
  let mut seiten = Vec::new();
  let mut rest = html;
  while let Some(start) = rest.find('<') {
    out.push_str(&rest[..start]);
    let Some(ende) = rest[start..].find('>') else { break };
    let tag = &rest[start..start + ende + 1];
    if tag.contains("pagebreak") {
      // Die Seitenzahl steht im Label, nicht im Text.
      if let Some(l) = tag.find("aria-label=\"").map(|i| i + 12) {
        if let Some(bis) = tag[l..].find('"') {
          seiten.push((out.chars().count(), tag[l..l + bis].to_string()));
        }
      }
    }
    // Eine Formel steht als Baum aus Zeichen da; aneinandergereiht ergäben
    // sie Unsinn — aus `³√2` würde `23`. Wie die Stelle im Text zu lesen ist,
    // sagt ihr `alttext`.
    let name = tag.trim_start_matches('<').trim_start_matches('/');
    if name.starts_with("math") {
      if let Some(l) = tag.find("alttext=\"").map(|i| i + 9) {
        if let Some(bis) = tag[l..].find('"') {
          out.push_str(&tag[l..l + bis]);
        }
      }
      if let Some(e) = rest[start..].find("</math>") {
        rest = &rest[start + e + 7..];
        continue;
      }
    }
    // Skript- und Stilblöcke tragen keinen Lesetext.
    if name.starts_with("script") || name.starts_with("style") {
      let zu = format!("</{}>", name.split([' ', '>']).next().unwrap_or(""));
      if let Some(e) = rest[start..].find(&zu) {
        rest = &rest[start + e + zu.len()..];
        continue;
      }
    }
    rest = &rest[start + ende + 1..];
  }
  out.push_str(rest);
  (out, seiten)
}

/// Kapitel eines Buches in Lesereihenfolge, Text ohne Markup. Grundlage des
/// Suchindex: ein Eintrag je Kapitel, damit ein Treffer das Kapitel öffnet
/// und nicht bloß das Buch.
pub(crate) fn kapitel(path: &Path) -> Result<(String, Vec<Kapitel>), String> {
  let buch = open(path)?;
  let wurzel = cache_root().join(&buch.key);
  // Das Inhaltsverzeichnis benennt Kapitel über ihre Href (ggf. mit
  // Fragment); für die Überschrift zählt der Teil davor.
  let mut titel: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
  for t in &buch.toc {
    let href = t.href.split('#').next().unwrap_or(&t.href);
    titel.entry(href).or_insert(&t.title);
  }
  let mut out = Vec::new();
  for seite in &buch.spine {
    let (text, seiten) = match read_text(&wurzel.join(&seite.href)) {
      Ok(t) => text_mit_seiten(&t),
      // Eine Seite, die das ZIP nicht hergibt, ist kein Grund, das ganze
      // Buch aus dem Index zu lassen.
      Err(_) => continue,
    };
    if text.trim().is_empty() {
      continue;
    }
    out.push(Kapitel {
      href: seite.href.clone(),
      titel: titel.get(seite.href.as_str()).map(|s| s.to_string()).unwrap_or_else(|| buch.title.clone()),
      text,
      seiten,
    });
  }
  Ok((buch.title, out))
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
    meta: pkg.meta,
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
  meta: Vec<(String, String)>,
  layout: String,
  manifest: Vec<Item>,
  spine: Vec<SpineRef>,
}

/// OPF: Metadaten, Manifest und Spine — Kern des Formats.
fn parse_opf(text: &str) -> Result<Package, String> {
  let mut reader = reader(text);
  let mut buf = Vec::new();
  let (mut title, mut creator, mut language) = (String::new(), None, None);
  let mut meta: Vec<(String, String)> = Vec::new();
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
        // Die Angaben des Titelblatts stehen als Dublin-Core-Elemente im OPF;
        // gesammelt werden sie alle, nicht nur die drei, die der Viewer in
        // seiner Fußzeile führt.
        if !matches!(open.as_slice(), b"meta" | b"package" | b"metadata") {
          meta.push((String::from_utf8_lossy(&open).into_owned(), value.clone()));
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
  Ok(Package { title, creator, language, meta, layout, manifest, spine })
}

/// Nav-Dokument (EPUB 3): das Inhaltsverzeichnis steht im `nav`-Element mit
/// `epub:type="toc"`; die Tiefe der Listen ist die Gliederungsebene.
///
/// Nur dieses eine Element zählt. Daneben stehen im selben Dokument die
/// Landmarken und die Seitenliste — letztere hat bei einem Buch mit
/// Druckseitenmarken hunderte Einträge, die sonst als lauter Zahlen hinter
/// den Kapiteln im Verzeichnis landen. Ein Nav ohne `epub:type` gilt als
/// Inhaltsverzeichnis, solange keines mit `toc` gefunden wurde.
fn parse_nav(text: &str, base: &str) -> Vec<TocItem> {
  let mut reader = reader(text);
  let mut buf = Vec::new();
  let mut out: Vec<TocItem> = Vec::new();
  let mut depth = 0usize;
  let mut href: Option<String> = None;
  let mut label = String::new();
  // Innerhalb welchen nav-Elements wir gerade sind und ob es ausdrücklich
  // das Inhaltsverzeichnis ist.
  let mut im_nav: Option<bool> = None;
  let mut toc_gefunden = false;
  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(e)) => match local(e.name().as_ref()) {
        b"nav" => {
          let typ = attr(&e, b"epub:type").or_else(|| attr(&e, b"type"));
          let ist_toc = match typ.as_deref() {
            Some(t) => t.split_whitespace().any(|x| x == "toc"),
            None => !toc_gefunden,
          };
          if ist_toc {
            // Ein zweites Verzeichnis ersetzt kein vorhandenes; das
            // ausdrückliche `toc` gewinnt gegen ein typloses davor.
            if typ.is_some() && !toc_gefunden {
              out.clear();
            }
            toc_gefunden = toc_gefunden || typ.is_some();
          }
          im_nav = Some(ist_toc);
          depth = 0;
        }
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
        b"nav" => im_nav = None,
        b"ol" => depth = depth.saturating_sub(1),
        b"a" => {
          if let Some(h) = href.take() {
            if im_nav != Some(false) {
              out.push(TocItem {
                title: label.trim().to_string(),
                href: join_rel(base, &h),
                level: depth.saturating_sub(1),
              });
            }
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
  let ohne_fragment = url_path.split('#').next().unwrap_or_default();
  let (pfad_teil, query) = match ohne_fragment.split_once('?') {
    Some((p, q)) => (p, Some(q)),
    None => (ohne_fragment, None),
  };
  let rel = decode(pfad_teil.trim_start_matches('/'));
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
  // Fundstellen der Suche werden beim Ausliefern eingesetzt. Der Reader zeigt
  // Kapitel in einer leeren Sandbox — dort läuft kein Skript, das sie
  // nachträglich markieren könnte, und die eigene Origin der Seite verwehrt
  // jeden Zugriff von außen.
  let woerter: Vec<String> = query
    .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("hit=")))
    .map(|v| decode(v).split(',').map(str::to_string).filter(|w| !w.is_empty()).collect())
    .unwrap_or_default();
  let typ = mime(&path);
  if typ == "application/xhtml+xml" {
    if let Ok(text) = String::from_utf8(bytes.clone()) {
      let text = if woerter.is_empty() { text } else { markiere(&text, &woerter) };
      return Ok((mit_blaetterhilfe(&text).into_bytes(), typ));
    }
  }
  Ok((bytes, typ))
}

/// Das Skript, das der Reader in jede Buchseite legt.
///
/// Der Viewer läuft auf `tauri://localhost`, die Seite auf `epub://localhost`
/// — verschiedene Protokolle, also verschiedene Ursprünge. Von außen ist an
/// den Scrollstand der Seite darum nicht heranzukommen, und ohne ihn gäbe es
/// kein Blättern innerhalb eines Kapitels: jeder Tastendruck spränge ins
/// nächste. Die Seite sagt ihn selbst, über `postMessage`.
///
/// Die Kapitel sind XHTML, also XML: `nodeName` kommt dort klein zurück
/// (`a`, nicht `A`). Elementnamen darum immer klein vergleichen.
const BLAETTERN: &str = "(function(){\
function e(){return document.scrollingElement||document.documentElement}\
function s(){var x=e();return{oben:x.scrollTop,rand:x.scrollHeight-x.clientHeight}}\
var seitig=false,zeiger=0,urtext=null,skala=1;\
function alle(w){var q=w.querySelectorAll('[role=\"doc-pagebreak\"]'),l=[],i;\
for(i=0;i<q.length;i++)l.push(q[i]);return l}\
function marken(){return alle(urtext||document)}\
function hier(){var mk=alle(document),i,n=-1;\
for(i=0;i<mk.length;i++)if(mk[i].getBoundingClientRect().top<=8)n=i;return n}\
function offen(){var mk=alle(document),i,l=[],h=e().clientHeight,t,u;\
for(i=0;i<mk.length;i++){t=mk[i].getBoundingClientRect().top;\
u=i+1<mk.length?mk[i+1].getBoundingClientRect().top:1e9;\
if(u>0&&t<h)l.push(mk[i].getAttribute('aria-label'))}\
return l}\
function m(){var v=s(),mk=marken(),i=seitig?zeiger:hier(),o=offen(),g=seitig?grenzen():[];\
if(seitig&&g[i])o=[g[i].l];\
parent.postMessage({ac:'stand',oben:v.oben,rand:v.rand,seitig:seitig,seiten:o,\
seite:seitig?(g[i]?g[i].l:''):(i<0||!mk[i]?'':mk[i].getAttribute('aria-label')),\
von:i+1,bis:seitig?g.length:mk.length},'*')}\
function sichern(){if(urtext)return;\
urtext=document.createDocumentFragment();\
while(document.body.firstChild)urtext.appendChild(document.body.firstChild)}\
function heilen(){if(!urtext)return;\
document.body.textContent='';document.body.appendChild(urtext);urtext=null}\
function stellen(an){var b=document.body,h=document.documentElement;\
if(!an){seitig=false;skala=1;b.style.transform='';b.style.transformOrigin='';\
b.style.height='';h.style.overflow='';heilen();m();return}\
var nah=hier();sichern();seitig=true;zeigeSeite(nah<0?0:nah)}\
function grenzen(){var mk=marken(),g=[],i,r;\
if(!mk.length)return g;\
r=document.createRange();r.setStart(urtext,0);r.setEndBefore(mk[0]);\
if(String(r).replace(/[\\s\\u00a0]/g,''))\
g.push({k:null,l:String(parseInt(mk[0].getAttribute('aria-label'),10)-1)});\
for(i=0;i<mk.length;i++)g.push({k:mk[i],l:mk[i].getAttribute('aria-label')});\
return g}\
function zeigeSeite(i){var b=document.body,h=document.documentElement,g=grenzen();\
if(!g.length){stellen(false);return}\
zeiger=Math.max(0,Math.min(g.length-1,i));\
var r=document.createRange();\
if(g[zeiger].k)r.setStartBefore(g[zeiger].k);else r.setStart(urtext,0);\
if(zeiger+1<g.length)r.setEndBefore(g[zeiger+1].k);\
else r.setEnd(urtext,urtext.childNodes.length);\
var kasten=document.createElement('div');kasten.appendChild(r.cloneContents());\
var noten=kasten.querySelectorAll('.footnotes'),q;\
for(q=0;q<noten.length;q++)noten[q].parentNode.removeChild(noten[q]);\
b.style.transform='';b.style.height='';skala=1;\
b.textContent='';b.appendChild(kasten);\
h.style.overflow='hidden';e().scrollTop=0;\
var bt=b.getBoundingClientRect().top,luft=32,\
kt=kasten.getBoundingClientRect(),\
tief=kt.top-bt+kt.height,frei=e().clientHeight-bt-luft;\
if(tief>frei){skala=frei/tief;\
b.style.transformOrigin='0 0';b.style.transform='scale('+skala+')'}\
var u=kasten.getBoundingClientRect().bottom,ziel=e().clientHeight-luft;\
if(u>ziel+0.5){skala=skala*(ziel-bt)/(u-bt);\
b.style.transformOrigin='0 0';b.style.transform='scale('+skala+')'}\
var bilder=kasten.querySelectorAll('img'),z;\
for(z=0;z<bilder.length;z++)if(!bilder[z].complete)\
bilder[z].addEventListener('load',function(){if(seitig)zeigeSeite(zeiger)});\
m()}\
addEventListener('message',function(ev){var d=ev.data||{},x=e(),v=s();\
if(d.ac==='blaettern'){\
if(seitig){if(d.richtung>0&&zeiger+1<grenzen().length){zeigeSeite(zeiger+1);return}\
if(d.richtung<0&&zeiger>0){zeigeSeite(zeiger-1);return}\
parent.postMessage({ac:'rand',richtung:d.richtung},'*');return}\
var w=x.clientHeight*0.9;\
if(d.richtung>0&&v.oben<v.rand-2){x.scrollTop=Math.min(v.rand,v.oben+w);m();return}\
if(d.richtung<0&&v.oben>2){x.scrollTop=Math.max(0,v.oben-w);m();return}\
parent.postMessage({ac:'rand',richtung:d.richtung},'*')}\
else if(d.ac==='anDenFuss'){if(seitig){zeigeSeite(grenzen().length-1);return}\
x.scrollTop=x.scrollHeight;m()}\
else if(d.ac==='schrift'){document.documentElement.style.fontSize=d.wert+'%';\
if(seitig)zeigeSeite(zeiger);else m()}\
else if(d.ac==='seitig'){stellen(!!d.an)}\
else if(d.ac==='marker'){document.documentElement.className=\
d.an?'ac-marken':'';if(seitig)zeigeSeite(zeiger);else m()}\
else if(d.ac==='marke'){if(seitig){var g=grenzen(),j;\
for(j=0;j<g.length;j++)if(g[j].k&&g[j].k.id===d.id){zeigeSeite(j);return}\
zeigeSeite(zeiger);return}\
var z=document.getElementById(d.id);if(z)z.scrollIntoView();m()}});\
addEventListener('scroll',m,{passive:true});\
addEventListener('load',m);\
if(window.requestAnimationFrame)requestAnimationFrame(m);\
addEventListener('resize',function(){if(seitig)zeigeSeite(zeiger)});\
addEventListener('keydown',function(ev){\
var k=ev.key;\
if(k!=='ArrowRight'&&k!=='ArrowLeft'&&k!=='PageDown'&&k!=='PageUp')return;\
ev.preventDefault();\
parent.postMessage({ac:'taste',key:k,shift:ev.shiftKey},'*')});\
var stil=document.createElement('style');\
stil.textContent='html.ac-marken [role=\"doc-pagebreak\"]::after{'+\
'content:\"|\" attr(aria-label);font-size:0.68em;vertical-align:0.42em;'+\
'color:#1d7fd6;opacity:0.85;padding:0 0.18em;white-space:nowrap}'+\
'#ac-note{position:absolute;left:8%;right:8%;z-index:9;'+\
'background:#fffdf5;border:1px solid #c9c3b0;border-radius:6px;'+\
'box-shadow:0 6px 18px rgba(0,0,0,.18);padding:0.6em 0.9em;'+\
'font-size:0.88em}'+\
'#ac-note hr,#ac-note .footnote-back{display:none}';\
document.head.appendChild(stil);\
function notiz(){var k=document.getElementById('ac-note');\
if(k)k.parentNode.removeChild(k)}\
function istA(x){return !!x&&String(x.nodeName||'').toLowerCase()==='a'}\
addEventListener('click',function(ev){var a=ev.target,i;\
for(i=0;i<4&&a&&!istA(a);i++)a=a.parentNode;\
if(!istA(a)){notiz();return}\
var h=a.getAttribute('href')||'';\
if(h.charAt(0)!=='#'){notiz();return}\
var ziel=null,q=(urtext||document).querySelectorAll('[id]'),j;\
for(j=0;j<q.length;j++)if(q[j].id===h.slice(1)){ziel=q[j];break}\
if(!ziel||!/doc-footnote|footnote/.test(ziel.getAttribute('role')+' '+ziel.className)){notiz();return}\
ev.preventDefault();notiz();\
var k=document.createElement('div');k.id='ac-note';\
k.appendChild(ziel.cloneNode(true));\
var r=a.getBoundingClientRect();\
k.style.top=(r.bottom+e().scrollTop+6)+'px';\
document.body.appendChild(k)});\
addEventListener('keydown',function(ev){if(ev.key==='Escape')notiz()});\
addEventListener('contextmenu',function(ev){\
var w=window.getSelection?String(window.getSelection()):'';\
if(!w)ev.preventDefault()});m()})()";

/// Setzt das Blätter-Skript in die Seite und sperrt zugleich jedes andere:
/// Das Buch läuft mit `allow-scripts`, damit dieses eine arbeiten kann — die
/// Regel mit dem Einmalwert lässt kein zweites zu. Ein fremdes ePub bringt
/// damit nichts zur Ausführung.
fn mit_blaetterhilfe(text: &str) -> String {
  let einmal = digest(&format!("{:?}{}", std::time::SystemTime::now(), text.len()));
  let kopf = format!(
    "<meta http-equiv=\"Content-Security-Policy\" \
     content=\"script-src 'nonce-{einmal}'; object-src 'none'\"/>"
  );
  // Die Kapitel sind XHTML, kein HTML: dort ist `&` der Anfang einer Entität
  // und `<` der eines Tags. Ohne CDATA bräche der Parser am ersten `&&` des
  // Skripts ab und zeigte statt der Seite seine Fehlermeldung.
  let skript = format!("<script nonce=\"{einmal}\">//<![CDATA[\n{BLAETTERN}\n//]]></script>");
  // Die Regel muss vor allem stehen, was sie treffen soll; das eigene Skript
  // darf ans Ende des Kopfes.
  let mit_regel = match text.find("<head>") {
    Some(k) => format!("{}{kopf}{}", &text[..k + 6], &text[k + 6..]),
    None => format!("{kopf}{text}"),
  };
  match mit_regel.find("</head>") {
    Some(k) => format!("{}{skript}{}", &mit_regel[..k], &mit_regel[k..]),
    None => format!("{mit_regel}{skript}"),
  }
}

/// Umschließt die Wörter im Text des Dokuments mit `<mark class="ac-hit">`;
/// die erste Marke bekommt `id="ac-hit"`, damit der Reader über das Fragment
/// dorthin springt. Markup bleibt unangetastet: Verglichen wird nur zwischen
/// den spitzen Klammern.
pub(crate) fn markiere(text: &str, woerter: &[String]) -> String {
  let klein = text.to_lowercase();
  let gesucht: Vec<String> = woerter.iter().map(|w| w.to_lowercase()).collect();
  let bytes = text.as_bytes();
  let mut out = String::with_capacity(text.len() + 256);
  let mut i = 0usize;
  let mut im_tag = false;
  let mut erste = true;
  while i < bytes.len() {
    match bytes[i] {
      b'<' => im_tag = true,
      b'>' => im_tag = false,
      _ => {}
    }
    if im_tag || bytes[i] == b'>' {
      out.push(bytes[i] as char);
      i += 1;
      continue;
    }
    let treffer = gesucht
      .iter()
      .find(|w| klein[i..].starts_with(w.as_str()) && text.is_char_boundary(i + w.len()));
    match treffer {
      Some(w) => {
        let id = if erste { " id=\"ac-hit\"" } else { "" };
        erste = false;
        out.push_str(&format!("<mark class=\"ac-hit\"{id}>{}</mark>", &text[i..i + w.len()]));
        i += w.len();
      }
      None => {
        let start = i;
        i += 1;
        while i < bytes.len() && !text.is_char_boundary(i) {
          i += 1;
        }
        out.push_str(&text[start..i]);
      }
    }
  }
  // Farbe mitliefern: Das Kapitel bringt sein eigenes Stylesheet mit, unseres
  // erreicht es nicht.
  let stil = "<style>mark.ac-hit{background:#f9e2af;color:#1e1e2e}</style>";
  match out.find("</head>") {
    Some(k) => format!("{}{stil}{}", &out[..k], &out[k..]),
    None => format!("{stil}{out}"),
  }
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

  /// Seitenliste und Landmarken stehen im selben Dokument wie das
  /// Inhaltsverzeichnis. Ein Buch mit Druckseitenmarken bringt hunderte
  /// Zahlen mit — die gehören nicht ins Verzeichnis.
  /// Fundstellen werden im Text markiert, nicht im Markup — ein Wort, das
  /// zufällig in einem Attribut steht, bleibt unangetastet. Die erste Marke
  /// trägt die Sprungmarke.
  #[test]
  fn markiert_nur_den_text() {
    let seite = r#"<html><head><title>T</title></head><body>
      <p class="kessel">Der Kessel und der kessel.</p></body></html>"#;
    let out = markiere(seite, &["kessel".to_string()]);
    assert_eq!(out.matches("<mark").count(), 2, "{out}");
    assert!(out.contains(r#"<mark class="ac-hit" id="ac-hit">Kessel</mark>"#), "{out}");
    // Das Klassen-Attribut bleibt, wie es war.
    assert!(out.contains(r#"<p class="kessel">"#), "{out}");
    // Die Farbe reist mit, das Kapitel bringt unser Stylesheet nicht mit.
    assert!(out.contains("mark.ac-hit{"), "{out}");
  }

  /// Eine Formel geht als das in den Index, was sie im Text bedeutet — nicht
  /// als die Ziffernfolge ihres Baums.
  #[test]
  fn formel_kommt_als_lesbare_stelle_in_den_index() {
    let seite = r#"<html><body><p>die Zahl <math xmlns="http://www.w3.org/1998/Math/MathML"
      alttext="³√2"><mroot><mn>2</mn><mn>3</mn></mroot></math> als Lösung</p></body></html>"#;
    let (text, _) = text_mit_seiten(seite);
    assert!(text.contains("die Zahl ³√2 als Lösung"), "{text}");
    assert!(!text.contains("23"), "{text}");
  }

  /// Das Blätter-Skript reist mit jeder Buchseite; die Regel davor läßt genau
  /// dieses eine zu und sperrt aus, was das ePub selbst mitbringt.
  #[test]
  fn blaetterhilfe_kommt_mit_eigener_regel() {
    let seite = "<html><head><title>T</title></head><body><p>Text</p></body></html>";
    let out = mit_blaetterhilfe(seite);
    let einmal = out
      .split("nonce-")
      .nth(1)
      .and_then(|s| s.split('\'').next())
      .expect("Einmalwert in der Regel");
    assert!(out.contains(&format!("<script nonce=\"{einmal}\">")), "{out}");
    assert!(out.contains("Content-Security-Policy"), "{out}");
    // Die Regel steht vor dem Skript, sonst träfe sie es nicht.
    assert!(out.find("Content-Security-Policy") < out.find("<script"), "{out}");
    assert!(out.contains("<p>Text</p>"), "{out}");
    // Zwei Seiten teilen ihren Einmalwert nicht.
    assert!(!mit_blaetterhilfe(seite).contains(einmal));
  }

  #[test]
  fn nav_nimmt_nur_das_inhaltsverzeichnis() {
    let nav = r#"<html><body>
      <nav epub:type="toc"><ol>
        <li><a href="text/kap1.xhtml">Erstes</a></li>
      </ol></nav>
      <nav epub:type="landmarks"><ol>
        <li><a href="text/cover.xhtml" epub:type="cover">Cover</a></li>
      </ol></nav>
      <nav epub:type="page-list"><ol>
        <li><a href="text/kap1.xhtml#page9">9</a></li>
        <li><a href="text/kap1.xhtml#page10">10</a></li>
      </ol></nav>
    </body></html>"#;
    let toc = parse_nav(nav, "");
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Erstes");
  }

  /// Ohne `epub:type` gilt das erste Verzeichnis; taucht später eines mit
  /// `toc` auf, gewinnt dieses.
  #[test]
  fn nav_ohne_typ_gilt_als_verzeichnis() {
    let ohne = r#"<html><body><nav><ol>
      <li><a href="k1.xhtml">Erstes</a></li>
    </ol></nav></body></html>"#;
    assert_eq!(parse_nav(ohne, "").len(), 1);

    let beides = r#"<html><body>
      <nav><ol><li><a href="k0.xhtml">Vorspann</a></li></ol></nav>
      <nav epub:type="toc"><ol><li><a href="k1.xhtml">Erstes</a></li></ol></nav>
    </body></html>"#;
    let toc = parse_nav(beides, "");
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Erstes");
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
