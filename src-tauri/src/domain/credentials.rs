//! Credential-Ablage der Pools. Der ApikeyStore abstrahiert den nativen
//! Secret-Store (macOS-Keychain / Secret Service / Credential Manager) —
//! die Implementierungen liegen in platform/.

use sha2::{Digest, Sha256};

use crate::domain::paths::Paths;

/// Service-Name der Einträge; Account ist die Pool-ID. Unter Linux legt die
/// keyring-Crate die Attribute service/username an — der apiKeyHelper liest
/// mit denselben Attributen über secret-tool.
pub(crate) const APIKEY_SERVICE: &str = "ai-control-apikey";

/// Fehler-Sentinel an die UI: Store nicht verfügbar und Datei-Ablage (noch)
/// nicht erlaubt — die UI fragt dann nach und wiederholt mit allow_file.
pub(crate) const KEYCHAIN_UNAVAILABLE: &str = "keychain-unavailable";

/// Ablage der API-Keys im nativen Secret-Store. Die Key-Datei im Pool-Ordner
/// bleibt Fallback, wenn der Store beim Schreiben nicht verfügbar ist.
pub(crate) trait ApikeyStore {
  fn set(&self, pool: &str, key: &str) -> Result<(), String>;
  fn has(&self, pool: &str) -> Result<bool, String>;
  fn delete(&self, pool: &str) -> Result<(), String>;
}

/// Keychain-Service-Name eines CLAUDE_CONFIG_DIR: claude legt pro Verzeichnis
/// einen suffixierten Eintrag an, Suffix = erste 8 Hex-Zeichen von SHA-256
/// über den Verzeichnispfad. Claudes Default-Verzeichnis läuft ohne gesetzte
/// Variable und trägt deshalb den unsuffixierten Eintrag — ein Pool, der
/// darauf verweist, erbt damit einen bestehenden Login.
pub(crate) fn keychain_service_for(paths: &Paths, dir: &std::path::Path) -> String {
  if dir == paths.default_claude_dir() {
    return "Claude Code-credentials".into();
  }
  let hash = Sha256::digest(dir.to_string_lossy().as_bytes());
  let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
  format!("Claude Code-credentials-{}", &hex[..8])
}

/// Keychain-Service-Name eines Pools über sein aufgelöstes Config-Verzeichnis.
pub(crate) fn keychain_service(paths: &Paths, pool: &str) -> Result<String, String> {
  Ok(keychain_service_for(
    paths,
    &crate::domain::pool::pool_config_dir(paths, pool)?,
  ))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  /// Referenzwert vom echten System: privateDefault → 096c4ef9
  /// (verifiziert 2026-07-03 gegen den von claude angelegten Eintrag).
  #[test]
  fn keychain_service_suffix() {
    let p = Paths { home: PathBuf::from("/Users/marcus.hinz") };
    assert_eq!(
      keychain_service_for(&p, &p.pool_dir("privateDefault")),
      "Claude Code-credentials-096c4ef9"
    );
  }

  /// Claudes Default-Verzeichnis läuft ohne CLAUDE_CONFIG_DIR und trägt den
  /// unsuffixierten Eintrag (am echten System 2026-07-22 gegengeprüft).
  #[test]
  fn keychain_service_default_dir_ohne_suffix() {
    let p = Paths { home: PathBuf::from("/Users/marcus.hinz") };
    assert_eq!(
      keychain_service_for(&p, &p.default_claude_dir()),
      "Claude Code-credentials"
    );
  }

  #[test]
  fn apikey_helper_kette_referenz() {
    let dir = PathBuf::from("/pools/abc");
    let cmd = crate::platform::apikey_helper_command(&dir, "abc");
    assert!(cmd.ends_with("|| cat '/pools/abc/apikey'"));
    #[cfg(target_os = "macos")]
    assert!(cmd.starts_with(
      "security find-generic-password -w -s ai-control-apikey -a abc 2>/dev/null"
    ));
    #[cfg(target_os = "linux")]
    assert!(cmd.starts_with(
      "secret-tool lookup service ai-control-apikey username abc 2>/dev/null"
    ));
  }
}
