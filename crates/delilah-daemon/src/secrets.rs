//! Secrets such as the Jira token: the macOS Keychain in the daemon, memory in tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const SERVICE: &str = "dev.delilah.jira";

#[derive(Clone)]
pub enum Secrets {
    Keychain,
    Memory(Arc<Mutex<HashMap<String, String>>>),
}

impl Secrets {
    pub fn get(&self, account: &str) -> Option<String> {
        match self {
            Self::Keychain => keychain::get(SERVICE, account),
            Self::Memory(map) => map.lock().unwrap().get(account).cloned(),
        }
    }

    pub fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        match self {
            Self::Keychain => keychain::set(SERVICE, account, secret),
            Self::Memory(map) => {
                map.lock().unwrap().insert(account.into(), secret.into());
                Ok(())
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod keychain {
    use security_framework::passwords::{get_generic_password, set_generic_password};

    pub fn get(service: &str, account: &str) -> Option<String> {
        let bytes = get_generic_password(service, account).ok()?;
        String::from_utf8(bytes).ok()
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        set_generic_password(service, account, secret.as_bytes())
            .map_err(|e| format!("cannot save to the Keychain: {e}"))
    }
}

#[cfg(not(target_os = "macos"))]
mod keychain {
    pub fn get(_: &str, _: &str) -> Option<String> {
        None
    }

    pub fn set(_: &str, _: &str, _: &str) -> Result<(), String> {
        Err("the Keychain is only available on macOS".into())
    }
}
