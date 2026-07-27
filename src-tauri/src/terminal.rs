use base64::{engine::general_purpose::STANDARD, Engine};
use portable_pty::{native_pty_system, ChildKiller, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::domain::paths::{
  commands_file, panel_file, panel_source_file, search_file, wiki_file, Paths,
};
use crate::domain::project::{
  project_config, project_pool_dir, verify_project_dir_in, ProjectConfig,
};
use crate::domain::registry::project_dir;
use crate::domain::settings::claude_command;

/// Eine laufende PTY-Session, gekoppelt an ein Terminal-Fenster (Key = Fenster-Label).
pub struct Session {
  writer: Box<dyn Write + Send>,
  master: Box<dyn MasterPty + Send>,
  killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals(pub Mutex<HashMap<String, Session>>);

/// Öffnet ein Terminal für ein Projekt: startet das eigene Binary als
/// Terminal-Prozess (`--terminal <projekt>`), damit jedes Terminal ein
/// eigenes Dock-Icon bekommt. Vorher tritt der rufende Prozess die
/// Aktivierung ab — Aktivierung ist seit macOS 14 kooperativ, sonst öffnet
/// das Terminal-Fenster hinter der aktiven App.
#[tauri::command]
pub fn open_terminal(app: AppHandle, project: String) -> Result<(), String> {
  // Prüft zugleich die Projekt-Identität: die config.json im registrierten
  // Ordner muss diese Projekt-ID tragen (verschobene/ersetzte Ordner fallen
  // hier auf, statt eine fremde Session zu starten).
  let dir = verify_project_dir_in(&Paths::real(), &project)?;
  if !dir.is_dir() {
    return Err(format!("Projektordner fehlt: {}", dir.display()));
  }
  // Pool-Pflicht vor dem Fenster: Ohne zugewiesenen Pool gibt es kein
  // Terminal. Die Prüfung gehört hierher und nicht erst in `term_start` —
  // dort stünde bereits ein Fenster, das nur noch scheitern könnte.
  project_pool_dir(&project)?;
  let bundle_id = app.config().identifier.clone();
  app
    .run_on_main_thread(move || crate::platform::yield_activation(&bundle_id))
    .map_err(|e| e.to_string())?;
  let exe = std::env::current_exe().map_err(|e| e.to_string())?;
  std::process::Command::new(exe)
    .args(["--terminal", &project])
    .spawn()
    .map_err(|e| e.to_string())?;
  Ok(())
}

/// Fenster-Hintergrund je Theme — muss zu den Theme-Definitionen in
/// terminal.ts passen, sonst blitzt beim Öffnen die falsche Farbe auf.
fn theme_background(theme: &str) -> (u8, u8, u8) {
  match theme {
    "dracula" => (0x28, 0x2a, 0x36),
    "solarized-dark" => (0x00, 0x2b, 0x36),
    "gruvbox" => (0x28, 0x28, 0x28),
    "one-dark" => (0x28, 0x2c, 0x34),
    _ => (0x1e, 0x1e, 0x2e), // Catppuccin Mocha
  }
}

/// Baut das Terminal-Fenster des Terminal-Prozesses. Die PTY entsteht erst,
/// wenn das Fenster geladen ist und `term_start` ruft — so gehen keine
/// Ausgaben verloren, bevor der Event-Listener steht.
pub fn build_window(
  app: &AppHandle,
  project: &str,
  cfg: &ProjectConfig,
) -> tauri::Result<()> {
  let (r, g, b) = theme_background(cfg.terminal.theme.as_deref().unwrap_or_default());
  let title = cfg
    .terminal
    .title
    .as_deref()
    .or(cfg.name.as_deref())
    .unwrap_or(project);
  let builder = WebviewWindowBuilder::new(
    app,
    format!("term-{project}"),
    WebviewUrl::App(format!("terminal.html?project={project}").into()),
  )
  .title(format!("{title} — Session"))
  .inner_size(980.0, 640.0)
  .background_color(tauri::window::Color(r, g, b, 0xff));

  // Titelbar liegt über dem Inhalt; der Header in terminal.html ist Drag-Region.
  #[cfg(target_os = "macos")]
  let builder = builder
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true);

  // Linux/GNOME: keine GTK-Deko — eigene Titelleiste im Header (terminal.html).
  #[cfg(target_os = "linux")]
  let builder = builder.decorations(false);

  builder.build()?;
  Ok(())
}

/// Startet die PTY für das rufende Fenster: das konfigurierte Claude-Kommando
/// (settings.json: claudeCommand, Default claude) im Projektordner, mit dem
/// Pool-Verzeichnis als CLAUDE_CONFIG_DIR. Die Login-Shell aus $SHELL (-l)
/// baut den PATH aus ihren Profil-Dateien auf — shell-agnostisch.
///
/// Nur das Terminal-Fenster selbst darf seine PTY starten: Die Terminals-Map
/// ist über das Fensterlabel adressiert, und `term_write` schreibt danach in
/// die laufende Shell. Ohne die Label-Prüfung könnte auch das abgelöste
/// Panel-Fenster — und damit jedes Skript in dessen Webview — eine Shell
/// starten und beschreiben.
#[tauri::command]
pub fn term_start(
  window: tauri::WebviewWindow,
  terminals: State<Terminals>,
  project: String,
  rows: u16,
  cols: u16,
) -> Result<(), String> {
  if window.label() != format!("term-{project}") {
    return Err(format!("PTY nur für das Terminal-Fenster: {}", window.label()));
  }
  let paths = Paths::real();
  let cwd = project_dir(&paths, &project)?;

  let pty = native_pty_system()
    .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
    .map_err(|e| e.to_string())?;

  // Puffer-Kanäle aller Module (Modul-Registry): flüchtige Puffer leeren
  // (Inhalte gelten pro Session), persistente (ToDo) nur anlegen; die Pfade
  // für Env und Watcher vormerken. Bewusst alle Module, nicht nur aktive:
  // Die Kanäle sind billig, und ein mitten in der Session zugeschaltetes
  // Modul findet sie vor; die Abwahl wirkt auf Tool-Liste und Tabs.
  let buffers: Vec<(&'static crate::domain::modules::BufferDesc, std::path::PathBuf)> =
    crate::domain::modules::MODULES
      .iter()
      .flat_map(|m| m.buffers)
      .map(|b| (b, (b.file)(&project)))
      .collect();
  crate::domain::project::migrate_todos_into_project(&Paths::real(), &project)?;
  for (b, path) in &buffers {
    if let Some(parent) = path.parent() {
      let _ = std::fs::create_dir_all(parent);
    }
    if !b.persistent || !path.is_file() {
      let _ = std::fs::write(path, "");
    }
  }
  // Quell-Verknüpfung der Vorsession lösen — der Dokument-Puffer startet leer,
  // Edits dürfen nicht in die zuletzt geöffnete Archiv-Datei laufen.
  match std::fs::remove_file(panel_source_file(&project)) {
    Ok(()) => {}
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(e.to_string()),
  }

  let mut cmd = crate::platform::shell_command(&claude_command(&paths));
  cmd.cwd(&cwd);
  // Aus der App-Umgebung geerbte Anthropic-Credentials rausnehmen — ein
  // ANTHROPIC_API_KEY sticht sonst den apiKeyHelper des Pools. Variablen, die
  // erst das Shell-Profil der Login-Shell setzt, erreicht das nicht.
  cmd.env_remove("ANTHROPIC_API_KEY");
  cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
  cmd.env("TERM", "xterm-256color");
  for (b, path) in &buffers {
    cmd.env(b.env, path);
  }
  cmd.env("AI_CENTRAL_PROJECT", &project);
  if let Some(pool_dir) = project_pool_dir(&project)? {
    cmd.env("CLAUDE_CONFIG_DIR", pool_dir);
  }

  let mut child = pty.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
  drop(pty.slave);

  let killer = child.clone_killer();
  let writer = pty.master.take_writer().map_err(|e| e.to_string())?;
  let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;

  let label = window.label().to_string();
  let app = window.app_handle().clone();

  // Output gebündelt emittieren statt pro read(): der Reader schiebt Chunks
  // in einen Channel, der Emitter sammelt alles, was innerhalb von 8 ms
  // ankommt, zu einem Event zusammen. Bei Output-Fluten (Scrollen, TUI-
  // Redraws) sinkt die Zahl der IPC-Events um Größenordnungen; die 8 ms
  // Zusatzlatenz beim Tasten-Echo liegen unter einem Frame.
  let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
  std::thread::spawn(move || {
    let mut buf = [0u8; 8192];
    loop {
      match reader.read(&mut buf) {
        Ok(0) | Err(_) => break,
        Ok(n) => {
          if tx.send(buf[..n].to_vec()).is_err() {
            break;
          }
        }
      }
    }
    let _ = child.wait();
    // tx wird hier gedroppt — der Emitter sieht den geschlossenen Channel
    // erst, nachdem er alle Rest-Chunks emittiert hat.
  });
  std::thread::spawn(move || {
    const FLUSH: std::time::Duration = std::time::Duration::from_millis(8);
    while let Ok(first) = rx.recv() {
      let mut chunk = first;
      let deadline = std::time::Instant::now() + FLUSH;
      loop {
        let now = std::time::Instant::now();
        if now >= deadline {
          break;
        }
        match rx.recv_timeout(deadline - now) {
          Ok(more) => chunk.extend_from_slice(&more),
          Err(_) => break,
        }
      }
      let _ = app.emit_to(&label, "pty-output", STANDARD.encode(&chunk));
    }
    let _ = app.emit_to(&label, "pty-exit", ());
  });

  for (b, path) in buffers {
    spawn_file_watcher(window.app_handle().clone(), path, b.event);
  }

  terminals.0.lock().unwrap().insert(
    window.label().to_string(),
    Session { writer, master: pty.master, killer },
  );
  Ok(())
}

/// Beobachtet eine Panel-Datei des Projekts (Entwurf oder Command-History)
/// und schickt neuen Inhalt unter `event` an alle Fenster dieses
/// Terminal-Prozesses (angedocktes Panel und ein evtl. abgelöstes
/// Panel-Fenster). Pollt per mtime — kein notify-Crate, die Datei ändert sich
/// nur, wenn geschrieben wird. Der Thread endet mit dem Prozess (Fenster zu).
fn spawn_file_watcher(app: AppHandle, path: std::path::PathBuf, event: &'static str) {
  std::thread::spawn(move || {
    let mtime = || std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut last = mtime();
    loop {
      std::thread::sleep(std::time::Duration::from_millis(200));
      let now = mtime();
      if now != last {
        last = now;
        if let Ok(content) = std::fs::read_to_string(&path) {
          let _ = app.emit(event, content);
        }
      }
    }
  });
}

/// Aktueller Inhalt eines Modul-Puffers (Erstbefüllung einer Panel-Ansicht);
/// `buffer` ist die Puffer-ID aus der Modul-Registry. Eine noch nicht
/// geschriebene Datei liest sich als leer — wie ein leerer Puffer.
#[tauri::command]
pub fn buffer_read(project: String, buffer: String) -> Result<String, String> {
  let b = crate::domain::modules::MODULES
    .iter()
    .flat_map(|m| m.buffers)
    .find(|b| b.id == buffer)
    .ok_or_else(|| format!("unbekannter Puffer: {buffer}"))?;
  Ok(std::fs::read_to_string((b.file)(&project)).unwrap_or_default())
}

/// Entfernt einen Befehl aus der Command-History (Löschen einer Kachel im
/// Panel) über seine stabile ID, die write_commands beim Schreiben vergibt.
/// Ein leer gewordener Record fällt mit weg; der Watcher meldet den neuen
/// Stand als `commands-update`. Doppelklick oder ein zweites Fenster auf
/// derselben Liste laufen ins „bereits entfernt" statt auf falsche Indizes.
#[tauri::command]
pub fn commands_delete(project: String, id: String) -> Result<(), String> {
  let path = commands_file(&project);
  let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
  let mut records: Vec<serde_json::Value> = text
    .lines()
    .filter(|l| !l.trim().is_empty())
    .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
    .collect::<Result<_, _>>()?;
  let mut found = false;
  for rec in records.iter_mut() {
    if let Some(cmds) = rec["commands"].as_array_mut() {
      let before = cmds.len();
      cmds.retain(|c| c["id"].as_str() != Some(id.as_str()));
      found = found || cmds.len() != before;
    }
  }
  if !found {
    return Err("Befehl bereits entfernt".into());
  }
  records.retain(|r| r["commands"].as_array().is_none_or(|c| !c.is_empty()));
  let mut out = String::new();
  for rec in &records {
    out.push_str(&rec.to_string());
    out.push('\n');
  }
  std::fs::write(&path, out).map_err(|e| e.to_string())
}

/// Entfernt ein ToDo aus der persistenten Liste (Löschen einer Kachel) über
/// seine stabile ID, die write_todos beim Schreiben vergibt. Der Watcher
/// meldet den neuen Stand als `todos-update`.
#[tauri::command]
pub fn todos_delete(project: String, id: String) -> Result<(), String> {
  let path = crate::domain::paths::todos_file(&project);
  let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
  let mut removed = false;
  let mut out = String::new();
  for line in text.lines().filter(|l| !l.trim().is_empty()) {
    let rec: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    if rec["id"].as_str() == Some(id.as_str()) {
      removed = true;
      continue;
    }
    out.push_str(line);
    out.push('\n');
  }
  if !removed {
    return Err("ToDo bereits entfernt".into());
  }
  std::fs::write(&path, out).map_err(|e| e.to_string())
}

/// Baut die JSONL-Zeile eines ToDos — gleiche Form wie call_write_todos im
/// MCP-Server; `id` und `ts` kommen vom Aufrufer, damit das Update beide
/// erhalten kann.
fn todo_line(
  id: &str,
  ts: u64,
  text: &str,
  note: Option<&str>,
  due: Option<&str>,
) -> Result<String, String> {
  let text = text.trim();
  if text.is_empty() {
    return Err("ToDo ohne Text".into());
  }
  let mut rec = serde_json::json!({ "id": id, "ts": ts, "text": text });
  if let Some(note) = note {
    rec["note"] = serde_json::json!(note);
  }
  if let Some(due) = due {
    if !crate::mcp::valid_due(due) {
      return Err(format!("ungültiges due-Datum: {due} (erwartet YYYY-MM-DD)"));
    }
    rec["due"] = serde_json::json!(due);
  }
  Ok(rec.to_string())
}

/// Ersetzt die Zeile mit passender ID durch Text/Notiz/Fälligkeit aus dem
/// Formular; ID und ts der Zeile bleiben, alle anderen Zeilen bleiben
/// wörtlich erhalten.
fn replace_todo_line(
  raw: &str,
  id: &str,
  text: &str,
  note: Option<&str>,
  due: Option<&str>,
) -> Result<String, String> {
  let mut found = false;
  let mut out = String::new();
  for line in raw.lines().filter(|l| !l.trim().is_empty()) {
    let rec: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    if rec["id"].as_str() == Some(id) {
      found = true;
      let ts = rec["ts"].as_u64().ok_or("ToDo ohne ts")?;
      out.push_str(&todo_line(id, ts, text, note, due)?);
    } else {
      out.push_str(line);
    }
    out.push('\n');
  }
  if !found {
    return Err("ToDo nicht gefunden".into());
  }
  Ok(out)
}

/// Legt ein ToDo aus dem Panel-Formular an (Plus-Button im ToDo-Tab). Der
/// Watcher meldet den neuen Stand als `todos-update`.
#[tauri::command]
pub fn todos_add(
  project: String,
  text: String,
  note: Option<String>,
  due: Option<String>,
) -> Result<(), String> {
  let ts = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0);
  let line = todo_line(
    &uuid::Uuid::new_v4().to_string(),
    ts,
    &text,
    note.as_deref(),
    due.as_deref(),
  )?;
  let path = crate::domain::paths::todos_file(&project);
  std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&path)
    .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()))
    .map_err(|e| e.to_string())
}

