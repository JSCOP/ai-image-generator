//! "다른 PC": galleries shared by other PCs ("이 PC 공유" there). Their records join this gallery with paths
//! `peer://<id>/<path there>`; their images come through the `peer` URI scheme and are cached on this PC.

use std::path::{Path, PathBuf};
use std::time::Duration;

use image_core::error::Category;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::gallery::{FolderSource, FolderView, GalleryItem, GalleryView};
use super::settings::{load_settings, save_settings, Peer, Settings, DEFAULT_SHARE_PORT};
use super::share::{ShareInfo, MAX_THUMB};
use super::{Ctx, ServiceError};

pub const PREFIX: &str = "peer://";
const LIST_TIMEOUT: Duration = Duration::from_secs(8);
const IMAGE_TIMEOUT: Duration = Duration::from_secs(60);
/// Thumbnail width the gallery cards ask for.
pub const THUMB_WIDTH: u32 = 480;

/// `100.1.2.3`, `js`, `js:9000` or `http://js:9000/` -> `http://js:9000` (port 8329 when none is given).
pub fn normalize_url(input: &str) -> Result<String, ServiceError> {
    let text = input.trim().trim_end_matches('/');
    let with_scheme = if text.contains("://") { text.to_string() } else { format!("http://{text}") };
    let url = url::Url::parse(&with_scheme)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|h| !h.is_empty()))
        .ok_or_else(|| ServiceError::new(Category::InvalidRequest, format!("다른 PC 주소가 올바르지 않습니다: {input}")))?;
    let host = url.host_str().unwrap_or_default();
    let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_string() };
    Ok(format!("{}://{host}:{}", url.scheme(), url.port().unwrap_or(DEFAULT_SHARE_PORT)))
}

fn client(timeout: Duration) -> Result<reqwest::Client, ServiceError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ServiceError::new(Category::Internal, format!("HTTP 클라이언트를 만들 수 없습니다: {e}")))
}

/// Plain words for what went wrong reaching another PC.
fn reach_error(url: &str, err: &reqwest::Error) -> ServiceError {
    let message = if err.is_timeout() || err.is_connect() {
        format!("{url}에 연결할 수 없습니다. 그 PC가 켜져 있고 Image Studio의 \"이 PC 공유\"가 켜져 있는지, Tailscale에 접속되어 있는지 확인하세요.")
    } else {
        format!("{url}에서 읽지 못했습니다: {err}")
    };
    ServiceError::new(Category::Network, message)
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

async fn get_json<T: DeserializeOwned>(url: &str, path: &str, token: &str, timeout: Duration) -> Result<T, ServiceError> {
    let response = client(timeout)?.get(format!("{url}{path}")).bearer_auth(token).send().await.map_err(|e| reach_error(url, &e))?;
    let status = response.status();
    if status.as_u16() == 401 {
        return Err(ServiceError::new(Category::Auth, format!("{url}: 공유 토큰이 맞지 않습니다. 그 PC의 \"이 PC 공유\"에 보이는 토큰을 다시 넣으세요.")));
    }
    if !status.is_success() {
        let detail = response.json::<ErrorBody>().await.map(|b| b.error).unwrap_or_default();
        return Err(ServiceError::new(Category::Provider, format!("{url}: HTTP {} {detail}", status.as_u16())));
    }
    response.json::<T>().await.map_err(|e| ServiceError::new(Category::Provider, format!("{url}의 응답을 읽지 못했습니다: {e}")))
}

/// Name and version of the PC at `url` (no image is read).
pub async fn check_peer(url: &str, token: &str) -> Result<ShareInfo, ServiceError> {
    let url = normalize_url(url)?;
    get_json(&url, "/v1/info", token.trim(), LIST_TIMEOUT).await
}

/// Checks the PC first, then saves it (its token goes to the key store); the name defaults to the PC's own name.
pub async fn add_peer(ctx: &Ctx, name: &str, url: &str, token: &str) -> Result<Settings, ServiceError> {
    let url = normalize_url(url)?;
    let token = token.trim();
    if token.is_empty() {
        return Err(ServiceError::new(Category::InvalidRequest, "그 PC의 공유 토큰을 넣어 주세요."));
    }
    let info = check_peer(&url, token).await?;
    let mut settings = load_settings(&ctx.home);
    let name = if name.trim().is_empty() { info.name } else { name.trim().to_string() };
    let id = match settings.peers.iter().find(|peer| peer.url.eq_ignore_ascii_case(&url)) {
        Some(known) => known.id.clone(),
        None => format!("peer-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
    };
    ctx.secrets.set(&id, token)?;
    settings.peers.retain(|peer| peer.id != id);
    settings.peers.push(Peer { id, name, url });
    save_settings(&ctx.home, &settings)?;
    Ok(settings)
}

/// Forgets the PC, its token and its cached images.
pub fn remove_peer(ctx: &Ctx, id: &str) -> Result<Settings, ServiceError> {
    let mut settings = load_settings(&ctx.home);
    let before = settings.peers.len();
    settings.peers.retain(|peer| peer.id != id);
    if settings.peers.len() == before {
        return Err(ServiceError::new(Category::NotFound, format!("등록된 다른 PC가 아닙니다: {id}")));
    }
    save_settings(&ctx.home, &settings)?;
    ctx.secrets.delete(id)?;
    if let Some(dir) = cache_dir().map(|dir| dir.join(id)) {
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(settings)
}

pub fn remote_path(id: &str, path: &str) -> String {
    format!("{PREFIX}{id}/{path}")
}

/// (peer id, path there) of a `peer://` path.
pub fn split_remote(path: &str) -> Option<(&str, &str)> {
    path.strip_prefix(PREFIX)?.split_once('/')
}

/// That PC's gallery with every path rewritten to `peer://<id>/...` and its PC name on each record and folder.
pub fn as_remote(peer: &Peer, view: GalleryView) -> GalleryView {
    let items = view
        .items
        .into_iter()
        .map(|item| GalleryItem {
            image_path: remote_path(&peer.id, &item.image_path),
            record_path: remote_path(&peer.id, &item.record_path),
            folder: remote_path(&peer.id, &item.folder),
            references: Vec::new(),
            mask: None,
            machine: Some(peer.name.clone()),
            ..item
        })
        .collect();
    let folders = view
        .folders
        .into_iter()
        .filter(|folder| folder.available)
        .map(|folder| FolderView {
            path: remote_path(&peer.id, &folder.path),
            gallery_root: remote_path(&peer.id, &folder.gallery_root),
            available: true,
            is_work_dir: false,
            source: FolderSource::Peer,
            machine: Some(peer.name.clone()),
            note: None,
        })
        .collect();
    GalleryView { items, folders, truncated: view.truncated }
}

/// One folder entry standing for a PC that could not be read.
pub fn unreachable(peer: &Peer, error: &ServiceError) -> FolderView {
    FolderView {
        path: remote_path(&peer.id, ""),
        gallery_root: remote_path(&peer.id, ""),
        available: false,
        is_work_dir: false,
        source: FolderSource::Peer,
        machine: Some(peer.name.clone()),
        note: Some(error.message.clone()),
    }
}

fn token_of(ctx: &Ctx, peer: &Peer) -> Result<String, ServiceError> {
    ctx.secrets
        .get(&peer.id)?
        .map(|token| token.expose().to_string())
        .ok_or_else(|| ServiceError::new(Category::Auth, format!("{}의 공유 토큰이 이 PC에 없습니다. 다시 추가하세요.", peer.name)))
}

/// The shared gallery of one PC.
pub async fn peer_gallery(ctx: &Ctx, peer: &Peer, limit: usize) -> Result<GalleryView, ServiceError> {
    let token = {
        let (ctx, peer) = (ctx.clone(), peer.clone());
        tokio::task::spawn_blocking(move || token_of(&ctx, &peer))
            .await
            .map_err(|e| ServiceError::new(Category::Internal, format!("작업이 중단되었습니다: {e}")))??
    };
    let view: GalleryView = get_json(&peer.url, &format!("/v1/gallery?limit={limit}"), &token, LIST_TIMEOUT).await?;
    Ok(as_remote(peer, view))
}

/// `%LOCALAPPDATA%\ImageStudio\peers` (images of other PCs, safe to delete).
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("ImageStudio").join("peers"))
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/png",
    }
}

