//! Volltext-Suche übers Panel-Archiv: SQLite-FTS5-Index, bei jeder Anfrage
//! frisch in-memory aus dem Archiv-Baum gebaut. Bei den Archiv-Größen dieser
//! App ist der Aufbau Millisekundensache; damit gibt es keinen persistierten
//! Index, keine Staleness und nichts, was gesynct werden könnte. Die
//! Tool-Schnittstelle bleibt engine-unabhängig.

use std::path::Path;

use rusqlite::Connection;

use crate::domain::archive_index::scan_with_bodies;

#[derive(serde::Serialize)]
pub(crate) struct Hit {
  /// Technische ID der Notiz — Adressat des Treffer-Sprungs.
  pub(crate) id: String,
  /// Pfad relativ zum Archiv-Home (Anzeige).
  pub(crate) relpath: String,
  pub(crate) title: String,
  /// Woher der Treffer stammt: `text` (Rumpf), sonst `title`, `description`,
  /// `tags` oder `name`. Nur ein Rumpf-Treffer hat eine Stelle im Dokument,
  /// die sich anspringen und markieren lässt.
  pub(crate) field: &'static str,
  /// Textausschnitt um die Fundstelle, Treffer in `**…**`. Bei Treffern
  /// außerhalb des Rumpfs steht hier der Inhalt des Feldes, ebenso markiert.
  pub(crate) snippet: String,
}

/// Indexspalten in der Reihenfolge, in der eine Fundstelle zählt: der Rumpf
/// zuerst, denn nur er trägt eine anspringbare Stelle.
const FELDER: &[(usize, &str)] = &[
  (6, "text"),
  (3, "title"),
  (5, "tags"),
  (4, "description"),
  (2, "name"),
];