/// Überschreibt Text, Notiz und Fälligkeit eines ToDos (Stift auf der
/// Kachel). Der Watcher meldet den neuen Stand als `todos-update`.
#[tauri::command]
pub fn todos_update(
  project: String,
  id: String,
  text: String,
  note: Option<String>,
  due: Option<String>,
) -> Result<(), String> {
  let path = crate::domain::paths::todos_file(&project);
  let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
  let out = replace_todo_line(&raw, &id, &text, note.as_deref(), due.as_deref())?;
  std::fs::write(&path, out).map_err(|e| e.to_string())
}

/// Quell-Verknüpfung des Dokument-Tabs setzen: ab jetzt schreibt jeder
/// Editor-Commit den Body in diese Archiv-Datei zurück.
fn link_panel_source(project: &str, path: &std::path::Path) -> Result<(), String> {
  std::fs::write(panel_source_file(project), path.display().to_string())
    .map_err(|e| e.to_string())
}

/// Aktuelle Quell-Verknüpfung, falls gesetzt.
fn panel_source(project: &str) -> Option<std::path::PathBuf> {
  std::fs::read_to_string(panel_source_file(project))
    .ok()
    .map(|s| std::path::PathBuf::from(s.trim()))
    .filter(|p| !p.as_os_str().is_empty())
}

