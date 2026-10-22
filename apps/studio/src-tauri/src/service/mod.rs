//! App services over `image-core`: plain Rust, no Tauri types, tested with fakes.

pub mod edit;
pub mod enhance;
pub mod gallery;
pub mod generate;
pub mod index;
pub mod matte;
pub mod peers;
pub mod profiles;
pub mod settings;
pub mod share;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use image_core::connection::{EnvSource, Secret};
use image_core::error::{redact, Category, EngineError};
use image_core::secrets::SecretStore;
use serde::Serialize;

/// Error every command returns. Never carries a key.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceError {
    pub category: Category,
    pub message: String,
}

impl ServiceError {
    pub fn new(category: Category, message: impl Into<String>) -> ServiceError {
        ServiceError { category, message: redact(&message.into(), None) }
    }

    /// Engine error with `key` (when known) and any bearer token removed from the message.
    pub fn from_engine(err: EngineError, key: Option<&Secret>) -> ServiceError {
        ServiceError { category: err.category, message: redact(&err.message, key) }
    }
}

impl From<EngineError> for ServiceError {
    fn from(err: EngineError) -> ServiceError {
        ServiceError::from_engine(err, None)
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ServiceError {}

/// What every service call needs: the app home (profiles.json, settings.json), environment and key store.
#[derive(Clone)]
pub struct Ctx {
    pub home: PathBuf,
    pub env: Arc<dyn EnvSource>,
    pub secrets: Arc<dyn SecretStore>,
}
