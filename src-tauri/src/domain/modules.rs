//! Modul-Registry: benennt die Funktionsblöcke der App (Entwurf, Befehle,
//! Archiv, …) mit ihren Beiträgen — MCP-Tools und Puffer-Kanäle. Die Tabelle
//! ist die eine Quelle für „was ist in diesem Projekt an?": Das Frontend baut
//! seine Tabs daraus (`enabled_modules`), der MCP-Server seine Tool-Liste
//! (tools/list) und den Guard vor dem Dispatch (tools/call).
//!
//! Die Abwahl eines Moduls ist Konfiguration, keine Sicherheitsgrenze —
//! Commands bleiben registriert, die Capability-Manifeste unverändert;
//! dieselbe Linie wie bei den Pools (Konfiguration trennen, nicht Zugriff).

use crate::domain::paths::Paths;

/// Ein Puffer-Kanal eines Moduls: MCP-Server oder Command schreibt die Datei,
/// der Watcher im Terminal-Prozess meldet neuen Inhalt als Event an die
/// Panel-Fenster.
pub(crate) struct BufferDesc {
  /// Schlüssel für `buffer_read` (Erstbefüllung der Ansicht).
  pub(crate) id: &'static str,
  /// Env-Variable, unter der die PTY den Dateipfad an claudes MCP-Kinder gibt.
  pub(crate) env: &'static str,
  /// Event an die Panel-Fenster bei Dateiänderung.
  pub(crate) event: &'static str,
  /// Pufferdatei des Projekts.
  pub(crate) file: fn(&str) -> std::path::PathBuf,
  /// Dateiname-Suffix (`<projekt>.<suffix>`) der maschinenlokalen Kanäle unter
  /// panels/ — muss zu `file` passen (Test buffer_datei_passt_zum_suffix);
  /// Grundlage der Löschvorschau. Leer bei Puffern, die im Projekt liegen.
  pub(crate) suffix: &'static str,
  /// Puffer liegt im Projekt (`.ai-central/`) statt unter panels/: er reist
  /// mit dem Repo und stirbt mit dem Punktordner, nicht als Panel-Kanal.
  pub(crate) in_project: bool,
  /// Persistente Puffer überleben Sessions — der Session-Start legt sie nur
  /// an, statt sie zu leeren.
  pub(crate) persistent: bool,
}

pub(crate) struct ModuleDesc {
  pub(crate) id: &'static str,
  /// Kern-Module sind nicht abschaltbar; ein Config-Eintrag wird ignoriert.
  pub(crate) core: bool,
  /// Aktiv ohne Eintrag in der Projekt-Config.
  pub(crate) default_enabled: bool,
  /// Modul setzt ein konfiguriertes Archiv-Home voraus — ohne Home fallen
  /// seine Tabs im Frontend weg. Seine MCP-Tools bleiben absichtlich
  /// gelistet: Das Home kann mitten in der Session gesetzt werden, und die
  /// Tools melden das fehlende Home selbst verständlich zurück.
  pub(crate) requires_archive: bool,
  pub(crate) mcp_tools: &'static [&'static str],
  pub(crate) buffers: &'static [BufferDesc],
}

