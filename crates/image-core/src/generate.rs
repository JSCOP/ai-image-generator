//! One image, end to end: validate → reserve → resume check under the lock → call → fit → publish.

use crate::adapter::{check_request, model_supported, reference_limit};
use crate::connection::Connection;
use crate::error::{Category, EngineError};
use crate::http::Http;
use crate::mask;
use crate::normalize::{alpha_stats, check_provider_transparency, encode_png, normalize, Fit};
use crate::paths::absolute;
use crate::profiles::ProviderKind;
use crate::provider::{build_call, classify_http_error, extract_image, Background, ImageRequest};
use crate::size::Size;
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
    /// The user's own words when `request.prompt` was rewritten (e.g. into English); recorded, never sent.
    pub original_prompt: Option<String>,
}

#[derive(Debug)]
pub enum JobOk {
    Generated { output: PathBuf },
    Skipped { output: PathBuf },
}

/// Checks that need no network; anything failing here costs nothing.
/// Capabilities follow the connection kind (spec §6.3), so a kind–model mismatch never reaches the provider.
pub fn validate_request(kind: ProviderKind, req: &ImageRequest) -> Result<(), EngineError> {
    model_supported(kind, &req.image_model)?;
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
    check_request(kind, req)?;
    if let Some(mask) = &req.mask {
        let Some(source) = req.references.first() else {
            return Err(EngineError::new(Category::InvalidRequest, "a mask needs the image to edit as the first reference image"));
        };
        if req.background == Background::Transparent {
            return Err(EngineError::new(Category::Unsupported, "mask edits cannot ask for a transparent background"));
        }
        mask::prepare(source, mask, req.size)?;
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
    validate_request(ctx.conn.kind, req)?;
    let hash = request_hash(req, job.fit)?;
    // Lock first, then inspect: a decision made before the lock may be stale by the time we hold it.
    // Early returns drop the reservation, which releases the lock without touching the record.
    let reservation = reserve(&job.output, job.record.as_deref(), &job.lock_dir)?;
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
            reservation.finalize_existing(&hash)?;
            return Ok(JobOk::Skipped { output: job.output.clone() });
        }
        Existing::Mismatch(message) | Existing::Corrupt(message) => return Err(EngineError::new(Category::Conflict, message)),
    }
    if cancel.is_cancelled() {
        return Err(EngineError::new(Category::Cancelled, "cancelled before start"));
    }
    let mut record = Record {
        prompt: req.prompt.clone(),
        original_prompt: job.original_prompt.clone(),
        image_model: req.image_model.clone(),
        size: req.size.to_string(),
        quality: req.quality.clone(),
        action: req.action.as_str().into(),
        reference_images: req.references.iter().map(|p| absolute(p).display().to_string()).collect(),
        output: absolute(&job.output).display().to_string(),
        request_hash: Some(hash),
        background: Some(req.background.as_str().into()),
        fit: Some(job.fit.as_str().into()),
        mask: req.mask.as_ref().map(|p| absolute(p).display().to_string()),
        connection_source: Some(ctx.conn.source.clone()),
        profile_id: ctx.conn.profile_id.clone(),
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

fn worker_failed(e: tokio::task::JoinError) -> EngineError {
    EngineError::new(Category::Internal, format!("image worker failed: {e}"))
}

/// One provider request: image bytes and the reported model.
async fn call_provider(ctx: &JobContext, req: &ImageRequest) -> Result<(Vec<u8>, Option<String>), EngineError> {
    let call = build_call(&ctx.conn, req)?;
    let (status, body) = ctx.http.post_json(&call.url, &call.body, ctx.conn.auth()).await?;
    if status != 200 {
        return Err(classify_http_error(status, &body, ctx.conn.api_key.as_ref()));
    }
    extract_image(ctx.conn.kind, &call.backend, &body)
}

/// Provider call through fitting; fills the record's provider-derived fields.
async fn produce(ctx: &JobContext, job: &JobSpec, record: &mut Record) -> Result<Vec<u8>, EngineError> {
    let req = &job.request;
    if req.mask.is_some() {
        return produce_masked(ctx, req, record).await;
    }
    let (bytes, response_model) = call_provider(ctx, req).await?;
    record.response_model = response_model;
    let (target, fit) = (req.size, job.fit);
    let transparent = req.background == Background::Transparent;
    let (normalized, alpha) = tokio::task::spawn_blocking(move || {
        if transparent {
            check_provider_transparency(&bytes)?;
        }
        let normalized = normalize(&bytes, target, fit)?;
        let alpha = alpha_stats(&normalized.png)?;
        if transparent && alpha.transparent_ratio == 0.0 {
            let message = "투명 배경을 요청했지만 크기 맞춤 후 저장 결과에 완전 투명 픽셀이 없습니다";
            return Err(EngineError::new(Category::Provider, message).retryable(false));
        }
        Ok::<_, EngineError>((normalized, alpha))
    })
    .await
    .map_err(worker_failed)??;
    record.provider_size = Some(normalized.provider_size.to_string());
    record.saved_size = Some(normalized.saved_size.to_string());
    record.fit_applied = Some(normalized.fit_applied.into());
    record.alpha = Some(alpha);
    Ok(normalized.png)
}

/// Mask edit: the provider sees only a crop around the marked area; its answer replaces the marked pixels.
async fn produce_masked(ctx: &JobContext, req: &ImageRequest, record: &mut Record) -> Result<Vec<u8>, EngineError> {
    let (source, mask_path, size) = (req.references[0].clone(), req.mask.clone().unwrap_or_default(), req.size);
    let edit = tokio::task::spawn_blocking(move || mask::prepare(&source, &mask_path, size)).await.map_err(worker_failed)??;
    let temp = |kind: &str| std::env::temp_dir().join(format!("image-core-{kind}-{}.png", uuid::Uuid::new_v4().simple()));
    let write = |path: &std::path::Path, png: Vec<u8>| {
        std::fs::write(path, png).map_err(|e| EngineError::new(Category::Internal, format!("could not write {}: {e}", path.display())))
    };
    let (crop_file, guide_file) = (temp("crop"), temp("guide"));
    // The mask guide goes in as the second image when the model takes one more reference.
    let guide = reference_limit(ctx.conn.kind, &req.image_model) > req.references.len();
    let mut call = req.clone();
    call.references[0] = crop_file.clone();
    if guide {
        call.references.insert(1, guide_file.clone());
    }
    call.size = mask::provider_size(edit.crop);
    call.prompt = edit.crop_prompt(&req.prompt, guide);
    call.mask = None;
    let written = write(&crop_file, edit.crop_png()?).and_then(|_| if guide { write(&guide_file, edit.guide_png()?) } else { Ok(()) });
    let sent = match written {
        Ok(()) => call_provider(ctx, &call).await,
        Err(err) => Err(err),
    };
    let _ = std::fs::remove_file(&crop_file);
    let _ = std::fs::remove_file(&guide_file);
    let (bytes, response_model) = sent?;
    record.response_model = response_model;
    let (png, provider_size, alpha) = tokio::task::spawn_blocking(move || {
        let edited = image::load_from_memory(&bytes)
            .map_err(|e| EngineError::new(Category::Provider, format!("provider returned an undecodable image: {e}")))?;
        let provider_size = Size { width: edited.width(), height: edited.height() };
        let png = encode_png(&image::DynamicImage::ImageRgba8(edit.composite(&edited)))?;
        let alpha = alpha_stats(&png)?;
        Ok::<_, EngineError>((png, provider_size, alpha))
    })
    .await
    .map_err(worker_failed)??;
    record.provider_size = Some(provider_size.to_string());
    record.saved_size = Some(size.to_string());
    record.fit_applied = Some("mask".into());
    record.alpha = Some(alpha);
    Ok(png)
}
