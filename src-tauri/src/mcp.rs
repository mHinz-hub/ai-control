//! Minimaler MCP-stdio-Server (`app --mcp-panel`), den claude als Tool-Server
//! startet. Er stellt genau ein Tool bereit: `write_panel(text)` schreibt den
//! Text in die Panel-Datei aus `AI_CENTRAL_PANEL` (dieselbe Env, die der
//! Terminal-Prozess der PTY mitgibt und die claude an seine MCP-Kinder vererbt).
//! Der bestehende Datei-Watcher im Terminal-Prozess zieht den Inhalt ins Panel.
//!
//! Protokoll: JSON-RPC 2.0, newline-getrennt (ein Objekt pro Zeile).

use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub fn run_mcp_panel() {
  let stdin = std::io::stdin();
  let mut stdout = std::io::stdout();
  let mut reader = stdin.lock();
  let mut line = String::new();

  loop {
    line.clear();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
      break; // stdin geschlossen -> claude hat den Server beendet
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
      continue;
    }
    let Ok(req) = serde_json::from_str::<Value>(trimmed) else {
      continue;
    };

    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let result = handle(method, &req);

    // Nur Requests (mit id) bekommen eine Antwort; Notifications nicht.
    let (Some(id), Some(result)) = (id, result) else {
      continue;
    };
    let resp = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    if writeln!(stdout, "{resp}").is_err() || stdout.flush().is_err() {
      break;
    }
  }
}

/// Liefert das `result` für einen Request, oder None für Notifications/Unbekanntes.
fn handle(method: &str, req: &Value) -> Option<Value> {
  match method {
    "initialize" => {
      // Vom Client vorgeschlagene Protokollversion übernehmen.
      let protocol = req["params"]["protocolVersion"]
        .as_str()
        .unwrap_or("2024-11-05");
      Some(json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "text-panel", "version": env!("CARGO_PKG_VERSION") },
      }))
    }
    // Die Tool-Liste kommt aus der Modul-Registry: nur Tools der im Projekt
    // aktiven Module. Eine laufende Session behält ihre Liste vom Start —
    // nachträglich abgeschaltete Module fängt der Guard in call_tool ab.
    "tools/list" => Some(json!({
      "tools": active_modules()
        .iter()
        .flat_map(|m| m.mcp_tools)
        .map(|name| tool_def(name))
        .collect::<Vec<Value>>(),
    })),
    "tools/call" => Some(call_tool(req)),
    "ping" => Some(json!({})),
    _ => None,
  }
}

/// Aktive Module des Projekts aus AI_CENTRAL_PROJECT. Ohne lesbare
/// Projekt-Config (Terminal außerhalb ai-central) gelten die
/// Registry-Defaults — die Tools erscheinen, ihre Aufrufe scheitern dann wie
/// bisher an der fehlenden Env.
fn active_modules() -> Vec<&'static crate::domain::modules::ModuleDesc> {
  let project = std::env::var("AI_CENTRAL_PROJECT").unwrap_or_default();
  crate::domain::modules::active_in(&crate::domain::paths::Paths::real(), &project)
    .unwrap_or_else(|_| {
      crate::domain::modules::MODULES
        .iter()
        .filter(|m| m.default_enabled)
        .collect()
    })
}

