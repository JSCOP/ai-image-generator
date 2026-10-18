//! Generation screen service: builds a single-mode spec, runs it through `image-core`, and turns the engine's
//! progress lines into `RunEvent`s and its result JSON into a `RunSummary`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image_core::connection::{resolve, Resolver};
use image_core::error::{redact, Category};
use image_core::paths::{absolute, safe_topic_slug, topic_dirs};
use image_core::profiles::ProfileStore;
use image_core::runner::{run_spec_with, Progress, RunOptions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::settings::{load_settings, Settings};
use super::{Ctx, ServiceError};

/// Images per run (`tools/image_core.py` LIMITS.max_count).
pub const MAX_COUNT: u32 = 50;
/// Reference images per run.
pub const MAX_REFERENCES: usize = 4;
/// Largest reference image file.
pub const MAX_REFERENCE_BYTES: u64 = 10 * 1024 * 1024;
/// Longest side of a reference image (spec §8).
pub const MAX_REFERENCE_SIDE: u32 = 8192;
/// Prompt words used for an automatic topic name.
pub const TOPIC_WORDS: usize = 5;
/// Room left for the words in an automatic topic (80 minus `-YYYYMMDD-HHMMSS`).
const TOPIC_WORD_CHARS: usize = 64;
/// Retry backoff base when `AI_IMAGE_RETRY_BACKOFF_SEC` is not set (same default as the CLI).
const DEFAULT_BACKOFF_SEC: f64 = 5.0;

/// Generate form input.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerateInput {
    /// Profile name or id; `None` uses the resolution order (environment, then the default profile).
    #[serde(default)]
    pub profile: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub avoid: String,
    pub image_model: String,
    pub size: String,
    pub quality: String,
    pub count: u32,
    #[serde(default)]
    pub references: Vec<PathBuf>,
    /// `auto`, `opaque` or `transparent`.
    #[serde(default = "auto")]
    pub background: String,
    /// Existing topic to continue; `None` names a new one from the prompt.
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub resume: bool,
}

fn auto() -> String {
    "auto".into()
}

/// Progress sent to the screen, as `{"event": "imageStarted", "data": {...}}`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "event", content = "data", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RunEvent {
    Started { run_id: String, total: u32, output_dir: String, topic: String },
    ImageStarted { index: u32, output: String },
    ImageRetry { index: u32, attempt: u32, delay_sec: f64, reason: String },
    ImageFinished { index: u32, output: String, elapsed_sec: f64 },
    ImageSkipped { index: u32, output: String },
    ImageFailed { index: u32, reason: String },
    Finished { summary: RunSummary },
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub ok: bool,
    pub outputs: Vec<String>,
    pub failures: Vec<FailureView>,
    pub topic: String,
    pub output_dir: String,
    pub elapsed_sec: f64,
    pub cancelled: bool,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FailureView {
    pub index: u32,
    pub reason: String,
    pub category: Option<Category>,
    pub provider_code: Option<String>,
}

fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::new(Category::InvalidRequest, message)
}

/// A reference image the screen may attach (thumbnail and size line).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceView {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