/// Verknüpfung nach einer Dokument-Operation nachziehen: zeigt sie auf den
/// alten Pfad, wandert sie mit; `None` löst sie (gelöschtes Dokument).
fn relink(
  project: &str,
  old: &std::path::Path,
  new: Option<&std::path::Path>,
) -> Result<(), String> {
  if panel_source(project).as_deref() != Some(old) {
    return Ok(());
  }
  match new {
    Some(p) => link_panel_source(project, p),
    None => std::fs::remove_file(panel_source_file(project)).map_err(|e| e.to_string()),
  }
}

/// Wie `relink`, für einen verschobenen Ordner: eine Verknüpfung auf ein
/// Dokument darin wandert mit.
fn relink_folder(
  project: &str,
  old: &std::path::Path,
  new: &std::path::Path,
) -> Result<(), String> {
  let Some(cur) = panel_source(project) else {
    return Ok(());
  };
  match cur.strip_prefix(old) {
    Ok(rest) => link_panel_source(project, &new.join(rest)),
    Err(_) => Ok(()),
  }
}

/// Schreibt den Panel-Inhalt (Titel-Edit im Panel). Der Watcher meldet die
/// Änderung als `panel-update` an alle Fenster. Zeigt der Dokument-Tab ein
/// Archiv-Dokument (Quell-Verknüpfung), wandert der Body implizit in die
/// Archiv-Datei zurück; ihre Frontmatter bleibt.
#[tauri::command]
pub fn panel_set(project: String, text: String) -> Result<(), String> {
  std::fs::write(panel_file(&project), &text).map_err(|e| e.to_string())?;
  match panel_source(&project) {
    Some(src) => crate::domain::archive_ops::write_body(&src, &text),
    None => Ok(()),
  }
}

