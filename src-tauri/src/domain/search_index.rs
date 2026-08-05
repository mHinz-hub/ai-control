//! Persistenter Suchindex je Projekt.
//!
//! Bisher wurde der FTS5-Index bei jeder Anfrage neu in-memory gebaut. Für
//! Notizen ist das billig, für Bücher nicht: Ein ePub müsste dafür jedes Mal
//! entpackt und Kapitel für Kapitel gelesen werden.
//!
//! Die Identität eines Eintrags ist darum sein **Inhalt**, nicht sein Pfad:
//! Der Schlüssel ist der SHA-256 der Datei, und was aus ihr in den Index
//! kommt, folgt deterministisch aus ihr selbst — bei Büchern die Kapitel in
//! Spine-Reihenfolge mit ihrer Href als Adresse. Dieselbe Datei ergibt damit
//! auf jeder Maschine denselben Index. Maschinenabhängig ist allein die
//! Zuordnung Hash → aktueller Pfad in `quellen`.
//!
//! Der Tokenizer ist `trigram`: Gesucht wird nach Zeichenketten, nicht nach
//! ganzen Wörtern — `ein` findet auch `keine` und `Verein`. Das kostet etwa
//! das Dreifache an Indexgröße und verlangt mindestens drei Zeichen je
//! Anfrage; dafür trifft die Suche das, was man eintippt, statt Wortgrenzen
//! zu verlangen, die in einem philosophischen Text niemand im Kopf hat.
//!
//! Der Abgleich vor jedem Suchlauf kostet einen `stat` je Datei. Stimmen
//! Größe und Zeitstempel, bleibt der Eintrag; sonst wird gehasht und, wenn
//! der Hash neu ist, neu gelesen. Verschwundene Dateien fliegen raus, ein
//! Hash ohne Pfad ebenso.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::domain::archive_index::{teile_fuer_index, Teil};
use crate::domain::paths::Paths;

/// Ablage des Index: eine Datei je Projekt, außerhalb des Archivs. Im Archiv
/// läge sie in dessen Git-Repo und Sync — abgeleitete Information, die dort
/// nichts zu suchen hat.
pub(crate) fn index_pfad(project: &str) -> PathBuf {
  let dir = Paths::real().config_dir().join("search");
  let _ = std::fs::create_dir_all(&dir);
  dir.join(format!("{project}.db"))
}

/// Version des Schemas. Ändert sie sich — anderer Tokenizer, andere Spalten —,
/// wird die Datei verworfen und neu aufgebaut; ein Index mit altem Aufbau
/// fände sonst still weniger als er soll.
const SCHEMA_VERSION: i64 = 3;

fn schema(conn: &Connection) -> Result<(), String> {
  conn
    .execute_batch(
      "CREATE TABLE IF NOT EXISTS quellen (
         relpath TEXT PRIMARY KEY,
         hash    TEXT NOT NULL,
         mtime   INTEGER NOT NULL,
         groesse INTEGER NOT NULL
       );
       CREATE INDEX IF NOT EXISTS quellen_hash ON quellen(hash);
       CREATE VIRTUAL TABLE IF NOT EXISTS docs USING fts5(
         hash UNINDEXED, teil UNINDEXED, kind UNINDEXED, doc_id UNINDEXED,
         seiten UNINDEXED,
         name, title, description, tags, body,
         tokenize = 'trigram'
       );",
    )
    .map_err(|e| e.to_string())?;
  conn
    .pragma_update(None, "user_version", SCHEMA_VERSION)
    .map_err(|e| e.to_string())
}

/// Aufbau der geöffneten Datei — 0 bei einer frisch angelegten.
fn version(conn: &Connection) -> i64 {
  conn
    .query_row("PRAGMA user_version", [], |r| r.get(0))
    .unwrap_or(0)
}

/// Index im Arbeitsspeicher — jeder Test bekommt seinen eigenen, ohne die
/// Config des Rechners anzufassen.
#[cfg(test)]
pub(crate) fn im_speicher() -> Result<Connection, String> {
  let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
  schema(&conn)?;
  Ok(conn)
}