/// Durchsucht das Archiv unter `home`. `query` ist FTS5-Syntax (Wörter,
/// "Phrasen", Präfix*); `tag` engt auf ein Schlagwort ein. Treffer nach
/// BM25-Rang, höchstens `limit`.
pub(crate) fn search(
  home: &Path,
  query: &str,
  tag: Option<&str>,
  limit: usize,
) -> Result<Vec<Hit>, String> {
  let q = sanitize_query(query);
  let t = tag.and_then(|t| quote_phrase(t, false)).map(|t| format!("tags:{t}"));
  let expr = match (q.is_empty(), t) {
    (false, Some(t)) => format!("({q}) AND {t}"),
    (false, None) => q,
    (true, Some(t)) => t,
    // Von vornherein leere Anfrage ist ein Bedienfehler. Eine Eingabe, die nur
    // aus Satzzeichen bestand (`!?`, ein einzelnes `"`, das angetippte `#`),
    // findet dagegen schlicht nichts — und spart sich den Index-Aufbau.
    (true, None) if query.trim().is_empty() && tag.is_none() => {
      return Err("leere Suchanfrage".into())
    }
    (true, None) => return Ok(Vec::new()),
  };
  let conn = build_index(home)?;
  // Je Feld ein eigener Ausschnitt: `snippet` markiert nur dort, wo die
  // Spalte selbst getroffen wurde. Daran hängt, woher der Treffer stammt —
  // ein Titel-Treffer ohne diese Unterscheidung zeigte den Textanfang ohne
  // jede Hervorhebung und sähe aus wie ein Zufallsfund.
  let spalten: String = FELDER
    .iter()
    .map(|(i, _)| format!("snippet(docs, {i}, '**', '**', ' … ', 12)"))
    .collect::<Vec<_>>()
    .join(", ");
  let mut stmt = conn
    .prepare(&format!(
      "SELECT id, relpath, title, {spalten} FROM docs WHERE docs MATCH ?1 ORDER BY rank LIMIT ?2"
    ))
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map(rusqlite::params![expr, limit as i64], |row| {
      let mut feld = FELDER[0].1;
      let mut ausschnitt = String::new();
      for (n, (_, name)) in FELDER.iter().enumerate() {
        let s: String = row.get(3 + n)?;
        if s.contains("**") {
          feld = name;
          ausschnitt = s;
          break;
        }
      }
      Ok(Hit {
        id: row.get(0)?,
        relpath: row.get(1)?,
        title: row.get(2)?,
        field: feld,
        snippet: ausschnitt,
      })
    })
    .map_err(|e| format!("Suchausdruck „{query}“: {e}"))?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Übersetzt die Nutzereingabe in einen FTS5-Ausdruck, der nicht scheitern kann.
///
/// Roh durchgereicht ist fast jede natürliche Eingabe ein Syntaxfehler: `ai-central`
/// liest FTS5 als Spaltenfilter (`no such column: control`), `C++` und eine offene
/// Klammer brechen den Parser. Die Live-Suche schickt zudem jeden Zwischenstand beim
/// Tippen ab, also auch das halbe `"Phrase`. Darum wird jedes Wort als Phrase
/// gequotet — die Tokenizer-Regeln bleiben dieselben, nur die Operatorzeichen
/// verlieren ihre Sonderbedeutung. Erhalten bleiben die zwei Formen, die Nutzer
/// bewusst tippen: "Phrasen in Anführungszeichen" und Präfix*.
fn sanitize_query(query: &str) -> String {
  let mut out: Vec<String> = Vec::new();
  // An `"` aufteilen: ungerade Segmente standen in Anführungszeichen und
  // bleiben als Ganzes eine Phrase, gerade zerfallen in Wörter. Ein fehlendes
  // schließendes Anführungszeichen (Tippzwischenstand) fällt damit von selbst
  // richtig heraus — der Rest der Eingabe ist das letzte ungerade Segment.
  for (i, teil) in query.split('"').enumerate() {
    if i % 2 == 1 {
      out.extend(quote_phrase(teil, false));
    } else {
      out.extend(
        teil
          .split_whitespace()
          .filter_map(|w| quote_phrase(w.trim_end_matches('*'), w.ends_with('*'))),
      );
    }
  }
  out.join(" ")
}

/// `term` als gequotete FTS5-Phrase, sofern überhaupt etwas Indexierbares darin
/// steht. Reine Satzzeichen ergäben die leere Phrase `""` — für FTS5 ein
/// Syntaxfehler. Gilt auch für den Tag-Filter: Das angetippte `#` liefert über
/// `panel-wiring.ts` den leeren Tag.
fn quote_phrase(term: &str, prefix: bool) -> Option<String> {
  if !term.chars().any(char::is_alphanumeric) {
    return None;
  }
  let quoted = term.replace('"', "");
  Some(if prefix { format!("\"{quoted}\"*") } else { format!("\"{quoted}\"") })
}

/// Baut den FTS5-Index in-memory aus dem Archiv-Baum.
fn build_index(home: &Path) -> Result<Connection, String> {
  let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
  conn
    .execute_batch(
      "CREATE VIRTUAL TABLE docs USING fts5(id UNINDEXED, relpath UNINDEXED, name, title, description, tags, body)",
    )
    .map_err(|e| e.to_string())?;
  let docs = scan_with_bodies(home)?;
  let mut insert = conn
    .prepare("INSERT INTO docs (id, relpath, name, title, description, tags, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
    .map_err(|e| e.to_string())?;
  for (doc, body) in &docs {
    insert
      .execute(rusqlite::params![
        doc.id,
        doc.relpath,
        doc.name,
        doc.title,
        doc.description.as_deref().unwrap_or(""),
        doc.tags.join(" "),
        body,
      ])
      .map_err(|e| e.to_string())?;
  }
  drop(insert);
  Ok(conn)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::tmp_paths;
  use std::fs;

  fn archiv() -> std::path::PathBuf {
    let home = tmp_paths().home.join("archiv");
    fs::create_dir_all(home.join("konzepte")).unwrap();
    fs::write(
      home.join("2026-07-19_1000-adr-logging.md"),
      "---\ntitle: \"ADR Logging\"\ntags: [\"adr\", \"infra\"]\n---\n\nStrukturiertes Logging mit tracing vereinheitlichen.\n",
    )
    .unwrap();
    fs::write(
      home.join("konzepte/2026-07-19_1005-notiz-deploy.md"),
      "---\ntitle: \"Notiz Deploy\"\ntags: [\"infra\"]\n---\n\nDeploy braucht lsregister auf macOS.\n",
    )
    .unwrap();
    home
  }

  #[test]
  fn findet_nach_inhaltswort() {
    let home = archiv();
    let hits = search(&home, "tracing", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "ADR Logging");
    assert_eq!(hits[0].field, "text");
    assert!(hits[0].snippet.contains("**tracing**"));
  }

  /// Woher der Treffer stammt, steht am Treffer: Nur ein Rumpf-Treffer hat
  /// eine Stelle im Dokument; Titel und Schlagwort zeigen das Feld selbst.
  #[test]
  fn nennt_das_getroffene_feld() {
    let home = archiv();
    let titel = search(&home, "ADR", None, 10).unwrap();
    assert_eq!(titel[0].field, "title");
    assert!(titel[0].snippet.contains("**ADR**"), "{}", titel[0].snippet);

    let tag = search(&home, "", Some("adr"), 10).unwrap();
    assert_eq!(tag[0].field, "tags");
  }

  /// Rohdaten-Dateien sind mit ihrem Inhalt auffindbar; Binärformate
  /// steuern nichts bei, auch wenn sie im Archiv liegen.
  #[test]
  fn findet_in_textdateien() {
    let home = archiv();
    fs::write(home.join("stack.yaml"), "dienste:\n  - kessel\n").unwrap();
    fs::write(home.join("werte.json"), "{\"marke\": \"kessel\"}\n").unwrap();
    fs::write(home.join("bild.png"), [0x89u8, 0x50, 0x4e, 0x47, 0x0d]).unwrap();

    let hits = search(&home, "kessel", None, 10).unwrap();
    let titel: Vec<_> = hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titel, ["stack.yaml", "werte.json"], "{titel:?}");
    assert!(hits[0].snippet.contains("**kessel**"), "{}", hits[0].snippet);
  }

  #[test]
  fn tag_filter_engt_ein() {
    let home = archiv();
    assert_eq!(search(&home, "", Some("infra"), 10).unwrap().len(), 2);
    assert_eq!(search(&home, "", Some("adr"), 10).unwrap().len(), 1);
    let hits = search(&home, "deploy", Some("adr"), 10).unwrap();
    assert!(hits.is_empty());
  }

  #[test]
  fn phrase_und_praefix() {
    let home = archiv();
    assert_eq!(search(&home, "\"Strukturiertes Logging\"", None, 10).unwrap().len(), 1);
    assert_eq!(search(&home, "lsregist*", None, 10).unwrap().len(), 1);
  }

  #[test]
  fn leere_anfrage_scheitert() {
    let home = archiv();
    assert!(search(&home, "  ", None, 10).is_err());
  }

  /// Eingaben, die roh durchgereicht einen FTS5-Syntaxfehler warfen. Die
  /// Bindestrich-Fälle sind die wichtigsten: Archiv-Dokumente heißen selbst so.
  /// Die angefangenen Phrasen stehen für die Tippzwischenstände, die die
  /// Live-Suche abschickt.
  #[test]
  fn sonderzeichen_werfen_keinen_syntaxfehler() {
    let home = archiv();
    let eingaben = [
      "ai-central",
      "adr-log",
      "TODO: fix",
      "C++",
      "wiki (",
      "\"",
      "\"Strukturiertes",
      "\"Strukturiertes Logging",
      "!?",
    ];
    for q in eingaben {
      assert!(search(&home, q, None, 10).is_ok(), "Suche scheiterte an „{q}“");
    }
  }

  /// Der Tag-Filter lief früher an der Quoting-Regel vorbei. `#` allein ist der
  /// erste Tastendruck jeder Tag-Suche und kommt als leerer Tag an.
  #[test]
  fn leerer_tag_wirft_keinen_syntaxfehler() {
    let home = archiv();
    // Das angetippte `#`: kein Fehler-Toast, sondern schlicht kein Treffer.
    assert_eq!(search(&home, "", Some(""), 10).unwrap().len(), 0);
    assert_eq!(search(&home, "", Some("!?"), 10).unwrap().len(), 0);
    // Mit Volltext daneben zählt nur dieser, der leere Tag engt nichts ein.
    assert_eq!(search(&home, "tracing", Some(""), 10).unwrap().len(), 1);
    // Fehler bleibt allein die komplett leere Anfrage ohne jeden Tag.
    assert!(search(&home, "", None, 10).is_err());
  }

  #[test]
  fn bindestrich_wort_findet_dokument() {
    let home = archiv();
    let hits = search(&home, "adr-logging", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "ADR Logging");
  }

}