pub(crate) const MODULES: &[ModuleDesc] = &[
  ModuleDesc {
    id: "draft",
    core: true,
    default_enabled: true,
    requires_archive: false,
    mcp_tools: &["write_panel"],
    buffers: &[BufferDesc {
      id: "panel",
      env: "AI_CENTRAL_PANEL",
      event: "panel-update",
      file: crate::domain::paths::panel_file,
      suffix: "md",
      in_project: false,
      persistent: false,
    }],
  },
  ModuleDesc {
    id: "commands",
    core: false,
    default_enabled: true,
    requires_archive: false,
    mcp_tools: &["write_commands", "show_commands"],
    buffers: &[BufferDesc {
      id: "commands",
      env: "AI_CENTRAL_COMMANDS",
      event: "commands-update",
      file: crate::domain::paths::commands_file,
      suffix: "commands.jsonl",
      in_project: false,
      persistent: false,
    }],
  },
  ModuleDesc {
    id: "todo",
    core: false,
    // Opt-in: der Tab erscheint erst, wenn das Modul im Projekt gewählt ist.
    default_enabled: false,
    requires_archive: false,
    mcp_tools: &["write_todos", "show_todos"],
    buffers: &[BufferDesc {
      id: "todos",
      env: "AI_CENTRAL_TODOS",
      event: "todos-update",
      file: crate::domain::paths::todos_file,
      suffix: "todos.jsonl",
      in_project: true,
      persistent: true,
    }],
  },
  ModuleDesc {
    id: "commit",
    core: false,
    default_enabled: true,
    requires_archive: false,
    mcp_tools: &["show_commit"],
    // Der Puffer trägt die Nachrichten-Vorschläge (JSON, je Repo einer); das
    // Fenster liest sie beim Öffnen und hört danach auf das Event. Ein
    // eigenes Fenster statt eines Panel-Tabs, deshalb gibt es keine Ansicht
    // in der Frontend-Registry.
    buffers: &[BufferDesc {
      id: "commit",
      env: "AI_CENTRAL_COMMIT",
      event: "commit-open",
      file: crate::domain::paths::commit_file,
      suffix: "commit",
      in_project: false,
      persistent: false,
    }],
  },
  ModuleDesc {
    id: "archive",
    core: false,
    default_enabled: true,
    requires_archive: true,
    mcp_tools: &["archive_panel", "show_archive", "search_archive"],
    buffers: &[
      BufferDesc {
        id: "search",
        env: "AI_CENTRAL_SEARCH",
        event: "search-update",
        file: crate::domain::paths::search_file,
        suffix: "search.json",
        in_project: false,
        persistent: false,
      },
      BufferDesc {
        id: "archive",
        env: "AI_CENTRAL_ARCHIVE",
        event: "archive-update",
        file: crate::domain::paths::archive_file,
        suffix: "archive.json",
        in_project: false,
        persistent: false,
      },
    ],
  },
];

/// Aktive Module des Projekts: MODULES, gefiltert durch die `modules`-
/// Abweichungen der Projekt-Config. Kern-Module sind immer dabei.
pub(crate) fn active_in(
  paths: &Paths,
  project: &str,
) -> Result<Vec<&'static ModuleDesc>, String> {
  let cfg = crate::domain::project::read_project_config_in(paths, project)?;
  Ok(
    MODULES
      .iter()
      .filter(|m| {
        m.core || cfg.modules.get(m.id).copied().unwrap_or(m.default_enabled)
      })
      .collect(),
  )
}

/// Modul, das dieses MCP-Tool beiträgt.
pub(crate) fn by_tool(tool: &str) -> Option<&'static ModuleDesc> {
  MODULES.iter().find(|m| m.mcp_tools.contains(&tool))
}

/// Registry-Zeile für den Settings-Dialog: `enabled` ist der effektive
/// Config-Schalter (ohne `requires_archive` — die Pfadfrage zeigt der
/// Dialog daneben an).
#[derive(serde::Serialize)]
pub(crate) struct ModuleInfo {
  pub(crate) id: &'static str,
  pub(crate) core: bool,
  pub(crate) enabled: bool,
}

pub(crate) fn module_infos_in(
  paths: &Paths,
  project: &str,
) -> Result<Vec<ModuleInfo>, String> {
  let cfg = crate::domain::project::read_project_config_in(paths, project)?;
  Ok(
    MODULES
      .iter()
      .map(|m| ModuleInfo {
        id: m.id,
        core: m.core,
        enabled: m.core || cfg.modules.get(m.id).copied().unwrap_or(m.default_enabled),
      })
      .collect(),
  )
}

