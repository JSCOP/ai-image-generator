//! Per-kind request building, response parsing, model lists and capabilities (spec §6.3, §6.4).
//! CLIProxyAPI and other OpenAI-compatible servers keep the stage 1 behaviour exactly.

use crate::catalog::{self, backend, Backend};
use crate::connection::Connection;
use crate::error::{Category, EngineError};
use crate::http::Http;
use crate::profiles::ProviderKind;
use crate::provider::{classify_http_error, mime_type, read_reference, Action, Background, ImageRequest, ProviderCall};
use crate::size::{aspect_line, gemini_aspect_ratio, gemini_image_size, Size};
use base64::Engine;
use serde_json::{json, Map, Value};
use std::path::Path;

const UNSUPPORTED_MODEL: &str =
    "is not an image model this CLI can call (gpt-image-*, gemini-*-image, grok-imagine-*); the legacy Responses API path was removed";

/// OpenAI documents up to 16 input images for GPT image edits.
const OPENAI_MAX_REFERENCES: usize = 16;
const GEMINI_MAX_MODEL_PAGES: usize = 10;
/// Gemini finish reasons that mean "no image, and asking again will not help".
const GEMINI_BLOCKED: [&str; 5] = ["IMAGE_SAFETY", "IMAGE_PROHIBITED_CONTENT", "IMAGE_OTHER", "NO_IMAGE", "IMAGE_RECITATION"];

fn lower(model: &str) -> String {
    model.to_lowercase()
}

fn is_gpt_image(model: &str) -> bool {
    lower(model).starts_with("gpt-image-")
}

fn is_grok_2_0(model: &str) -> bool {
    let m = lower(model);
    m == "grok-imagine-image-2.0" || m.starts_with("grok-imagine-image-2.0-")
}

/// Longest accepted model id.
const MAX_MODEL_ID_LEN: usize = 128;

/// Model ids go into URL paths, so only `A-Z a-z 0-9 . _ -` are allowed and never `..`
/// (no `/`, `#`, `?`, `%`, `:`, whitespace). Every catalog and documented provider id fits.
pub fn validate_model_id(model: &str) -> Result<(), EngineError> {
    let charset = model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if model.is_empty() || model.len() > MAX_MODEL_ID_LEN || !charset || model.contains("..") {
        return Err(EngineError::new(
            Category::InvalidRequest,
            format!(
                "모델 ID {model:?}를 쓸 수 없습니다: 영문자, 숫자, '.', '_', '-'만 쓸 수 있고 '..'은 안 됩니다(최대 {MAX_MODEL_ID_LEN}자)."
            ),
        ));
    }
    Ok(())
}

/// Kind–model mismatch (e.g. a Gemini profile asked for `gpt-image-2.5`) is rejected before any paid call.
pub fn model_supported(kind: ProviderKind, model: &str) -> Result<(), EngineError> {
    validate_model_id(model)?;
    let (ok, allowed) = match kind {
        ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => {
            return match backend(model) {
                Backend::Unsupported => Err(EngineError::new(Category::Unsupported, format!("{model} {UNSUPPORTED_MODEL}"))),
                _ => Ok(()),
            }
        }
        ProviderKind::Openai if lower(model) == "gpt-image-2.5" => {
            // Not an official id (only -sunburst/-flare and their snapshots); never swapped silently.
            return Err(EngineError::new(
                Category::Unsupported,
                format!(
                    "{model} 모델은 {} 프로필에서 쓸 수 없습니다(OpenAI 공식 API에 없는 ID). gpt-image-2.5-sunburst 또는 gpt-image-2.5-flare를 지정해 주세요.",
                    kind.label()
                ),
            ));
        }
        ProviderKind::Openai => (is_gpt_image(model), "gpt-image-*"),
        ProviderKind::Gemini => (backend(model) == Backend::Gemini, "gemini-*image*"),
        // Video models (grok-imagine-video*) share the prefix but are not image models.
        ProviderKind::Xai => (lower(model).starts_with("grok-imagine-image"), "grok-imagine-image*"),
    };
    if ok {
        Ok(())
    } else {
        Err(EngineError::new(
            Category::Unsupported,
            format!("{model} 모델은 {} 프로필에서 쓸 수 없습니다({allowed} 모델만 가능).", kind.label()),
        ))
    }
}

/// How many reference images this kind/model accepts (0 = no reference editing).
pub fn reference_limit(kind: ProviderKind, model: &str) -> usize {
    match kind {
        ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible if catalog::supports_references(model) => usize::MAX,
        ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => 0,
        ProviderKind::Openai => OPENAI_MAX_REFERENCES,
        ProviderKind::Gemini => usize::MAX,
        ProviderKind::Xai if is_grok_2_0(model) => 5,
        ProviderKind::Xai => 1,
    }
}

