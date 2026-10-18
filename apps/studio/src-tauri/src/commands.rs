//! Tauri commands: thin wrappers over `service`. Every error is a `ServiceError`; no command returns a key.

use std::path::{Path, PathBuf};

use image_core::check::ConnectionReport;
use image_core::error::Category;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::service::gallery::{self, GalleryView};
use crate::service::generate::{self, GenerateInput, RunEvent, RunSummary};
use crate::service::profiles::{self, ConnectionsView, ModelView, ProfileInput, ProfileView};
use crate::service::settings::{self, default_work_dir, Settings};
use crate::service::{Ctx, ServiceError};
use crate::state::AppState;

/// What the app shows about itself: version, app home and default work folder.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// Folder of profiles.json and settings.json.
    pub home: PathBuf,
    pub work_dir: PathBuf,
}

pub fn app_info_from(home: &Path) -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION").to_string(), home: home.to_path_buf(), work_dir: default_work_dir() }
}

/// File and Credential Manager work runs off the async runtime threads (profile locks wait up to 5 s).
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, ServiceError> + Send + 'static) -> Result<T, ServiceError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| ServiceError::new(Category::Internal, format!("작업이 중단되었습니다: {e}")))?
}

fn ctx(state: &State<'_, AppState>) -> Ctx {
    state.ctx.clone()
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    app_info_from(&state.ctx.home)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, ServiceError> {
    let home = state.ctx.home.clone();
    blocking(move || Ok(settings::load_settings(&home))).await
}

/// Saves settings, then allows the (possibly new) work folder's gallery so generated images show at once.
#[tauri::command]
pub async fn save_settings<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>, settings: Settings) -> Result<(), ServiceError> {
    let home = state.ctx.home.clone();
    let lock = state.settings_lock.clone();
    let saved = blocking(move || {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        settings::save_settings(&home, &settings)?;
        Ok(settings)
    })
    .await?;
    allow_galleries(&app, &saved);
    Ok(())
}

#[tauri::command]
pub async fn connections(state: State<'_, AppState>) -> Result<ConnectionsView, ServiceError> {
    let ctx = ctx(&state);
    blocking(move || profiles::connections(&ctx)).await
}

#[tauri::command]
pub async fn save_profile(state: State<'_, AppState>, input: ProfileInput) -> Result<ProfileView, ServiceError> {
    let ctx = ctx(&state);
    blocking(move || profiles::save_profile(&ctx, input)).await
}

#[tauri::command]
pub async fn remove_profile(state: State<'_, AppState>, id: String) -> Result<(), ServiceError> {
    let ctx = ctx(&state);
    blocking(move || profiles::remove_profile(&ctx, &id)).await
}

#[tauri::command]
pub async fn set_default_profile(state: State<'_, AppState>, id: Option<String>) -> Result<(), ServiceError> {
    let ctx = ctx(&state);
    blocking(move || profiles::set_default_profile(&ctx, id.as_deref())).await
}

#[tauri::command]
pub async fn check_profile(state: State<'_, AppState>, name: Option<String>) -> Result<ConnectionReport, ServiceError> {
    let ctx = ctx(&state);
    profiles::check_profile(&ctx, name.as_deref()).await
}

/// Runs one generation; progress goes to `on_event` (first event `started` carries the run id for `cancel_generation`).
#[tauri::command]
pub async fn start_generation(
    state: State<'_, AppState>,
    input: GenerateInput,
    on_event: Channel<RunEvent>,
) -> Result<RunSummary, ServiceError> {
    let ctx = ctx(&state);
    let run_id = format!("run-{}", uuid::Uuid::new_v4().simple());
    // Dropping the guard (run ended or the command was dropped) forgets the run.
    let guard = state.runs.register(&run_id);
    let emit = move |event: RunEvent| {
        // A closed webview cannot receive events; the run itself goes on until cancelled.
        let _ = on_event.send(event);
    };
    generate::run_generation(&ctx, input, run_id, emit, guard.token().clone()).await
}

/// Cancels a running generation; false when no run has this id (already ended).
#[tauri::command]
pub fn cancel_generation(state: State<'_, AppState>, run_id: String) -> bool {
    state.runs.cancel(&run_id)
}

/// Lets the webview load images (asset protocol) from every registered gallery; repeats are harmless (a set).
pub fn allow_galleries<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    for root in gallery::gallery_roots(settings) {
        // A path the scope cannot hold only means its images do not show.
        let _ = app.asset_protocol_scope().allow_directory(&root, true);
    }
}

/// Newest records of the work folder and the registered folders (default limit 2000).
#[tauri::command]
pub async fn gallery<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>, limit: Option<usize>) -> Result<GalleryView, ServiceError> {
    let home = state.ctx.home.clone();
    let settings = blocking(move || Ok(settings::load_settings(&home))).await?;
    allow_galleries(&app, &settings);
    blocking(move || Ok(gallery::gallery(&settings, limit.unwrap_or(gallery::DEFAULT_LIMIT)))).await
}

/// Registers a folder; returns the saved settings.
#[tauri::command]
pub async fn add_gallery_folder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<Settings, ServiceError> {
    let home = state.ctx.home.clone();
    let lock = state.settings_lock.clone();
    let settings = blocking(move || {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        gallery::add_folder(&home, &path)
    })
    .await?;
    allow_galleries(&app, &settings);
    Ok(settings)
}

/// Unregisters a folder (never the work folder); returns the saved settings.
#[tauri::command]
pub async fn remove_gallery_folder(state: State<'_, AppState>, path: PathBuf) -> Result<Settings, ServiceError> {
    let home = state.ctx.home.clone();
    let lock = state.settings_lock.clone();
    blocking(move || {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        gallery::remove_folder(&home, &path)
    })
    .await
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>, name: Option<String>) -> Result<Vec<ModelView>, ServiceError> {
    let ctx = ctx(&state);
    profiles::models(&ctx, name.as_deref()).await
}