/// Lädt ein Archiv-Dokument in den Dokument-Puffer (Treffer-Klick in der
/// Suche) — ohne Frontmatter-Block, wie ein frischer Entwurf — und setzt die
/// Quell-Verknüpfung fürs Zurückschreiben. Der Watcher meldet den neuen
/// Inhalt als `panel-update`.
#[tauri::command]
pub fn panel_load(project: String, path: String) -> Result<(), String> {
  let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
  let body = crate::domain::archive::strip_frontmatter(&text);
  std::fs::write(panel_file(&project), body).map_err(|e| e.to_string())?;
  link_panel_source(&project, std::path::Path::new(&path))
}

/// Suche aus dem Panel-Suchfeld: läuft wie das MCP-Tool search_archive und
/// schreibt die Treffer-Datei; der Watcher zieht sie in die Ansicht (beide
/// Fenster).
#[tauri::command]
pub fn search_run(project: String, query: String, tag: Option<String>) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let hits = crate::domain::archive_search::search(&home, &query, tag.as_deref(), 20)?;
  let payload = serde_json::json!({
    "query": query,
    "tag": tag,
    "home": home.display().to_string(),
    "hits": hits,
  });
  std::fs::write(search_file(&project), payload.to_string()).map_err(|e| e.to_string())
}