/// MCP-Definition eines Tools. Welche davon gelistet werden, entscheidet die
/// Modul-Registry (active_modules); jeder Name aus MODULES muss hier einen
/// Arm haben.
fn tool_def(name: &str) -> Value {
  match name {
    "write_panel" => json!({
          "name": "write_panel",
          "description":
            "Legt einen längeren Entwurf (ADR, E-Mail, Dokument, Spezifikation, \
             Commit-Message, Textbaustein) im ai-central-Panel neben dem Terminal \
             ab, wo er mit der Maus selektierbar und über einen Button kopierbar \
             ist. Statt den Text zusätzlich als Fließtext auszugeben, dieses Tool \
             aufrufen und im Chat nur kurz bestätigen. Für eine bestehende Datei \
             IMMER `path` statt `text` übergeben — der Server liest die Datei \
             selbst von der Platte, ohne dass ihr Inhalt generiert werden muss. \
             `path` muss im Projekt-, Arbeits- oder Archiv-Ordner liegen \
             (reguläre Datei, max. 2 MB).",
          "inputSchema": {
            "type": "object",
            "properties": {
              "text": {
                "type": "string",
                "description": "Vollständiger Entwurf als Markdown-Rohtext.",
              },
              "path": {
                "type": "string",
                "description":
                  "Statt `text`: Pfad einer vorhandenen Datei, deren Inhalt ins \
                   Panel geladen wird. Genau eines von beiden angeben.",
              }
            },
          },
        }),
    "write_commands" => json!({
          "name": "write_commands",
          "description":
            "Listet Shell-Befehle, die der Nutzer ausführen soll, als kopierbare \
             Kacheln im ai-central-Panel. IMMER nutzen, wenn ein Befehl für den \
             Nutzer bestimmt ist — statt ihn als Codeblock in den Chat zu \
             schreiben; im Chat nur kurz einordnen. Die Befehle werden an die \
             Befehls-History der Session angehängt (flüchtig, startet mit \
             jeder Session leer).",
          "inputSchema": {
            "type": "object",
            "properties": {
              "commands": {
                "type": "array",
                "description": "Befehle in Ausführungsreihenfolge.",
                "items": {
                  "type": "object",
                  "properties": {
                    "cmd": {
                      "type": "string",
                      "description": "Der Befehl, exakt so ausführbar.",
                    },
                    "note": {
                      "type": "string",
                      "description": "Optionale Kurznotiz, was der Befehl tut.",
                    }
                  },
                  "required": ["cmd"],
                },
              }
            },
            "required": ["commands"],
          },
        }),
    "show_commands" => json!({
          "name": "show_commands",
          "description":
            "Zeigt die Befehls-History im ai-central-Panel (Kachel-Ansicht mit \
             allen bisher ausgegebenen Befehlen), ohne etwas anzuhängen. Nutzen, \
             wenn der Nutzer die Befehlsliste sehen will („zeig die Befehle“, \
             „zeige die Befehlsliste“).",
          "inputSchema": { "type": "object", "properties": {} },
        }),
    "show_commit" => json!({
          "name": "show_commit",
          "description":
            "Öffnet den Commit-Dialog des Projekts als eigenes Fenster: alle \
             Git-Repos des Projekts mit ihren geänderten Dateien, Diff und \
             Push-Vorprüfung. Nutzen, wenn der Nutzer committen will („mach \
             den Commit-Dialog auf“, /commit). `messages` füllt die \
             Nachrichtenfelder vor — pro Repo einen eigenen Vorschlag aus \
             dessen anstehenden Änderungen; der Nutzer kann sie im Dialog \
             ändern.",
          "inputSchema": {
            "type": "object",
            "properties": {
              "messages": {
                "type": "array",
                "description":
                  "Je ein Vorschlag pro Repo mit Änderungen.",
                "items": {
                  "type": "object",
                  "properties": {
                    "repo": {
                      "type": "string",
                      "description":
                        "Ordnername der Repo-Wurzel, wie ihn der Dialog links \
                         zeigt (z. B. `ai-central`).",
                    },
                    "message": {
                      "type": "string",
                      "description":
                        "Commit-Nachricht dieses Repos: eine Betreffzeile, \
                         sachlich, was die Änderung dort tut.",
                    },
                  },
                  "required": ["repo", "message"],
                },
              },
            },
          },
        }),
    "write_todos" => json!({
          "name": "write_todos",
          "description":
            "Hängt Aufgaben an die persistente ToDo-Liste des Projekts an; \
             sie erscheinen als Kacheln im ToDo-Tab des ai-central-Panels und \
             überleben die Session. Nutzen, wenn der Nutzer etwas auf die \
             ToDo-Liste setzen will („auf die Liste“, „als ToDo merken“). \
             `due` optional als ISO-Datum YYYY-MM-DD — das Panel zeigt die \
             Fälligkeit als Ampel.",
          "inputSchema": {
            "type": "object",
            "properties": {
              "todos": {
                "type": "array",
                "description": "Aufgaben in Listenreihenfolge.",
                "items": {
                  "type": "object",
                  "properties": {
                    "text": {
                      "type": "string",
                      "description": "Die Aufgabe, knapp formuliert.",
                    },
                    "note": {
                      "type": "string",
                      "description": "Optionale Kurznotiz.",
                    },
                    "due": {
                      "type": "string",
                      "description": "Optionales Fälligkeitsdatum (YYYY-MM-DD).",
                    }
                  },
                  "required": ["text"],
                },
              }
            },
            "required": ["todos"],
          },
        }),
    "show_todos" => json!({
          "name": "show_todos",
          "description":
            "Zeigt die ToDo-Liste im ai-central-Panel (Kachel-Ansicht), ohne \
             etwas hinzuzufügen. Nutzen, wenn der Nutzer die ToDos sehen will \
             („zeig die ToDos“).",
          "inputSchema": { "type": "object", "properties": {} },
        }),
    "archive_panel" => json!({
          "name": "archive_panel",
          "description":
            "Archiviert den aktuell im Panel liegenden Entwurf dauerhaft als \
             Markdown-Datei im Archiv-Home des Projekts. Auf Wunsch nutzen \
             (Nutzer sagt etwa „archiviere das“). Beim Archivieren `folder`, \
             `description` und `tags` mitgeben — einmalige Kuratierung im Moment \
             des Archivierens, landet im Frontmatter. Ist kein Archiv-Home \
             konfiguriert, meldet das Tool das zurück; gesetzt wird es in den \
             Projekt-Einstellungen oder im Panel.",
          "inputSchema": {
            "type": "object",
            "properties": {
              "title": {
                "type": "string",
                "description":
                  "Titel des Dokuments; ohne ihn gilt die erste Überschrift \
                   des Entwurfs.",
              },
              "folder": {
                "type": "string",
                "description":
                  "Optionaler Unterordner im Archiv-Home, relativ (z. B. \
                   `konzepte/panel`). Wird angelegt.",
              },
              "description": {
                "type": "string",
                "description": "Einzeiler zum Inhalt fürs Frontmatter.",
              },
              "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Schlagwörter fürs Frontmatter (kurze Slugs).",
              }
            },
          },
        }),
    "show_archive" => json!({
          "name": "show_archive",
          "description":
            "Zeigt die Archiv-Übersicht des Projekts als Wiki-Seite im Panel: \
             Dokumente nach Ordnern gruppiert, mit Beschreibungen und \
             klickbaren Schlagwort-Links. Mit `tag` stattdessen die Seite eines \
             Schlagworts. Nutzen, wenn der Nutzer das Archiv sehen will \
             („zeig das Archiv“).",
          "inputSchema": {
            "type": "object",
            "properties": {
              "tag": {
                "type": "string",
                "description": "Optional: Seite dieses Schlagworts statt der Übersicht.",
              }
            },
          },
        }),
    "search_archive" => json!({
          "name": "search_archive",
          "description":
            "Volltext-Suche über das Panel-Archiv des Projekts (FTS5-Syntax: \
             Wörter, \"Phrasen\", Präfix*). Die Treffer erscheinen als Kacheln \
             im Panel; das Tool liefert sie zusätzlich mit Pfad und Snippet \
             zurück. Nutzen, wenn der Nutzer im Archiv suchen will („such im \
             Archiv nach …“).",
          "inputSchema": {
            "type": "object",
            "properties": {
              "query": {
                "type": "string",
                "description": "Suchanfrage (FTS5-Syntax).",
              },
              "tag": {
                "type": "string",
                "description": "Optional: auf ein Schlagwort einengen.",
              }
            },
          },
        }),
    other => unreachable!("tool_def ohne Definition: {other}"),
  }
}