/// Öffnet den Index und legt ihn bei Bedarf an. Ist die Datei unbrauchbar,
/// wird sie verworfen und neu aufgebaut — der Index ist abgeleitet, ein
/// Neuaufbau kostet nur Zeit.
pub(crate) fn oeffne(pfad: &Path) -> Result<Connection, String> {
  let alt = pfad.is_file();
  if let Ok(conn) = Connection::open(pfad) {
    // Eine bestehende Datei mit passendem Aufbau wird weiterbenutzt; eine
    // frische bekommt ihr Schema.
    if !alt || version(&conn) == SCHEMA_VERSION {
      schema(&conn)?;
      return Ok(conn);
    }
  }
  // Alter Aufbau oder beschädigt: verwerfen. Der Index ist abgeleitet, der
  // Neuaufbau kostet nur Zeit.
  let _ = std::fs::remove_file(pfad);
  let conn = Connection::open(pfad).map_err(|e| e.to_string())?;
  schema(&conn)?;
  Ok(conn)
}

/// SHA-256 einer Datei, hexadezimal.
fn hash_von(pfad: &Path) -> Result<String, String> {
  let daten = std::fs::read(pfad).map_err(|e| format!("{}: {e}", pfad.display()))?;
  let mut h = Sha256::new();
  h.update(&daten);
  Ok(format!("{:x}", h.finalize()))
}

/// Was das Dateisystem über eine Datei sagt, ohne sie zu lesen.
fn stat(pfad: &Path) -> Result<(i64, i64), String> {
  let m = std::fs::metadata(pfad).map_err(|e| format!("{}: {e}", pfad.display()))?;
  let mtime = m
    .modified()
    .map_err(|e| e.to_string())?
    .duration_since(std::time::UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs() as i64;
  Ok((mtime, m.len() as i64))
}

/// Bringt den Index auf den Stand des Archivs. Liefert, wie viele Dateien neu
/// gelesen wurden — für Tests und Protokoll.
pub(crate) fn abgleichen(conn: &Connection, home: &Path) -> Result<usize, String> {
  let dateien = crate::domain::archive_index::dateien_im_archiv(home)?;
  let bekannt: HashMap<String, (String, i64, i64)> = conn
    .prepare("SELECT relpath, hash, mtime, groesse FROM quellen")
    .and_then(|mut s| {
      s.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
      })?
      .collect()
    })
    .map_err(|e| e.to_string())?;

  let mut gesehen: HashSet<String> = HashSet::new();
  let mut gelesen = 0usize;
  for relpath in &dateien {
    gesehen.insert(relpath.clone());
    let pfad = home.join(relpath);
    let (mtime, groesse) = stat(&pfad)?;
    if let Some((_, m, g)) = bekannt.get(relpath) {
      if *m == mtime && *g == groesse {
        continue;
      }
    }
    let hash = hash_von(&pfad)?;
    // Denselben Inhalt gibt es schon (verschoben, umbenannt, zweite Kopie):
    // Der Index bleibt, nur die Zuordnung wird nachgezogen.
    let vorhanden: i64 = conn
      .query_row("SELECT count(*) FROM docs WHERE hash = ?1", params![hash], |r| r.get(0))
      .map_err(|e| e.to_string())?;
    if vorhanden == 0 {
      for teil in teile_fuer_index(home, relpath)? {
        eintragen(conn, &hash, &teil)?;
      }
      gelesen += 1;
    }
    conn
      .execute(
        "INSERT INTO quellen (relpath, hash, mtime, groesse) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(relpath) DO UPDATE SET hash=?2, mtime=?3, groesse=?4",
        params![relpath, hash, mtime, groesse],
      )
      .map_err(|e| e.to_string())?;
  }

  // Verschwundene Dateien und damit verwaiste Inhalte.
  for relpath in bekannt.keys().filter(|r| !gesehen.contains(*r)) {
    conn
      .execute("DELETE FROM quellen WHERE relpath = ?1", params![relpath])
      .map_err(|e| e.to_string())?;
  }
  conn
    .execute(
      "DELETE FROM docs WHERE hash NOT IN (SELECT hash FROM quellen)",
      [],
    )
    .map_err(|e| e.to_string())?;
  Ok(gelesen)
}

