//! JSON spec → parallel generation → result JSON, with the `tools/ai_image.py` contract.

use crate::connection::{resolve_connection, EnvSource};
use crate::error::{Category, EngineError};
use crate::generate::{generate_one, validate_request, JobContext, JobOk, JobSpec};
use crate::http::Http;
use crate::normalize::Fit;
use crate::paths::{absolute, metadata_path, safe_topic_slug, topic_dirs};
use crate::provider::{Action, Background, ImageRequest};
use crate::size::parse_size;
use crate::store::{inspect_existing, request_hash, Existing};
use rand::Rng;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_IMAGE_MODEL: &str = "gpt-image-2.5";
pub const DEFAULT_CONCURRENCY: i64 = 30;
pub const MAX_CONCURRENCY: i64 = 32;
pub const DEFAULT_RETRIES: i64 = 2;
pub const MAX_RETRIES: i64 = 5;

pub type Progress = Arc<dyn Fn(Value) + Send + Sync>;

pub struct RunOutcome {
    pub result: Value,
    pub exit_code: i32,
}

pub struct RunOptions {
    /// Retry backoff base `B`: delay = min(60, B·2^(n-1)) + U(0, B/2).
    pub backoff_base_sec: f64,
    /// Relative `topic_root`, reference and preset paths resolve against this.
    pub cwd: PathBuf,
}

/// Reads `AI_IMAGE_RETRY_BACKOFF_SEC` (default 5) and the process working directory.
pub async fn run_spec(spec: Value, env: Arc<dyn EnvSource>, progress: Progress, cancel: CancellationToken) -> RunOutcome {
    let backoff_base_sec = env.process("AI_IMAGE_RETRY_BACKOFF_SEC").and_then(|v| v.trim().parse().ok()).unwrap_or(5.0);
    let cwd = std::env::current_dir().unwrap_or_default();
    run_spec_with(spec, env, progress, cancel, RunOptions { backoff_base_sec, cwd }).await
}

pub async fn run_spec_with(
    spec: Value,
    env: Arc<dyn EnvSource>,
    progress: Progress,
    cancel: CancellationToken,
    opts: RunOptions,
) -> RunOutcome {
    let Value::Object(spec) = spec else {
        return RunOutcome { result: json!({"ok": false, "error": "spec must be a JSON object"}), exit_code: 1 };
    };
    let mode = spec.get("mode").map(py_str).unwrap_or_else(|| "single".into());
    if !matches!(mode.as_str(), "single" | "jobs" | "batch") {
        let error = format!("unknown mode {}; use 'single', 'jobs' or 'batch'", py_repr(spec.get("mode").unwrap_or(&Value::Null)));
        return RunOutcome { result: json!({"ok": false, "error": error}), exit_code: 1 };
    }
    let run = Runner { spec, env, progress, cancel, opts };
    let mut result = match run.unsupported_keys() {
        Some(err) => error_result(&err),
        None => match mode.as_str() {
            "single" => run.single().await,
            "jobs" => run.jobs().await,
            _ => crate::batch::run_batch(&run).await,
        },
    };
    let warnings = run.warnings();
    if !warnings.is_empty() {
        result["warnings"] = json!(warnings);
    }
    let exit_code = if result["ok"] == true { 0 } else { 2 };
    RunOutcome { result, exit_code }
}

pub(crate) struct Runner {
    pub(crate) spec: Map<String, Value>,
    pub(crate) env: Arc<dyn EnvSource>,
    pub(crate) progress: Progress,
    pub(crate) cancel: CancellationToken,
    pub(crate) opts: RunOptions,
}

pub(crate) fn error_result(err: &EngineError) -> Value {
    json!({"ok": false, "error": err.message, "error_category": err.category})
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(Category::InvalidRequest, message)
}

/// Python `str(value)` for spec values.
pub(crate) fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// Python `repr(value)` for error messages.
pub(crate) fn py_repr(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{s}'"),
        other => py_str(other),
    }
}

pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

/// `spec.get(key) or default` with Python truthiness.
fn or_default(value: Option<&Value>, default: &str) -> String {
    if truthy(value) {
        py_str(value.unwrap())
    } else {
        default.to_string()
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn now_ts() -> f64 {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    (secs * 1000.0).round() / 1000.0
}

fn round2(secs: f64) -> f64 {
    (secs * 100.0).round() / 100.0
}

pub(crate) struct Limits {
    pub(crate) timeout: Option<Duration>,
    pub(crate) concurrency: usize,
    pub(crate) retries: u32,
}

/// Per-image fields that a job may override.
pub(crate) struct ImageFields<'a> {
    pub(crate) prompt: String,
    pub(crate) size: &'a Value,
    pub(crate) quality: String,
    pub(crate) image_model: String,
    pub(crate) action: String,
    pub(crate) references: Vec<PathBuf>,
    pub(crate) background: Option<&'a Value>,
    pub(crate) fit: Option<&'a Value>,
}

pub(crate) struct Item {
    pub(crate) index: usize,
    pub(crate) id: Option<String>,
    pub(crate) job: JobSpec,
    pub(crate) event: Map<String, Value>,
}

pub(crate) struct ItemResult {
    pub(crate) index: usize,
    pub(crate) id: Option<String>,
    pub(crate) output: String,
    pub(crate) outcome: Result<JobOk, EngineError>,
    pub(crate) retried: u32,
}

pub(crate) struct EventNames {
    pub(crate) started: &'static str,
    pub(crate) finished: &'static str,
    pub(crate) failed: &'static str,
    pub(crate) skipped: &'static str,
    /// Python's single mode sends `reason: ""` on success; jobs mode omits it.
    pub(crate) reason_on_success: bool,
}

const SINGLE_EVENTS: EventNames = EventNames {
    started: "image_started",
    finished: "image_finished",
    failed: "image_failed",
    skipped: "image_skipped",
    reason_on_success: true,
};
pub(crate) const JOB_EVENTS: EventNames =
    EventNames { started: "job_started", finished: "job_finished", failed: "job_failed", skipped: "job_skipped", reason_on_success: false };

impl Runner {
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.spec.get(key).filter(|v| !v.is_null())
    }

    fn unsupported_keys(&self) -> Option<EngineError> {
        let jobs = self.spec.get("jobs").and_then(Value::as_array).cloned().unwrap_or_default();
        let objects = std::iter::once(&self.spec).chain(jobs.iter().filter_map(Value::as_object));
        for object in objects {
            let has = |key: &str| object.get(key).is_some_and(|v| !v.is_null());
            let message = if has("profile") {
                "profile is not supported yet (planned for stage 2: connection profiles); set CLIPROXY_BASE_URL and CLIPROXY_API_KEY instead"
            } else if has("mask") || has("keep_outside_mask") {
                "mask editing is not supported yet (planned for stage 4)"
            } else if truthy(object.get("remove_background")) {
                "remove_background is not supported yet (planned for stage 4); use background \"transparent\" with gpt-image-2.5-sunburst or gpt-image-2.5-flare"
            } else {
                continue;
            };
            return Some(EngineError::new(Category::Unsupported, message));
        }
        None
    }

    fn warnings(&self) -> Vec<String> {
        let jobs = self.spec.get("jobs").and_then(Value::as_array).cloned().unwrap_or_default();
        let uses_model = self.get("model").is_some() || jobs.iter().any(|j| j.get("model").is_some_and(|v| !v.is_null()));
        if uses_model {
            vec!["spec key 'model' is ignored: the legacy Responses API path was removed; image_model selects the model".into()]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn resolve_path(&self, value: &str) -> PathBuf {
        let path = PathBuf::from(value);
        absolute(&if path.is_absolute() { path } else { self.opts.cwd.join(path) })
    }

    pub(crate) fn topic_root(&self) -> PathBuf {
        match self.get("topic_root").filter(|v| truthy(Some(v))) {
            Some(value) => self.resolve_path(&py_str(value)),
            None => absolute(&self.opts.cwd),
        }
    }

    pub(crate) fn default_image_model(&self) -> String {
        if truthy(self.spec.get("image_model")) {
            return py_str(&self.spec["image_model"]);
        }
        self.env.process("CLIPROXY_IMAGE_MODEL").unwrap_or_else(|| DEFAULT_IMAGE_MODEL.into())
    }

    pub(crate) fn limits(&self) -> Result<Limits, EngineError> {
        let bad = || invalid("job_timeout_sec, concurrency, and retries must be numbers");
        let raw = match self.spec.get("job_timeout_sec").or_else(|| self.spec.get("timeout_sec")) {
            Some(value) => value.clone(),
            None => json!(self.env.process("CLIPROXY_TIMEOUT").unwrap_or_else(|| "600".into())),
        };
        let timeout = match &raw {
            Value::Null => None,
            Value::String(s) if s.is_empty() => None,
            value => Some(number(value).ok_or_else(bad)?).filter(|t| *t > 0.0).map(Duration::from_secs_f64),
        };
        let integer = |key: &str, default: i64| match self.spec.get(key) {
            None => Ok(default),
            Some(value) => number(value).map(|n| n.trunc() as i64).ok_or_else(bad),
        };
        let concurrency = integer("concurrency", DEFAULT_CONCURRENCY)?.clamp(1, MAX_CONCURRENCY) as usize;
        let retries = integer("retries", DEFAULT_RETRIES)?.clamp(0, MAX_RETRIES) as u32;
        Ok(Limits { timeout, concurrency, retries })
    }

    /// Validates one image's fields into a request (same messages as `ai_image.py`).
    pub(crate) fn build_request(&self, fields: ImageFields<'_>) -> Result<(ImageRequest, Fit), EngineError> {
        let size = match fields.size {
            Value::String(text) => parse_size(text).map_err(invalid)?,
            other => return Err(invalid(format!("invalid size {}; expected WIDTHxHEIGHT", py_repr(other)))),
        };
        if !matches!(fields.quality.as_str(), "low" | "medium" | "high") {
            return Err(invalid(format!("invalid quality '{}'", fields.quality)));
        }
        let action = Action::parse(&fields.action)
            .ok_or_else(|| invalid(format!("invalid action '{}'; expected auto, generate, or edit", fields.action)))?;
        let background = match fields.background {
            None => Background::Auto,
            Some(value) => Background::parse(&py_str(value))
                .ok_or_else(|| invalid(format!("invalid background {}; expected auto, opaque, or transparent", py_repr(value))))?,
        };
        let fit = match fields.fit {
            None if background == Background::Transparent => Fit::Contain,
            None => Fit::Cover,
            Some(value) => {
                Fit::parse(&py_str(value)).ok_or_else(|| invalid(format!("invalid fit {}; expected cover or contain", py_repr(value))))?
            }
        };
        let request = ImageRequest {
            prompt: fields.prompt,
            image_model: fields.image_model,
            size,
            quality: fields.quality,
            action,
            references: fields.references,
            background,
        };
        validate_request(&request)?;
        Ok((request, fit))
    }

    pub(crate) fn references(&self, value: Option<&Value>) -> Vec<PathBuf> {
        value.and_then(Value::as_array).into_iter().flatten().map(|v| self.resolve_path(&py_str(v))).collect()
    }

    pub(crate) fn job_spec(
        &self,
        request: ImageRequest,
        fit: Fit,
        output: PathBuf,
        workspace: &Path,
        slug: &str,
        timeout: Option<Duration>,
    ) -> JobSpec {
        let record = (self.spec.get("record_metadata") != Some(&Value::Bool(false))).then(|| metadata_path(&output, workspace, slug));
        JobSpec { request, fit, output, record, lock_dir: topic_dirs(workspace, slug).1, resume: truthy(self.spec.get("resume")), timeout }
    }

    pub(crate) fn connect(&self, limits: &Limits) -> Result<Arc<JobContext>, EngineError> {
        let conn = resolve_connection(self.env.as_ref())?;
        // The per-image deadline fires first so its message and record win.
        let http = Http::new(limits.timeout.map(|t| t + Duration::from_secs(5)));
        Ok(Arc::new(JobContext { http: Arc::new(http), conn: Arc::new(conn) }))
    }

    async fn single(&self) -> Value {
        match self.single_inner().await {
            Ok(value) => value,
            Err(err) => error_result(&err),
        }
    }

    async fn single_inner(&self) -> Result<Value, EngineError> {
        let prompt = self.get("prompt").map(py_str).unwrap_or_default().trim().to_string();
        if prompt.is_empty() {
            return Err(invalid("spec.prompt is required when mode=single"));
        }
        let topic = or_default(self.spec.get("topic"), "ai-image-request");
        let slug = safe_topic_slug(&topic);
        let count = match self.spec.get("count") {
            None => 1,
            Some(value) => number(value).ok_or_else(|| invalid("count must be a number"))?.trunc().max(1.0) as usize,
        };
        let workspace = self.topic_root();
        let (output_dir, metadata_dir) = topic_dirs(&workspace, &slug);
        let limits = self.limits()?;
        let default_size = json!("1920x1080");
        let fields = ImageFields {
            prompt,
            size: self.spec.get("size").unwrap_or(&default_size),
            quality: self.spec.get("quality").map(py_str).unwrap_or_else(|| "high".into()),
            image_model: self.default_image_model(),
            action: or_default(self.spec.get("action"), "generate"),
            references: self.references(self.spec.get("reference_images")),
            background: self.get("background"),
            fit: self.get("fit"),
        };
        let (request, fit) = self.build_request(fields)?;
        let names: Vec<String> =
            (1..=count).map(|i| if count == 1 { format!("{slug}.png") } else { format!("{slug}_{i:03}.png") }).collect();
        if truthy(self.spec.get("dry_run")) {
            let planned: Vec<String> = names.iter().map(|n| output_dir.join(n).display().to_string()).collect();
            return Ok(json!({"ok": true, "mode": "single", "dry_run": true, "outputs": [], "failures": [],
                "output_dir": output_dir.display().to_string(), "metadata_dir": metadata_dir.display().to_string(),
                "planned_outputs": planned}));
        }
        let ctx = self.connect(&limits)?;
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| EngineError::new(Category::Internal, format!("cannot create {}: {e}", output_dir.display())))?;
        let started = Instant::now();
        let items = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let output = absolute(&output_dir.join(name));
                let mut event = Map::new();
                event.insert("mode".into(), json!("single"));
                event.insert("index".into(), json!(i + 1));
                event.insert("output".into(), json!(output.display().to_string()));
                let job = self.job_spec(request.clone(), fit, output, &workspace, &slug, limits.timeout);
                Item { index: i + 1, id: None, job, event }
            })
            .collect();
        let workers = limits.concurrency.min(count);
        let results = self.execute(items, workers, limits.retries, ctx.clone(), &SINGLE_EVENTS, None).await;
        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        let mut retried = 0;
        for r in results {
            retried += r.retried;
            match r.outcome {
                Ok(_) => outputs.push(r.output),
                Err(err) => failures.push(json!({"index": r.index, "reason": err.reason(), "error": err})),
            }
        }
        outputs.sort();
        let output_dir = absolute(&output_dir).display().to_string();
        Ok(json!({
            "ok": failures.is_empty(), "mode": "single", "outputs": outputs, "failures": failures,
            "topic_dir": output_dir, "output_dir": output_dir, "metadata_dir": metadata_dir.display().to_string(),
            "concurrency": workers, "retried": retried, "elapsed_sec": round2(started.elapsed().as_secs_f64()),
            "connection_source": ctx.conn.source,
        }))
    }

    async fn jobs(&self) -> Value {
        match self.jobs_inner().await {
            Ok(value) => value,
            Err(err) => error_result(&err),
        }
    }

    async fn jobs_inner(&self) -> Result<Value, EngineError> {
        let parent_topic = or_default(self.spec.get("topic"), "ai-image-jobs");
        let slug = safe_topic_slug(&parent_topic);
        let default_size = self.spec.get("size").cloned().unwrap_or(json!("1920x1080"));
        let default_quality = self.spec.get("quality").map(py_str).unwrap_or_else(|| "high".into());
        let default_image_model = self.default_image_model();
        let default_action = or_default(self.spec.get("action"), "generate");
        let dry_run = truthy(self.spec.get("dry_run"));
        let text = |key: &str| self.get(key).map(py_str).unwrap_or_default().trim().to_string();
        let (prefix, suffix) = (text("prompt_prefix"), text("prompt_suffix"));
        let workspace = self.topic_root();
        let (output_dir, metadata_dir) = topic_dirs(&workspace, &slug);
        let limits = self.limits()?;
        let jobs = match self.spec.get("jobs") {
            Some(Value::Array(jobs)) if !jobs.is_empty() => jobs.clone(),
            _ => return Err(invalid("spec.jobs must be a non-empty array")),
        };
        let mut seen = std::collections::HashSet::new();
        for (idx, job) in jobs.iter().enumerate() {
            let Some(job) = job.as_object() else { return Err(invalid(format!("jobs[{idx}] must be an object"))) };
            let body = job.get("prompt").filter(|v| !v.is_null()).map(py_str).unwrap_or_default();
            if body.trim().is_empty() && prefix.is_empty() && suffix.is_empty() {
                return Err(invalid(format!("jobs[{idx}].prompt is required")));
            }
            let job_slug = safe_topic_slug(&or_default(job.get("id"), &format!("job_{:03}", idx + 1)));
            if !seen.insert(job_slug.to_lowercase()) {
                return Err(invalid("job IDs collide after filename normalization"));
            }
        }
        let mut items = Vec::new();
        let mut planned = Vec::new();
        let mut failures = Vec::new();
        for (idx, job) in jobs.iter().enumerate() {
            let job = job.as_object().unwrap();
            let id = or_default(job.get("id"), &format!("job_{:03}", idx + 1));
            let field = |key: &str| job.get(key).filter(|v| truthy(Some(v)));
            let body = job.get("prompt").filter(|v| !v.is_null()).map(py_str).unwrap_or_default().trim().to_string();
            let prompt =
                [prefix.as_str(), body.as_str(), suffix.as_str()].iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join("\n");
            let references = match job.get("reference_images") {
                Some(Value::Null) | None => self.references(self.spec.get("reference_images")),
                Some(value) => self.references(Some(value)),
            };
            let fields = ImageFields {
                prompt,
                size: field("size").unwrap_or(&default_size),
                quality: field("quality").map(py_str).unwrap_or_else(|| default_quality.clone()),
                image_model: field("image_model").map(py_str).unwrap_or_else(|| default_image_model.clone()),
                action: field("action").map(py_str).unwrap_or_else(|| default_action.clone()),
                references,
                background: job.get("background").filter(|v| !v.is_null()).or(self.get("background")),
                fit: job.get("fit").filter(|v| !v.is_null()).or(self.get("fit")),
            };
            let output = absolute(&output_dir.join(format!("{}.png", safe_topic_slug(&id))));
            match self.build_request(fields) {
                Err(err) => failures.push(json!({"id": id, "index": idx, "reason": err.reason(), "error": err})),
                Ok(_) if dry_run => planned.push(json!({"id": id, "output": output.display().to_string()})),
                Ok((request, fit)) => {
                    let mut event = Map::new();
                    event.insert("mode".into(), json!("jobs"));
                    event.insert("id".into(), json!(id));
                    event.insert("index".into(), json!(idx));
                    event.insert("output".into(), json!(output.display().to_string()));
                    let job = self.job_spec(request, fit, output, &workspace, &slug, limits.timeout);
                    items.push(Item { index: idx, id: Some(id), job, event });
                }
            }
        }
        let started = Instant::now();
        let workers = limits.concurrency.min(jobs.len());
        let mut outputs = Vec::new();
        let mut retried = 0;
        let mut source = Value::Null;
        if !dry_run && !items.is_empty() {
            let ctx = self.connect(&limits)?;
            source = json!(ctx.conn.source);
            std::fs::create_dir_all(&output_dir)
                .map_err(|e| EngineError::new(Category::Internal, format!("cannot create {}: {e}", output_dir.display())))?;
            for r in self.execute(items, workers, limits.retries, ctx, &JOB_EVENTS, None).await {
                retried += r.retried;
                let id = r.id.unwrap_or_default();
                match r.outcome {
                    Ok(_) => outputs.push(json!({"id": id, "output": r.output})),
                    Err(err) => failures.push(json!({"id": id, "index": r.index, "reason": err.reason(), "error": err})),
                }
            }
        } else if !dry_run {
            std::fs::create_dir_all(&output_dir).ok();
        }
        let by_id = |v: &Value| v["id"].as_str().unwrap_or_default().to_string();
        outputs.sort_by_key(by_id);
        planned.sort_by_key(by_id);
        failures.sort_by_key(by_id);
        let output_dir = absolute(&output_dir).display().to_string();
        let mut result = json!({
            "ok": failures.is_empty(), "mode": "jobs", "dry_run": dry_run,
            "topic_dir": output_dir, "output_dir": output_dir, "metadata_dir": metadata_dir.display().to_string(),
            "outputs": outputs, "planned_outputs": planned, "failures": failures,
            "concurrency": workers, "retried": retried, "elapsed_sec": round2(started.elapsed().as_secs_f64()),
        });
        if !source.is_null() {
            result["connection_source"] = source;
        }
        Ok(result)
    }

    /// Run items with at most `workers` in flight, retrying retryable failures.
    /// With `stop_after`, items not yet started are skipped once that many have failed.
    pub(crate) async fn execute(
        &self,
        items: Vec<Item>,
        workers: usize,
        retries: u32,
        ctx: Arc<JobContext>,
        names: &'static EventNames,
        stop_after: Option<usize>,
    ) -> Vec<ItemResult> {
        let semaphore = Arc::new(Semaphore::new(workers.max(1)));
        let stop = stop_after.map(|limit| Arc::new(StopAfter { limit, failed: AtomicUsize::new(0), token: CancellationToken::new() }));
        let mut tasks = tokio::task::JoinSet::new();
        // A task that panics still owes the caller a result for its image.
        let mut owners = std::collections::HashMap::new();
        for item in items {
            let (semaphore, ctx, cancel, progress) = (semaphore.clone(), ctx.clone(), self.cancel.clone(), self.progress.clone());
            let (backoff, stop) = (self.opts.backoff_base_sec, stop.clone());
            let owner = (item.index, item.id.clone(), item.job.output.display().to_string());
            let handle = tasks.spawn(async move { run_item(item, semaphore, ctx, cancel, progress, backoff, retries, names, stop).await });
            owners.insert(handle.id(), owner);
        }
        let mut results = Vec::new();
        while let Some(joined) = tasks.join_next_with_id().await {
            match joined {
                Ok((_, result)) => results.push(result),
                Err(err) => {
                    let Some((index, id, output)) = owners.remove(&err.id()) else { continue };
                    let outcome = Err(EngineError::new(Category::Internal, format!("image task stopped unexpectedly: {err}")));
                    results.push(ItemResult { index, id, output, outcome, retried: 0 });
                }
            }
        }
        results
    }
}

