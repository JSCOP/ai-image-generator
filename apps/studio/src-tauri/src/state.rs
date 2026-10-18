//! Shared app state managed by Tauri.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use image_core::connection::{EnvSource, SystemEnv};
use image_core::profiles::ProfileStore;
use image_core::secrets::{KeyringStore, MemoryStore, SecretStore};

use crate::service::Ctx;

pub struct AppState {
    pub ctx: Ctx,
    /// Serialises settings.json writes from this process.
    pub settings_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn new(ctx: Ctx) -> AppState {
        AppState { ctx, settings_lock: Arc::new(Mutex::new(())) }
    }

    /// Real environment and the user's Credential Manager (memory only where that is unsupported).
    pub fn system() -> AppState {
        let secrets: Arc<dyn SecretStore> = match KeyringStore::system(&SystemEnv) {
            Ok(keyring) => Arc::new(keyring),
            Err(_) => Arc::new(MemoryStore::new()),
        };
        AppState::new(Ctx { home: app_home(&SystemEnv), env: Arc::new(SystemEnv), secrets })
    }
}

/// `IMAGE_STUDIO_HOME`, else `%APPDATA%\ImageStudio` (same folder as the CLI), else the OS config folder.
pub fn app_home(env: &dyn EnvSource) -> PathBuf {
    ProfileStore::default_dir(env)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("ImageStudio")))
        .unwrap_or_else(|| PathBuf::from("ImageStudio"))
}