fn eintragen(conn: &Connection, hash: &str, teil: &Teil) -> Result<(), String> {
  conn
    .execute(
      "INSERT INTO docs (hash, teil, kind, doc_id, seiten, name, title, description, tags, body)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
      params![
        hash,
        teil.teil,
        teil.kind,
        teil.doc_id,
        teil.seiten,
        teil.name,
        teil.title,
        teil.description,
        teil.tags,
        teil.body
      ],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::tmp_paths;
  use std::fs;

  fn archiv() -> PathBuf {
    let home = tmp_paths().home.join("archiv");
    fs::create_dir_all(home.join("konzepte")).unwrap();
    fs::write(home.join("a.md"), "---\ntitle: \"A\"\n---\n\nKessel und Rohr.\n").unwrap();
    fs::write(home.join("konzepte/b.md"), "---\ntitle: \"B\"\n---\n\nNur Rohr.\n").unwrap();
    home
  }

  fn eintraege(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM docs", [], |r| r.get(0)).unwrap()
  }

  #[test]
  fn liest_nur_was_neu_oder_geaendert_ist() {
    let home = archiv();
    let conn = im_speicher().unwrap();
    assert_eq!(abgleichen(&conn, &home).unwrap(), 2);
    // Zweiter Lauf ohne Änderung: nichts zu lesen.
    assert_eq!(abgleichen(&conn, &home).unwrap(), 0);
    assert_eq!(eintraege(&conn), 2);

    fs::write(home.join("c.md"), "---\ntitle: \"C\"\n---\n\nNeu dazu.\n").unwrap();
    assert_eq!(abgleichen(&conn, &home).unwrap(), 1);
    assert_eq!(eintraege(&conn), 3);
  }

  /// Der Schlüssel ist der Inhalt: Verschieben und Umbenennen zieht nur die
  /// Zuordnung nach, ohne die Datei erneut zu lesen.
  #[test]
  fn verschieben_liest_nicht_neu() {
    let home = archiv();
    let conn = im_speicher().unwrap();
    abgleichen(&conn, &home).unwrap();
    fs::rename(home.join("a.md"), home.join("konzepte/a.md")).unwrap();
    assert_eq!(abgleichen(&conn, &home).unwrap(), 0);
    assert_eq!(eintraege(&conn), 2);
    let pfad: String = conn
      .query_row("SELECT relpath FROM quellen WHERE relpath LIKE '%a.md'", [], |r| r.get(0))
      .unwrap();
    assert_eq!(pfad, "konzepte/a.md");
  }

  /// Was von der Platte verschwindet, verschwindet aus dem Index.
  #[test]
  fn geloeschtes_fliegt_raus() {
    let home = archiv();
    let conn = im_speicher().unwrap();
    abgleichen(&conn, &home).unwrap();
    fs::remove_file(home.join("a.md")).unwrap();
    abgleichen(&conn, &home).unwrap();
    assert_eq!(eintraege(&conn), 1);
    let rest: String = conn.query_row("SELECT title FROM docs", [], |r| r.get(0)).unwrap();
    assert_eq!(rest, "B");
  }

  /// Ein geänderter Inhalt ist ein neuer Schlüssel — der alte Eintrag geht
  /// mit, sonst stünden beide Fassungen im Index.
  #[test]
  fn geaenderter_inhalt_ersetzt_den_alten() {
    let home = archiv();
    let conn = im_speicher().unwrap();
    abgleichen(&conn, &home).unwrap();
    fs::write(home.join("a.md"), "---\ntitle: \"A\"\n---\n\nGanz anderer Text.\n").unwrap();
    abgleichen(&conn, &home).unwrap();
    assert_eq!(eintraege(&conn), 2);
    let treffer: i64 = conn
      .query_row("SELECT count(*) FROM docs WHERE body LIKE '%Kessel%'", [], |r| r.get(0))
      .unwrap();
    assert_eq!(treffer, 0);
  }
}