fn call_tool(req: &Value) -> Value {
  let name = req["params"]["name"].as_str().unwrap_or("");
  // Guard für Sessions, deren Tool-Liste älter ist als die Projekt-Config:
  // Tools mittlerweile abgeschalteter Module werden abgewiesen.
  if let Some(m) = crate::domain::modules::by_tool(name) {
    if !active_modules().iter().any(|a| a.id == m.id) {
      return err(format!("Modul „{}“ ist in diesem Projekt abgeschaltet.", m.id));
    }
  }
  match name {
    "write_panel" => call_write(req),
    "write_commands" => call_write_commands(req),
    "show_commands" => call_show_commands(),
    "show_commit" => call_show_commit(req),
    "write_todos" => call_write_todos(req),
    "show_todos" => call_show_todos(),
    "archive_panel" => call_archive(req),
    "search_archive" => call_search(req),
    "show_archive" => call_show_archive(req),
    other => err(format!("Unbekanntes Tool: {other}")),
  }
}

/// Erfolgs-Antwort eines Tool-Aufrufs (ein Text-Content).
fn ok(text: String) -> Value {
  json!({ "content": [{ "type": "text", "text": text }] })
}

/// Fehler-Antwort eines Tool-Aufrufs.
fn err(text: String) -> Value {
  json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// Pfad aus der PTY-Umgebung; fehlt die Variable, läuft das Terminal
/// außerhalb von ai-central.
fn env_path(var: &str) -> Result<String, Value> {
  std::env::var(var)
    .map_err(|_| err(format!("{var} nicht gesetzt (Terminal außerhalb ai-central).")))
}

/// Schreibt Text in die Panel-Datei aus AI_CENTRAL_PANEL. Ein frischer
/// Entwurf löst die Quell-Verknüpfung des Dokument-Tabs (`<panel>.source`,
/// gesetzt beim Öffnen eines Archiv-Dokuments) — Edits gehören danach wieder
/// nur dem Panel, nicht der zuletzt geöffneten Archiv-Datei.
fn write_panel_text(text: &str) -> Result<(), Value> {
  let path = env_path("AI_CENTRAL_PANEL")?;
  std::fs::write(&path, text).map_err(|e| err(format!("Panel-Datei nicht schreibbar: {e}")))?;
  match std::fs::remove_file(format!("{path}.source")) {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(err(format!("Quell-Verknüpfung nicht gelöst: {e}"))),
  }
}

fn call_write(req: &Value) -> Value {
  let args = &req["params"]["arguments"];
  // `path` lädt eine vorhandene Datei serverseitig — der schnelle Weg, ohne
  // dass das Modell den Inhalt Token für Token als `text` generieren muss.
  // Der Zugriff ist auf Projekt-, Arbeits- und Archiv-Ordner begrenzt
  // (read_for_panel_in): das Tool ist promptfrei freigegeben und darf das
  // Read-Permission-Modell von Claude Code nicht umgehen.
  let (text, ok_msg) = match args["path"].as_str() {
    Some(src) => {
      let project = std::env::var("AI_CENTRAL_PROJECT").unwrap_or_default();
      let paths = crate::domain::paths::Paths::real();
      match crate::domain::project::read_for_panel_in(&paths, &project, src) {
        Ok(content) => (content, "Datei ins Panel geladen."),
        Err(e) => return err(format!("Datei nicht geladen: {e}")),
      }
    }
    None => (
      args["text"].as_str().unwrap_or("").to_string(),
      "Entwurf ins Panel geschrieben.",
    ),
  };
  match write_panel_text(&text) {
    Ok(()) => ok(ok_msg.into()),
    Err(e) => e,
  }
}

/// Merkt, ob dieser Server-Prozess (= eine claude-Session) schon einen
/// Session-Marker in die History geschrieben hat.
static SESSION_MARKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn call_write_commands(req: &Value) -> Value {
  // Jeder Eintrag bekommt beim Schreiben eine stabile ID — das Panel löscht
  // darüber, statt Positionen über Watcher-Latenz und Fenstergrenzen zu
  // reichen (der frühere Index+Text-Abgleich war ein Provisorium).
  let mut commands = req["params"]["arguments"]["commands"].clone();
  if let Some(arr) = commands.as_array_mut() {
    for c in arr {
      if let Some(obj) = c.as_object_mut() {
        obj.insert("id".into(), json!(uuid::Uuid::new_v4().to_string()));
      }
    }
  }
  let count = commands.as_array().map(Vec::len).unwrap_or(0);
  let path = match env_path("AI_CENTRAL_COMMANDS") {
    Ok(path) => path,
    Err(e) => return e,
  };
  let ts = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0);
  // Ein Record pro Aufruf; der erste Aufruf der Session bekommt einen
  // Marker-Record davor (Session-Trenner in der Kachel-Ansicht).
  let mut out = String::new();
  if !SESSION_MARKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
    out.push_str(&json!({ "ts": ts, "session": true }).to_string());
    out.push('\n');
  }
  out.push_str(&json!({ "ts": ts, "commands": commands }).to_string());
  out.push('\n');
  let res = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&path)
    .and_then(|mut f| std::io::Write::write_all(&mut f, out.as_bytes()));
  match res {
    Ok(()) => ok(format!("{count} Befehl(e) als Kacheln im Panel abgelegt.")),
    Err(e) => err(format!("Command-History nicht schreibbar: {e}")),
  }
}

