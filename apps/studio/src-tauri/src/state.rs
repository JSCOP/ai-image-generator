//! Shared app state managed by Tauri.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use image_core::connection::{EnvSource, SystemEnv};
use image_core::profiles::ProfileStore;
use image_core::secrets::{KeyringStore, MemoryStore, SecretStore};

use crate::service::Ctx;

pub struct AppState {
    pub ctx: Ctx,
    /// Serialises settings.json writes from this process.
    pub settings_lock: Arc<Mutex<()>>,
    /// Generations in progress.
    pub runs: Runs,
}

impl AppState {
    pub fn new(ctx: Ctx) -> AppState {
        AppState { ctx, settings_lock: Arc::new(Mutex::new(())), runs: Runs::default() }
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

/// Cancellation tokens of running generations, by run id.
#[derive(Clone, Default)]
pub struct Runs {
    tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

/// A registered run; dropping it (run ended, or its command future was dropped) forgets the run.
pub struct RunGuard {
    runs: Runs,
    id: String,
    token: CancellationToken,
}

impl RunGuard {
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.runs.lock().remove(&self.id);
    }
}

impl Runs {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.tokens.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn register(&self, id: &str) -> RunGuard {
        let token = CancellationToken::new();
        self.lock().insert(id.to_string(), token.clone());
        RunGuard { runs: self.clone(), id: id.to_string(), token }
    }

    /// Whether a run with this id was running.
    pub fn cancel(&self, id: &str) -> bool {
        match self.lock().get(id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    /// Cancels every run; returns how many there were.
    pub fn cancel_all(&self) -> usize {
        let tokens = self.lock();
        tokens.values().for_each(CancellationToken::cancel);
        tokens.len()
    }

    pub fn active(&self) -> usize {
        self.lock().len()
    }

    /// Waits until no run is left (true) or `limit` passes (false).
    pub async fn wait_idle(&self, limit: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            if self.active() == 0 {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// `IMAGE_STUDIO_HOME`, else `%APPDATA%\ImageStudio` (same folder as the CLI), else the OS config folder.
pub fn app_home(env: &dyn EnvSource) -> PathBuf {
    ProfileStore::default_dir(env)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("ImageStudio")))
        .unwrap_or_else(|| PathBuf::from("ImageStudio"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Runs;

    #[test]
    fn runs_cancel_one_or_all_and_forget_finished() {
        let runs = Runs::default();
        let a = runs.register("a");
        let b = runs.register("b");
        assert_eq!(runs.active(), 2);
        assert!(runs.cancel("a"));
        assert!(a.token().is_cancelled() && !b.token().is_cancelled());
        assert!(!runs.cancel("missing"));
        drop(a);
        assert_eq!(runs.active(), 1);
        assert_eq!(runs.cancel_all(), 1);
        assert!(b.token().is_cancelled());
        drop(b);
        assert_eq!(runs.active(), 0);
    }

    #[tokio::test]
    async fn wait_idle_returns_when_runs_end_or_time_runs_out() {
        let runs = Runs::default();
        assert!(runs.wait_idle(Duration::from_millis(10)).await);
        let guard = runs.register("slow");
        assert!(!runs.wait_idle(Duration::from_millis(50)).await);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(guard);
        });
        assert!(runs.wait_idle(Duration::from_secs(5)).await);
    }
}
