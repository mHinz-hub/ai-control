//! Wurzelpfade der App und Home-Kontraktion/-Expansion.

use std::path::PathBuf;

/// Dateiname der Projekt-Registry unter ~/.config/ai-central.
const PROJECTS_FILE: &str = "projects.json";

/// Wurzelpfade; in Tests mit temporärem home instanziierbar.
pub(crate) struct Paths {
  pub(crate) home: PathBuf,
}

impl Paths {
  pub(crate) fn real() -> Self {
    Paths { home: crate::platform::home_dir() }
  }

  /// Default-Root: Alt-Layout (Discovery ohne Registry) und Ablageort neuer
  /// Projekte ohne gewählten Zielordner.
  pub(crate) fn projects_dir(&self) -> PathBuf {
    self.home.join("claude-projects")
  }

  pub(crate) fn config_dir(&self) -> PathBuf {
    self.home.join(".config").join("ai-central")
  }

  pub(crate) fn projects_file(&self) -> PathBuf {
    self.config_dir().join(PROJECTS_FILE)
  }

  pub(crate) fn pools_dir(&self) -> PathBuf {
    self.config_dir().join("pools")
  }

  /// Ehemals gemeinsames Icons-Verzeichnis aller Projekte — Icons liegen jetzt
  /// im .ai-central/-Ordner des Projekts; hiervon liest nur noch die Migration.
  pub(crate) fn icons_dir(&self) -> PathBuf {
    self.config_dir().join("icons")
  }

  pub(crate) fn pool_dir(&self, pool: &str) -> PathBuf {
    self.pools_dir().join(pool)
  }

  /// Claudes eigenes Konfigurationsverzeichnis — das, was ohne gesetztes
  /// CLAUDE_CONFIG_DIR benutzt wird. Ein Pool kann darauf verweisen, statt
  /// ein eigenes Verzeichnis anzulegen (siehe pool::pool_config_dir).
  pub(crate) fn default_claude_dir(&self) -> PathBuf {
    self.home.join(".claude")
  }

  /// Panel-Dateien pro Projekt: der Skill schreibt seinen Entwurf hier hinein,
  /// der Terminal-Prozess beobachtet die Datei und zeigt sie im Panel.
  pub(crate) fn panels_dir(&self) -> PathBuf {
    self.config_dir().join("panels")
  }
}

/// Zweite Linie: Die Projekt-ID wird zum Dateinamen der Panel-Kanäle — ein
/// Wert mit Pfad-Bestandteilen (etwa aus einer verfälschten Registry oder
/// AI_CENTRAL_PROJECT-Env) bricht hier laut ab, statt einen Pfad zu bilden.
fn checked(project: &str) -> &str {
  crate::domain::check_name(project).unwrap();
  project
}

/// Panel-Datei eines Projekts (Kanal Skill -> Panel). Der Pfad landet als
/// AI_CENTRAL_PANEL in der PTY-Umgebung.
pub(crate) fn panel_file(project: &str) -> PathBuf {
  Paths::real().panels_dir().join(format!("{}.md", checked(project)))
}

/// Quell-Verknüpfung des Panel-Inhalts: absoluter Pfad des Archiv-Dokuments,
/// aus dem der Dokument-Tab gerade geladen ist. Solange sie existiert,
/// schreibt jeder Editor-Commit den Body dorthin zurück; ein frischer Entwurf
/// (write_panel, Session-Start) entfernt sie. Liegt neben der Panel-Datei als
/// `<panel>.source`, damit der MCP-Server sie aus AI_CENTRAL_PANEL ableiten
/// kann.
pub(crate) fn panel_source_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.md.source", checked(project)))
}

/// Command-History eines Projekts (JSONL, anhängend — flüchtig, wird beim
/// Session-Start geleert). Der Pfad landet als AI_CENTRAL_COMMANDS in der
/// PTY-Umgebung.
pub(crate) fn commands_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.commands.jsonl", checked(project)))
}

/// Suchtreffer-Datei eines Projekts (JSON, letzter search_archive-Aufruf —
/// flüchtig, wird beim Session-Start geleert). Der Pfad landet als
/// AI_CENTRAL_SEARCH in der PTY-Umgebung.
pub(crate) fn search_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.search.json", checked(project)))
}

/// Archiv-Puffer eines Projekts (JSON, jeweils letzte Archiv-Seite bzw. letztes
/// geöffnetes Dokument — flüchtig, wird beim Session-Start geleert). Der Pfad
/// landet als AI_CENTRAL_ARCHIVE in der PTY-Umgebung.
pub(crate) fn archive_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.archive.json", checked(project)))
}

/// Puffer des Commit-Fensters: `show_commit` schreibt die Nachrichten-
/// Vorschläge (JSON, je Repo einer) hinein, der Watcher meldet das dem
/// Terminal-Fenster, das den Dialog öffnet — der ihn dann liest. Der Pfad
/// landet als AI_CENTRAL_COMMIT in der PTY-Umgebung.
pub(crate) fn commit_file(project: &str) -> PathBuf {
  Paths::real().panels_dir().join(format!("{}.commit", checked(project)))
}

/// Persistente ToDo-Liste eines Projekts (JSONL, anhängend — überlebt
/// Sessions; write_todos hängt an, Kachel-Löschen entfernt Zeilen). Der Pfad
/// landet als AI_CENTRAL_TODOS in der PTY-Umgebung.
///
/// Liegt im Projekt neben der Config und reist damit über git mit — anders als
/// die flüchtigen Panel-Kanäle, die maschinenlokal unter panels/ bleiben.
///
/// Ohne Registry-Eintrag gibt es keinen Projektordner; das ist derselbe
/// Programmierfehler wie ein Projektname mit Pfadanteilen und scheitert wie
/// dieser laut — die Aufrufer bekommen ihre ID aus Registry oder PTY-Umgebung.
pub(crate) fn todos_file(project: &str) -> PathBuf {
  todos_file_in(&Paths::real(), project)
}

pub(crate) fn todos_file_in(paths: &Paths, project: &str) -> PathBuf {
  crate::domain::registry::project_dir(paths, checked(project))
    .expect("ToDo-Liste eines nicht registrierten Projekts")
    .join(crate::domain::project::PROJECT_CONFIG_DIR)
    .join(TODOS_FILE)
}

/// Dateiname der ToDo-Liste im Projekt-Punktordner.
pub(crate) const TODOS_FILE: &str = "todos.jsonl";

/// Alter Ablageort der ToDo-Liste (maschinenlokal unter panels/), aus dem der
/// Session-Start einmalig ins Projekt umzieht.
pub(crate) fn legacy_todos_file(paths: &Paths, project: &str) -> PathBuf {
  paths
    .panels_dir()
    .join(format!("{}.todos.jsonl", checked(project)))
}

/// "~" bzw. "~/x" relativ zum Home auflösen; alles andere unverändert.
pub(crate) fn expand_home(paths: &Paths, p: &str) -> PathBuf {
  if p == "~" {
    return paths.home.clone();
  }
  match p.strip_prefix("~/") {
    Some(rest) => paths.home.join(rest),
    None => PathBuf::from(p),
  }
}

/// Pfad unterhalb von Home als "~/…" schreiben — Registry-Einträge im
/// Home-Bereich bleiben damit maschinenübergreifend stabil.
pub(crate) fn contract_home(paths: &Paths, p: &std::path::Path) -> String {
  match p.strip_prefix(&paths.home) {
    Ok(rest) => format!("~/{}", rest.display()),
    Err(_) => p.display().to_string(),
  }
}
