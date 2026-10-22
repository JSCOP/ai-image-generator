//! "이 PC 공유": a small read-only HTTP server so other PCs on the private network can show this PC's galleries.
//! Only loopback and Tailscale addresses may connect, every request needs the share token, nothing is ever written,
//! and only image files inside this PC's listed galleries are served.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use image_core::error::Category;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use super::gallery::{gallery_roots, gallery_with, DEFAULT_LIMIT};
use super::index::load_index;
use super::settings::load_settings;
use super::{Ctx, ServiceError};

/// Key-store id of this PC's share token.
pub const TOKEN_ID: &str = "gallery-share";
/// Longest side of a thumbnail (`/v1/image?w=`).
pub const MAX_THUMB: u32 = 1024;
const HEAD_LIMIT: usize = 16 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Loopback (this PC) and Tailscale (100.64.0.0/10, fd7a:115c:a1e0::/48); nothing from the LAN or the internet.
pub fn allowed_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64),
        IpAddr::V6(v6) => {
            let s = v6.segments();
            v6.is_loopback() || (s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0) || v6.to_ipv4_mapped().is_some_and(|v4| allowed_peer(IpAddr::V4(v4)))
        }
    }
}

/// This PC's Tailscale address: the local side of a route to Tailscale's own DNS address (no packet is sent).
pub fn tailscale_address() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("100.100.100.100:53").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (allowed_peer(ip) && !ip.is_loopback()).then_some(ip)
}

pub fn machine_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "이 PC".to_string())
}

/// The share token, made on first use.
pub fn share_token(ctx: &Ctx) -> Result<String, ServiceError> {
    if let Some(token) = ctx.secrets.get(TOKEN_ID)? {
        return Ok(token.expose().to_string());
    }
    new_share_token(ctx)
}

/// A new token: PCs that used the old one can no longer read until they get the new one.
pub fn new_share_token(ctx: &Ctx) -> Result<String, ServiceError> {
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    ctx.secrets.set(TOKEN_ID, &token)?;
    Ok(token)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    pub name: String,
    pub version: String,
}

/// Binds every IPv4 address (Tailscale's may change); the address check refuses everything but Tailscale and loopback.
pub async fn bind(port: u16) -> Result<TcpListener, ServiceError> {
    TcpListener::bind(("0.0.0.0", port)).await.map_err(|e| {
        ServiceError::new(Category::InvalidRequest, format!("공유 포트 {port}를 열 수 없습니다(다른 프로그램이 쓰는 중일 수 있습니다): {e}"))
    })
}

/// Serves until `cancel`; each connection is one request.
pub async fn serve(listener: TcpListener, home: PathBuf, token: String, cancel: CancellationToken) {
    loop {
        let accepted = tokio::select! {
            _ = cancel.cancelled() => return,
            accepted = listener.accept() => accepted,
        };
        let Ok((stream, peer)) = accepted else { continue };
        let (home, token) = (home.clone(), token.clone());
        tokio::spawn(async move {
            let _ = handle(stream, peer, &home, &token).await;
        });
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, value: &impl Serialize) -> Reply {
        Reply { status, content_type: "application/json; charset=utf-8", body: serde_json::to_vec(value).unwrap_or_default() }
    }

    fn error(status: u16, message: &str) -> Reply {
        Reply::json(status, &serde_json::json!({ "error": message }))
    }
}

async fn handle(mut stream: TcpStream, peer: SocketAddr, home: &Path, token: &str) -> std::io::Result<()> {
    if !allowed_peer(peer.ip()) {
        return Ok(());
    }
    let head = match tokio::time::timeout(READ_TIMEOUT, read_head(&mut stream)).await {
        Ok(Ok(Some(head))) => head,
        _ => return Ok(()),
    };
    let reply = respond(&head, home, token).await;
    let status_text = match reply.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let header = format!(
        "HTTP/1.1 {} {status_text}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&reply.body).await?;
    stream.shutdown().await
}

/// Request line and headers (up to the blank line); None when the client sends too much or closes first.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(Some(String::from_utf8_lossy(&buf[..end]).to_string()));
        }
        if buf.len() > HEAD_LIMIT {
            return Ok(None);
        }
    }
}