pub fn supports_references(kind: ProviderKind, model: &str) -> bool {
    model_supported(kind, model).is_ok() && reference_limit(kind, model) > 0
}

pub fn supports_native_transparency(kind: ProviderKind, model: &str) -> bool {
    match kind {
        ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => catalog::supports_native_transparency(model),
        ProviderKind::Openai => {
            let m = lower(model);
            m.starts_with("gpt-image-2.5-sunburst") || m.starts_with("gpt-image-2.5-flare")
        }
        ProviderKind::Gemini | ProviderKind::Xai => false,
    }
}

/// Capability checks that need neither files nor network (the request may still fail at the provider).
pub fn check_request(kind: ProviderKind, req: &ImageRequest) -> Result<(), EngineError> {
    model_supported(kind, &req.image_model)?;
    let model = &req.image_model;
    let limit = reference_limit(kind, model);
    if !req.references.is_empty() && limit == 0 {
        return Err(EngineError::new(
            Category::Unsupported,
            format!("{model} reference-image editing is not yet supported by this CLI; use a Gemini image model or gpt-image model"),
        ));
    }
    if req.references.len() > limit {
        return Err(EngineError::new(
            Category::Unsupported,
            format!("{model} accepts at most {limit} reference image(s) on this connection; got {}", req.references.len()),
        ));
    }
    if req.action == Action::Edit && req.references.is_empty() {
        if is_gpt_image(model) {
            return Err(EngineError::new(Category::InvalidRequest, "GPT image edits require at least one reference image"));
        }
        if kind == ProviderKind::Xai {
            return Err(EngineError::new(Category::InvalidRequest, "Grok image edits require at least one reference image"));
        }
    }
    if req.background == Background::Transparent && !supports_native_transparency(kind, model) {
        return Err(EngineError::new(
            Category::Unsupported,
            format!(
                "background \"transparent\" needs a model with native transparency (gpt-image-2.5, gpt-image-2.5-sunburst or gpt-image-2.5-flare); {model} does not support it"
            ),
        ));
    }
    Ok(())
}

pub fn build_call(conn: &Connection, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    check_request(conn.kind, req)?;
    match conn.kind {
        ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => match backend(&req.image_model) {
            Backend::Gemini => gemini_call(root_base_url(&conn.base_url), req),
            _ => openai_images_call(&conn.base_url, req),
        },
        ProviderKind::Openai => openai_images_call(&conn.base_url, req),
        ProviderKind::Gemini => gemini_call(&conn.base_url, req),
        ProviderKind::Xai => xai_call(&conn.base_url, req),
    }
}

fn root_base_url(base_url: &str) -> &str {
    base_url.strip_suffix("/v1").unwrap_or(base_url)
}

/// `base` plus path segments, each percent-encoded as one segment (a `/`, `?` or `#` inside cannot change the path).
fn url_with_segments(base: &str, segments: &[&str]) -> Result<String, EngineError> {
    let bad = || EngineError::new(Category::InvalidRequest, format!("base_url 형식이 올바르지 않습니다: {base}"));
    let mut url = reqwest::Url::parse(base).map_err(|_| bad())?;
    url.path_segments_mut().map_err(|_| bad())?.pop_if_empty().extend(segments);
    Ok(url.into())
}

fn data_url(path: &Path) -> Result<String, EngineError> {
    Ok(format!("data:{};base64,{}", mime_type(path), read_reference(path)?))
}

fn gemini_call(root: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
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
    let url = url_with_segments(root, &["v1beta", "models", &format!("{}:generateContent", req.image_model)])?;
    Ok(ProviderCall { url, body, backend: Backend::Gemini })
}

