//! API-Key-Ablage über die keyring-Crate — Linux: Secret Service,
//! Windows: Credential Manager. macOS hat eine eigene Implementierung über
//! das security-CLI (ACL-Gründe, siehe macos.rs).

use crate::domain::credentials::{ApikeyStore, APIKEY_SERVICE};

pub(crate) struct KeychainStore;

impl KeychainStore {
  fn entry(pool: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(APIKEY_SERVICE, pool).map_err(|e| e.to_string())
  }
}

impl ApikeyStore for KeychainStore {
  fn set(&self, pool: &str, key: &str) -> Result<(), String> {
    Self::entry(pool)?.set_password(key).map_err(|e| e.to_string())
  }

  fn has(&self, pool: &str) -> Result<bool, String> {
    match Self::entry(pool)?.get_password() {
      Ok(_) => Ok(true),
      // Kein Eintrag oder kein Store verfügbar → die Key-Datei entscheidet.
      Err(
        keyring::Error::NoEntry
        | keyring::Error::PlatformFailure(_)
        | keyring::Error::NoStorageAccess(_),
      ) => Ok(false),
      Err(e) => Err(e.to_string()),
    }
  }

  fn delete(&self, pool: &str) -> Result<(), String> {
    match Self::entry(pool)?.delete_credential() {
      Ok(())
      | Err(
        keyring::Error::NoEntry
        | keyring::Error::PlatformFailure(_)
        | keyring::Error::NoStorageAccess(_),
      ) => Ok(()),
      Err(e) => Err(e.to_string()),
    }
  }
}
