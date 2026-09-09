use keyring::Entry;

const SERVICE_NAME: &str = "3mf-katalog-manager";

#[derive(Debug, Clone)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedTokens {
    access_token: String,
    refresh_token: Option<String>,
}

pub trait TokenStore: Send + Sync {
    fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String>;
    fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String>;
    fn delete(&self, account_key: &str) -> Result<(), String>;
}

/// Speichert Tokens im OS-Schluesselbund (libsecret/Keychain/Credential
/// Manager je Plattform, ueber das `keyring`-Crate). Wird bewusst NICHT
/// automatisiert getestet - ein Test wuerde den echten System-Schluesselbund
/// beruehren, der in CI/Sandbox-Umgebungen nicht garantiert verfuegbar ist.
/// Fuer Tests siehe `InMemoryTokenStore` unten.
pub struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String> {
        let serialized = serde_json::to_string(&SerializedTokens {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
        })
        .map_err(|e| e.to_string())?;
        let entry = Entry::new(SERVICE_NAME, account_key).map_err(|e| e.to_string())?;
        entry.set_password(&serialized).map_err(|e| e.to_string())
    }

    fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String> {
        let entry = Entry::new(SERVICE_NAME, account_key).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(serialized) => {
                let parsed: SerializedTokens =
                    serde_json::from_str(&serialized).map_err(|e| e.to_string())?;
                Ok(Some(StoredTokens {
                    access_token: parsed.access_token,
                    refresh_token: parsed.refresh_token,
                }))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, account_key: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, account_key).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
pub struct InMemoryTokenStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, StoredTokens>>,
}

#[cfg(test)]
impl InMemoryTokenStore {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
impl TokenStore for InMemoryTokenStore {
    fn save(&self, account_key: &str, tokens: &StoredTokens) -> Result<(), String> {
        self.entries
            .lock()
            .unwrap()
            .insert(account_key.to_string(), tokens.clone());
        Ok(())
    }

    fn load(&self, account_key: &str) -> Result<Option<StoredTokens>, String> {
        Ok(self.entries.lock().unwrap().get(account_key).cloned())
    }

    fn delete(&self, account_key: &str) -> Result<(), String> {
        self.entries.lock().unwrap().remove(account_key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_round_trips_tokens() {
        let store = InMemoryTokenStore::new();
        let tokens = StoredTokens {
            access_token: "access-123".to_string(),
            refresh_token: Some("refresh-456".to_string()),
        };
        store.save("gdrive:user@example.com", &tokens).expect("save");

        let loaded = store
            .load("gdrive:user@example.com")
            .expect("load")
            .expect("present");
        assert_eq!(loaded.access_token, "access-123");
        assert_eq!(loaded.refresh_token, Some("refresh-456".to_string()));
    }

    #[test]
    fn in_memory_store_returns_none_for_missing_account() {
        let store = InMemoryTokenStore::new();
        assert!(store.load("gdrive:nobody@example.com").expect("load").is_none());
    }

    #[test]
    fn in_memory_store_deletes_tokens() {
        let store = InMemoryTokenStore::new();
        let tokens = StoredTokens {
            access_token: "a".to_string(),
            refresh_token: None,
        };
        store.save("gdrive:user@example.com", &tokens).expect("save");
        store.delete("gdrive:user@example.com").expect("delete");
        assert!(store.load("gdrive:user@example.com").expect("load").is_none());
    }
}
