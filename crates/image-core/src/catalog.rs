//! Image model catalog and capability rules (`tools/image_core.py`, `docs/SSOT.md`).

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    Gemini,
    OpenAiImages,
    Unsupported,
}

pub fn backend(model: &str) -> Backend {
    let model = model.to_lowercase();
    if model.starts_with("gemini-") && model.contains("image") {
        Backend::Gemini
    } else if model.starts_with("grok-imagine-") || model.starts_with("gpt-image-") {
        Backend::OpenAiImages
    } else {
        Backend::Unsupported
    }
}

/// Filter used by `--list-models`.
pub fn is_listable_image_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    backend(id) == Backend::Gemini || lower.starts_with("gpt-image-") || lower.starts_with("grok-imagine-image")
}

/// Reference images this client can send (Grok edits are not implemented yet).
pub fn supports_references(model: &str) -> bool {
    backend(model) == Backend::Gemini || model.to_lowercase().starts_with("gpt-image-")
}

/// Native transparent output: documented for -sunburst/-flare; CLIProxyAPI's bare `gpt-image-2.5` returned real alpha (2026-10-19).
pub fn supports_native_transparency(model: &str) -> bool {
    matches!(model.to_lowercase().as_str(), "gpt-image-2.5" | "gpt-image-2.5-sunburst" | "gpt-image-2.5-flare")
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    pub reference_images: bool,
    pub native_transparency: bool,
}

const fn model(
    id: &'static str,
    label: &'static str,
    provider: &'static str,
    reference_images: bool,
    native_transparency: bool,
) -> ModelInfo {
    ModelInfo { id, label, provider, reference_images, native_transparency }
}

static MODELS: [ModelInfo; 9] = [
    model("gpt-image-2.5", "GPT Image 2.5 (최신)", "OpenAI", true, true),
    model("gpt-image-2.5-flare", "GPT Image 2.5 Flare", "OpenAI", true, true),
    model("gpt-image-2.5-sunburst", "GPT Image 2.5 Sunburst", "OpenAI", true, true),
    model("gpt-image-2", "GPT Image 2", "OpenAI", true, false),
    model("gpt-image-1.5", "GPT Image 1.5", "OpenAI", true, false),
    model("gemini-3.1-flash-image", "Gemini Flash Image", "Google Gemini", true, false),
    model("grok-imagine-image-2.0", "Grok Imagine 2.0", "xAI Grok", false, false),
    model("grok-imagine-image-quality", "Grok Imagine Quality", "xAI Grok", false, false),
    model("grok-imagine-image", "Grok Imagine", "xAI Grok", false, false),
];

pub fn catalog() -> &'static [ModelInfo] {
    &MODELS
}
