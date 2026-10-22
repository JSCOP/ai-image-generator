//! Tauri commands: thin wrappers over `service`. Every error is a `ServiceError`; no command returns a key.

use std::path::{Path, PathBuf};

use image_core::check::ConnectionReport;
use image_core::error::Category;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::service::enhance::{self, EnhanceInput, EnhanceView};
use crate::service::gallery::{self, GalleryView};
use crate::service::generate::{self, GenerateInput, ReferenceView, RunEvent, RunSummary};
use crate::service::index::{self, load_index};
use crate::service::matte::{self, MatteEvent, MatteResult};
use crate::service::peers;
use crate::service::profiles::{self, ConnectionsView, ModelView, ProfileInput, ProfileView};
use crate::service::settings::{self, default_work_dir, Settings, ShareSettings};
use crate::service::share::{self, ShareInfo};
use crate::service::{Ctx, ServiceError};
use crate::state::{AppState, ShareRun};

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

/// Checks a picked or dropped reference image and lets the webview show that one file as a thumbnail.
#[tauri::command]
pub async fn inspect_reference<R: Runtime>(app: AppHandle<R>, path: PathBuf) -> Result<ReferenceView, ServiceError> {
    let checked = path.clone();
    let view = blocking(move || generate::inspect_reference(&checked)).await?;
    // A file the scope cannot hold only means its thumbnail does not show.
    let _ = app.asset_protocol_scope().allow_file(&path);
    Ok(view)
}

/// Cancels a running generation; false when no run has this id (already ended).
#[tauri::command]
pub fn cancel_generation(state: State<'_, AppState>, run_id: String) -> bool {
    state.runs.cancel(&run_id)
}

/// Lets the webview load images (asset protocol) from every listed gallery of this PC; repeats are harmless (a set).
pub fn allow_galleries<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let index = load_index(&app.state::<AppState>().ctx.home);
    for root in gallery::gallery_roots(settings, &index) {
        // A path the scope cannot hold only means its images do not show.
        let _ = app.asset_protocol_scope().allow_directory(&root, true);
    }
}

/// What the gallery screen gets: this PC's galleries and the other PCs' shared ones, newest first.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GalleryScreen {
    #[serde(flatten)]
    pub view: GalleryView,
    /// "이 PC에서 찾기" has run at least once (else the screen runs it once by itself).
    pub scanned: bool,
}

/// Newest records of this PC (work folder, added folders, found galleries) and, unless `peers` is false, of the other
/// PCs (default limit 2000).
#[tauri::command]
pub async fn gallery<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    limit: Option<usize>,
    peers: Option<bool>,
) -> Result<GalleryScreen, ServiceError> {
    let limit = limit.unwrap_or(gallery::DEFAULT_LIMIT);
    let home = state.ctx.home.clone();
    let (settings, index) = blocking(move || Ok((settings::load_settings(&home), load_index(&home)))).await?;
    allow_galleries(&app, &settings);
    let scanned = index.scanned_at.is_some();
    let peers = if peers.unwrap_or(true) { settings.peers.clone() } else { Vec::new() };
    let local = {
        let settings = settings.clone();
        blocking(move || Ok(gallery::gallery_with(&settings, &index, limit)))
    };
    let ctx = ctx(&state);
    let remote = futures_join(peers.iter().map(|peer| {
        let ctx = ctx.clone();
        let peer = peer.clone();
        async move { (peers::peer_gallery(&ctx, &peer, limit).await, peer) }
    }));
    let (local, remote) = tokio::join!(local, remote);
    let mut view = local?;
    for (result, peer) in remote {
        match result {
            Ok(remote) => {
                view.items.extend(remote.items);
                view.folders.extend(remote.folders);
                view.truncated |= remote.truncated;
            }
            Err(error) => view.folders.push(peers::unreachable(&peer, &error)),
        }
    }
    view.truncated |= gallery::sort_and_limit(&mut view.items, limit);
    Ok(GalleryScreen { view, scanned })
}

/// Runs every future at once (each on the runtime) and keeps their order: the slowest PC sets the time, not the sum.
async fn futures_join<F>(futures: impl Iterator<Item = F>) -> Vec<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    let handles: Vec<_> = futures.map(tauri::async_runtime::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(output) = handle.await {
            out.push(output);
        }
    }
    out
}

/// "이 PC에서 찾기": every ImageGallery on the fixed drives joins the gallery; returns how many were found.
#[tauri::command]
pub async fn discover_galleries<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> Result<usize, ServiceError> {
    let home = state.ctx.home.clone();
    let lock = state.settings_lock.clone();
    let found = blocking(move || {
        let found = index::find_galleries(&index::search_starts(), index::MAX_DEPTH, index::MAX_DIRS, index::MAX_TIME);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        index::record_roots(&home, &found, true)
            .map_err(|e| ServiceError::new(Category::Internal, format!("찾은 갤러리 목록을 저장할 수 없습니다: {e}")))?;
        Ok(found.len())
    })
    .await?;
    let settings = settings::load_settings(&state.ctx.home);
    allow_galleries(&app, &settings);
    Ok(found)
}

/// "이 PC 공유" as the screen shows it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShareView {
    pub enabled: bool,
    pub port: u16,
    pub running: bool,
    /// This PC's Tailscale address, when it is connected.
    pub address: Option<String>,
    pub name: String,
    pub token: String,
    pub error: Option<String>,
}

fn share_view(state: &AppState, settings: &ShareSettings, token: String) -> ShareView {
    let run = state.share.lock().unwrap_or_else(|e| e.into_inner());
    ShareView {
        enabled: settings.enabled,
        port: settings.port,
        running: run.stop.is_some(),
        address: share::tailscale_address().map(|ip| ip.to_string()),
        name: share::machine_name(),
        token,
        error: run.error.clone(),
    }
}