pub(crate) struct StopAfter {
    limit: usize,
    failed: AtomicUsize,
    token: CancellationToken,
}

fn emit(progress: &Progress, name: &str, base: &Map<String, Value>, extra: Value) {
    let mut event = Map::new();
    event.insert("event".into(), json!(name));
    for (k, v) in base {
        event.insert(k.clone(), v.clone());
    }
    if let Value::Object(extra) = extra {
        event.extend(extra);
    }
    event.insert("ts".into(), json!(now_ts()));
    progress(Value::Object(event));
}

#[allow(clippy::too_many_arguments)]
async fn run_item(
    item: Item,
    semaphore: Arc<Semaphore>,
    ctx: Arc<JobContext>,
    cancel: CancellationToken,
    progress: Progress,
    backoff: f64,
    retries: u32,
    names: &'static EventNames,
    stop: Option<Arc<StopAfter>>,
) -> ItemResult {
    let output = item.job.output.display().to_string();
    let finish = |outcome, retried| ItemResult { index: item.index, id: item.id.clone(), output: output.clone(), outcome, retried };
    let permit = tokio::select! {
        permit = semaphore.acquire_owned() => permit.ok(),
        _ = cancel.cancelled() => None,
    };
    if permit.is_none() || cancel.is_cancelled() {
        return finish(Err(EngineError::new(Category::Cancelled, "cancelled before start")), 0);
    }
    if let Some(stop) = stop.as_ref().filter(|s| s.token.is_cancelled()) {
        let message = format!("not started: stopped after {} failures", stop.limit);
        return finish(Err(EngineError::new(Category::Cancelled, message)), 0);
    }
    if item.job.resume {
        let reusable = request_hash(&item.job.request, item.job.fit)
            .map(|hash| matches!(inspect_existing(&item.job.output, item.job.record.as_deref(), &hash), Existing::Reusable))
            .unwrap_or(false);
        if reusable {
            emit(&progress, names.skipped, &item.event, json!({}));
            return finish(Ok(JobOk::Skipped { output: item.job.output.clone() }), 0);
        }
    }
    emit(&progress, names.started, &item.event, json!({}));
    let started = Instant::now();
    let mut attempt = 0;
    let outcome = loop {
        match generate_one(&ctx, &item.job, &cancel).await {
            Err(err) if err.retryable && attempt < retries && !cancel.is_cancelled() => {
                attempt += 1;
                let jitter = if backoff > 0.0 { rand::thread_rng().gen_range(0.0..backoff / 2.0) } else { 0.0 };
                let delay = (backoff * 2f64.powi(attempt as i32 - 1)).min(60.0) + jitter;
                emit(
                    &progress,
                    "retry",
                    &item.event,
                    json!({"attempt": attempt, "retries": retries,
                    "delay_sec": round2(delay), "reason": err.reason()}),
                );
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs_f64(delay)) => {}
                    _ = cancel.cancelled() => break Err(EngineError::new(Category::Cancelled, "cancelled")),
                }
            }
            other => break other,
        }
    };
    let elapsed = round2(started.elapsed().as_secs_f64());
    match &outcome {
        Ok(_) if names.reason_on_success => emit(&progress, names.finished, &item.event, json!({"elapsed_sec": elapsed, "reason": ""})),
        Ok(_) => emit(&progress, names.finished, &item.event, json!({"elapsed_sec": elapsed})),
        Err(err) => {
            emit(&progress, names.failed, &item.event, json!({"elapsed_sec": elapsed, "reason": err.reason()}));
            if let Some(stop) = &stop {
                if stop.failed.fetch_add(1, Ordering::SeqCst) + 1 >= stop.limit {
                    stop.token.cancel();
                }
            }
        }
    }
    drop(permit);
    finish(outcome, attempt)
}
