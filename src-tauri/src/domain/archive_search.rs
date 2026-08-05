//! Volltext-Suche übers Panel-Archiv: SQLite-FTS5 über den persistenten
//! Index (`search_index`). Vor jedem Lauf wird abgeglichen — das kostet einen
//! `stat` je Datei und hält den Index am Bestand, ohne Bücher erneut zu
//! entpacken. Die Tool-Schnittstelle bleibt engine-unabhängig.

use std::path::Path;

use crate::domain::search_index;

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
  /// Adresse innerhalb der Datei: bei Büchern das Kapitel, sonst leer.
  pub(crate) teil: String,
  /// Titel des Buchs, aus dem das Kapitel stammt; leer bei allem anderen.
  /// Ohne ihn stünde über einem Treffer nur „Teil II" — bei einem Regal voll
  /// Bänden sagt das nichts.
  pub(crate) buch: String,
  /// Wie oft die Suchwörter im Rumpf dieses Eintrags vorkommen. Ein Treffer
  /// ist ein Dokument bzw. ein Kapitel — ohne diese Zahl sähe ein Kapitel mit
  /// einer Fundstelle aus wie eines mit hundert.
  pub(crate) count: usize,
}

/// Zählt die Vorkommen der Suchwörter im Text — ohne Rücksicht auf Groß- und
/// Kleinschreibung, wie der Trigramm-Index selbst.
fn vorkommen(text: &str, woerter: &[String]) -> usize {
  let klein = text.to_lowercase();
  woerter.iter().map(|w| klein.matches(w.as_str()).count()).sum()
}

/// Indexspalten in der Reihenfolge, in der eine Fundstelle zählt: der Rumpf
/// zuerst, denn nur er trägt eine anspringbare Stelle.
const FELDER: &[(usize, &str)] = &[
  (9, "text"),
  (6, "title"),
  (8, "tags"),
  (7, "description"),
  (5, "name"),
];

/// Durchsucht das Archiv unter `home`. `query` ist FTS5-Syntax (Wörter,
/// "Phrasen", Präfix*); `tag` engt auf ein Schlagwort ein. Treffer nach
/// BM25-Rang — alle, die es gibt.
pub(crate) fn search(
  projekt: &str,
  home: &Path,
  query: &str,
  tag: Option<&str>,
) -> Result<Vec<Hit>, String> {
  let conn = search_index::oeffne(&search_index::index_pfad(projekt))?;
  search_index::abgleichen(&conn, home)?;
  search_in(&conn, query, tag)
}

