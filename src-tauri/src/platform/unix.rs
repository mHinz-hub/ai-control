//! Unix-Gemeinsames (macOS + Linux): Prozesse, Shell, Dateirechte, Symlinks.

use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn home_dir() -> PathBuf {
  PathBuf::from(std::env::var("HOME").expect("HOME nicht gesetzt"))
}

/// Kommando für die PTY: Login-Shell aus $SHELL (-l) baut den PATH aus ihren
/// Profil-Dateien auf — shell-agnostisch.
pub(crate) fn shell_command(cmd: &str) -> portable_pty::CommandBuilder {
  let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
  let mut c = portable_pty::CommandBuilder::new(&shell);
  c.args(["-lc", cmd]);
  c
}

/// PIDs der eingebauten Terminal-Prozesse (`app --terminal <projekt>`).
pub(crate) fn terminal_pids(project: &str) -> Vec<u32> {
  let out = Command::new("pgrep")
    .args(["-f", "--", &format!("--terminal {project}$")])
    .output();
  match out {
    Ok(o) => String::from_utf8_lossy(&o.stdout)
      .lines()
      .filter_map(|l| l.trim().parse().ok())
      .collect(),
    Err(_) => Vec::new(),
  }
}

/// SIGTERM auf die exakte PID. Der Prozesstod schließt den PTY-Master,
/// claude bekommt HUP und endet — wie beim Schließen des Fensters.
pub(crate) fn kill_terminal(pid: u32) -> Result<(), String> {
  let out = Command::new("kill")
    .arg(pid.to_string())
    .output()
    .map_err(|e| e.to_string())?;
  if out.status.success() {
    Ok(())
  } else {
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
  }
}

/// Secret-Datei von Anfang an mit 0600 anlegen: `create_new` + Modus statt
/// schreiben-und-nachträglich-abdichten — so gibt es kein Fenster, in dem die
/// Datei mit umask-Rechten lesbar ist, und ein untergeschobener Symlink wird
/// nicht gefolgt (die alte Datei wird vorher entfernt, `create_new` lehnt
/// Vorhandenes ab).
pub(crate) fn write_secret_file(path: &Path, content: &str) -> Result<(), String> {
  use std::io::Write;
  use std::os::unix::fs::OpenOptionsExt;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
  }
  match std::fs::remove_file(path) {
    Ok(()) => {}
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(format!("{}: {e}", path.display())),
  }
  let mut f = std::fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .open(path)
    .map_err(|e| format!("{}: {e}", path.display()))?;
  f.write_all(content.as_bytes())
    .map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn symlink(target: &Path, link: &Path) -> Result<(), String> {
  std::os::unix::fs::symlink(target, link).map_err(|e| format!("{}: {e}", link.display()))
}