/// Öffnet eine Kachel-Ansicht, ohne der Datei etwas hinzuzufügen: mtime
/// anfassen genügt — der Watcher im Terminal-Prozess meldet die Datei als
/// Update, das Panel schaltet auf den zugehörigen Tab um.
fn touch_buffer(env: &str, ok_msg: &str, err_ctx: &str) -> Value {
  let path = match env_path(env) {
    Ok(path) => path,
    Err(e) => return e,
  };
  let res = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&path)
    .and_then(|f| f.set_modified(std::time::SystemTime::now()));
  match res {
    Ok(()) => ok(ok_msg.into()),
    Err(e) => err(format!("{err_ctx}: {e}")),
  }
}

fn call_show_commands() -> Value {
  touch_buffer(
    "AI_CENTRAL_COMMANDS",
    "Befehls-History im Panel geöffnet.",
    "Command-History nicht erreichbar",
  )
}

/// Öffnet den Commit-Dialog. Die Signaldatei trägt die Vorschläge als JSON
/// (`[{"repo": …, "message": …}]`): der Watcher liefert ihren Inhalt mit dem
/// Öffnen-Event, das Fenster verteilt sie auf die Repos. Ohne `messages`
/// bleibt sie leer.
fn call_show_commit(req: &Value) -> Value {
  let path = match env_path("AI_CENTRAL_COMMIT") {
    Ok(path) => path,
    Err(e) => return e,
  };
  let content = match req["params"]["arguments"].get("messages") {
    Some(m) => m.to_string(),
    None => String::new(),
  };
  match std::fs::write(&path, content) {
    Ok(()) => ok("Commit-Dialog geöffnet.".into()),
    Err(e) => err(format!("Commit-Dialog nicht erreichbar: {e}")),
  }
}