/// Constant-time comparison, so the token cannot be guessed byte by byte from response times.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn respond(head: &str, home: &Path, token: &str) -> Reply {
    let mut lines = head.split("\r\n");
    let mut request = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (request.next().unwrap_or_default(), request.next().unwrap_or_default());
    if method != "GET" {
        return Reply::error(405, "GET only");
    }
    let bearer = lines.filter_map(|line| line.split_once(':')).find(|(name, _)| name.trim().eq_ignore_ascii_case("authorization"));
    let given = bearer.and_then(|(_, value)| value.trim().strip_prefix("Bearer ").map(str::trim)).unwrap_or_default();
    if token.is_empty() || !same_token(given, token) {
        return Reply::error(401, "공유 토큰이 맞지 않습니다.");
    }
    let (route, query) = target.split_once('?').unwrap_or((target, ""));
    let params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
    let param = |name: &str| params.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
    let home = home.to_path_buf();
    match route {
        "/v1/info" => Reply::json(200, &ShareInfo { name: machine_name(), version: env!("CARGO_PKG_VERSION").to_string() }),
        "/v1/gallery" => {
            let limit = param("limit").and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_LIMIT);
            let view = tokio::task::spawn_blocking(move || gallery_with(&load_settings(&home), &load_index(&home), limit)).await;
            match view {
                Ok(view) => Reply::json(200, &view),
                Err(_) => Reply::error(500, "갤러리를 읽지 못했습니다."),
            }
        }
        "/v1/image" => {
            let Some(path) = param("path") else { return Reply::error(400, "path가 없습니다.") };
            let width = param("w").and_then(|v| v.parse::<u32>().ok()).map(|w| w.clamp(32, MAX_THUMB));
            tokio::task::spawn_blocking(move || image_reply(&home, Path::new(&path), width))
                .await
                .unwrap_or_else(|_| Reply::error(500, "이미지를 읽지 못했습니다."))
        }
        _ => Reply::error(404, "없는 주소입니다."),
    }
}

fn image_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Whether `path` is an image file inside one of this PC's listed galleries (after resolving `..` and links).
pub fn shared_image(home: &Path, path: &Path) -> Option<PathBuf> {
    image_type(path)?;
    let file = std::fs::canonicalize(path).ok().filter(|file| file.is_file())?;
    let roots = gallery_roots(&load_settings(home), &load_index(home));
    roots.iter().filter_map(|root| std::fs::canonicalize(root).ok()).any(|root| file.starts_with(&root)).then_some(file)
}

fn image_reply(home: &Path, path: &Path, width: Option<u32>) -> Reply {
    let Some(file) = shared_image(home, path) else { return Reply::error(403, "공유한 갤러리 안의 이미지만 볼 수 있습니다.") };
    let content_type = image_type(&file).unwrap_or("application/octet-stream");
    let Some(width) = width else {
        return match std::fs::read(&file) {
            Ok(body) => Reply { status: 200, content_type, body },
            Err(_) => Reply::error(404, "이미지를 읽지 못했습니다."),
        };
    };
    match thumbnail(&file, width) {
        Some((body, content_type)) => Reply { status: 200, content_type, body },
        None => Reply::error(500, "미리보기를 만들지 못했습니다."),
    }
}

/// JPEG for opaque images, PNG when a pixel is see-through (so the checkerboard still shows).
pub fn thumbnail(file: &Path, width: u32) -> Option<(Vec<u8>, &'static str)> {
    let image = image::open(file).ok()?;
    // Never enlarged: a small image is sent at its own size.
    let small = if image.width() <= width && image.height() <= width { image } else { image.thumbnail(width, width) };
    let mut out = std::io::Cursor::new(Vec::new());
    if small.color().has_alpha() && small.to_rgba8().pixels().any(|p| p[3] < 255) {
        small.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some((out.into_inner(), "image/png"))
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82).encode_image(&small.to_rgb8()).ok()?;
        Some((out.into_inner(), "image/jpeg"))
    }
}
