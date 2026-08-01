//! HTML-Notizen: Gegenstück zu den Frontmatter-Funktionen in archive.rs.
//! Dieselben Metadaten (id, title, project, created, description, tags)
//! stehen hier an den Stellen, die HTML dafür vorsieht — `<title>` und
//! `<meta name=…>`. Geparst wird mit denselben Mitteln wie der
//! Frontmatter-Block: Zeichenkettensuche, kein HTML-Parser; die Dateien
//! schreibt dieselbe Anwendung.

use std::collections::HashMap;

/// Gerüst einer neuen HTML-Notiz.
pub(crate) fn skeleton(title: &str, project: &str, iso: &str) -> String {
  format!(
    "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
     <meta name=\"id\" content=\"{}\">\n\
     <meta name=\"project\" content=\"{}\">\n\
     <meta name=\"created\" content=\"{iso}\">\n\
     <meta name=\"source\" content=\"ai-central\">\n\
     <title>{}</title>\n</head>\n<body>\n</body>\n</html>\n",
    uuid::Uuid::new_v4(),
    escape(project),
    escape(title),
  )
}

/// Metadaten einer HTML-Notiz: `<title>` plus alle `<meta name=… content=…>`.
/// Schlagwörter stehen in `keywords`, kommagetrennt.
pub(crate) fn parse_meta(text: &str) -> HashMap<String, String> {
  let mut map = HashMap::new();
  if let Some(title) = between(text, "<title>", "</title>") {
    map.insert("title".to_string(), unescape(title.trim()));
  }
  let mut rest = text;
  while let Some(start) = rest.find("<meta ") {
    rest = &rest[start + 6..];
    let Some(end) = rest.find('>') else {
      break;
    };
    let tag = &rest[..end];
    rest = &rest[end + 1..];
    if let (Some(name), Some(content)) = (attr(tag, "name"), attr(tag, "content")) {
      map.insert(name, unescape(&content));
    }
  }
  map
}

/// Setzt den Anzeige-Titel: den Inhalt von `<title>`.
pub(crate) fn set_title(text: &str, title: &str) -> Result<String, String> {
  let start = text.find("<title>").ok_or("kein <title> in der HTML-Notiz")?;
  let end = text.find("</title>").ok_or("kein <title> in der HTML-Notiz")?;
  Ok(format!("{}{}{}", &text[..start + 7], escape(title), &text[end..]))
}

/// Setzt ein `<meta name=… content=…>`; fehlt es, kommt es vor den `<title>`.
pub(crate) fn set_meta(text: &str, name: &str, value: &str) -> Result<String, String> {
  let line = format!("<meta name=\"{name}\" content=\"{}\">", escape(value));
  let mut rest = text;
  let mut offset = 0;
  while let Some(start) = rest.find("<meta ") {
    let abs = offset + start;
    rest = &rest[start + 6..];
    offset = abs + 6;
    let Some(end) = rest.find('>') else {
      break;
    };
    if attr(&rest[..end], "name").as_deref() == Some(name) {
      return Ok(format!("{}{line}{}", &text[..abs], &text[offset + end + 1..]));
    }
    rest = &rest[end + 1..];
    offset += end + 1;
  }
  let at = text.find("<title>").ok_or("kein <head> in der HTML-Notiz")?;
  Ok(format!("{}{line}\n{}", &text[..at], &text[at..]))
}

/// Rumpf der Notiz: Inhalt zwischen `<body>` und `</body>`; ohne die Marken
/// gilt der ganze Text als Rumpf.
pub(crate) fn body(text: &str) -> &str {
  match (text.find("<body>"), text.rfind("</body>")) {
    (Some(a), Some(b)) if b > a + 6 => text[a + 6..b].trim_matches('\n'),
    _ => text,
  }
}

/// Ersetzt den Rumpf; Kopf und Marken bleiben.
pub(crate) fn replace_body(text: &str, html: &str) -> Result<String, String> {
  let a = text.find("<body>").ok_or("kein <body> in der HTML-Notiz")?;
  let b = text.rfind("</body>").ok_or("kein <body> in der HTML-Notiz")?;
  Ok(format!("{}\n{}\n{}", &text[..a + 6], html.trim(), &text[b..]))
}