/// Der Suchlauf auf einem bereits abgeglichenen Index.
pub(crate) fn search_in(
  conn: &rusqlite::Connection,
  query: &str,
  tag: Option<&str>,
) -> Result<Vec<Hit>, String> {
  let q = sanitize_query(query);
  // Die reinen Suchwörter — Grundlage der Fundstellen-Zählung.
  let woerter = suchwoerter(query);
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
  // Je Feld ein eigener Ausschnitt: `snippet` markiert nur dort, wo die
  // Spalte selbst getroffen wurde. Daran hängt, woher der Treffer stammt —
  // ein Titel-Treffer ohne diese Unterscheidung zeigte den Textanfang ohne
  // jede Hervorhebung und sähe aus wie ein Zufallsfund.
  let spalten: String = FELDER
    .iter()
    .map(|(i, _)| format!("snippet(docs, {i}, '**', '**', ' … ', 12)"))
    .collect::<Vec<_>>()
    .join(", ");
  // Der Pfad steht nicht im Index — er ist das einzige Maschinenabhängige und
  // kommt aus der Zuordnung `quellen`. Liegt derselbe Inhalt mehrfach im
  // Archiv, gewinnt der alphabetisch erste Pfad.
  let mut stmt = conn
    .prepare(&format!(
      "SELECT docs.doc_id, docs.kind, docs.teil, docs.title,
              (SELECT min(relpath) FROM quellen WHERE quellen.hash = docs.hash),
              docs.body, docs.name,
              {spalten}
       FROM docs WHERE docs MATCH ?1 ORDER BY rank"
    ))
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map(rusqlite::params![expr], |row| {
      let mut feld = FELDER[0].1;
      let mut ausschnitt = String::new();
      for (n, (_, name)) in FELDER.iter().enumerate() {
        let s: String = row.get(7 + n)?;
        if s.contains("**") {
          feld = name;
          ausschnitt = s;
          break;
        }
      }
      let doc_id: String = row.get(0)?;
      let kind: String = row.get(1)?;
      let teil: String = row.get(2)?;
      let relpath: String = row.get::<_, Option<String>>(4)?.unwrap_or_default();
      // Notizen sprechen ihre Frontmatter-ID an, alles andere den Pfad.
      let id = if !doc_id.is_empty() {
        doc_id
      } else if kind == "epub" {
        format!("epub:{relpath}")
      } else {
        format!("file:{relpath}")
      };
      let body: String = row.get(5)?;
      Ok(Hit {
        id,
        relpath,
        title: row.get(3)?,
        field: feld,
        snippet: ausschnitt,
        teil,
        buch: if kind == "epub" { row.get(6)? } else { String::new() },
        count: if feld == "text" { vorkommen(&body, &woerter) } else { 1 },
      })
    })
    .map_err(|e| format!("Suchausdruck „{query}“: {e}"))?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Eine einzelne Fundstelle im Dokument — die Ebene, auf der zitiert wird.
#[derive(serde::Serialize)]
pub(crate) struct Stelle {
  /// Laufende Nummer im Dokument; adressiert den Sprung.
  pub(crate) nr: usize,
  /// Druckseite, sofern das Dokument Seitenmarken trägt.
  pub(crate) seite: String,
  /// Wo auf der Seite: `oben`, `Mitte`, `unten`.
  pub(crate) lage: &'static str,
  /// Der Satz um die Fundstelle, Treffer in `**…**`.
  pub(crate) zeile: String,
}

/// Alle Fundstellen eines Treffers, mit Seite und Lage. Getrennt vom
/// Suchlauf: Ein Kapitel kann tausende haben, und die Trefferliste soll
/// lesbar bleiben — geholt wird erst, wer aufklappt.
pub(crate) fn stellen(
  conn: &rusqlite::Connection,
  hash_oder_id: &str,
  teil: &str,
  query: &str,
) -> Result<Vec<Stelle>, String> {
  let woerter = suchwoerter(query);
  if woerter.is_empty() {
    return Ok(Vec::new());
  }
  let (body, seiten): (String, String) = conn
    .query_row(
      "SELECT body, seiten FROM docs
       WHERE (doc_id = ?1 OR hash IN (SELECT hash FROM quellen WHERE relpath = ?1))
         AND teil = ?2",
      rusqlite::params![hash_oder_id, teil],
      |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|e| format!("kein Eintrag zu {hash_oder_id}: {e}"))?;
  let marken = seitenmarken(&seiten);
  let klein = body.to_lowercase();
  let zeichen: Vec<char> = body.chars().collect();

  let mut roh: Vec<usize> = Vec::new();
  for w in &woerter {
    let mut i = klein.find(w.as_str());
    while let Some(pos) = i {
      roh.push(klein[..pos].chars().count());
      i = klein[pos + w.len()..].find(w.as_str()).map(|n| pos + w.len() + n);
    }
  }
  roh.sort_unstable();
  roh.dedup();

  Ok(
    roh
      .iter()
      .enumerate()
      .map(|(n, &pos)| {
        let (seite, lage) = seite_bei(pos, &marken);
        Stelle { nr: n, seite, lage, zeile: satz_um(&zeichen, pos, &woerter) }
      })
      .collect(),
  )
}

/// Die reinen Suchwörter einer Anfrage, klein — ohne Operatorzeichen.
fn suchwoerter(query: &str) -> Vec<String> {
  query
    .split(['"', ' ', '\t', '\n'])
    .map(|w| w.trim_end_matches('*').trim().to_lowercase())
    .filter(|w| w.chars().any(char::is_alphanumeric))
    .collect()
}

/// `offset:seite,…` in Paare.
fn seitenmarken(roh: &str) -> Vec<(usize, String)> {
  roh
    .split(',')
    .filter_map(|p| p.split_once(':'))
    .filter_map(|(o, s)| o.parse().ok().map(|o: usize| (o, s.to_string())))
    .collect()
}

/// Seite und Lage einer Position: die letzte Marke davor gilt, die Lage ist
/// ihr Anteil bis zur nächsten.
fn seite_bei(pos: usize, marken: &[(usize, String)]) -> (String, &'static str) {
  let Some(i) = marken.iter().rposition(|(o, _)| *o <= pos) else {
    return (String::new(), "");
  };
  let (start, seite) = &marken[i];
  let ende = marken.get(i + 1).map(|(o, _)| *o).unwrap_or(start + 3000);
  let anteil = if ende > *start {
    (pos - start) as f32 / (ende - start) as f32
  } else {
    0.0
  };
  let lage = if anteil < 0.34 {
    "oben"
  } else if anteil < 0.67 {
    "Mitte"
  } else {
    "unten"
  };
  (seite.clone(), lage)
}

/// Der Satz um die Fundstelle, Treffer in `**…**`. Satzgrenze ist ein Punkt,
/// Frage- oder Ausrufezeichen; ohne eine solche in Reichweite wird hart
/// geschnitten.
fn satz_um(zeichen: &[char], pos: usize, woerter: &[String]) -> String {
  let von = (pos.saturating_sub(160)..pos)
    .rev()
    .find(|&i| matches!(zeichen.get(i), Some('.') | Some('!') | Some('?')))
    .map(|i| i + 1)
    .unwrap_or(pos.saturating_sub(120));
  let bis = (pos..(pos + 220).min(zeichen.len()))
    .find(|&i| matches!(zeichen.get(i), Some('.') | Some('!') | Some('?')))
    .map(|i| i + 1)
    .unwrap_or((pos + 160).min(zeichen.len()));
  let roh: String = zeichen[von..bis].iter().collect();
  let text = roh.split_whitespace().collect::<Vec<_>>().join(" ");
  // Die Fundstellen im Ausschnitt markieren — dieselbe Auszeichnung wie im
  // Schnipsel der Kachel.
  let klein = text.to_lowercase();
  let mut out = String::with_capacity(text.len() + 8);
  let mut i = 0usize;
  let bytes: Vec<char> = text.chars().collect();
  while i < bytes.len() {
    let rest: String = bytes[i..].iter().collect();
    let rest_klein: String = klein.chars().skip(i).collect();
    match woerter.iter().find(|w| rest_klein.starts_with(w.as_str())) {
      Some(w) => {
        let n = w.chars().count();
        out.push_str("**");
        out.extend(bytes[i..i + n].iter());
        out.push_str("**");
        i += n;
      }
      None => {
        out.push(bytes[i]);
        i += 1;
        let _ = &rest;
      }
    }
  }
  out.trim().to_string()
}

/// Übersetzt die Nutzereingabe in einen FTS5-Ausdruck, der nicht scheitern kann.
///
/// Roh durchgereicht ist fast jede natürliche Eingabe ein Syntaxfehler: `ai-central`
/// liest FTS5 als Spaltenfilter (`no such column: control`), `C++` und eine offene
/// Klammer brechen den Parser. Die Live-Suche schickt zudem jeden Zwischenstand beim
/// Tippen ab, also auch das halbe `"Phrase`. Darum wird jedes Wort als Phrase
/// gequotet — nur die Operatorzeichen verlieren ihre Sonderbedeutung.
///
/// Ein angehängter Stern entfällt: Der Trigramm-Tokenizer sucht ohnehin nach
/// Zeichenketten, jede Anfrage ist damit von sich aus Teilwort-Suche.
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
          .filter_map(|w| quote_phrase(w.trim_end_matches('*'), false)),
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::tmp_paths;
  use std::fs;

  /// Jeder Test bekommt seinen eigenen Index — im Arbeitsspeicher, damit
  /// weder die Config des Rechners noch ein anderer Test hineinredet.
  fn index(home: &std::path::Path) -> rusqlite::Connection {
    let conn = crate::domain::search_index::im_speicher().unwrap();
    crate::domain::search_index::abgleichen(&conn, home).unwrap();
    conn
  }

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
    let hits = search_in(&index(&home), "tracing", None).unwrap();
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
    let titel = search_in(&index(&home), "ADR", None).unwrap();
    assert_eq!(titel[0].field, "title");
    assert!(titel[0].snippet.contains("**ADR**"), "{}", titel[0].snippet);

    let tag = search_in(&index(&home), "", Some("adr")).unwrap();
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

    let hits = search_in(&index(&home), "kessel", None).unwrap();
    let titel: Vec<_> = hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titel, ["stack.yaml", "werte.json"], "{titel:?}");
    assert!(hits[0].snippet.contains("**kessel**"), "{}", hits[0].snippet);
  }

  #[test]
  fn tag_filter_engt_ein() {
    let home = archiv();
    assert_eq!(search_in(&index(&home), "", Some("infra")).unwrap().len(), 2);
    assert_eq!(search_in(&index(&home), "", Some("adr")).unwrap().len(), 1);
    let hits = search_in(&index(&home), "deploy", Some("adr")).unwrap();
    assert!(hits.is_empty());
  }

  /// Teilwort-Suche: Der Trigramm-Tokenizer trifft Zeichenketten, nicht
  /// Wortgrenzen — genau das, was man beim Recherchieren eintippt.
  #[test]
  fn findet_wortteile() {
    let home = archiv();
    fs::write(
      home.join("verein.md"),
      "---\ntitle: \"Verein\"\n---\n\nKeine Vereinheitlichung ohne Grundlage.\n",
    )
    .unwrap();
    let conn = index(&home);
    // „ein" steckt in „Keine", „Vereinheitlichung", „vereinheitlichen".
    let hits = search_in(&conn, "ein", None).unwrap();
    assert!(hits.len() >= 2, "{}", hits.len());
    // Mitten im Wort, nicht nur am Anfang.
    assert_eq!(search_in(&conn, "heitlich", None).unwrap().len(), 2);
  }

  /// Die Kachel sagt, wie viel im Dokument steckt: ein Kapitel mit einer
  /// Fundstelle sieht sonst aus wie eines mit hundert.
  #[test]
  fn zaehlt_die_fundstellen_je_treffer() {
    let home = archiv();
    fs::write(
      home.join("viel.md"),
      "---\ntitle: \"Viel\"\n---\n\nRohr, Rohr und nochmal Rohr; rohrfrei.\n",
    )
    .unwrap();
    let hits = search_in(&index(&home), "rohr", None).unwrap();
    let viel = hits.iter().find(|h| h.title == "Viel").unwrap();
    assert_eq!(viel.count, 4);
  }

  /// Seite und Lage: Die letzte Marke vor der Fundstelle gilt, ihr Anteil
  /// bis zur nächsten sagt oben, Mitte oder unten.
  #[test]
  fn seite_und_lage_aus_den_marken() {
    let marken = vec![(0usize, "84".to_string()), (100, "85".to_string()), (200, "86".to_string())];
    assert_eq!(seite_bei(10, &marken), ("84".into(), "oben"));
    assert_eq!(seite_bei(150, &marken), ("85".into(), "Mitte"));
    assert_eq!(seite_bei(190, &marken), ("85".into(), "unten"));
    // Vor der ersten Marke gibt es keine Seite.
    assert_eq!(seite_bei(0, &[]), (String::new(), ""));
  }

  /// Die Zeile ist der Satz um die Fundstelle, der Treffer darin markiert.
  #[test]
  fn zeile_ist_der_satz_um_die_stelle() {
    let text: Vec<char> =
      "Erster Satz. Hier steht das Rohr im Keller. Dritter Satz.".chars().collect();
    let pos = "Erster Satz. Hier steht das ".chars().count();
    let z = satz_um(&text, pos, &["rohr".to_string()]);
    assert_eq!(z, "Hier steht das **Rohr** im Keller.");
  }

  #[test]
  fn phrase_und_praefix() {
    let home = archiv();
    assert_eq!(search_in(&index(&home), "\"Strukturiertes Logging\"", None).unwrap().len(), 1);
    assert_eq!(search_in(&index(&home), "lsregist*", None).unwrap().len(), 1);
  }

  #[test]
  fn leere_anfrage_scheitert() {
    let home = archiv();
    assert!(search_in(&index(&home), "  ", None).is_err());
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
      assert!(search_in(&index(&home), q, None).is_ok(), "Suche scheiterte an „{q}“");
    }
  }

  /// Der Tag-Filter lief früher an der Quoting-Regel vorbei. `#` allein ist der
  /// erste Tastendruck jeder Tag-Suche und kommt als leerer Tag an.
  #[test]
  fn leerer_tag_wirft_keinen_syntaxfehler() {
    let home = archiv();
    // Das angetippte `#`: kein Fehler-Toast, sondern schlicht kein Treffer.
    assert_eq!(search_in(&index(&home), "", Some("")).unwrap().len(), 0);
    assert_eq!(search_in(&index(&home), "", Some("!?")).unwrap().len(), 0);
    // Mit Volltext daneben zählt nur dieser, der leere Tag engt nichts ein.
    assert_eq!(search_in(&index(&home), "tracing", Some("")).unwrap().len(), 1);
    // Fehler bleibt allein die komplett leere Anfrage ohne jeden Tag.
    assert!(search_in(&index(&home), "", None).is_err());
  }

  #[test]
  fn bindestrich_wort_findet_dokument() {
    let home = archiv();
    let hits = search_in(&index(&home), "adr-logging", None).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "ADR Logging");
  }

}
