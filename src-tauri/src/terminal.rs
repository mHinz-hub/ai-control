use base64::{engine::general_purpose::STANDARD, Engine};
use portable_pty::{native_pty_system, ChildKiller, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::domain::paths::{commands_file, panel_file, search_file, wiki_file, Paths};
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

  // Panel-Kanal: leere Datei anlegen (definierter Startzustand für den
  // Watcher) und ihren Pfad als AI_CONTROL_PANEL in die PTY geben — der Skill
  // schreibt seinen Entwurf dorthin.
  let panel_path = panel_file(&project);
  if let Some(parent) = panel_path.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  let _ = std::fs::write(&panel_path, "");

  // Command-Kanal: History-Datei (JSONL) leeren — die Befehls-History ist
  // flüchtig und gilt nur für diese Session; write_commands hängt Records an.
  let commands_path = commands_file(&project);
  let _ = std::fs::write(&commands_path, "");

  // Such-Kanal: Treffer-Datei leeren — search_archive schreibt den jeweils
  // letzten Suchlauf hinein, der Watcher zieht ihn als Kacheln ins Panel.
  let search_path = search_file(&project);
  let _ = std::fs::write(&search_path, "");

  // Wiki-Kanal: Puffer leeren — show_archive und wiki_open schreiben die
  // jeweils aktuelle Wiki-Seite (JSON) hinein.
  let wiki_path = wiki_file(&project);
  let _ = std::fs::write(&wiki_path, "");

  let mut cmd = crate::platform::shell_command(&claude_command(&paths));
  cmd.cwd(&cwd);
  // Aus der App-Umgebung geerbte Anthropic-Credentials rausnehmen — ein
  // ANTHROPIC_API_KEY sticht sonst den apiKeyHelper des Pools. Variablen, die
  // erst das Shell-Profil der Login-Shell setzt, erreicht das nicht.
  cmd.env_remove("ANTHROPIC_API_KEY");
  cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
  cmd.env("TERM", "xterm-256color");
  cmd.env("AI_CONTROL_PANEL", &panel_path);
  cmd.env("AI_CONTROL_COMMANDS", &commands_path);
  cmd.env("AI_CONTROL_SEARCH", &search_path);
  cmd.env("AI_CONTROL_WIKI", &wiki_path);
  cmd.env("AI_CONTROL_PROJECT", &project);
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

  spawn_file_watcher(window.app_handle().clone(), panel_path, "panel-update");
  spawn_file_watcher(
    window.app_handle().clone(),
    commands_path,
    "commands-update",
  );
  spawn_file_watcher(window.app_handle().clone(), search_path, "search-update");
  spawn_file_watcher(window.app_handle().clone(), wiki_path, "wiki-update");

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

/// Aktueller Panel-Inhalt (Erstbefüllung eines gerade geöffneten Panel-Fensters).
#[tauri::command]
pub fn panel_read(project: String) -> String {
  std::fs::read_to_string(panel_file(&project)).unwrap_or_default()
}

/// Aktuelle Command-History (JSONL; Erstbefüllung der Kachel-Ansicht).
#[tauri::command]
pub fn commands_read(project: String) -> String {
  std::fs::read_to_string(commands_file(&project)).unwrap_or_default()
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

/// Schreibt den Panel-Inhalt (Titel-Edit im Panel). Der Watcher meldet die
/// Änderung als `panel-update` an alle Fenster.
#[tauri::command]
pub fn panel_set(project: String, text: String) -> Result<(), String> {
  std::fs::write(panel_file(&project), text).map_err(|e| e.to_string())
}

/// Letzter Suchlauf (JSON; Erstbefüllung der Treffer-Ansicht).
#[tauri::command]
pub fn search_read(project: String) -> String {
  std::fs::read_to_string(search_file(&project)).unwrap_or_default()
}

/// Lädt ein Archiv-Dokument in den Dokument-Puffer (Treffer-Klick in der
/// Suche) — ohne Frontmatter-Block, wie ein frischer Entwurf. Der Watcher
/// meldet den neuen Inhalt als `panel-update`.
#[tauri::command]
pub fn panel_load(project: String, path: String) -> Result<(), String> {
  let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
  let body = crate::domain::archive::strip_frontmatter(&text);
  std::fs::write(panel_file(&project), body).map_err(|e| e.to_string())
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

/// Aktueller Wiki-Puffer (JSON; Erstbefüllung der Wiki-Ansicht).
#[tauri::command]
pub fn wiki_read(project: String) -> String {
  std::fs::read_to_string(wiki_file(&project)).unwrap_or_default()
}

/// Öffnet ein Wiki-Ziel (Klick auf einen `[[…]]`-Link oder Suchtreffer):
/// `tag:x` → Schlagwort-Seite, `tag:` → Archiv-Übersicht, sonst
/// Dokument-Auflösung über den Index. Der `tag:`-Namensraum ist damit dort
/// interpretiert, wo archive_page ihn erzeugt — nicht im Frontend. Schreibt
/// den Wiki-Puffer; der Watcher meldet ihn als `wiki-update`.
#[tauri::command]
pub fn wiki_open(project: String, name: String) -> Result<(), String> {
  let home = crate::domain::archive::require_archive_home(&project)?;
  let json = match name.strip_prefix("tag:") {
    Some(tag) => serde_json::to_string(&crate::domain::archive_index::archive_page(
      &home,
      (!tag.is_empty()).then_some(tag),
    )?),
    None => serde_json::to_string(&crate::domain::archive_index::wiki_doc(&home, &name)?),
  }
  .map_err(|e| e.to_string())?;
  std::fs::write(wiki_file(&project), json).map_err(|e| e.to_string())
}

/// Löst das Panel in ein eigenes Fenster ab. Existiert es schon, kommt es nach
/// vorn. `panel-detached` blendet das angedockte Panel im Terminal-Fenster aus.
/// Async, weil Fenster-Erzeugung aus einem synchronen Command auf dem
/// GTK-Mainloop klemmen kann (Tauri-Vorgabe für window create in Commands).
#[tauri::command]
pub async fn open_panel_window(app: AppHandle, project: String) -> Result<(), String> {
  let label = format!("panel-{project}");
  if let Some(w) = app.get_webview_window(&label) {
    let _ = w.set_focus();
    return Ok(());
  }
  let cfg = project_config(&project)?;
  let (r, g, b) = theme_background(cfg.terminal.theme.as_deref().unwrap_or_default());
  let title = cfg
    .terminal
    .title
    .as_deref()
    .or(cfg.name.as_deref())
    .unwrap_or(project.as_str());
  let builder = WebviewWindowBuilder::new(
    &app,
    &label,
    WebviewUrl::App(format!("panel.html?project={project}").into()),
  )
  .title(format!("{title} — Dokument"))
  .inner_size(480.0, 640.0)
  .background_color(tauri::window::Color(r, g, b, 0xff));

  // Linux/GNOME: keine GTK-Deko — eigene Kopfleiste in panel.html, wie beim
  // Terminal-Fenster.
  #[cfg(target_os = "linux")]
  let builder = builder.decorations(false);

  builder.build().map_err(|e| e.to_string())?;
  app.emit("panel-detached", ()).map_err(|e| e.to_string())?;
  Ok(())
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
