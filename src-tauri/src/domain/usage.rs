//! Verbrauch: aggregiert message.usage aus den Transcript-JSONLs aller Pools
//! zu Pool × Projekt, mit Kostenschätzung nach Modell-Raten.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::paths::Paths;
use crate::domain::pool::{pool_config_dir, read_pool, POOL_FILE};
use crate::domain::registry::load_registry;

#[derive(Serialize, Default)]
pub(crate) struct UsageTotals {
  #[serde(rename = "inputTokens")]
  input_tokens: u64,
  #[serde(rename = "outputTokens")]
  output_tokens: u64,
  #[serde(rename = "cacheCreationTokens")]
  cache_creation_tokens: u64,
  #[serde(rename = "cacheReadTokens")]
  cache_read_tokens: u64,
  #[serde(rename = "costUsd")]
  cost_usd: f64,
}

#[derive(Serialize)]
pub(crate) struct UsageRow {
  pool: String,
  project: String,
  #[serde(flatten)]
  totals: UsageTotals,
}

/// USD pro MTok (input, output, cache_write, cache_read).
/// Cache: write 1.25× / read 0.1× des Input-Preises.
fn model_rates(model: &str) -> (f64, f64, f64, f64) {
  let (input, output) = if model.contains("fable") || model.contains("mythos") {
    (10.0, 50.0)
  } else if model.contains("opus") {
    (5.0, 25.0)
  } else if model.contains("haiku") {
    (1.0, 5.0)
  } else {
    // sonnet und Unbekanntes
    (3.0, 15.0)
  };
  (input, output, input * 1.25, input * 0.1)
}

/// Tage seit Unix-Epoche für ein Kalenderdatum (Howard Hinnant, days_from_civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
  let y = if m <= 2 { y - 1 } else { y };
  let era = if y >= 0 { y } else { y - 399 } / 400;
  let yoe = y - era * 400;
  let mp = (m + 9) % 12;
  let doy = (153 * mp + 2) / 5 + d - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era * 146097 + doe - 719468
}

/// "YYYY-MM-DDTHH:MM:SS(.mmm)Z" (UTC) → Unix-Sekunden.
fn parse_ts(ts: &str) -> Option<i64> {
  if ts.len() < 19 {
    return None;
  }
  let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
  let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
  let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
  Some(days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + s)
}

/// Projektpfad so kodieren, wie claude die Transcript-Ordner benennt
/// (alles außer [a-zA-Z0-9] wird '-').
fn encode_project_path(path: &str) -> String {
  path
    .chars()
    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
    .collect()
}

/// Eine Transcript-Zeile auswerten: nur Zeilen mit message.usage zählen,
/// Zeitfilter, Dedup über message.id+requestId (Retries doppeln sonst).
fn add_usage_line(
  line: &str,
  cutoff: i64,
  seen: &mut HashSet<String>,
  totals: &mut UsageTotals,
) {
  let v: serde_json::Value = match serde_json::from_str(line) {
    Ok(v) => v,
    // Nicht-JSON (z. B. angeschnittene letzte Zeile einer laufenden Session)
    Err(_) => return,
  };
  let usage = &v["message"]["usage"];
  if !usage.is_object() {
    return;
  }
  match v["timestamp"].as_str().and_then(parse_ts) {
    Some(t) if t >= cutoff => {}
    _ => return,
  }
  if let Some(id) = v["message"]["id"].as_str() {
    let key = format!("{id}:{}", v["requestId"].as_str().unwrap_or(""));
    if !seen.insert(key) {
      return;
    }
  }
  let model = v["message"]["model"].as_str().unwrap_or("");
  if model == "<synthetic>" {
    return;
  }
  let input = usage["input_tokens"].as_u64().unwrap_or(0);
  let output = usage["output_tokens"].as_u64().unwrap_or(0);
  let cache_write = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
  let cache_read = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
  let (ri, ro, rw, rr) = model_rates(model);
  totals.input_tokens += input;
  totals.output_tokens += output;
  totals.cache_creation_tokens += cache_write;
  totals.cache_read_tokens += cache_read;
  totals.cost_usd += input as f64 / 1e6 * ri
    + output as f64 / 1e6 * ro
    + cache_write as f64 / 1e6 * rw
    + cache_read as f64 / 1e6 * rr;
}