fn call_show_todos() -> Value {
  touch_buffer(
    "AI_CENTRAL_TODOS",
    "ToDo-Liste im Panel geöffnet.",
    "ToDo-Liste nicht erreichbar",
  )
}

/// Prüft ein Fälligkeitsdatum: YYYY-MM-DD, Monat 01–12, Tag 01–31.
pub(crate) fn valid_due(due: &str) -> bool {
  let b = due.as_bytes();
  if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
    return false;
  }
  let (y, m, d) = (
    due[0..4].parse::<u16>(),
    due[5..7].parse::<u8>(),
    due[8..10].parse::<u8>(),
  );
  matches!((y, m, d), (Ok(_), Ok(m), Ok(d)) if (1..=12).contains(&m) && (1..=31).contains(&d))
}

/// Hängt ToDos an die persistente Liste (JSONL, ein ToDo pro Zeile); jedes
/// bekommt eine stabile ID fürs Kachel-Löschen. Der Watcher zieht den neuen
/// Stand als `todos-update` ins Panel.
fn call_write_todos(req: &Value) -> Value {
  let path = match env_path("AI_CENTRAL_TODOS") {
    Ok(path) => path,
    Err(e) => return e,
  };
  let Some(todos) = req["params"]["arguments"]["todos"].as_array() else {
    return err("todos fehlt".into());
  };
  let ts = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0);
  let mut out = String::new();
  for todo in todos {
    let text = todo["text"].as_str().unwrap_or("").trim();
    if text.is_empty() {
      return err("ToDo ohne text".into());
    }
    let mut rec = json!({
      "id": uuid::Uuid::new_v4().to_string(),
      "ts": ts,
      "text": text,
    });
    if let Some(note) = todo["note"].as_str() {
      rec["note"] = json!(note);
    }
    if let Some(due) = todo["due"].as_str() {
      if !valid_due(due) {
        return err(format!("ungültiges due-Datum: {due} (erwartet YYYY-MM-DD)"));
      }
      rec["due"] = json!(due);
    }
    out.push_str(&rec.to_string());
    out.push('\n');
  }
  let res = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&path)
    .and_then(|mut f| std::io::Write::write_all(&mut f, out.as_bytes()));
  match res {
    Ok(()) => ok(format!("{} ToDo(s) auf der Liste.", todos.len())),
    Err(e) => err(format!("ToDo-Liste nicht schreibbar: {e}")),
  }
}

