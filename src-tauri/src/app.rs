//! Tauri-Verdrahtung: Haupt-App (Tray, main-/popup-Fenster, Watcher) und
//! Terminal-Prozess. Das Popup ist auf allen OS dasselbe HTML-Fenster; nur
//! der Klick-Trigger kommt aus platform::init_tray (Anchor-Kontrakt).

use crate::domain::paths::Paths;
use crate::domain::pool::provision_pools_for_panel;
use crate::domain::project::project_config;
use crate::domain::watcher::spawn_session_watcher;
use crate::platform::Anchor;
use crate::{commands, terminal};

/// Sperrt das Auto-Hide des Popups bei Fokusverlust nach dem Anzeigen, bis das
/// Popup einmal Focused(true) gemeldet hat (KDE feuert nach show/set_focus ein
/// spurioses Focused(false)). Als Tauri-State geführt.
#[cfg(target_os = "linux")]
pub(crate) struct PopupBlurGuard(pub(crate) std::sync::Arc<std::sync::atomic::AtomicBool>);

/// Popup an den Klick-Koordinaten platzieren, in den Monitor geklemmt.
#[cfg(target_os = "linux")]
fn position_popup(w: &tauri::WebviewWindow, x: i32, y: i32) {
  use tauri::{PhysicalPosition, PhysicalSize};
  let size = w.outer_size().unwrap_or_else(|_| PhysicalSize::new(320, 300));
  let (mon_pos, mon_size) = w
    .current_monitor()
    .ok()
    .flatten()
    .map(|m| (*m.position(), *m.size()))
    .unwrap_or((PhysicalPosition::new(0, 0), PhysicalSize::new(1920, 1080)));
  // Rechte Kante an den Klick; oben/unten ergibt sich aus dem Klemmen an die
  // Monitorkante (Panel oben -> unter dem Icon, Panel unten -> darüber).
  let max_x = mon_pos.x + mon_size.width as i32 - size.width as i32;
  let max_y = mon_pos.y + mon_size.height as i32 - size.height as i32;
  let px = (x - size.width as i32).clamp(mon_pos.x, max_x.max(mon_pos.x));
  let py = y.clamp(mon_pos.y, max_y.max(mon_pos.y));
  let _ = w.set_position(PhysicalPosition::new(px, py));
}

/// Popup zeigen — ein Codepfad für alle OS, nur die Platzierung folgt dem
/// Anchor aus dem Tray-Trigger.
pub(crate) fn show_popup(app: &tauri::AppHandle, anchor: Anchor) {
  use tauri::Manager;
  let win = app.clone();
  let _ = app.run_on_main_thread(move || {
    let Some(w) = win.get_webview_window("popup") else {
      return;
    };
    match anchor {
      // Nativer Tray (macOS/Windows): rechte Popup-Kante an der rechten
      // Icon-Kante; macOS unter dem Menüleisten-Icon, Windows über der Taskbar.
      Anchor::IconRect { rect, popup_below } => {
        let scale = w.scale_factor().unwrap_or(1.0);
        let pos = rect.position.to_physical::<f64>(scale);
        let size = rect.size.to_physical::<f64>(scale);
        let win_w = w.outer_size().map(|s| s.width as f64).unwrap_or(320.0);
        let x = (pos.x + size.width - win_w).max(0.0);
        let y = if popup_below {
          pos.y + size.height
        } else {
          let win_h = w.outer_size().map(|s| s.height as f64).unwrap_or(300.0);
          (pos.y - win_h).max(0.0)
        };
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
      }
      // SNI (KDE/XFCE/Cinnamon): Activate-Koordinaten, wenn der Host welche
      // mitgibt (Cinnamon); sonst Zeigerposition — der Zeiger sitzt beim
      // Klick auf dem Icon. Unter Wayland liefert cursor_position (0,0).
      Anchor::Click { x, y } => {
        #[cfg(not(target_os = "linux"))]
        let _ = (x, y);
        #[cfg(target_os = "linux")]
        {
          // Gelöst wird die Sperre vom ersten Focused(true) im Event-Handler.
          if let Some(g) = win.try_state::<PopupBlurGuard>() {
            g.0.store(true, std::sync::atomic::Ordering::SeqCst);
          }
          if (x, y) != (0, 0) {
            position_popup(&w, x, y);
          } else {
            match win.cursor_position() {
              Ok(p) => position_popup(&w, p.x as i32, p.y as i32),
              Err(_) => {
                let _ = w.center();
              }
            }
          }
        }
      }
      // GNOME-Extension/KWin: der Compositor positioniert selbst.
      Anchor::Managed => {}
    }
    let _ = w.show();
    let _ = w.set_focus();
  });
}

/// Popup verstecken (GNOME-Toggle über D-Bus Hide()).
pub(crate) fn hide_popup(app: &tauri::AppHandle) {
  use tauri::Manager;
  let win = app.clone();
  let _ = app.run_on_main_thread(move || {
    if let Some(w) = win.get_webview_window("popup") {
      let _ = w.hide();
    }
  });
}

