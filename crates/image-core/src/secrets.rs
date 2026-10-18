//! API key storage. Keys live only in the Windows Credential Manager (or memory in tests), never in files.

use crate::connection::{EnvSource, Secret};
use crate::error::{Category, EngineError};
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

/// Credential Manager service name unless `IMAGE_STUDIO_KEYRING_SERVICE` overrides it.
pub const DEFAULT_SERVICE: &str = "ImageStudio";
/// Windows stores passwords as UTF-16; the credential blob holds at most 2560 bytes.
pub const MAX_KEY_UTF16_UNITS: usize = 1280;

pub trait SecretStore: Send + Sync {
    fn get(&self, profile_id: &str) -> Result<Option<Secret>, EngineError>;
    fn set(&self, profile_id: &str, key: &str) -> Result<(), EngineError>;
    /// Missing entries are not an error.
    fn delete(&self, profile_id: &str) -> Result<(), EngineError>;
}

/// Trim the key and reject empty or over-long values. The key never appears in the error.
fn clean_key(key: &str) -> Result<&str, EngineError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(EngineError::new(Category::InvalidRequest, "키가 비어 있습니다."));
    }
    if key.encode_utf16().count() > MAX_KEY_UTF16_UNITS {
        return Err(too_long());
    }
    Ok(key)
}

fn too_long() -> EngineError {
    EngineError::new(Category::InvalidRequest, format!("키가 너무 깁니다(최대 {MAX_KEY_UTF16_UNITS}자)."))
}

/// In-process store for tests and non-Windows builds.
#[derive(Default)]
pub struct MemoryStore {
    keys: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> MemoryStore {
        MemoryStore::default()
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.keys.lock().map(|k| k.len()).unwrap_or(0);
        f.debug_struct("MemoryStore").field("entries", &count).finish()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, profile_id: &str) -> Result<Option<Secret>, EngineError> {
        Ok(self.keys.lock().unwrap_or_else(|e| e.into_inner()).get(profile_id).map(|k| Secret::new(k.clone())))
    }

    fn set(&self, profile_id: &str, key: &str) -> Result<(), EngineError> {
        let key = clean_key(key)?;
        self.keys.lock().unwrap_or_else(|e| e.into_inner()).insert(profile_id.to_string(), key.to_string());
        Ok(())
    }

    fn delete(&self, profile_id: &str) -> Result<(), EngineError> {
        self.keys.lock().unwrap_or_else(|e| e.into_inner()).remove(profile_id);
        Ok(())
    }
}

/// Windows Credential Manager entries: service = `self.service`, user = profile id,
/// target name `"{id}.{service}"`. Builds entries from a store instance, never the global default store.
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// The user's Credential Manager; service from `IMAGE_STUDIO_KEYRING_SERVICE` or `"ImageStudio"`.
    pub fn system(env: &dyn EnvSource) -> Result<KeyringStore, EngineError> {
        if !cfg!(windows) {
            return Err(EngineError::new(Category::Unsupported, "키 보관은 Windows 자격 증명 관리자에서만 지원합니다."));
        }
        let service = env.process("IMAGE_STUDIO_KEYRING_SERVICE").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Ok(KeyringStore::with_service(service.as_deref().unwrap_or(DEFAULT_SERVICE)))
    }

    pub fn with_service(service: &str) -> KeyringStore {
        KeyringStore { service: service.to_string() }
    }

    pub fn service(&self) -> &str {
        &self.service
    }
}

#[cfg(windows)]
mod platform {
    use super::{too_long, KeyringStore};
    use crate::error::{Category, EngineError};
    use keyring_core::api::CredentialStoreApi;
    use keyring_core::{Entry, Error};

    impl KeyringStore {
        pub(super) fn entry(&self, profile_id: &str) -> Result<Entry, EngineError> {
            let store = windows_native_keyring_store::Store::new().map_err(map_error)?;
            store.build(&self.service, profile_id, None).map_err(map_error)
        }
    }

    /// Describe a keyring error without any payload that could hold key bytes.
    pub(super) fn map_error(err: Error) -> EngineError {
        let detail = match &err {
            Error::TooLong(..) => return too_long(),
            Error::PlatformFailure(e) => format!("플랫폼 오류 ({e})"),
            Error::NoStorageAccess(e) => format!("저장소에 접근할 수 없음 ({e})"),
            Error::BadEncoding(_) | Error::BadDataFormat(..) => "저장된 키 형식이 올바르지 않음".to_string(),
            Error::Invalid(attr, reason) => format!("잘못된 값 '{attr}': {reason}"),
            Error::Ambiguous(_) => "같은 이름의 항목이 여러 개 있음".to_string(),
            other => other.to_string(),
        };
        EngineError::new(Category::Internal, format!("자격 증명 관리자 오류: {detail}"))
    }

    pub(super) fn is_no_entry(err: &Error) -> bool {
        matches!(err, Error::NoEntry)
    }
}

#[cfg(windows)]
impl SecretStore for KeyringStore {
    fn get(&self, profile_id: &str) -> Result<Option<Secret>, EngineError> {
        match self.entry(profile_id)?.get_password() {
            Ok(key) => Ok(Some(Secret::new(key))),
            Err(e) if platform::is_no_entry(&e) => Ok(None),
            Err(e) => Err(platform::map_error(e)),
        }
    }

    fn set(&self, profile_id: &str, key: &str) -> Result<(), EngineError> {
        let key = clean_key(key)?;
        self.entry(profile_id)?.set_password(key).map_err(platform::map_error)
    }

    fn delete(&self, profile_id: &str) -> Result<(), EngineError> {
        match self.entry(profile_id)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(e) if platform::is_no_entry(&e) => Ok(()),
            Err(e) => Err(platform::map_error(e)),
        }
    }
}

#[cfg(not(windows))]
impl SecretStore for KeyringStore {
    fn get(&self, _profile_id: &str) -> Result<Option<Secret>, EngineError> {
        Err(unsupported())
    }

    fn set(&self, _profile_id: &str, key: &str) -> Result<(), EngineError> {
        clean_key(key)?;
        Err(unsupported())
    }

    fn delete(&self, _profile_id: &str) -> Result<(), EngineError> {
        Err(unsupported())
    }
}

#[cfg(not(windows))]
fn unsupported() -> EngineError {
    EngineError::new(Category::Unsupported, "키 보관은 Windows 자격 증명 관리자에서만 지원합니다.")
}