/// `dir` gibt es hier bewusst nicht mehr: Das Setzen des Archiv-Homes vergibt
/// über `add_archive_permission` dauerhafte Rechte in der settings.json des
/// Projekts (additionalDirectories + Edit-Allow) — das bleibt der UI mit
/// Nutzer-Dialog vorbehalten, nicht einem Tool-Argument des Modells.
fn call_archive(req: &Value) -> Value {
  let project = std::env::var("AI_CENTRAL_PROJECT").unwrap_or_default();
  let args = &req["params"]["arguments"];
  let meta = crate::domain::archive::ArchiveMeta {
    title: args["title"].as_str().map(str::to_string),
    folder: args["folder"].as_str().map(str::to_string),
    description: args["description"].as_str().map(str::to_string),
    tags: args["tags"]
      .as_array()
      .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
      .unwrap_or_default(),
  };
  match crate::domain::archive::archive_panel_content(&project, None, &meta) {
    Ok(path) => ok(format!("Archiviert: {}", path.display())),
    Err(e) => err(format!(
      "Nicht archiviert: {e}. Archiv-Ordner im Panel oder in den Projekt-Einstellungen wählen."
    )),
  }
}

/// Archiv-Übersicht bzw. Schlagwort-Seite generieren und in den Wiki-Puffer
/// schreiben — der Watcher zieht sie als Wiki-Ansicht ins Panel.
fn call_show_archive(req: &Value) -> Value {
  let project = std::env::var("AI_CENTRAL_PROJECT").unwrap_or_default();
  let home = match crate::domain::archive::require_archive_home(&project) {
    Ok(home) => home,
    Err(e) => return err(e),
  };
  let tag = req["params"]["arguments"]["tag"].as_str();
  // Erst die Invarianten des Notizmodells herstellen — sonst zeigt die
  // Übersicht Ordner ohne Knotentext und Dokumente ohne ID, und ein Klick
  // darauf öffnet nichts oder das Falsche.
  let display =
    match crate::domain::project::display_name_in(&crate::domain::paths::Paths::real(), &project) {
      Ok(d) => d,
      Err(e) => return err(e),
    };
  if let Err(e) = crate::domain::archive_ops::ensure_node_texts(&home, &display) {
    return err(e);
  }
  if let Err(e) = crate::domain::archive_ops::ensure_ids(&home) {
    return err(e);
  }
  let page = match crate::domain::archive_index::archive_page(&home, tag) {
    Ok(page) => page,
    Err(e) => return err(e),
  };
  let path = match env_path("AI_CENTRAL_WIKI") {
    Ok(path) => path,
    Err(e) => return e,
  };
  let json = match serde_json::to_string(&page) {
    Ok(json) => json,
    Err(e) => return err(e.to_string()),
  };
  match std::fs::write(&path, json) {
    Ok(()) => ok(match tag {
      Some(t) => format!("Schlagwort-Seite #{t} im Panel."),
      None => "Archiv-Übersicht im Panel.".to_string(),
    }),
    Err(e) => err(format!("Wiki-Datei nicht schreibbar: {e}")),
  }
}

/// Volltext-Suche übers Archiv: Treffer als JSON in die Suchtreffer-Datei
/// (der Watcher zieht sie als Kacheln ins Panel) und als Text zurück an claude.
fn call_search(req: &Value) -> Value {
  let project = std::env::var("AI_CENTRAL_PROJECT").unwrap_or_default();
  let home = match crate::domain::archive::require_archive_home(&project) {
    Ok(home) => home,
    Err(e) => return err(e),
  };
  let args = &req["params"]["arguments"];
  let query = args["query"].as_str().unwrap_or("");
  let tag = args["tag"].as_str();
  let hits = match crate::domain::archive_search::search(&home, query, tag, 20) {
    Ok(hits) => hits,
    Err(e) => return err(format!("Suche fehlgeschlagen: {e}")),
  };
  let search_path = match env_path("AI_CENTRAL_SEARCH") {
    Ok(path) => path,
    Err(e) => return e,
  };
  let payload = json!({
    "query": query,
    "tag": tag,
    "home": home.display().to_string(),
    "hits": hits,
  });
  if let Err(e) = std::fs::write(&search_path, payload.to_string()) {
    return err(format!("Suchtreffer-Datei nicht schreibbar: {e}"));
  }
  let list: Vec<String> = hits
    .iter()
    .map(|h| format!("- {} — {} ({})", h.relpath, h.title, h.snippet))
    .collect();
  ok(format!(
    "{} Treffer, als Kacheln im Panel. Archiv: {}\n{}",
    hits.len(),
    home.display(),
    list.join("\n")
  ))
}