/// Haupt-App: reine Tray-App ohne Dock-Eintrag.
pub(crate) fn main_builder() -> tauri::Builder<tauri::Wry> {
  tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // Nur Tray-Icon in der Menüleiste, kein Dock-Eintrag.
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);

      // Alt-Layout migrieren: ai-central.json → .ai-central/config.json,
      // Pool-Zuordnung in die Registry, Icons in den Projekt-Config-Ordner.
      crate::domain::project::migrate_layout_in(&Paths::real())?;

      // Session-Watcher: synct bei Session-Ende (Prozess verschwindet).
      spawn_session_watcher();

      // Panel-MCP-Server + Tool-Freigabe in alle Pools legen (auch bestehende),
      // damit write_panel in jedem CLAUDE_CONFIG_DIR ohne Rückfrage bereitsteht.
      provision_pools_for_panel(&Paths::real());

      // Alle pro-Terminal-.desktop-Dateien neu schreiben + verwaiste entfernen,
      // damit sie da sind, bevor ein Terminal startet (No-op außerhalb Linux).
      crate::platform::sync_all_desktops(&Paths::real());

      // Fenster im Code statt in tauri.conf.json, damit der Terminal-Prozess
      // (gleiches Binary, gleiche Config) kein main-Fenster anlegt.
      let main_win = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
        .title("aiCentral")
        .inner_size(800.0, 600.0)
        // Fenster-Icon (_NET_WM_ICON) — deckt X11-DEs wie XFCE direkt ab,
        // unabhängig von der .desktop-Zuordnung; auf Windows das Titel-/Taskbar-Icon.
        .icon(tauri::image::Image::from_bytes(include_bytes!(
          "../icons/128x128.png"
        ))?)?
        .visible(false);
      // Wie die Terminal-Fenster: eigener Header als Titelleiste. macOS behält die
      // Ampel (Overlay), Linux ist dekorationslos (eigene Fensterknöpfe im Header).
      #[cfg(target_os = "macos")]
      let main_win = main_win
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
      #[cfg(target_os = "linux")]
      let main_win = main_win.decorations(false);
      main_win.build()?;

      // Rahmenloses Popup-Fenster — dieselbe Optik auf allen Plattformen. Nur
      // der Weg zum Klick unterscheidet sich (platform::init_tray).
      tauri::WebviewWindowBuilder::new(app, "popup", tauri::WebviewUrl::App("popup.html".into()))
        .title("ai-central-popup")
        .inner_size(320.0, 300.0)
        .decorations(false)
        .visible(false)
        .transparent(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .build()?;

      #[cfg(target_os = "linux")]
      {
        use tauri::Manager;
        app.manage(PopupBlurGuard(std::sync::Arc::new(
          std::sync::atomic::AtomicBool::new(false),
        )));

        // Muffin (Cinnamon) mappt das Popup ohne Fokus: das set_focus aus
        // show_popup läuft, bevor der WM das Fenster gemappt hat, und wird
        // von der Focus-Stealing-Prevention verworfen — ohne initialen Fokus
        // greift das Blur-Schließen nie. Deshalb nach jedem Map erneut
        // präsentieren: present() besorgt sich unter X11 den echten
        // Server-Timestamp, damit gewährt der WM den Fokus. Nur für den
        // SNI-Pfad unter X11 — auf GNOME zeigt die Extension das Popup, dort
        // hat das present() beim Map den Klick-Weg gebrochen (Ubuntu 17.07).
        use gtk::prelude::*;
        let gtk_win = app.get_webview_window("popup").unwrap().gtk_window()?;
        if gtk_win.display().type_().name().starts_with("GdkX11")
          && !crate::platform::is_gnome()
        {
          gtk_win.connect_map_event(|w, _| {
            w.present();
            glib::Propagation::Proceed
          });
        }
      }

      // Der gesamte Tray-Kontrakt: Icon zeigen, Klick als Anchor melden —
      // was dann passiert, entscheidet ausschließlich dieser Code.
      crate::platform::init_tray(
        app.handle(),
        crate::platform::TrayCallbacks {
          show: Box::new(|app, anchor| show_popup(app, anchor)),
          hide: Box::new(hide_popup),
        },
      )?;

      Ok(())
    })
    // Hauptfenster schließen versteckt nur; Beenden geht übers Tray-Menü.
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        window.hide().unwrap();
      }
      // Popup schließt bei Fokusverlust.
      if let tauri::WindowEvent::Focused(focused) = event {
        if window.label() == "popup" {
          // KDE feuert direkt nach show/set_focus ein spurioses Focused(false),
          // vor dem echten Fokus. Die Sperre steht ab dem Anzeigen und fällt mit
          // dem ersten Focused(true) — das spuriose Blur läuft ins Leere.
          // GNOME/macOS setzen die Sperre nie.
          #[cfg(target_os = "linux")]
          {
            use tauri::Manager;
            if let Some(g) = window.try_state::<PopupBlurGuard>() {
              if *focused {
                g.0.store(false, std::sync::atomic::Ordering::SeqCst);
              } else if g.0.load(std::sync::atomic::Ordering::SeqCst) {
                return;
              }
            }
          }
          if !focused {
            let _ = window.hide();
          }
        }
      }
    })
    .invoke_handler(invoke_handlers())
}

