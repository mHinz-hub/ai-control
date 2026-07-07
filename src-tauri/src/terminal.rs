use base64::{engine::general_purpose::STANDARD, Engine};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

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
  let dir = crate::project_dir(&crate::Paths::real(), &project)?;
  if !dir.is_dir() {
    return Err(format!("Projektordner fehlt: {}", dir.display()));
  }
  let bundle_id = app.config().identifier.clone();
  app
    .run_on_main_thread(move || yield_activation_to_bundle(&bundle_id))
    .map_err(|e| e.to_string())?;
  let exe = std::env::current_exe().map_err(|e| e.to_string())?;
  std::process::Command::new(exe)
    .args(["--terminal", &project])
    .spawn()
    .map_err(|e| e.to_string())?;
  Ok(())
}

/// Tritt die Aktivierung an den nächsten startenden Prozess mit dieser
/// Bundle-ID ab (der Terminal-Prozess läuft unter derselben).
#[cfg(target_os = "macos")]
fn yield_activation_to_bundle(bundle_id: &str) {
  use objc2::MainThreadMarker;
  use objc2_app_kit::NSApplication;
  use objc2_foundation::NSString;
  let mtm =
    MainThreadMarker::new().expect("yield_activation_to_bundle läuft nicht auf dem Main-Thread");
  NSApplication::sharedApplication(mtm)
    .yieldActivationToApplicationWithBundleIdentifier(&NSString::from_str(bundle_id));
}

/// Linux kennt keine kooperative Aktivierungsabtretung.
#[cfg(target_os = "linux")]
fn yield_activation_to_bundle(_bundle_id: &str) {}

/// Selbst-Aktivierung des frisch gestarteten Terminal-Prozesses (Ready-Event);
/// die Gegenseite hat vorher per yield abgetreten.
#[cfg(target_os = "macos")]
pub fn activate_self(app: &AppHandle) {
  use objc2::MainThreadMarker;
  use objc2_app_kit::NSApplication;
  let mtm = MainThreadMarker::new().expect("activate_self läuft nicht auf dem Main-Thread");
  NSApplication::sharedApplication(mtm).activate();
  if let Some(window) = app.webview_windows().values().next() {
    window.set_focus().expect("Terminal-Fenster nicht fokussierbar");
  }
}

/// Linux: Fenster ohne NSApplication über die Tauri-API fokussieren.
#[cfg(target_os = "linux")]
pub fn activate_self(app: &AppHandle) {
  if let Some(window) = app.webview_windows().values().next() {
    window.set_focus().expect("Terminal-Fenster nicht fokussierbar");
  }
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

/// Setzt das Dock-Icon dieses Terminal-Prozesses aus einer PNG/ICNS-Datei.
#[cfg(target_os = "macos")]
pub fn set_dock_icon(path: &str) {
  use objc2::{AnyThread, MainThreadMarker};
  use objc2_app_kit::{NSApplication, NSImage};
  use objc2_foundation::NSString;

  let mtm = MainThreadMarker::new().expect("set_dock_icon läuft nicht auf dem Main-Thread");
  // Nicht ladbar (Datei weg, TCC-geschützter Ordner wie ~/Downloads): Terminal
  // ohne eigenes Dock-Icon starten statt den Prozess zu beenden.
  let Some(img) = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))
  else {
    eprintln!("Dock-Icon nicht ladbar, Terminal startet ohne: {path}");
    return;
  };
  // unsafe laut objc2-Signatur; das NSImage stammt aus einer Datei und ist gültig.
  unsafe {
    NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img));
  }
}

/// Linux kennt kein Dock-Icon pro Prozess.
#[cfg(target_os = "linux")]
pub fn set_dock_icon(_path: &str) {}

/// Holt das Terminal-Fenster eines laufenden Projekts in den Vordergrund:
/// aktiviert den Terminal-Prozess über seine PID. Aktivierung ist seit
/// macOS 14 kooperativ — die Tray-App tritt sie vorher ab.
#[cfg(target_os = "macos")]
pub fn focus_terminal(pid: u32) {
  use objc2::MainThreadMarker;
  use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};
  let Some(term) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
  else {
    eprintln!("Terminal-Prozess {pid} nicht gefunden");
    return;
  };
  let mtm = MainThreadMarker::new().expect("focus_terminal läuft nicht auf dem Main-Thread");
  NSApplication::sharedApplication(mtm).yieldActivationToApplication(&term);
  term.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
}

/// Linux: Fremdprozess-Fokus über die PID steht ohne NSRunningApplication
/// nicht zur Verfügung.
#[cfg(target_os = "linux")]
pub fn focus_terminal(_pid: u32) {}

/// Baut das Terminal-Fenster des Terminal-Prozesses. Die PTY entsteht erst,
/// wenn das Fenster geladen ist und `term_start` ruft — so gehen keine
/// Ausgaben verloren, bevor der Event-Listener steht.
pub fn build_window(
  app: &AppHandle,
  project: &str,
  cfg: &crate::TerminalConfig,
) -> tauri::Result<()> {
  let (r, g, b) = theme_background(cfg.theme.as_deref().unwrap_or_default());
  let title = cfg.title.as_deref().unwrap_or(project);
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
/// Pool-Verzeichnis als CLAUDE_CONFIG_DIR. zsh -i lädt die .zshrc (PATH, fnm).
#[tauri::command]
pub fn term_start(
  window: tauri::WebviewWindow,
  terminals: State<Terminals>,
  project: String,
  rows: u16,
  cols: u16,
) -> Result<(), String> {
  let paths = crate::Paths::real();
  let cwd = crate::project_dir(&paths, &project)?;

  let pty = native_pty_system()
    .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
    .map_err(|e| e.to_string())?;

  let mut cmd = CommandBuilder::new("/bin/zsh");
  cmd.args(["-ic", &crate::claude_command(&paths)]);
  cmd.cwd(&cwd);
  cmd.env("TERM", "xterm-256color");
  if let Some(pool_dir) = crate::project_pool_dir(&project)? {
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

  terminals.0.lock().unwrap().insert(
    window.label().to_string(),
    Session { writer, master: pty.master, killer },
  );
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
