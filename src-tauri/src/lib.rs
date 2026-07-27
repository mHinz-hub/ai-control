//! Einstieg: Argument-Dispatch auf die drei Prozessrollen eines Binaries —
//! Haupt-App (Tray), Terminal-Prozess (`--terminal <projekt>`) und
//! MCP-stdio-Server (`--mcp-panel`). Fachlogik liegt in domain/, OS-Aufrufe
//! in platform/, Tauri-Verdrahtung in app.rs/commands.rs.

mod app;
mod commands;
mod domain;
mod mcp;
mod platform;
mod terminal;

/// Panic-Tracer: erzwingt bei jedem Panic einen vollständigen Backtrace
/// (unabhängig von RUST_BACKTRACE), gibt ihn klar abgegrenzt auf stderr aus und
/// schreibt ihn zusätzlich in eine kopierbare Datei (Pfad pro Prozess, damit
/// Haupt- und Terminal-Prozesse sich nicht überschreiben).
fn install_panic_tracer() {
  std::panic::set_hook(Box::new(|info| {
    let bt = std::backtrace::Backtrace::force_capture();
    let dump = format!(
      "\n========== ai-central PANIC (pid {}) ==========\n{info}\n\n{bt}\n========== END ==========\n",
      std::process::id()
    );
    eprintln!("{dump}");
    let path =
      std::env::temp_dir().join(format!("ai-central-panic-{}.log", std::process::id()));
    if std::fs::write(&path, &dump).is_ok() {
      eprintln!("Backtrace kopierbar in: {}", path.display());
    }
  }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  install_panic_tracer();

  let mut args = std::env::args().skip(1);
  let first = args.next();

  // `app --mcp-panel`: reiner MCP-stdio-Server (write_panel), kein Tauri/GTK.
  // Von claude als Tool-Server gestartet.
  if first.as_deref() == Some("--mcp-panel") {
    mcp::run_mcp_panel();
    return;
  }

  // Frisch aus dem DMG oder aus Downloads gestartet: einmal anbieten, sich
  // nach ~/Applications zu holen. Nur die Haupt-App fragt — Terminal-Prozesse
  // starten aus demselben Bundle, das dann längst am Ziel liegt.
  if first.is_none() {
    platform::offer_move_to_applications(&domain::paths::Paths::real());
  }

  // generate_context! darf pro Crate nur einmal expandieren (_EMBED_INFO_PLIST).
  let context = tauri::generate_context!();
  match first.as_deref() {
    // `app --terminal <projekt>`: eigener Prozess pro Terminal-Fenster,
    // damit jedes Terminal ein eigenes Dock-Icon bekommt.
    Some("--terminal") => {
      let project = args.next().expect("--terminal braucht einen Projektnamen");
      // Eigene Wayland-app_id pro Terminal-Prozess -> eigener Dock-Eintrag
      // (muss vor dem GTK-Init stehen; No-op außerhalb Linux).
      platform::set_app_id(&format!("aicentral-{project}"));
      let icon = domain::project::project_config(&project)
        .expect("Projekt-Config nicht lesbar")
        .terminal
        .icon
        .map(|i| {
          domain::project::resolve_icon_path(&domain::paths::Paths::real(), &project, &i)
            .expect("Projekt nicht registriert")
            .to_string_lossy()
            .into_owned()
        });
      app::terminal_builder(project)
        .build(context)
        .expect("error while building tauri application")
        // Das Dock-Icon erst nach dem App-Start setzen: in setup() gesetzt
        // überschreibt macOS es beim Anlegen des Dock-Tiles wieder.
        .run(move |app, event| match event {
          tauri::RunEvent::Ready => {
            if let Some(icon) = icon.as_deref() {
              platform::set_dock_icon(icon);
            }
            platform::activate_self(app);
          }
          // Klick aufs Dock-Icon: das Terminal-Fenster nach vorn. macOS
          // unternimmt von sich aus nichts, solange irgendein Fenster des
          // Prozesses sichtbar ist — mit offenem Commit- oder Archiv-Fenster
          // bliebe der Klick also wirkungslos.
          // `Reopen` gibt es nur auf macOS/iOS, daher der cfg-Gate.
          #[cfg(any(target_os = "macos", target_os = "ios"))]
          tauri::RunEvent::Reopen { .. } => {
            use tauri::Manager;
            platform::activate_self(app);
            for (label, w) in app.webview_windows() {
              if label.starts_with("term-") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
              }
            }
          }
          _ => {}
        });
    }
    _ => {
      // Feste Wayland-app_id fürs Hauptfenster (vor GTK-Init) — GNOME ordnet
      // dem offenen Fenster über ai-central.desktop das App-Icon zu.
      platform::set_app_id("ai-central");
      app::main_builder()
        .run(context)
        .expect("error while running tauri application");
    }
  }
}