/// Checks a picked or dropped reference before it joins the form: a file, at most 10 MB, an image (header only) with
/// each side at most 8192 px (spec §8).
pub fn inspect_reference(path: &Path) -> Result<ReferenceView, ServiceError> {
    let meta = std::fs::metadata(path).map_err(|_| invalid(format!("참조 이미지를 찾을 수 없습니다: {}", path.display())))?;
    if !meta.is_file() {
        return Err(invalid(format!("참조 이미지가 파일이 아닙니다: {}", path.display())));
    }
    if meta.len() > MAX_REFERENCE_BYTES {
        return Err(invalid(format!("참조 이미지는 장당 10MB 이하여야 합니다: {}", path.display())));
    }
    let not_image = || invalid(format!("이미지 파일이 아닙니다(PNG, JPEG, WebP, GIF): {}", path.display()));
    let (width, height) = image::ImageReader::open(path)
        .map_err(|_| not_image())?
        .with_guessed_format()
        .map_err(|_| not_image())?
        .into_dimensions()
        .map_err(|_| not_image())?;
    if width > MAX_REFERENCE_SIDE || height > MAX_REFERENCE_SIDE {
        return Err(invalid(format!("참조 이미지는 한 변이 {MAX_REFERENCE_SIDE}px 이하여야 합니다({width}×{height}): {}", path.display())));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    Ok(ReferenceView { path: path.display().to_string(), name, bytes: meta.len(), width, height })
}

/// Python `compose_prompt`: the prompt, then `Avoid: ...` on its own line when set.
pub fn compose(prompt: &str, avoid: &str) -> String {
    let (prompt, avoid) = (prompt.trim(), avoid.trim());
    match (prompt.is_empty(), avoid.is_empty()) {
        (_, true) => prompt.to_string(),
        (true, false) => format!("Avoid: {avoid}"),
        (false, false) => format!("{prompt}\nAvoid: {avoid}"),
    }
}

/// First words of the prompt that contain a letter or digit, slugged, plus `-YYYYMMDD-HHMMSS`; at most 80 chars.
pub fn auto_topic(prompt: &str, now: chrono::DateTime<chrono::Local>) -> String {
    let words: Vec<String> =
        prompt.split_whitespace().filter(|word| word.chars().any(char::is_alphanumeric)).map(safe_topic_slug).take(TOPIC_WORDS).collect();
    let joined: String = words.join("_").chars().take(TOPIC_WORD_CHARS).collect();
    let mut head = joined.trim_matches(|c| matches!(c, '.' | '_' | '-')).to_string();
    if head.is_empty() {
        head = "image".into();
    }
    safe_topic_slug(&format!("{head}-{}", now.format("%Y%m%d-%H%M%S")))
}

/// Checks the form and builds the engine spec. Returns the spec and the topic it uses.
pub fn build_spec(
    input: &GenerateInput,
    settings: &Settings,
    now: chrono::DateTime<chrono::Local>,
) -> Result<(Value, String), ServiceError> {
    if input.prompt.trim().is_empty() {
        return Err(invalid("프롬프트를 입력하세요."));
    }
    if !(1..=MAX_COUNT).contains(&input.count) {
        return Err(invalid(format!("장수는 1장부터 {MAX_COUNT}장까지입니다.")));
    }
    if !matches!(input.background.as_str(), "auto" | "opaque" | "transparent") {
        return Err(invalid(format!("배경 옵션이 올바르지 않습니다: {}", input.background)));
    }
    if input.references.len() > MAX_REFERENCES {
        return Err(invalid(format!("참조 이미지는 최대 {MAX_REFERENCES}장입니다.")));
    }
    for path in &input.references {
        let meta = std::fs::metadata(path).map_err(|_| invalid(format!("참조 이미지를 찾을 수 없습니다: {}", path.display())))?;
        if !meta.is_file() {
            return Err(invalid(format!("참조 이미지가 파일이 아닙니다: {}", path.display())));
        }
        if meta.len() > MAX_REFERENCE_BYTES {
            return Err(invalid(format!("참조 이미지는 장당 10MB 이하여야 합니다: {}", path.display())));
        }
    }
    let topic = match input.topic.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(topic) => safe_topic_slug(topic),
        None => auto_topic(&input.prompt, now),
    };
    let references: Vec<String> = input.references.iter().map(|p| p.display().to_string()).collect();
    let mut spec = json!({
        "mode": "single",
        "prompt": compose(&input.prompt, &input.avoid),
        "topic": topic,
        "topic_root": settings.work_dir.display().to_string(),
        "count": input.count,
        "size": input.size,
        "quality": input.quality,
        "image_model": input.image_model,
        "reference_images": references,
        "background": input.background,
        "resume": input.resume,
    });
    if let Some(profile) = input.profile.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        spec["profile"] = json!(profile);
    }
    Ok((spec, topic))
}

/// Top-level engine error in a result JSON (`{"ok": false, "error", "error_category"}`), if any.
fn result_error(result: &Value) -> Option<ServiceError> {
    let message = result.get("error")?.as_str()?;
    let category = result.get("error_category").and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or(Category::Internal);
    Some(ServiceError::new(category, message))
}

fn u32_of(event: &Value, key: &str) -> u32 {
    event.get(key).and_then(Value::as_u64).unwrap_or_default() as u32
}

fn str_of(event: &Value, key: &str) -> String {
    redact(event.get(key).and_then(Value::as_str).unwrap_or_default(), None)
}

fn f64_of(event: &Value, key: &str) -> f64 {
    event.get(key).and_then(Value::as_f64).unwrap_or_default()
}

