//! OS credential storage. Callers dispatch these operations to a worker.

use std::{collections::HashMap, sync::Mutex};

use parchmint_platform_api::{CredentialSecret, CredentialStore, PlatformError};

#[derive(Default)]
pub(crate) struct NativeCredentialStore {
    // In particular, Windows Credential Manager does not serialize writes.
    operations: Mutex<()>,
}

/// Explicitly selected, process-lifetime fallback when the OS store is unavailable.
/// This never writes to the filesystem and is not used as an automatic fallback.
#[derive(Default)]
pub struct SessionCredentialStore {
    values: Mutex<HashMap<String, String>>,
}

impl CredentialStore for SessionCredentialStore {
    fn read(&self, profile_id: &str) -> Result<Option<CredentialSecret>, PlatformError> {
        let values = self.values.lock().map_err(|_| failure("read credential"))?;
        Ok(values.get(profile_id).cloned().map(CredentialSecret::new))
    }

    fn write(&self, profile_id: &str, secret: &CredentialSecret) -> Result<(), PlatformError> {
        let mut values = self
            .values
            .lock()
            .map_err(|_| failure("write credential"))?;
        values.insert(profile_id.to_owned(), secret.expose().to_owned());
        Ok(())
    }

    fn delete(&self, profile_id: &str) -> Result<(), PlatformError> {
        let mut values = self
            .values
            .lock()
            .map_err(|_| failure("delete credential"))?;
        values.remove(profile_id);
        Ok(())
    }
}

fn entry(profile_id: &str) -> Result<keyring::Entry, PlatformError> {
    if profile_id.is_empty()
        || profile_id.len() > 128
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(failure("access credential"));
    }
    keyring::Entry::new("ParchMint AI assistant", profile_id)
        .map_err(|_| failure("access credential"))
}

fn failure(operation: &'static str) -> PlatformError {
    PlatformError::Failed {
        operation,
        reason: "Secure credential storage is unavailable".into(),
    }
}

impl CredentialStore for NativeCredentialStore {
    fn read(&self, profile_id: &str) -> Result<Option<CredentialSecret>, PlatformError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| failure("read credential"))?;
        match entry(profile_id)?.get_password() {
            Ok(value) => Ok(Some(CredentialSecret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(failure("read credential")),
        }
    }

    fn write(&self, profile_id: &str, secret: &CredentialSecret) -> Result<(), PlatformError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| failure("write credential"))?;
        entry(profile_id)?
            .set_password(secret.expose())
            .map_err(|_| failure("write credential"))
    }

    fn delete(&self, profile_id: &str) -> Result<(), PlatformError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| failure("delete credential"))?;
        match entry(profile_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(failure("delete credential")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_credentials_are_scoped_and_disappear_with_the_store() {
        let store = SessionCredentialStore::default();
        store
            .write("profile-a", &CredentialSecret::new("secret-a"))
            .unwrap();
        assert!(store.read("profile-b").unwrap().is_none());
        assert_eq!(
            store.read("profile-a").unwrap().unwrap().expose(),
            "secret-a"
        );
        store.delete("profile-a").unwrap();
        assert!(store.read("profile-a").unwrap().is_none());
        assert!(
            SessionCredentialStore::default()
                .read("profile-a")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn credentials_are_redacted_and_invalid_profile_ids_are_rejected() {
        assert_eq!(
            format!("{:?}", CredentialSecret::new("secret")),
            "[REDACTED]"
        );
        assert!(entry("bad/profile").is_err());
    }
}