/// Stage 1 `/images/*` body: GPT gets PNG output and JSON `images` edits, Grok only `b64_json` generation.
fn openai_images_call(base_url: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    let is_gpt = is_gpt_image(&req.image_model);
    let mut body = Map::new();
    body.insert("model".into(), json!(req.image_model));
    let prompt = if is_gpt {
        format!(
            "{}

{}",
            req.prompt,
            aspect_line(req.size)
        )
    } else {
        req.prompt.clone()
    };
    body.insert("prompt".into(), json!(prompt));
    body.insert("n".into(), json!(1));
    body.insert("size".into(), json!(req.size.padded16().to_string()));
    body.insert("quality".into(), json!(req.quality));
    if is_gpt {
        body.insert("output_format".into(), json!("png"));
        if req.background != Background::Auto {
            body.insert("background".into(), json!(req.background.as_str()));
        }
        if !req.references.is_empty() {
            let images = req.references.iter().map(|p| Ok(json!({"image_url": data_url(p)?}))).collect::<Result<Vec<_>, EngineError>>()?;
            body.insert("images".into(), Value::Array(images));
        }
    } else {
        body.insert("response_format".into(), json!("b64_json"));
    }
    let endpoint = if is_gpt && !req.references.is_empty() { "edits" } else { "generations" };
    Ok(ProviderCall { url: format!("{base_url}/images/{endpoint}"), body: Value::Object(body), backend: Backend::OpenAiImages })
}

/// xAI ratios as (label, width, height, grok-imagine-image-2.0 only).
const XAI_RATIOS: [(&str, f64, f64, bool); 11] = [
    ("1:1", 1.0, 1.0, false),
    ("3:4", 3.0, 4.0, false),
    ("4:3", 4.0, 3.0, false),
    ("9:16", 9.0, 16.0, false),
    ("16:9", 16.0, 9.0, false),
    ("2:3", 2.0, 3.0, false),
    ("3:2", 3.0, 2.0, false),
    ("1:2", 1.0, 2.0, false),
    ("2:1", 2.0, 1.0, false),
    ("21:9", 21.0, 9.0, true),
    ("5:2", 5.0, 2.0, true),
];

/// Closest documented ratio (compared on a log scale so 2:1 and 1:2 are equally far from 1:1).
pub fn xai_aspect_ratio(size: Size, model: &str) -> &'static str {
    let wanted = (size.width as f64 / size.height as f64).ln();
    let newest = is_grok_2_0(model);
    let mut best = ("1:1", f64::INFINITY);
    for (label, w, h, only_2_0) in XAI_RATIOS {
        if only_2_0 && !newest {
            continue;
        }
        let distance = (wanted - (w / h).ln()).abs();
        if distance < best.1 {
            best = (label, distance);
        }
    }
    best.0
}

pub fn xai_resolution(size: Size) -> &'static str {
    if size.width.max(size.height) <= 1536 {
        "1k"
    } else {
        "2k"
    }
}

