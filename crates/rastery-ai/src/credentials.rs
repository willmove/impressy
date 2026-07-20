use std::collections::HashMap;
use std::sync::Mutex;

use crate::{AiError, ApiKey, ProviderId, Result};

const SERVICE: &str = "app.rastery.Rastery";

pub trait CredentialStore: Send + Sync {
    fn set(&self, provider: ProviderId, key: &ApiKey) -> Result<()>;
    fn get(&self, provider: ProviderId) -> Result<ApiKey>;
    fn delete(&self, provider: ProviderId) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCredentialStore;

impl SystemCredentialStore {
    fn entry(provider: ProviderId) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, provider.as_str()).map_err(|_| AiError::CredentialStore)
    }
}

impl CredentialStore for SystemCredentialStore {
    fn set(&self, provider: ProviderId, key: &ApiKey) -> Result<()> {
        Self::entry(provider)?
            .set_password(key.expose())
            .map_err(|_| AiError::CredentialStore)
    }

    fn get(&self, provider: ProviderId) -> Result<ApiKey> {
        match Self::entry(provider)?.get_password() {
            Ok(value) => ApiKey::new(value),
            Err(keyring::Error::NoEntry) => Err(AiError::CredentialMissing),
            Err(_) => Err(AiError::CredentialStore),
        }
    }

    fn delete(&self, provider: ProviderId) -> Result<()> {
        match Self::entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AiError::CredentialStore),
        }
    }
}

#[derive(Default)]
pub struct MemoryCredentialStore {
    values: Mutex<HashMap<ProviderId, ApiKey>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn set(&self, provider: ProviderId, key: &ApiKey) -> Result<()> {
        self.values
            .lock()
            .map_err(|_| AiError::CredentialStore)?
            .insert(provider, key.clone());
        Ok(())
    }

    fn get(&self, provider: ProviderId) -> Result<ApiKey> {
        self.values
            .lock()
            .map_err(|_| AiError::CredentialStore)?
            .get(&provider)
            .cloned()
            .ok_or(AiError::CredentialMissing)
    }

    fn delete(&self, provider: ProviderId) -> Result<()> {
        self.values
            .lock()
            .map_err(|_| AiError::CredentialStore)?
            .remove(&provider);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip_and_delete() {
        let store = MemoryCredentialStore::default();
        let key = ApiKey::new("test-key").expect("key");
        store.set(ProviderId::OpenAi, &key).expect("set");
        assert_eq!(
            format!("{}", store.get(ProviderId::OpenAi).expect("get")),
            "[REDACTED]"
        );
        store.delete(ProviderId::OpenAi).expect("delete");
        assert!(matches!(
            store.get(ProviderId::OpenAi),
            Err(AiError::CredentialMissing)
        ));
    }
}