/// Starts the server for `settings` (stops a running one first); a failed start is kept as the view's error.
pub async fn apply_share(state: &AppState, settings: &ShareSettings) {
    let stop = {
        let mut run = state.share.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(stop) = run.stop.take() {
            stop.cancel();
        }
        *run = ShareRun::default();
        if !settings.enabled {
            return;
        }
        let stop = tokio_util::sync::CancellationToken::new();
        run.stop = Some(stop.clone());
        run.port = Some(settings.port);
        stop
    };
    let ctx = state.ctx.clone();
    let token = match tauri::async_runtime::spawn_blocking(move || share::share_token(&ctx)).await {
        Ok(Ok(token)) => token,
        Ok(Err(e)) => return fail_share(state, &stop, e.message),
        Err(e) => return fail_share(state, &stop, e.to_string()),
    };
    match share::bind(settings.port).await {
        Ok(listener) => {
            tauri::async_runtime::spawn(share::serve(listener, state.ctx.home.clone(), token, stop));
        }
        Err(e) => fail_share(state, &stop, e.message),
    }
}

fn fail_share(state: &AppState, stop: &tokio_util::sync::CancellationToken, error: String) {
    let mut run = state.share.lock().unwrap_or_else(|e| e.into_inner());
    stop.cancel();
    run.stop = None;
    run.error = Some(error);
}

#[tauri::command]
pub async fn share_status(state: State<'_, AppState>) -> Result<ShareView, ServiceError> {
    let ctx = ctx(&state);
    let (settings, token) = blocking(move || Ok((settings::load_settings(&ctx.home), share::share_token(&ctx)?))).await?;
    Ok(share_view(&state, &settings.share, token))
}

/// Turns "이 PC 공유" on or off (and sets its port); the server starts or stops at once.
#[tauri::command]
pub async fn set_share(state: State<'_, AppState>, share: ShareSettings) -> Result<ShareView, ServiceError> {
    if share.port < 1024 {
        return Err(ServiceError::new(Category::InvalidRequest, "공유 포트는 1024~65535 사이여야 합니다."));
    }
    let home = state.ctx.home.clone();
    let lock = state.settings_lock.clone();
    let wanted = share.clone();
    blocking(move || {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut saved = settings::load_settings(&home);
        saved.share = wanted;
        settings::save_settings(&home, &saved)
    })
    .await?;
    apply_share(&state, &share).await;
    share_status(state).await
}

/// A new share token (other PCs need the new one); a running server switches to it.
#[tauri::command]
pub async fn new_share_token(state: State<'_, AppState>) -> Result<ShareView, ServiceError> {
    let ctx = ctx(&state);
    let settings = blocking(move || {
        share::new_share_token(&ctx)?;
        Ok(settings::load_settings(&ctx.home))
    })
    .await?;
    apply_share(&state, &settings.share).await;
    share_status(state).await
}

/// Name and version of the PC at `url` with `token` (nothing is saved).
#[tauri::command]
pub async fn check_peer(url: String, token: String) -> Result<ShareInfo, ServiceError> {
    peers::check_peer(&url, &token).await
}

/// Checks and saves another PC; returns the saved settings.
#[tauri::command]
pub async fn add_peer(state: State<'_, AppState>, name: String, url: String, token: String) -> Result<Settings, ServiceError> {
    let ctx = ctx(&state);
    peers::add_peer(&ctx, &name, &url, &token).await
}

#[tauri::command]
pub async fn remove_peer(state: State<'_, AppState>, id: String) -> Result<Settings, ServiceError> {
    let ctx = ctx(&state);
    let lock = state.settings_lock.clone();
    blocking(move || {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        peers::remove_peer(&ctx, &id)
    })
    .await
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

/// Rewrites the prompt as one English image prompt with a chat model; nothing is generated or saved.
#[tauri::command]
pub async fn enhance_prompt(state: State<'_, AppState>, input: EnhanceInput) -> Result<EnhanceView, ServiceError> {
    let ctx = ctx(&state);
    enhance::enhance(&ctx, input).await
}

/// Chat models of a connection for "영어로 다듬기" (name null = the active connection).
#[tauri::command]
pub async fn text_models(state: State<'_, AppState>, name: Option<String>) -> Result<Vec<String>, ServiceError> {
    let ctx = ctx(&state);
    enhance::text_models(&ctx, name.as_deref()).await
}

/// "배경 제거 (내 PC)": cuts the subject out of an image into a new gallery image (first use downloads the model).
#[tauri::command]
pub async fn remove_background<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: PathBuf,
    on_event: Channel<MatteEvent>,
) -> Result<MatteResult, ServiceError> {
    let ctx = ctx(&state);
    let result = matte::remove_background(&ctx, path, move |event| {
        let _ = on_event.send(event);
    })
    .await?;
    // The work folder is already allowed; a new file under it shows without more.
    let _ = app.asset_protocol_scope().allow_file(&result.output);
    Ok(result)
}

/// Ctrl+V: saves a pasted image into the work folder's `ImageGallery/references` and returns it as a reference.
#[tauri::command]
pub async fn paste_reference<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    data: String,
) -> Result<ReferenceView, ServiceError> {
    let home = state.ctx.home.clone();
    let view =
        blocking(move || generate::save_pasted_reference(&settings::load_settings(&home).work_dir, &data, chrono::Local::now())).await?;
    let _ = app.asset_protocol_scope().allow_file(&view.path);
    Ok(view)
}