/// Schreibt die Modul-Abweichung in die Projekt-Config; der Default-Wert
/// löscht den Eintrag (nur Abweichungen werden gespeichert).
pub(crate) fn set_module_in(
  paths: &Paths,
  project: &str,
  module: &str,
  enabled: bool,
) -> Result<(), String> {
  let m = MODULES
    .iter()
    .find(|m| m.id == module)
    .ok_or_else(|| format!("unbekanntes Modul: {module}"))?;
  if m.core {
    return Err(format!("Modul {module} ist nicht abschaltbar"));
  }
  let mut cfg = crate::domain::project::read_project_config_in(paths, project)?;
  if enabled == m.default_enabled {
    cfg.modules.remove(module);
  } else {
    cfg.modules.insert(module.to_string(), enabled);
  }
  crate::domain::project::write_project_config_in(paths, project, &cfg)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::testutil::{create_project, tmp_paths};

  fn ids(mods: &[&'static ModuleDesc]) -> Vec<&'static str> {
    mods.iter().map(|m| m.id).collect()
  }

  #[test]
  fn defaults_alle_aktiv() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    assert_eq!(
      ids(&active_in(&p, "proj").unwrap()),
      vec!["draft", "commands", "commit", "archive"]
    );
  }

  #[test]
  fn config_schaltet_ab_kern_bleibt() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    let cfg = p
      .projects_dir()
      .join("proj")
      .join(".ai-central")
      .join("config.json");
    std::fs::write(
      &cfg,
      r#"{"id": "proj", "name": "proj", "modules": {"commands": false, "draft": false}}"#,
    )
    .unwrap();
    // commands ist ab; draft ignoriert den Eintrag (Kern-Modul).
    assert_eq!(
      ids(&active_in(&p, "proj").unwrap()),
      vec!["draft", "commit", "archive"]
    );
  }

  #[test]
  fn unbekanntes_projekt_scheitert() {
    let p = tmp_paths();
    assert!(active_in(&p, "fehlt").is_err());
  }

  #[test]
  fn todo_ist_opt_in() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    set_module_in(&p, "proj", "todo", true).unwrap();
    assert_eq!(
      ids(&active_in(&p, "proj").unwrap()),
      vec!["draft", "commands", "todo", "commit", "archive"]
    );
  }

  #[test]
  fn buffer_datei_passt_zum_suffix() {
    for b in MODULES.iter().flat_map(|m| m.buffers).filter(|b| !b.in_project) {
      let name = (b.file)("proj");
      let name = name.file_name().unwrap().to_str().unwrap();
      assert_eq!(name, format!("proj.{}", b.suffix));
    }
  }

  /// Puffer im Projekt liegen im Punktordner neben der Config — nicht unter
  /// panels/, sonst reisten sie nicht mit dem Repo.
  #[test]
  fn projekt_puffer_liegt_im_punktordner() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    for b in MODULES.iter().flat_map(|m| m.buffers).filter(|b| b.in_project) {
      let f = crate::domain::paths::todos_file_in(&p, "proj");
      assert_eq!(f.file_name().unwrap(), crate::domain::paths::TODOS_FILE);
      assert_eq!(
        f.parent().unwrap().file_name().unwrap(),
        crate::domain::project::PROJECT_CONFIG_DIR
      );
      assert!(!f.starts_with(p.panels_dir()), "{} liegt unter panels/", b.id);
    }
  }

  #[test]
  fn set_module_roundtrip() {
    let p = tmp_paths();
    create_project(&p, "proj").unwrap();
    set_module_in(&p, "proj", "commands", false).unwrap();
    assert_eq!(
      ids(&active_in(&p, "proj").unwrap()),
      vec!["draft", "commit", "archive"]
    );
    assert!(!module_infos_in(&p, "proj").unwrap()[1].enabled);
    // Zurück auf den Default löscht den Eintrag statt `true` zu speichern.
    set_module_in(&p, "proj", "commands", true).unwrap();
    let cfg = crate::domain::project::read_project_config_in(&p, "proj").unwrap();
    assert!(cfg.modules.is_empty());
    // Kern-Modul und unbekannte ID scheitern laut.
    assert!(set_module_in(&p, "proj", "draft", false).is_err());
    assert!(set_module_in(&p, "proj", "gibtsnicht", true).is_err());
  }

  #[test]
  fn tool_zuordnung() {
    assert_eq!(by_tool("write_panel").unwrap().id, "draft");
    assert_eq!(by_tool("search_archive").unwrap().id, "archive");
    assert!(by_tool("unbekannt").is_none());
  }

  #[test]
  fn buffer_und_tool_ids_eindeutig() {
    let mut tools: Vec<&str> = MODULES.iter().flat_map(|m| m.mcp_tools).copied().collect();
    let mut buffers: Vec<&str> =
      MODULES.iter().flat_map(|m| m.buffers).map(|b| b.id).collect();
    let (t, b) = (tools.len(), buffers.len());
    tools.sort();
    tools.dedup();
    buffers.sort();
    buffers.dedup();
    assert_eq!(tools.len(), t);
    assert_eq!(buffers.len(), b);
  }
}
