//! Wurzelpfade der App und Home-Kontraktion/-Expansion.

use std::path::PathBuf;

/// Dateiname der Projekt-Registry unter ~/.config/ai-control.
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
    self.home.join(".config").join("ai-control")
  }

  pub(crate) fn projects_file(&self) -> PathBuf {
    self.config_dir().join(PROJECTS_FILE)
  }

  pub(crate) fn pools_dir(&self) -> PathBuf {
    self.config_dir().join("pools")
  }

  /// Ehemals gemeinsames Icons-Verzeichnis aller Projekte — Icons liegen jetzt
  /// im .ai-control/-Ordner des Projekts; hiervon liest nur noch die Migration.
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
  fn panels_dir(&self) -> PathBuf {
    self.config_dir().join("panels")
  }
}

/// Zweite Linie: Die Projekt-ID wird zum Dateinamen der Panel-Kanäle — ein
/// Wert mit Pfad-Bestandteilen (etwa aus einer verfälschten Registry oder
/// AI_CONTROL_PROJECT-Env) bricht hier laut ab, statt einen Pfad zu bilden.
fn checked(project: &str) -> &str {
  crate::domain::check_name(project).unwrap();
  project
}

/// Panel-Datei eines Projekts (Kanal Skill -> Panel). Der Pfad landet als
/// AI_CONTROL_PANEL in der PTY-Umgebung.
pub(crate) fn panel_file(project: &str) -> PathBuf {
  Paths::real().panels_dir().join(format!("{}.md", checked(project)))
}

/// Command-History eines Projekts (JSONL, anhängend — flüchtig, wird beim
/// Session-Start geleert). Der Pfad landet als AI_CONTROL_COMMANDS in der
/// PTY-Umgebung.
pub(crate) fn commands_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.commands.jsonl", checked(project)))
}

/// Suchtreffer-Datei eines Projekts (JSON, letzter search_archive-Aufruf —
/// flüchtig, wird beim Session-Start geleert). Der Pfad landet als
/// AI_CONTROL_SEARCH in der PTY-Umgebung.
pub(crate) fn search_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.search.json", checked(project)))
}

/// Wiki-Puffer eines Projekts (JSON, jeweils letzte Wiki-Seite bzw. letztes
/// geöffnetes Dokument — flüchtig, wird beim Session-Start geleert). Der Pfad
/// landet als AI_CONTROL_WIKI in der PTY-Umgebung.
pub(crate) fn wiki_file(project: &str) -> PathBuf {
  Paths::real()
    .panels_dir()
    .join(format!("{}.wiki.json", checked(project)))
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