fn xai_call(base_url: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError> {
    let mut body = Map::new();
    body.insert("model".into(), json!(req.image_model));
    body.insert("prompt".into(), json!(req.prompt));
    body.insert("n".into(), json!(1));
    body.insert("response_format".into(), json!("b64_json"));
    body.insert("aspect_ratio".into(), json!(xai_aspect_ratio(req.size, &req.image_model)));
    body.insert("resolution".into(), json!(xai_resolution(req.size)));
    if is_grok_2_0(&req.image_model) {
        let quality = if req.quality == "low" { "low" } else { "medium" };
        body.insert("quality".into(), json!(quality));
    }
    match req.references.as_slice() {
        [] => {}
        [single] => {
            body.insert("image".into(), json!({"url": data_url(single)?}));
        }
        many => {
            let images = many.iter().map(|p| Ok(json!({"url": data_url(p)?}))).collect::<Result<Vec<_>, EngineError>>()?;
            body.insert("images".into(), Value::Array(images));
        }
    }
    let endpoint = if req.references.is_empty() { "generations" } else { "edits" };
    Ok(ProviderCall { url: format!("{base_url}/images/{endpoint}"), body: Value::Object(body), backend: Backend::OpenAiImages })
}

fn no_image() -> EngineError {
    EngineError::new(Category::Provider, "No image data found in provider response.")
}

/// Image bytes and the provider's reported model (`model` or Gemini `modelVersion`).
pub fn extract_image(kind: ProviderKind, backend: &Backend, body: &[u8]) -> Result<(Vec<u8>, Option<String>), EngineError> {
    let response: Value = serde_json::from_slice(body).map_err(|_| no_image())?;
    let candidates = || response["candidates"].as_array().into_iter().flatten();
    let (encoded, model) = match backend {
        Backend::Gemini => {
            // Gemini official may return thinking drafts (`thought: true`) before the final image.
            let skip_thoughts = kind == ProviderKind::Gemini;
            let data = candidates()
                .flat_map(|c| c["content"]["parts"].as_array().into_iter().flatten())
                .filter(|p| !(skip_thoughts && p["thought"].as_bool() == Some(true)))
                .filter_map(|p| p.get("inlineData").or_else(|| p.get("inline_data")))
                .find_map(|inline| inline["data"].as_str().map(str::to_string));
            if data.is_none() && kind == ProviderKind::Gemini {
                if let Some(err) = gemini_block(candidates()).or_else(|| gemini_prompt_block(&response)) {
                    return Err(err);
                }
            }
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

fn gemini_block<'a>(mut candidates: impl Iterator<Item = &'a Value>) -> Option<EngineError> {
    candidates.find_map(|c| {
        let reason = c["finishReason"].as_str().filter(|r| GEMINI_BLOCKED.contains(r))?;
        let detail = c["finishMessage"].as_str().map(|m| format!(": {}", m.trim())).unwrap_or_default();
        let mut err =
            EngineError::new(Category::Provider, format!("Gemini returned no image (finishReason {reason}){detail}")).retryable(false);
        err.provider_code = Some(reason.to_string());
        Some(err)
    })
}

/// The prompt itself was blocked (`promptFeedback.blockReason`, no candidates): asking again will not help.
fn gemini_prompt_block(response: &Value) -> Option<EngineError> {
    let reason = response["promptFeedback"]["blockReason"].as_str().map(str::trim).filter(|r| !r.is_empty())?;
    let mut err = EngineError::new(
        Category::Provider,
        format!("Gemini가 요청을 차단해 이미지를 만들지 않았습니다(blockReason {reason}). 프롬프트나 참조 이미지를 바꿔 주세요."),
    )
    .retryable(false);
    err.provider_code = Some(reason.to_string());
    Some(err)
}

/// Model ids the connection lists. Official kinds return image models only; proxies return everything (at most 1000).
pub async fn list_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError> {
    let prefix = match conn.kind {
        ProviderKind::Gemini => return gemini_models(http, conn).await,
        ProviderKind::Openai => Some("gpt-image-"),
        ProviderKind::Xai => Some("grok-imagine-image"),
        ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => None,
    };
    let parsed = get_json(http, conn, &format!("{}/models", conn.base_url)).await?;
    Ok(parsed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().filter(|s| !s.is_empty()))
        .filter(|id| prefix.is_none_or(|p| id.to_lowercase().starts_with(p)))
        .map(str::to_string)
        .take(1000)
        .collect())
}

async fn get_json(http: &Http, conn: &Connection, url: &str) -> Result<Value, EngineError> {
    let (status, body) = http.get(url, conn.auth()).await?;
    if status != 200 {
        return Err(classify_http_error(status, &body, conn.api_key.as_ref()));
    }
    serde_json::from_slice(&body)
        .map_err(|_| EngineError::new(Category::Provider, "모델 목록 응답을 해석하지 못했습니다.").with_status(status))
}

/// `GET {base}/v1beta/models` page by page (at most 10 pages), keeping `generateContent` image models.
async fn gemini_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError> {
    let endpoint = format!("{}/v1beta/models", conn.base_url);
    let mut ids = Vec::new();
    let mut token: Option<String> = None;
    for _ in 0..GEMINI_MAX_MODEL_PAGES {
        let mut url = reqwest::Url::parse(&endpoint)
            .map_err(|_| EngineError::new(Category::InvalidRequest, format!("base_url 형식이 올바르지 않습니다: {}", conn.base_url)))?;
        url.query_pairs_mut().append_pair("pageSize", "1000");
        if let Some(token) = &token {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let page = get_json(http, conn, url.as_str()).await?;
        for model in page["models"].as_array().into_iter().flatten() {
            let Some(name) = model["name"].as_str() else { continue };
            let id = name.strip_prefix("models/").unwrap_or(name);
            let generates =
                model["supportedGenerationMethods"].as_array().into_iter().flatten().any(|m| m.as_str() == Some("generateContent"));
            if generates && id.to_lowercase().contains("image") && !ids.iter().any(|known| known == id) {
                ids.push(id.to_string());
            }
        }
        token = page["nextPageToken"].as_str().filter(|t| !t.is_empty()).map(str::to_string);
        if token.is_none() {
            break;
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::url_with_segments;

    #[test]
    fn segments_cannot_change_the_path() {
        let url = url_with_segments("http://127.0.0.1:9", &["v1beta", "models", "x/../../admin?#:generateContent"]).unwrap();
        assert_eq!(url, "http://127.0.0.1:9/v1beta/models/x%2F..%2F..%2Fadmin%3F%23:generateContent");
        let url = url_with_segments("https://h/base/", &["v1beta", "models", "gemini-3.1-flash-image:generateContent"]).unwrap();
        assert_eq!(url, "https://h/base/v1beta/models/gemini-3.1-flash-image:generateContent");
    }
}