/// Öffnet ein Wiki-Ziel: `tag:x` → Schlagwort-Seite, `tag:` →
/// Archiv-Übersicht in den Wiki-Puffer (`wiki-update`); ein Dokumentname
/// (Wikilink, Backlink) wird über den Index aufgelöst und wie ein
/// Suchtreffer in den Dokument-Puffer geladen (`panel-update`).
#[tauri::command]
pub fn wiki_open(project: String, name: String) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  match name.strip_prefix("tag:") {
    Some(tag) => {
      let display = crate::domain::project::display_name_in(&Paths::real(), &project)?;
      crate::domain::archive_ops::ensure_node_texts(&home, &display)?;
      crate::domain::archive_ops::ensure_ids(&home)?;
      let json = serde_json::to_string(&crate::domain::archive_index::archive_page(
        &home,
        (!tag.is_empty()).then_some(tag),
      )?)
      .map_err(|e| e.to_string())?;
      std::fs::write(wiki_file(&project), json).map_err(|e| e.to_string())
    }
    None => {
      let rel = crate::domain::archive_index::resolve_doc(&home, &name)?;
      let path = home.join(rel);
      let text =
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
      let body = crate::domain::archive::strip_frontmatter(&text);
      std::fs::write(panel_file(&project), body).map_err(|e| e.to_string())?;
      link_panel_source(&project, &path)
    }
  }
}

/// Liest den Body eines Archiv-Dokuments (ohne Frontmatter) — Inhalt der
/// Notiz-Ansicht im Archiv-Tab.
#[tauri::command]
pub fn archive_read(project: String, id: String) -> Result<String, String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let relpath = crate::domain::archive_index::resolve_id(&home, &id)?;
  let path = crate::domain::archive_ops::doc_path(&home, &relpath)?;
  let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
  Ok(crate::domain::archive::strip_frontmatter(&text).to_string())
}

/// Öffnet ein Buch (`.epub`) aus dem Archiv für den Viewer: einmal in den
/// Cache entpacken, Lesereihenfolge, Inhaltsverzeichnis und Metadaten aus
/// seinen Verwaltungsdateien lesen. Die Seiten selbst holt der Viewer über
/// das `epub://`-Protokoll.
#[tauri::command]
pub fn epub_open(project: String, id: String) -> Result<crate::domain::epub::Book, String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let relpath = crate::domain::archive_index::resolve_id(&home, &id)?;
  let path = crate::domain::archive_ops::file_path(&home, &relpath)?;
  crate::domain::epub::open(&path)
}

/// Schreibt den Body einer Archiv-Notiz zurück (Bearbeiten im
/// Archiv-Fenster); die Frontmatter der Datei bleibt.
#[tauri::command]
pub fn archive_write(project: String, id: String, text: String) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let relpath = crate::domain::archive_index::resolve_id(&home, &id)?;
  let path = crate::domain::archive_ops::doc_path(&home, &relpath)?;
  crate::domain::archive_ops::write_body(&path, &text)
}

/// Ordner-Knoten des Archivs (Pfad + Titel, sortiert) — Zielordner-Baum des
/// Archiv-Dialogs.
#[tauri::command]
pub fn archive_folders(
  project: String,
) -> Result<Vec<crate::domain::archive_index::FolderNode>, String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  crate::domain::archive_index::folder_nodes(&home)
}

