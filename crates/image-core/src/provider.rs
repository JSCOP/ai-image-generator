//! Request types, HTTP error classification (`scripts/gen_image.py`) and the per-kind entry points in `adapter`.

use crate::adapter;
use crate::catalog::Backend;
use crate::connection::{Connection, Secret};
use crate::error::{redact, Category, EngineError};
use crate::http::Http;
use crate::profiles::ProviderKind;
use crate::size::Size;
use base64::Engine;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Auto,
    Generate,
    Edit,
}

impl Action {
    pub fn parse(text: &str) -> Option<Action> {
        match text {
            "auto" => Some(Action::Auto),
            "generate" => Some(Action::Generate),
            "edit" => Some(Action::Edit),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Action::Auto => "auto",
            Action::Generate => "generate",
            Action::Edit => "edit",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Background {
    Auto,
    Opaque,
    Transparent,
}

impl Background {
    pub fn parse(text: &str) -> Option<Background> {
        match text {
            "auto" => Some(Background::Auto),
            "opaque" => Some(Background::Opaque),
            "transparent" => Some(Background::Transparent),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Background::Auto => "auto",
            Background::Opaque => "opaque",
            Background::Transparent => "transparent",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImageRequest {
    pub prompt: String,
    pub image_model: String,
    pub size: Size,
    pub quality: String,
    pub action: Action,
    pub references: Vec<PathBuf>,
    pub background: Background,
    /// OpenAI-style mask over the first reference (alpha 0 = edit); see `mask`.
    pub mask: Option<PathBuf>,
}

#[derive(Debug)]
pub struct ProviderCall {
    pub url: String,
    pub body: Value,
    pub backend: Backend,
}

pub fn mime_type(path: &Path) -> &'static str {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    }
}

pub(crate) fn read_reference(path: &Path) -> Result<String, EngineError> {
    let bytes = std::fs::read(path)
        .map_err(|e| EngineError::new(Category::InvalidRequest, format!("reference image not readable: {}: {e}", path.display())))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Request for this connection's kind; see `adapter::build_call`.
pub fn build_call(conn: &Connection, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    adapter::build_call(conn, req)
}

/// Image bytes and the provider's reported model; see `adapter::extract_image`.
pub fn extract_image(kind: ProviderKind, backend: &Backend, body: &[u8]) -> Result<(Vec<u8>, Option<String>), EngineError> {
    adapter::extract_image(kind, backend, body)
}

const QUOTA_MARKERS: [&str; 3] = ["spending-limit", "quota", "billing"];

pub fn classify_http_error(status: u16, body: &[u8], key: Option<&Secret>) -> EngineError {
    let parsed: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let error = &parsed["error"];
    let provider_code = ["code", "status", "type"].iter().find_map(|k| error[*k].as_str().filter(|s| !s.is_empty())).map(str::to_string);
    let detail = match error["message"].as_str() {
        Some(message) => message.to_string(),
        None => String::from_utf8_lossy(body).chars().take(200).collect(),
    };
    let haystack = format!("{} {}", provider_code.as_deref().unwrap_or(""), detail).to_lowercase();
    let category = if QUOTA_MARKERS.iter().any(|m| haystack.contains(m)) {
        Category::Quota
    } else {
        match status {
            401 => Category::Auth,
            402 => Category::Quota,
            403 => Category::Permission,
            404 => Category::NotFound,
            408 | 425 => Category::Timeout,
            429 => Category::RateLimit,
            500..=599 => Category::Provider,
            _ => Category::InvalidRequest,
        }
    };
    let retryable = category != Category::Quota && matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504);
    let code = provider_code.as_deref().map(|c| format!(" {c}")).unwrap_or_default();
    let message = redact(&format!("HTTP {status}{code}: {}", detail.trim()), key);
    EngineError { category, provider_code: provider_code.map(|c| redact(&c, key)), http_status: Some(status), retryable, message }
}

/// Model ids for this connection's kind; see `adapter::list_models`.
pub async fn list_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError> {
    adapter::list_models(http, conn).await
}