/// Engine progress line (single mode) to a screen event; other lines are dropped.
pub fn event_from_progress(event: &Value) -> Option<RunEvent> {
    let index = u32_of(event, "index");
    Some(match event.get("event")?.as_str()? {
        "image_started" => RunEvent::ImageStarted { index, output: str_of(event, "output") },
        "retry" => RunEvent::ImageRetry {
            index,
            attempt: u32_of(event, "attempt"),
            delay_sec: f64_of(event, "delay_sec"),
            reason: str_of(event, "reason"),
        },
        "image_finished" => RunEvent::ImageFinished { index, output: str_of(event, "output"), elapsed_sec: f64_of(event, "elapsed_sec") },
        "image_skipped" => RunEvent::ImageSkipped { index, output: str_of(event, "output") },
        "image_failed" => RunEvent::ImageFailed { index, reason: str_of(event, "reason") },
        _ => return None,
    })
}

fn summary_from(result: &Value, topic: &str, output_dir: &str, cancelled: bool) -> RunSummary {
    let strings = |key: &str| -> Vec<String> {
        result
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let failures = result
        .get("failures")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|f| {
                    let error = f.get("error");
                    FailureView {
                        index: u32_of(f, "index"),
                        reason: str_of(f, "reason"),
                        category: error.and_then(|e| e.get("category")).and_then(|c| serde_json::from_value(c.clone()).ok()),
                        provider_code: error.and_then(|e| e.get("provider_code")).and_then(Value::as_str).map(str::to_string),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    RunSummary {
        ok: result["ok"] == true,
        outputs: strings("outputs"),
        failures,
        topic: topic.to_string(),
        output_dir: result.get("output_dir").and_then(Value::as_str).unwrap_or(output_dir).to_string(),
        elapsed_sec: f64_of(result, "elapsed_sec"),
        cancelled,
        warnings: strings("warnings"),
    }
}

/// Runs one generation. Form, connection and capability errors return `Err` before any event or request;
/// after `Started` every image reports its progress and the run ends with `Finished` (cancelled runs too).
pub async fn run_generation(
    ctx: &Ctx,
    input: GenerateInput,
    run_id: String,
    emit: impl Fn(RunEvent) + Send + Sync + 'static,
    cancel: CancellationToken,
) -> Result<RunSummary, ServiceError> {
    let settings = load_settings(&ctx.home);
    let (spec, topic) = build_spec(&input, &settings, chrono::Local::now())?;
    let store = Arc::new(ProfileStore::open(ctx.home.clone()));
    let resolver = Resolver { env: ctx.env.as_ref(), profiles: Some(store.as_ref()), secrets: ctx.secrets.as_ref() };
    resolve(&resolver, input.profile.as_deref().map(str::trim).filter(|p| !p.is_empty()))?;

    let backoff =
        ctx.env.process("AI_IMAGE_RETRY_BACKOFF_SEC").map(|v| v.trim().parse().unwrap_or(f64::NAN)).unwrap_or(DEFAULT_BACKOFF_SEC);
    let options = || {
        let mut opts = RunOptions::new(backoff, settings.work_dir.clone());
        opts.profiles = Some(store.clone());
        opts.secrets = ctx.secrets.clone();
        opts
    };
    // A dry run checks size, quality, model and capabilities offline, so a bad request never starts.
    let mut dry = spec.clone();
    dry["dry_run"] = json!(true);
    let quiet: Progress = Arc::new(|_| {});
    let planned = run_spec_with(dry, ctx.env.clone(), quiet, CancellationToken::new(), options()).await.result;
    if let Some(err) = result_error(&planned) {
        return Err(err);
    }
    let output_dir = absolute(&topic_dirs(&settings.work_dir, &topic).0).display().to_string();
    let output_dir = planned.get("output_dir").and_then(Value::as_str).map(str::to_string).unwrap_or(output_dir);

    let emit = Arc::new(emit);
    emit(RunEvent::Started { run_id, total: input.count, output_dir: output_dir.clone(), topic: topic.clone() });
    let sink = emit.clone();
    let progress: Progress = Arc::new(move |event: Value| {
        if let Some(event) = event_from_progress(&event) {
            sink(event);
        }
    });
    let result = run_spec_with(spec, ctx.env.clone(), progress, cancel.clone(), options()).await.result;
    if let Some(err) = result_error(&result) {
        return Err(err);
    }
    let summary = summary_from(&result, &topic, &output_dir, cancel.is_cancelled());
    emit(RunEvent::Finished { summary: summary.clone() });
    Ok(summary)
}