/// Eine Kommando-Liste für beide Prozesse (Hauptfenster und Terminal).
/// Zwei getrennte Listen hatten den Settings-Dialog gebrochen, weil die
/// Archiv-Kommandos nur im Terminal-Prozess registriert waren.
fn invoke_handlers() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
  tauri::generate_handler![
    commands::list_projects,
    commands::create_project_full,
    commands::add_project,
    commands::delete_preview,
    commands::delete_project_scoped,
    commands::project_work_dirs,
    commands::set_project_dir,
    commands::add_work_dir,
    commands::remove_work_dir,
    commands::list_pools,
    commands::create_oauth_pool,
    commands::create_reference_pool,
    commands::default_config_dir,
    commands::create_apikey_pool,
    commands::rename_pool,
    commands::delete_pool,
    commands::assign_pool,
    commands::unassign_pool,
    commands::set_terminal_config,
    commands::project_icon,
    commands::pool_label,
    commands::usage_stats,
    commands::stop_project,
    commands::restart_project,
    commands::start_or_focus_cmd,
    commands::open_main_window,
    commands::quit_app,
    commands::sync_setting,
    commands::set_sync_setting,
    commands::terminal_font_size,
    commands::set_terminal_font_size,
    commands::link_pool_runtime,
    commands::oauth_login,
    commands::keychain_status,
    commands::set_apikey,
    commands::panel_archive_dir_cmd,
    commands::panel_archive_cmd,
    commands::panel_title_cmd,
    commands::set_archive_home_cmd,
    commands::change_archive_home_cmd,
    commands::clear_archive_home_cmd,
    commands::archive_docs_cmd,
    commands::reveal_path_cmd,
    commands::git_repos,
    commands::git_diff,
    commands::git_push_check,
    commands::git_commit,
    commands::spellcheck_lang,
    commands::enabled_modules,
    commands::module_registry,
    commands::set_module,
    terminal::open_terminal,
    terminal::term_start,
    terminal::term_log,
    terminal::term_write,
    terminal::term_resize,
    terminal::buffer_read,
    terminal::commands_delete,
    terminal::todos_delete,
    terminal::todos_add,
    terminal::todos_update,
    terminal::panel_set,
    terminal::search_run,
    terminal::panel_load,
    terminal::wiki_open,
    terminal::archive_read,
    terminal::epub_open,
    terminal::archive_write,
    terminal::archive_set_title,
    terminal::archive_folders,
    terminal::panel_save_as,
    terminal::archive_delete,
    terminal::archive_create_folder,
    terminal::archive_create_doc,
    terminal::archive_create_html,
    terminal::open_panel_window,
    terminal::open_commit_window
  ]
}

/// Terminal-Prozess: ein Fenster mit eigener PTY; Activation-Policy bleibt
/// Regular, dadurch Dock-Icon und Cmd-Tab-Eintrag pro Terminal.
pub(crate) fn terminal_builder(project: String) -> tauri::Builder<tauri::Wry> {
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_clipboard_manager::init())
    // Seiten der entpackten Bücher für den ePub-Viewer im Archiv-Fenster.
    // Eigenes Protokoll statt des Asset-Protokolls: dessen Adressen tragen
    // den ganzen Dateipfad als EIN kodiertes Segment, womit die relativen
    // Verweise der Buchseiten (Bilder, CSS, Schriften) ins Leere liefen.
    .register_uri_scheme_protocol("epub", |_ctx, request| {
      match crate::domain::epub::serve(request.uri().path()) {
        Ok((bytes, mime)) => tauri::http::Response::builder()
          .header(tauri::http::header::CONTENT_TYPE, mime)
          .body(bytes)
          .unwrap(),
        Err(e) => tauri::http::Response::builder()
          .status(404)
          .body(e.into_bytes())
          .unwrap(),
      }
    })
    .manage(terminal::Terminals::default())
    .setup(move |app| {
      let cfg = project_config(&project)?;
      terminal::build_window(app.handle(), &project, &cfg)?;
      Ok(())
    })
    // Fenster zu → PTY-Session abräumen; danach endet der Prozess. Das
    // abgelöste Panel-Fenster hat keine PTY: sein Schließen dockt das Panel
    // wieder an (panel-attached), der Prozess läuft weiter.
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::Destroyed = event {
        if window.label().starts_with("panel-") {
          use tauri::{Emitter, Manager};
          let _ = window.app_handle().emit("panel-window-closed", ());
        } else {
          // Hauptfenster des Projekts zu: das Archiv-Fenster geht mit —
          // sonst hielte es den Terminal-Prozess allein am Leben.
          use tauri::Manager;
          for (label, w) in window.app_handle().webview_windows() {
            if label.starts_with("panel-") {
              let _ = w.close();
            }
          }
          terminal::close(window);
        }
      }
    })
    .invoke_handler(invoke_handlers())
}
