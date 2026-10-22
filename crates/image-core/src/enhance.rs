//! Prompt enhancement: a chat model rewrites the user's idea (any language) as one detailed English image prompt.
//! Text only; nothing is generated or saved.

use crate::catalog::is_listable_image_model;
use crate::connection::Connection;
use crate::error::{Category, EngineError};
use crate::http::Http;
use crate::profiles::ProviderKind;
use crate::provider::classify_http_error;
use serde_json::{json, Value};

/// Suggested chat model when the user has not picked one.
pub const DEFAULT_TEXT_MODEL: &str = "gpt-6-luna";
/// Longest prompt (characters) sent for enhancement.
pub const MAX_PROMPT_CHARS: usize = 4000;
const MAX_MODEL_ID_LEN: usize = 128;

const SYSTEM_PROMPT: &str = "You are an expert prompt writer for AI image generators (GPT Image, Gemini, Grok Imagine).
Rewrite the user's idea, written in any language, as ONE detailed English image prompt.
- Keep the user's subject, intent and every concrete detail; never change what is depicted.
- Add what makes the image clear: subject details, setting, composition and framing, lighting, color palette, mood, medium or art style, and camera or lens terms when photographic.
- Text that must appear inside the image stays in its original language, in double quotes.
- If things to avoid are given, end with one sentence starting with \"Avoid:\".
- Write 60 to 160 words as a single paragraph.
- Output only the prompt: no title, no explanation, no quotes around it, no markdown.";

pub struct EnhanceRequest<'a> {
    pub prompt: &'a str,
    /// Things to keep out of the image ("피할 것"); may be empty.
    pub avoid: &'a str,
    /// Image model the prompt is for, as a hint to the writer.
    pub image_model: Option<&'a str>,
    /// Chat model that writes the prompt.
    pub text_model: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enhanced {
    pub prompt: String,
    /// Model id the provider reported, when it did.
    pub response_model: Option<String>,
}

/// Chat models worth offering for enhancement: no image, audio, speech, embedding or moderation models.
pub fn is_text_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    const SKIP: [&str; 10] = ["image", "tts", "voice", "audio", "whisper", "embedding", "dall-e", "moderation", "transcribe", "review"];
    !is_listable_image_model(id) && !SKIP.iter().any(|s| lower.contains(s))
}

fn check_model(model: &str) -> Result<(), EngineError> {
    if model.is_empty() || model.len() > MAX_MODEL_ID_LEN || model.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(EngineError::new(Category::InvalidRequest, format!("다듬기 모델 ID {model:?}를 쓸 수 없습니다.")));
    }
    Ok(())
}

fn unsupported_kind() -> EngineError {
    EngineError::new(
        Category::Unsupported,
        "공식 Gemini 연결은 아직 프롬프트 다듬기를 지원하지 않습니다. 프록시나 OpenAI 호환 연결을 골라 주세요.",
    )
}

/// One `POST {base}/chat/completions` call. Input is checked before any request.
pub async fn enhance_prompt(http: &Http, conn: &Connection, req: &EnhanceRequest<'_>) -> Result<Enhanced, EngineError> {
    let prompt = req.prompt.trim();
    if prompt.is_empty() {
        return Err(EngineError::new(Category::InvalidRequest, "다듬을 프롬프트가 비어 있습니다."));
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(EngineError::new(Category::InvalidRequest, format!("프롬프트가 너무 깁니다(최대 {MAX_PROMPT_CHARS}자).")));
    }
    let model = req.text_model.trim();
    check_model(model)?;
    if conn.kind == ProviderKind::Gemini {
        return Err(unsupported_kind());
    }

    let mut user = format!("Idea:\n{prompt}");
    let avoid = req.avoid.trim();
    if !avoid.is_empty() {
        user.push_str(&format!("\n\nAvoid:\n{avoid}"));
    }
    if let Some(image_model) = req.image_model.map(str::trim).filter(|m| !m.is_empty()) {
        user.push_str(&format!("\n\nTarget image model: {image_model}"));
    }
    let body = json!({
        "model": model,
        "messages": [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": user}],
    });
    let (status, bytes) = http.post_json(&format!("{}/chat/completions", conn.base_url), &body, conn.auth()).await?;
    if status != 200 {
        return Err(classify_http_error(status, &bytes, conn.api_key.as_ref()));
    }
    let bad = || EngineError::new(Category::Provider, "다듬기 응답을 해석하지 못했습니다.").with_status(status);
    let parsed: Value = serde_json::from_slice(&bytes).map_err(|_| bad())?;
    let content = parsed["choices"][0]["message"]["content"].as_str().ok_or_else(bad)?;
    let prompt = clean(content);
    if prompt.is_empty() {
        return Err(
            EngineError::new(Category::Provider, "다듬기 모델이 빈 답을 보냈습니다. 다른 모델로 다시 시도해 주세요.").with_status(status)
        );
    }
    let response_model = parsed["model"].as_str().filter(|m| !m.is_empty()).map(str::to_string);
    Ok(Enhanced { prompt, response_model })
}

/// Drops a markdown fence and quotes some models wrap around the answer.
fn clean(text: &str) -> String {
    let mut s = text.trim();
    if let Some(inner) = s.strip_prefix("```") {
        let inner = inner.split_once('\n').map_or("", |(_, rest)| rest);
        s = inner.trim_end().strip_suffix("```").unwrap_or(inner).trim();
    }
    for (open, close) in [('"', '"'), ('\u{201c}', '\u{201d}')] {
        if s.len() > 1 && s.starts_with(open) && s.ends_with(close) {
            s = s[open.len_utf8()..s.len() - close.len_utf8()].trim();
        }
    }
    s.to_string()
}

/// Chat models the connection lists (proxies, OpenAI, xAI, OpenAI-compatible). Official Gemini: none.
pub async fn text_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError> {
    if conn.kind == ProviderKind::Gemini {
        return Ok(Vec::new());
    }
    let (status, body) = http.get(&format!("{}/models", conn.base_url), conn.auth()).await?;
    if status != 200 {
        return Err(classify_http_error(status, &body, conn.api_key.as_ref()));
    }
    let parsed: Value = serde_json::from_slice(&body)
        .map_err(|_| EngineError::new(Category::Provider, "모델 목록 응답을 해석하지 못했습니다.").with_status(status))?;
    let mut out: Vec<String> = Vec::new();
    for id in parsed["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str()) {
        if is_text_model(id) && check_model(id).is_ok() && !out.iter().any(|known| known == id) {
            out.push(id.to_string());
        }
        if out.len() >= 1000 {
            break;
        }
    }
    Ok(out)
}
