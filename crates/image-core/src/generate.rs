//! One image, end to end: validate → resume check → reserve → call → fit → publish.

use crate::catalog::{backend, supports_native_transparency, supports_references, Backend};
use crate::connection::Connection;
use crate::error::{Category, EngineError};
use crate::http::Http;
use crate::normalize::{alpha_stats, normalize, Fit};
use crate::paths::absolute;
use crate::provider::{build_call, classify_http_error, extract_image, Action, Background, ImageRequest};
use crate::store::{inspect_existing, request_hash, reserve, Existing, Record, Reservation};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const ENGINE: &str = concat!("image-core ", env!("CARGO_PKG_VERSION"));

pub struct JobContext {
    pub http: Arc<Http>,
    pub conn: Arc<Connection>,
}

pub struct JobSpec {
    pub request: ImageRequest,
    pub fit: Fit,
    pub output: PathBuf,
    /// `None` when the spec sets `record_metadata: false`.
    pub record: Option<PathBuf>,
    pub lock_dir: PathBuf,
    pub resume: bool,
    /// Wall-clock limit for this attempt; the failure is recorded before returning.
    pub timeout: Option<Duration>,
}

#[derive(Debug)]
pub enum JobOk {
    Generated { output: PathBuf },
    Skipped { output: PathBuf },
}

/// Checks that need no network; anything failing here costs nothing.
pub fn validate_request(req: &ImageRequest) -> Result<(), EngineError> {
    if backend(&req.image_model) == Backend::Unsupported {
        return Err(EngineError::new(
            Category::Unsupported,
            format!(
                "{} is not an image model this CLI can call (gpt-image-*, gemini-*-image, grok-imagine-*); the legacy Responses API path was removed",
                req.image_model
            ),
        ));
    }
    for path in &req.references {
        if !path.is_file() {
            return Err(EngineError::new(Category::InvalidRequest, format!("reference image not found: {}", path.display())));
        }
        let readable =
            image::ImageReader::open(path).and_then(|r| r.with_guessed_format()).ok().and_then(|r| r.into_dimensions().ok()).is_some();
        if !readable {
            return Err(EngineError::new(Category::InvalidRequest, format!("reference image is not a readable image: {}", path.display())));
        }
    }
    if !req.references.is_empty() && !supports_references(&req.image_model) {
        return Err(EngineError::new(
            Category::Unsupported,
            format!(
                "{} reference-image editing is not yet supported by this CLI; use a Gemini image model or gpt-image model",
                req.image_model
            ),
        ));
    }
    if req.action == Action::Edit && req.references.is_empty() && req.image_model.to_lowercase().starts_with("gpt-image-") {
        return Err(EngineError::new(Category::InvalidRequest, "GPT image edits require at least one reference image"));
    }
    if req.background == Background::Transparent && !supports_native_transparency(&req.image_model) {
        return Err(EngineError::new(
            Category::Unsupported,
            format!(
                "background \"transparent\" needs a model with native transparency (gpt-image-2.5-sunburst or gpt-image-2.5-flare); {} does not support it",
                req.image_model
            ),
        ));
    }
    Ok(())
}

/// Python `{seconds:g}`: `0.2` → "0.2", `600.0` → "600".
pub fn format_seconds(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs.fract() == 0.0 {
        format!("{}", secs as u64)
    } else {
        format!("{secs}")
    }
}

pub async fn generate_one(ctx: &JobContext, job: &JobSpec, cancel: &CancellationToken) -> Result<JobOk, EngineError> {
    let req = &job.request;
    validate_request(req)?;
    let hash = request_hash(req, job.fit)?;
    match inspect_existing(&job.output, job.record.as_deref(), &hash) {
        Existing::Absent => {}
        _ if !job.resume => {
            return Err(EngineError::new(
                Category::InvalidRequest,
                format!("Output already exists; use a new topic or resume: {}", job.output.display()),
            ))
        }
        Existing::Reusable => return Ok(JobOk::Skipped { output: job.output.clone() }),
        Existing::NeedsFinalize => {
            let reservation = reserve(&job.output, job.record.as_deref(), &job.lock_dir)?;
            reservation.finalize_existing()?;
            return Ok(JobOk::Skipped { output: job.output.clone() });
        }
        Existing::Mismatch(message) | Existing::Corrupt(message) => return Err(EngineError::new(Category::Conflict, message)),
    }
    if cancel.is_cancelled() {
        return Err(EngineError::new(Category::Cancelled, "cancelled before start"));
    }
    let reservation = reserve(&job.output, job.record.as_deref(), &job.lock_dir)?;
    let mut record = Record {
        prompt: req.prompt.clone(),
        image_model: req.image_model.clone(),
        size: req.size.to_string(),
        quality: req.quality.clone(),
        action: req.action.as_str().into(),
        reference_images: req.references.iter().map(|p| absolute(p).display().to_string()).collect(),
        output: absolute(&job.output).display().to_string(),
        request_hash: Some(hash),
        background: Some(req.background.as_str().into()),
        fit: Some(job.fit.as_str().into()),
        connection_source: Some(ctx.conn.source.into()),
        engine: Some(ENGINE.into()),
        ..Record::default()
    };
    if let Err(err) = reservation.write_pending(&record) {
        reservation.fail(record, &err);
        return Err(err);
    }
    let attempt = produce(ctx, job, &mut record);
    let outcome = tokio::select! {
        result = attempt => result,
        _ = cancel.cancelled() => Err(EngineError::new(Category::Cancelled, "cancelled")),
        _ = sleep_or_forever(job.timeout) => Err(EngineError::new(
            Category::Timeout,
            format!("timed out after {}s", format_seconds(job.timeout.unwrap_or_default())),
        )),
    };
    finish(reservation, record, outcome)
}

async fn sleep_or_forever(timeout: Option<Duration>) {
    match timeout {
        Some(timeout) => tokio::time::sleep(timeout).await,
        None => std::future::pending().await,
    }
}

fn finish(reservation: Reservation, record: Record, outcome: Result<Vec<u8>, EngineError>) -> Result<JobOk, EngineError> {
    let published = outcome.and_then(|png| reservation.publish(&png, record.clone()));
    match published {
        Ok(_) => Ok(JobOk::Generated { output: reservation.output.clone() }),
        Err(err) => {
            reservation.fail(record, &err);
            Err(err)
        }
    }
}

/// Provider call through fitting; fills the record's provider-derived fields.
async fn produce(ctx: &JobContext, job: &JobSpec, record: &mut Record) -> Result<Vec<u8>, EngineError> {
    let req = &job.request;
    let call = build_call(&ctx.conn.base_url, req)?;
    let key = ctx.conn.api_key.as_ref();
    let (status, body) = ctx.http.post_json(&call.url, &call.body, key).await?;
    if status != 200 {
        return Err(classify_http_error(status, &body, key));
    }
    let (bytes, response_model) = extract_image(&call.backend, &body)?;
    record.response_model = response_model;
    let (target, fit) = (req.size, job.fit);
    let (normalized, alpha) = tokio::task::spawn_blocking(move || {
        let normalized = normalize(&bytes, target, fit)?;
        let alpha = alpha_stats(&normalized.png)?;
        Ok::<_, EngineError>((normalized, alpha))
    })
    .await
    .map_err(|e| EngineError::new(Category::Internal, format!("image worker failed: {e}")))??;
    record.provider_size = Some(normalized.provider_size.to_string());
    record.saved_size = Some(normalized.saved_size.to_string());
    record.fit = Some(normalized.fit_applied.into());
    record.alpha = Some(alpha);
    Ok(normalized.png)
}