/// Legt den Entwurf als Datei an einem frei gewählten Pfad ab (Speichern-
/// Dialog) — ohne Archiv, ohne Frontmatter: der Panel-Inhalt, wie er ist.
#[tauri::command]
pub fn panel_save_as(project: String, path: String) -> Result<(), String> {
  let text = std::fs::read_to_string(panel_file(&project)).map_err(|e| e.to_string())?;
  std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

/// Setzt den Anzeige-Titel einer Notiz (Klick auf den Titel im Archiv);
/// danach die frische Übersicht, damit Baum und Karten den neuen Titel zeigen.
#[tauri::command]
pub fn archive_set_title(project: String, id: String, title: String) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let relpath = crate::domain::archive_index::resolve_id(&home, &id)?;
  let path = crate::domain::archive_ops::doc_path(&home, &relpath)?;
  crate::domain::archive_ops::set_title(&path, &title)?;
  // Der technische Name folgt dem Titel — beide dürfen nicht auseinander
  // laufen. Bei einem Knotentext wandert der gleichnamige Ordner mit; die
  // Archiv-Wurzel (index.md) behält ihren Namen, sie ist die Konvention.
  let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
  let name = crate::domain::archive::strip_stamp(&stem).to_string();
  let slug = crate::domain::archive::slugify(&title);
  if name != slug && name != "index" {
    let dir = path.with_file_name(&name);
    if dir.is_dir() {
      let old_rel = dir.strip_prefix(&home).unwrap().display().to_string();
      let new_rel = match old_rel.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{slug}"),
        None => slug.clone(),
      };
      crate::domain::archive_ops::move_folder(&home, &old_rel, &new_rel)?;
      relink_folder(&project, &home.join(&old_rel), &home.join(&new_rel))?;
    } else {
      let new_rel = crate::domain::archive_ops::rename_doc(&home, &relpath, &title)?;
      relink(&project, &path, Some(&home.join(new_rel)))?;
    }
  }
  wiki_refresh_page(&project, &home)
}

/// Frische Archiv-Übersicht in den Wiki-Puffer — Abschluss der
/// Dokument-/Ordner-Operationen; der Watcher meldet `wiki-update`.
fn wiki_refresh_page(project: &str, home: &std::path::Path) -> Result<(), String> {
  let display = crate::domain::project::display_name_in(&Paths::real(), project)?;
  crate::domain::archive_ops::ensure_node_texts(home, &display)?;
  crate::domain::archive_ops::ensure_ids(home)?;
  let json = serde_json::to_string(&crate::domain::archive_index::archive_page(home, None)?)
    .map_err(|e| e.to_string())?;
  std::fs::write(wiki_file(project), json).map_err(|e| e.to_string())
}



/// Löscht ein Archiv-Dokument; danach zeigt das Wiki die Übersicht.
#[tauri::command]
pub fn archive_delete(project: String, id: String) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let relpath = crate::domain::archive_index::resolve_id(&home, &id)?;
  crate::domain::archive_ops::delete_doc(&home, &relpath)?;
  relink(&project, &home.join(&relpath), None)?;
  wiki_refresh_page(&project, &home)
}


/// Legt einen Ordner im Archiv an (Plus im Baum); danach Übersicht.
#[tauri::command]
pub fn archive_create_folder(project: String, parent: String, name: String) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let folder = join_under(&home, &parent, &name)?;
  crate::domain::archive_ops::create_folder(&home, &folder)?;
  let display = crate::domain::project::display_name_in(&Paths::real(), &project)?;
  crate::domain::archive_ops::ensure_node_texts(&home, &display)?;
  crate::domain::archive_ops::ensure_ids(&home)?;
  wiki_refresh_page(&project, &home)
}

/// Pfad eines neuen Kindes unterhalb des Knotens `parent` (ID; leer =
/// Wurzel) — der Name wird als Slug angehängt.
fn join_under(
  home: &std::path::Path,
  parent: &str,
  name: &str,
) -> Result<String, String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Name fehlt".into());
  }
  let slug = crate::domain::archive::slugify(name);
  if parent.is_empty() {
    return Ok(slug);
  }
  // Der Zielordner ist der Ordner neben dem Knotentext des Elternteils.
  let rel = crate::domain::archive_index::resolve_id(home, parent)?;
  let stem = std::path::Path::new(&rel)
    .file_stem()
    .unwrap_or_default()
    .to_string_lossy()
    .to_string();
  let dir = match rel.rsplit_once('/') {
    Some((head, _)) => format!("{head}/{stem}"),
    None => stem,
  };
  Ok(format!("{dir}/{slug}"))
}

/// Legt ein leeres Dokument an (Plus im Listenkopf) und öffnet es im
/// Dokument-Tab: leerer Dokument-Puffer plus Quell-Verknüpfung — das
/// Getippte landet über `panel_set` in der Archiv-Datei. Die Übersicht
/// bleibt hier unangetastet; der Wiki-Tab lädt sie beim nächsten Aktivieren
/// frisch (zwei konkurrierende Puffer-Events würden sonst um den aktiven Tab
/// rennen).
#[tauri::command]
pub fn archive_create_doc(
  project: String,
  parent: String,
  name: String,
) -> Result<String, String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let display = crate::domain::project::display_name_in(&Paths::real(), &project)?;
  let rel_new = join_under(&home, &parent, &name)?;
  let folder = rel_new.rsplit_once('/').map(|(h, _)| h.to_string()).unwrap_or_default();
  let rel = crate::domain::archive_ops::create_doc(&home, &folder, &name, &display)?;
  // Bearbeitet wird im Archiv — der Entwurfs-Puffer bleibt unberührt; die
  // frische Übersicht zeigt die neue Notiz, ihre ID öffnet dort den Editor.
  crate::domain::archive_ops::ensure_ids(&home)?;
  wiki_refresh_page(&project, &home)?;
  note_id(&home.join(rel))
}

