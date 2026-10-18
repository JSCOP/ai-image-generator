//! Provider request bodies, response parsing and HTTP error classification (`scripts/gen_image.py`).

use crate::catalog::{backend, Backend};
use crate::connection::{Connection, Secret};
use crate::error::{redact, Category, EngineError};
use crate::http::Http;
use crate::size::{gemini_aspect_ratio, gemini_image_size, Size};
use base64::Engine;
use serde_json::{json, Map, Value};
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

fn read_reference(path: &Path) -> Result<String, EngineError> {
    let bytes = std::fs::read(path)
        .map_err(|e| EngineError::new(Category::InvalidRequest, format!("reference image not readable: {}: {e}", path.display())))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn root_base_url(base_url: &str) -> &str {
    base_url.strip_suffix("/v1").unwrap_or(base_url)
}

pub fn build_call(base_url: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    match backend(&req.image_model) {
        Backend::Gemini => build_gemini(base_url, req),
        Backend::OpenAiImages => build_openai_images(base_url, req),
        Backend::Unsupported => Err(EngineError::new(
            Category::Unsupported,
            format!(
                "{} is not an image model this CLI can call (gpt-image-*, gemini-*-image, grok-imagine-*); the legacy Responses API path was removed",
                req.image_model
            ),
        )),
    }
}

fn build_gemini(base_url: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    let mut parts = vec![json!({"text": req.prompt})];
    for path in &req.references {
        parts.push(json!({"inlineData": {"mimeType": mime_type(path), "data": read_reference(path)?}}));
    }
    let body = json!({
        "contents": [{"role": "user", "parts": parts}],
        "generationConfig": {
            "responseModalities": ["IMAGE", "TEXT"],
            "imageConfig": {"aspectRatio": gemini_aspect_ratio(req.size), "imageSize": gemini_image_size(req.size)},
        },
    });
    let url = format!("{}/v1beta/models/{}:generateContent", root_base_url(base_url), req.image_model);
    Ok(ProviderCall { url, body, backend: Backend::Gemini })
}

fn build_openai_images(base_url: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    let is_gpt = req.image_model.to_lowercase().starts_with("gpt-image-");
    if !req.references.is_empty() && !is_gpt {
        return Err(EngineError::new(
            Category::Unsupported,
            format!(
                "{} reference-image editing is not yet supported by this CLI; use a Gemini image model or gpt-image model",
                req.image_model
            ),
        ));
    }
    if is_gpt && req.action == Action::Edit && req.references.is_empty() {
        return Err(EngineError::new(Category::InvalidRequest, "GPT image edits require at least one reference image"));
    }
    let mut body = Map::new();
    body.insert("model".into(), json!(req.image_model));
    body.insert("prompt".into(), json!(req.prompt));
    body.insert("n".into(), json!(1));
    body.insert("size".into(), json!(req.size.padded16().to_string()));
    body.insert("quality".into(), json!(req.quality));
    if is_gpt {
        body.insert("output_format".into(), json!("png"));
        if req.background != Background::Auto {
            body.insert("background".into(), json!(req.background.as_str()));
        }
        if !req.references.is_empty() {
            let images = req
                .references
                .iter()
                .map(|p| Ok(json!({"image_url": format!("data:{};base64,{}", mime_type(p), read_reference(p)?)})))
                .collect::<Result<Vec<_>, EngineError>>()?;
            body.insert("images".into(), Value::Array(images));
        }
    } else {
        body.insert("response_format".into(), json!("b64_json"));
    }
    let endpoint = if is_gpt && !req.references.is_empty() { "edits" } else { "generations" };
    Ok(ProviderCall { url: format!("{base_url}/images/{endpoint}"), body: Value::Object(body), backend: Backend::OpenAiImages })
}

/// Image bytes and the provider's reported model (`model` or Gemini `modelVersion`).
pub fn extract_image(backend: &Backend, body: &[u8]) -> Result<(Vec<u8>, Option<String>), EngineError> {
    let no_image = || EngineError::new(Category::Provider, "No image data found in provider response.");
    let response: Value = serde_json::from_slice(body).map_err(|_| no_image())?;
    let (encoded, model) = match backend {
        Backend::Gemini => {
            let parts = response["candidates"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|c| c["content"]["parts"].as_array().into_iter().flatten());
            let data = parts
                .filter_map(|p| p.get("inlineData").or_else(|| p.get("inline_data")))
                .find_map(|inline| inline["data"].as_str().map(str::to_string));
            (data, response["modelVersion"].as_str())
        }
        _ => {
            let data = response["data"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|item| item["b64_json"].as_str().filter(|s| !s.is_empty()).map(str::to_string));
            (data, response["model"].as_str())
        }
    };
    let encoded = encoded.ok_or_else(no_image)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| EngineError::new(Category::Provider, "provider image data is not valid base64"))?;
    let model = model.map(str::trim).filter(|m| !m.is_empty()).map(str::to_string);
    Ok((bytes, model))
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

/// `GET <base>/models` ids (at most 1000).
pub async fn list_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError> {
    let (status, body) = http.get(&format!("{}/models", conn.base_url), conn.auth()).await?;
    if status != 200 {
        return Err(classify_http_error(status, &body, conn.api_key.as_ref()));
    }
    let parsed: Value = serde_json::from_slice(&body)
        .map_err(|_| EngineError::new(Category::Provider, "모델 목록 응답을 해석하지 못했습니다.").with_status(status))?;
    Ok(parsed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().filter(|s| !s.is_empty()).map(str::to_string))
        .take(1000)
        .collect())
}