/// Aggregiert message.usage aus den Transcript-JSONLs aller Pools:
/// <pool>/projects/<kodierter-projektpfad>/*.jsonl → Pool × Projekt.
/// Sichtfenster ist, was claude noch nicht weggeräumt hat (cleanupPeriodDays).
pub(crate) fn usage_stats_in(paths: &Paths, days: u32) -> Result<Vec<UsageRow>, String> {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|e| e.to_string())?
    .as_secs() as i64;
  let cutoff = now - days as i64 * 86400;

  // kodierter Projektpfad → Projektname
  let mut names = HashMap::new();
  for (id, entry) in load_registry(paths)? {
    let name = crate::domain::project::display_name_in(paths, &id)?;
    names.insert(encode_project_path(&entry.dir.to_string_lossy()), name);
  }

  let mut seen = HashSet::new();
  let mut rows: HashMap<(String, String), UsageTotals> = HashMap::new();

  if !paths.pools_dir().is_dir() {
    return Ok(Vec::new());
  }
  for pool_entry in
    fs::read_dir(paths.pools_dir()).map_err(|e| format!("{}: {e}", paths.pools_dir().display()))?
  {
    let pool_entry = pool_entry.map_err(|e| format!("{}: {e}", paths.pools_dir().display()))?;
    let id = pool_entry.file_name().to_string_lossy().into_owned();
    // Anzeigename aus pool.json; Ordner ohne pool.json unter der ID ausweisen.
    // Die Transkripte liegen im Config-Verzeichnis des Pools — bei einem
    // referenzierten Pool also außerhalb seiner Hülle.
    let (pool, config_dir) = if pool_entry.path().join(POOL_FILE).is_file() {
      (read_pool(paths, &id)?.name, pool_config_dir(paths, &id)?)
    } else {
      (id, pool_entry.path())
    };
    let projects_root = config_dir.join("projects");
    if !projects_root.is_dir() {
      continue;
    }
    for proj_entry in
      fs::read_dir(&projects_root).map_err(|e| format!("{}: {e}", projects_root.display()))?
    {
      let proj_entry = proj_entry.map_err(|e| format!("{}: {e}", projects_root.display()))?;
      if !proj_entry.path().is_dir() {
        continue;
      }
      let encoded = proj_entry.file_name().to_string_lossy().into_owned();
      // Unbekannte Ordner (Sessions außerhalb der Projekte) unter dem
      // kodierten Namen ausweisen statt verschlucken.
      let project = names.get(&encoded).cloned().unwrap_or(encoded);
      let totals = rows.entry((pool.clone(), project)).or_default();
      for file in fs::read_dir(proj_entry.path())
        .map_err(|e| format!("{}: {e}", proj_entry.path().display()))?
      {
        let file = file.map_err(|e| format!("{}: {e}", proj_entry.path().display()))?;
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
          continue;
        }
        let f = fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for line in BufReader::new(f).lines() {
          let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
          add_usage_line(&line, cutoff, &mut seen, totals);
        }
      }
    }
  }

  let mut out: Vec<UsageRow> = rows
    .into_iter()
    .map(|((pool, project), totals)| UsageRow { pool, project, totals })
    .collect();
  out.sort_by(|a, b| {
    a.pool
      .cmp(&b.pool)
      .then(b.totals.cost_usd.total_cmp(&a.totals.cost_usd))
  });
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::{create_project, make_apikey_pool, map_store, tmp_paths};

  fn usage_line(id: &str, req: &str, ts: &str, model: &str, input: u64, output: u64) -> String {
    serde_json::json!({
      "type": "assistant",
      "timestamp": ts,
      "requestId": req,
      "message": {
        "id": id,
        "model": model,
        "usage": {
          "input_tokens": input,
          "output_tokens": output,
          "cache_creation_input_tokens": 1_000_000,
          "cache_read_input_tokens": 2_000_000
        }
      }
    })
    .to_string()
  }

  #[test]
  fn verbrauch_aggregation_dedup_und_zeitfilter() {
    let p = tmp_paths();
    let kunde = make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    create_project(&p, "proj").unwrap();

    let proj_path = p.projects_dir().join("proj");
    let encoded = encode_project_path(&proj_path.to_string_lossy());
    let dir = p.pool_dir(&kunde).join("projects").join(&encoded);
    fs::create_dir_all(&dir).unwrap();

    let lines = [
      // Sonnet: 1M in ($3) + 1M out ($15) + 1M cache-write ($3.75) + 2M cache-read ($0.60)
      usage_line("msg_1", "req_1", "2099-01-01T10:00:00.000Z", "claude-sonnet-5", 1_000_000, 1_000_000),
      // Retry-Duplikat: gleiche message.id + requestId → zählt nicht
      usage_line("msg_1", "req_1", "2099-01-01T10:00:01.000Z", "claude-sonnet-5", 1_000_000, 1_000_000),
      // außerhalb des Zeitfensters → zählt nicht
      usage_line("msg_2", "req_2", "2020-01-01T00:00:00.000Z", "claude-sonnet-5", 5, 5),
      // kein usage-Objekt → ignoriert
      r#"{"type":"user","timestamp":"2099-01-01T10:00:02.000Z"}"#.to_string(),
      // kaputte Zeile (laufende Session) → ignoriert
      r#"{"type":"assist"#.to_string(),
    ];
    fs::write(dir.join("session.jsonl"), lines.join("\n")).unwrap();

    // Cutoff = jetzt − 365 Tage: schließt 2020 aus, 2099 ein — datumsunabhängig
    let rows = usage_stats_in(&p, 365).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.pool, "kunde");
    assert_eq!(r.project, "proj");
    assert_eq!(r.totals.input_tokens, 1_000_000);
    assert_eq!(r.totals.output_tokens, 1_000_000);
    assert_eq!(r.totals.cache_creation_tokens, 1_000_000);
    assert_eq!(r.totals.cache_read_tokens, 2_000_000);
    assert!((r.totals.cost_usd - 22.35).abs() < 1e-9);
  }

  #[test]
  fn verbrauch_leer_ohne_transcripts() {
    let p = tmp_paths();
    make_apikey_pool(&p, &map_store(), "kunde", "sk-1");
    assert!(usage_stats_in(&p, 30).unwrap().is_empty());
  }

  #[test]
  fn parse_ts_referenz() {
    // 2026-01-01 = 1767225600; +181 Tage bis 01.07. +10 h = 1782900000
    assert_eq!(parse_ts("2026-07-01T10:00:00.000Z"), Some(1_782_900_000));
    assert_eq!(parse_ts("kein datum"), None);
  }
}