/// Technische ID einer eben angelegten Notiz.
fn note_id(path: &std::path::Path) -> Result<String, String> {
  let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
  let meta = if path.extension().is_some_and(|e| e == "html") {
    crate::domain::archive_html::parse_meta(&text)
  } else {
    crate::domain::archive::parse_frontmatter(&text)
  };
  meta.get("id").cloned().ok_or_else(|| format!("keine ID in {}", path.display()))
}

/// Legt eine leere HTML-Notiz unter dem Knoten an (Kontextmenü im Baum).
#[tauri::command]
pub fn archive_create_html(
  project: String,
  parent: String,
  name: String,
) -> Result<String, String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let display = crate::domain::project::display_name_in(&Paths::real(), &project)?;
  let rel_new = join_under(&home, &parent, &name)?;
  let folder = rel_new.rsplit_once('/').map(|(h, _)| h.to_string()).unwrap_or_default();
  let rel = crate::domain::archive_ops::create_html(&home, &folder, &name, &display)?;
  crate::domain::archive_ops::ensure_ids(&home)?;
  wiki_refresh_page(&project, &home)?;
  note_id(&home.join(rel))
}

/// Nebenfenster eines Projekts (Archiv, Commit): gleiche Optik, gleiche
/// Monitorwahl, gleiche Plattform-Deko — nur Label, Seite, Titelzusatz und
/// die Größenanteile unterscheiden sich.
///
/// Die Größe wird EINMAL beim Öffnen gesetzt; danach fasst niemand sie mehr
/// an, was der Nutzer zieht, bleibt. Maßgeblich ist der Monitor des
/// Terminal-Fensters, von dem der Öffner kam — bei mehreren Monitoren zählt
/// der, vor dem der Nutzer sitzt (Wayland kennt zudem keinen „primären").
async fn open_project_window(
  app: &AppHandle,
  project: &str,
  label: &str,
  url: String,
  title_suffix: &str,
  (fw, fh): (f64, f64),
) -> Result<(), String> {
  let cfg = project_config(project)?;
  let (r, g, b) = theme_background(cfg.terminal.theme.as_deref().unwrap_or_default());
  let title = cfg
    .terminal
    .title
    .as_deref()
    .or(cfg.name.as_deref())
    .unwrap_or(project);
  let monitor = app
    .get_webview_window(&format!("term-{project}"))
    .and_then(|w| w.current_monitor().ok().flatten())
    .or_else(|| app.primary_monitor().ok().flatten())
    .or_else(|| app.available_monitors().ok().and_then(|m| m.into_iter().next()));
  let (w, h) = match &monitor {
    Some(m) => {
      let s = m.size().to_logical::<f64>(m.scale_factor());
      (s.width * fw, s.height * fh)
    }
    // Kein Monitor abfragbar: Standard-Bildschirm 1920×1080 annehmen,
    // gleiche Faktoren.
    None => (1920.0 * fw, 1080.0 * fh),
  };
  let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
    .title(format!("{title} — {title_suffix}"))
    .inner_size(w, h)
    .background_color(tauri::window::Color(r, g, b, 0xff));
  // Zentriert auf DEM Monitor, dessen Größe oben zählte — .center() nähme
  // sonst einen anderen.
  let builder = match &monitor {
    Some(m) => {
      let scale = m.scale_factor();
      let ms = m.size().to_logical::<f64>(scale);
      let mp = m.position().to_logical::<f64>(scale);
      builder.position(mp.x + (ms.width - w) / 2.0, mp.y + (ms.height - h) / 2.0)
    }
    None => builder.center(),
  };
  // macOS: Titelbar als Overlay über der eigenen Kopfleiste — eine Zeile,
  // wie im Terminal-Fenster; die Topbar der Seite ist Drag-Region.
  #[cfg(target_os = "macos")]
  let builder = builder
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true);
  // Linux/GNOME: keine GTK-Deko — eigene Kopfleiste in der Seite.
  #[cfg(target_os = "linux")]
  let builder = builder.decorations(false);
  builder.build().map_err(|e| e.to_string())?;
  Ok(())
}

/// Löst das Panel in ein eigenes Fenster ab. Existiert es schon, kommt es nach
/// vorn. Async, weil Fenster-Erzeugung aus einem synchronen Command auf dem
/// GTK-Mainloop klemmen kann (Tauri-Vorgabe für window create in Commands).
///
/// Größe: 85 % der Bildschirmbreite, 92 % der Höhe (Vorbild Trilium, kein
/// Vollbild) — das Archiv ist die Lesefläche und darf groß sein.
#[tauri::command]
pub async fn open_panel_window(
  app: AppHandle,
  project: String,
  mode: Option<String>,
) -> Result<(), String> {
  let label = format!("panel-{project}");
  if let Some(w) = app.get_webview_window(&label) {
    // Fenster steht schon: nach vorn holen und auf den gewünschten Tab
    // schalten (Archiv-/Such-Öffner im Terminal-Header).
    let _ = w.set_focus();
    if let Some(m) = &mode {
      app
        .emit_to(tauri::EventTarget::webview_window(&label), "panel-mode", m)
        .map_err(|e| e.to_string())?;
    }
    return Ok(());
  }
  // Der beim Ablösen aktive Tab geht als URL-Parameter mit — das neue
  // Fenster startet dort, statt in seiner Default-Ansicht.
  let url = match &mode {
    Some(m) => format!("panel.html?project={project}&mode={m}"),
    None => format!("panel.html?project={project}"),
  };
  open_project_window(&app, &project, &label, url, "Dokument", (0.85, 0.92)).await
}