/// What the gallery page asks the `peer` scheme for: `t/<id>/<path there>` (thumbnail) or `f/<id>/<path there>` (full).
pub struct ImageRequest {
    pub thumb: bool,
    pub id: String,
    pub path: String,
}

pub fn parse_request(uri_path: &str) -> Option<ImageRequest> {
    let decoded = percent_encoding::percent_decode_str(uri_path.trim_start_matches('/')).decode_utf8().ok()?;
    let (kind, rest) = decoded.split_once('/')?;
    let (id, path) = rest.split_once('/')?;
    let thumb = match kind {
        "t" => true,
        "f" => false,
        _ => return None,
    };
    (!id.is_empty() && !path.is_empty()).then(|| ImageRequest { thumb, id: id.to_string(), path: path.to_string() })
}

/// Bytes and content type of an image of another PC, from the cache or fetched (then cached).
pub async fn peer_image(ctx: &Ctx, request: &ImageRequest) -> Result<(Vec<u8>, String), ServiceError> {
    let settings = load_settings(&ctx.home);
    let peer = settings
        .peers
        .iter()
        .find(|peer| peer.id == request.id)
        .cloned()
        .ok_or_else(|| ServiceError::new(Category::NotFound, "등록되지 않은 PC입니다."))?;
    let digest = Sha256::digest(format!("{}|{}|{}", peer.url, request.thumb, request.path).as_bytes());
    let name: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    let cached = cache_dir().map(|dir| dir.join(&peer.id).join(name));
    if let Some(file) = &cached {
        if let (Ok(body), Ok(kind)) = (std::fs::read(file), std::fs::read_to_string(file.with_extension("type"))) {
            return Ok((body, kind));
        }
    }
    let token = {
        let (ctx, peer) = (ctx.clone(), peer.clone());
        tokio::task::spawn_blocking(move || token_of(&ctx, &peer))
            .await
            .map_err(|e| ServiceError::new(Category::Internal, format!("작업이 중단되었습니다: {e}")))??
    };
    let query = {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("path", &request.path);
        if request.thumb {
            query.append_pair("w", &THUMB_WIDTH.min(MAX_THUMB).to_string());
        }
        query.finish()
    };
    let url = format!("{}/v1/image?{query}", peer.url);
    let response = client(IMAGE_TIMEOUT)?.get(&url).bearer_auth(&token).send().await.map_err(|e| reach_error(&peer.url, &e))?;
    if !response.status().is_success() {
        return Err(ServiceError::new(Category::Provider, format!("{}: HTTP {}", peer.url, response.status().as_u16())));
    }
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| content_type(Path::new(&request.path)).to_string());
    let body = response.bytes().await.map_err(|e| reach_error(&peer.url, &e))?.to_vec();
    if let Some(file) = cached {
        // A cache that cannot be written only means the image is fetched again next time.
        if file.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok()) {
            let _ = std::fs::write(file.with_extension("type"), &kind);
            let _ = std::fs::write(&file, &body);
        }
    }
    Ok((body, kind))
}