/// Reiner Text einer HTML-Notiz — Grundlage für den Suchindex.
pub(crate) fn strip_tags(text: &str) -> String {
  let rest = body(text);
  // Skript- und Stilblöcke tragen keinen Lesetext.
  for marker in ["script", "style"] {
    while let Some(a) = rest.find(&format!("<{marker}")) {
      let Some(b) = rest[a..].find(&format!("</{marker}>")) else {
        break;
      };
      let cut = format!("{}{}", &rest[..a], &rest[a + b + marker.len() + 3..]);
      return strip_tags_inner(&cut);
    }
  }
  strip_tags_inner(rest)
}

fn strip_tags_inner(text: &str) -> String {
  let mut out = String::new();
  let mut in_tag = false;
  for c in text.chars() {
    match c {
      '<' => in_tag = true,
      '>' => {
        in_tag = false;
        out.push(' ');
      }
      _ if !in_tag => out.push(c),
      _ => {}
    }
  }
  unescape(out.split_whitespace().collect::<Vec<_>>().join(" ").as_str())
}

/// Wert eines Attributs im Tag-Inneren (`name="wert"`).
fn attr(tag: &str, name: &str) -> Option<String> {
  let key = format!("{name}=\"");
  let start = tag.find(&key)? + key.len();
  let end = tag[start..].find('"')? + start;
  Some(tag[start..end].to_string())
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
  let start = text.find(open)? + open.len();
  let end = text[start..].find(close)? + start;
  Some(&text[start..end])
}

fn escape(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
  s.replace("&quot;", "\"")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
  use super::*;

  fn doc() -> String {
    skeleton("Titel", "proj", "2026-07-26T10:00:00Z")
  }

  #[test]
  fn geruest_traegt_id_und_titel() {
    let html = doc();
    let meta = parse_meta(&html);
    assert_eq!(meta.get("title").map(String::as_str), Some("Titel"));
    assert_eq!(meta.get("id").map(String::len), Some(36));
    assert_eq!(meta.get("project").map(String::as_str), Some("proj"));
    assert_eq!(meta.get("created").map(String::as_str), Some("2026-07-26T10:00:00Z"));
    assert_eq!(body(&html), "");
  }

  #[test]
  fn titel_und_meta_setzen() {
    let html = set_title(&doc(), "Neuer <Titel>").unwrap();
    assert_eq!(parse_meta(&html).get("title").map(String::as_str), Some("Neuer <Titel>"));

    // Vorhandenes meta wird ersetzt, fehlendes ergänzt.
    let html = set_meta(&html, "project", "andere").unwrap();
    let html = set_meta(&html, "description", "Kurz \"gefasst\"").unwrap();
    let meta = parse_meta(&html);
    assert_eq!(meta.get("project").map(String::as_str), Some("andere"));
    assert_eq!(meta.get("description").map(String::as_str), Some("Kurz \"gefasst\""));
    assert_eq!(meta.get("title").map(String::as_str), Some("Neuer <Titel>"));
    // Kein Duplikat.
    assert_eq!(html.matches("name=\"project\"").count(), 1);
  }

  #[test]
  fn rumpf_lesen_und_ersetzen() {
    let html = replace_body(&doc(), "<p>Erster Absatz</p>").unwrap();
    assert_eq!(body(&html), "<p>Erster Absatz</p>");
    let html = replace_body(&html, "<p>Zweiter</p>").unwrap();
    assert_eq!(body(&html), "<p>Zweiter</p>");
    // Der Kopf bleibt unangetastet.
    assert_eq!(parse_meta(&html).get("title").map(String::as_str), Some("Titel"));
  }

  #[test]
  fn suchtext_ohne_markup() {
    let html = replace_body(
      &doc(),
      "<h1>Über&#nbsp;schrift</h1>\n<p>Text mit <b>Fett</b> und &amp; Zeichen.</p>",
    )
    .unwrap();
    let text = strip_tags(&html);
    assert!(text.contains("Text mit Fett und & Zeichen."));
    assert!(!text.contains('<'));
  }

  #[test]
  fn fehlende_marken_brechen_laut_ab() {
    assert!(set_title("<html></html>", "T").is_err());
    assert!(replace_body("<html></html>", "x").is_err());
  }
}