/// Debug: JS-Fehler aus dem Terminal-Fenster ins Dev-Log.
#[tauri::command]
pub fn term_log(window: tauri::WebviewWindow, msg: String) {
  eprintln!("[{}] {msg}", window.label());
}

/// Tastatureingaben des Fensters in die PTY.
#[tauri::command]
pub fn term_write(
  window: tauri::WebviewWindow,
  terminals: State<Terminals>,
  data: String,
) -> Result<(), String> {
  let mut map = terminals.0.lock().unwrap();
  let s = map
    .get_mut(window.label())
    .ok_or("keine Session für dieses Fenster")?;
  s.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn term_resize(
  window: tauri::WebviewWindow,
  terminals: State<Terminals>,
  rows: u16,
  cols: u16,
) -> Result<(), String> {
  let map = terminals.0.lock().unwrap();
  let s = map
    .get(window.label())
    .ok_or("keine Session für dieses Fenster")?;
  s.master
    .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
    .map_err(|e| e.to_string())
}

/// Beim Schließen eines Terminal-Fensters: Kind killen, Session verwerfen.
/// Das Droppen des Masters schließt die PTY — claude bekommt HUP.
pub fn close(window: &tauri::Window) {
  let terminals = window.state::<Terminals>();
  let session = terminals.0.lock().unwrap().remove(window.label());
  if let Some(mut s) = session {
    let _ = s.killer.kill();
  }
}

#[cfg(test)]
mod tests {
  use super::{replace_todo_line, todo_line};

  #[test]
  fn todo_zeile_baut_und_prueft() {
    let line = todo_line("i1", 100, " Aufgabe ", Some("Notiz"), Some("2026-07-24")).unwrap();
    let rec: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(rec["id"], "i1");
    assert_eq!(rec["ts"], 100);
    assert_eq!(rec["text"], "Aufgabe");
    assert_eq!(rec["note"], "Notiz");
    assert_eq!(rec["due"], "2026-07-24");

    // Ohne note/due fehlen die Felder.
    let rec: serde_json::Value =
      serde_json::from_str(&todo_line("i1", 100, "x", None, None).unwrap()).unwrap();
    assert!(rec.get("note").is_none());
    assert!(rec.get("due").is_none());

    assert!(todo_line("i1", 100, "  ", None, None).is_err());
    assert!(todo_line("i1", 100, "x", None, Some("24.07.2026")).is_err());
  }

  #[test]
  fn update_ersetzt_nur_die_passende_zeile() {
    let raw = concat!(
      r#"{"id":"a","ts":100,"text":"alt","note":"n"}"#, "\n",
      r#"{"id":"b","ts":200,"text":"bleibt"}"#, "\n",
    );
    let out = replace_todo_line(raw, "a", "neu", None, Some("2026-07-24")).unwrap();
    let lines: Vec<serde_json::Value> =
      out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    // ID und ts bleiben, note ist weg, due ist neu.
    assert_eq!(lines[0]["id"], "a");
    assert_eq!(lines[0]["ts"], 100);
    assert_eq!(lines[0]["text"], "neu");
    assert!(lines[0].get("note").is_none());
    assert_eq!(lines[0]["due"], "2026-07-24");
    assert_eq!(lines[1]["text"], "bleibt");

    assert!(replace_todo_line(raw, "fehlt", "x", None, None).is_err());
  }
}

/// Öffnet den Commit-Dialog des Projekts. Wie beim Panel-Fenster: existiert
/// es schon, kommt es nach vorn; async aus demselben Grund (Fenster-Erzeugung
/// im synchronen Command klemmt auf dem GTK-Mainloop).
#[tauri::command]
pub async fn open_commit_window(app: AppHandle, project: String) -> Result<(), String> {
  let label = format!("commit-{project}");
  if let Some(w) = app.get_webview_window(&label) {
    let _ = w.set_focus();
    return Ok(());
  }
  // Das Theme geht als URL-Parameter mit: die Seite hat es damit sofort und
  // muss nicht erst die Projektliste holen (die je Projekt ein `pgrep`
  // kostet, nur um einen Theme-Namen zu erfahren).
  let cfg = project_config(&project)?;
  let theme = cfg.terminal.theme.unwrap_or_default();
  let url = format!("commit.html?project={project}&theme={theme}");
  open_project_window(&app, &project, &label, url, "Commit", (0.8, 0.85)).await
}
