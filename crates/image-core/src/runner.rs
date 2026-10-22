//! JSON spec → parallel generation → result JSON, with the `tools/ai_image.py` contract.

use crate::connection::{resolve, Connection, EnvSource, Resolver};
use crate::error::{Category, EngineError};
use crate::generate::{generate_one, validate_request, JobContext, JobOk, JobSpec};
use crate::http::Http;
use crate::normalize::Fit;
use crate::paths::{absolute, expand_user, metadata_path, safe_topic_slug, topic_dirs};
use crate::profiles::{GenerationStatus, ProfileStore, ProviderKind};
use crate::provider::{Action, Background, ImageRequest};
use crate::secrets::{KeyringStore, MemoryStore, SecretStore};
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
/// Largest accepted `job_timeout_sec` / `CLIPROXY_TIMEOUT` (one day; Python has no limit).
pub const MAX_JOB_TIMEOUT_SEC: f64 = 86_400.0;
/// Largest accepted `AI_IMAGE_RETRY_BACKOFF_SEC`; the delay itself is capped at 60 s plus jitter of up to half of it.
pub const MAX_RETRY_BACKOFF_SEC: f64 = 600.0;
/// Largest accepted single-mode `count` (Python has no limit); larger runs belong in batch mode or several runs.
pub const MAX_SINGLE_COUNT: usize = 1000;

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
    /// Connection profiles; `None` resolves from the environment only.
    pub profiles: Option<Arc<ProfileStore>>,
    /// Where profile keys are read from.
    pub secrets: Arc<dyn SecretStore>,
}

impl RunOptions {
    /// No profile store and an empty in-memory key store.
    pub fn new(backoff_base_sec: f64, cwd: PathBuf) -> RunOptions {
        RunOptions { backoff_base_sec, cwd, profiles: None, secrets: Arc::new(MemoryStore::new()) }
    }
}

/// Reads `AI_IMAGE_RETRY_BACKOFF_SEC` (default 5), the process working directory, the profile folder
/// (`ProfileStore::default_dir`) and the Credential Manager key store.
pub async fn run_spec(spec: Value, env: Arc<dyn EnvSource>, progress: Progress, cancel: CancellationToken) -> RunOutcome {
    // An unreadable value is refused by `run_spec_with` (Python fails at import on it).
    let backoff_base_sec = env.process("AI_IMAGE_RETRY_BACKOFF_SEC").map(|v| v.trim().parse().unwrap_or(f64::NAN)).unwrap_or(5.0);
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut opts = RunOptions::new(backoff_base_sec, cwd);
    opts.profiles = ProfileStore::default_dir(env.as_ref()).map(|dir| Arc::new(ProfileStore::open(dir)));
    if let Ok(keyring) = KeyringStore::system(env.as_ref()) {
        opts.secrets = Arc::new(keyring);
    }
    run_spec_with(spec, env, progress, cancel, opts).await
}

pub async fn run_spec_with(
    spec: Value,
    env: Arc<dyn EnvSource>,
    progress: Progress,
    cancel: CancellationToken,
    opts: RunOptions,
) -> RunOutcome {
    let backoff = opts.backoff_base_sec;
    if !(backoff.is_finite() && (0.0..=MAX_RETRY_BACKOFF_SEC).contains(&backoff)) {
        let error = format!("AI_IMAGE_RETRY_BACKOFF_SEC must be a number of seconds from 0 to {MAX_RETRY_BACKOFF_SEC}");
        return RunOutcome { result: error_result(&invalid(error)), exit_code: 1 };
    }
    let Value::Object(spec) = spec else {
        return RunOutcome { result: json!({"ok": false, "error": "spec must be a JSON object"}), exit_code: 1 };
    };
    let mode = spec.get("mode").map(py_str).unwrap_or_else(|| "single".into());
    if !matches!(mode.as_str(), "single" | "jobs" | "batch") {
        let error = format!("unknown mode {}; use 'single', 'jobs' or 'batch'", py_repr(spec.get("mode").unwrap_or(&Value::Null)));
        return RunOutcome { result: json!({"ok": false, "error": error}), exit_code: 1 };
    }
    let not_resolved = Err(EngineError::new(Category::Internal, "connection not resolved"));
    let mut run = Runner { spec, env, progress, cancel, opts, conn: not_resolved };
    let mut result = match run.unsupported_keys() {
        Some(err) => error_result(&err),
        None => {
            let explicit = run.explicit_profile();
            run.conn = run.resolve_connection(explicit.as_deref());
            match &run.conn {
                // A named profile that cannot be used stops the run, dry runs included.
                Err(err) if explicit.is_some() => error_result(err),
                _ => match mode.as_str() {
                    "single" => run.single().await,
                    "jobs" => run.jobs().await,
                    _ => crate::batch::run_batch(&run).await,
                },
            }
        }
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
    /// Resolved once per run (no network). An error surfaces only when a request is about to be sent.
    pub(crate) conn: Result<Arc<Connection>, EngineError>,
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

/// Whether a failure is a provider answer about generating at all (spec §6.2 "성공/할당량 오류로만 갱신"):
/// permission, spending/quota or rate-limit HTTP answers. Transport failures, 401, 404, 5xx and local refusals are not.
fn limits_generation(err: &EngineError) -> bool {
    err.http_status.is_some() && matches!(err.category, Category::Permission | Category::Quota | Category::RateLimit)
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
    pub(crate) mask: Option<PathBuf>,
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
    pub(crate) model: String,
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
        let single = self.spec.get("mode").map(py_str).is_none_or(|m| m == "single");
        let objects = std::iter::once((false, &self.spec)).chain(jobs.iter().filter_map(Value::as_object).map(|job| (true, job)));
        for (is_job, object) in objects {
            let has = |key: &str| object.get(key).is_some_and(|v| !v.is_null());
            let message = if is_job && has("profile") {
                "profile can only be set at the top level of the spec: one run uses one connection"
            } else if has("mask") && (is_job || !single) {
                "mask is supported in single mode only"
            } else if object.get("keep_outside_mask") == Some(&Value::Bool(false)) {
                "keep_outside_mask false is not supported: pixels outside the mask are always kept"
            } else if truthy(object.get("remove_background")) {
                "remove_background is not supported yet (planned for stage 4); use background \"transparent\" with gpt-image-2.5-sunburst or gpt-image-2.5-flare"
            } else {
                continue;
            };
            return Some(EngineError::new(Category::Unsupported, message));
        }
        None
    }

    /// Spec `profile` (name or id), when set.
    fn explicit_profile(&self) -> Option<String> {
        self.get("profile").filter(|v| truthy(Some(v))).map(py_str)
    }

    fn resolve_connection(&self, explicit: Option<&str>) -> Result<Arc<Connection>, EngineError> {
        let resolver = Resolver { env: self.env.as_ref(), profiles: self.opts.profiles.as_deref(), secrets: self.opts.secrets.as_ref() };
        resolve(&resolver, explicit).map(Arc::new)
    }

    /// Kind used for offline capability checks. Without a usable connection, stage 1 (CLIProxyAPI) rules apply
    /// and the connection error is reported when a request would be sent.
    fn kind(&self) -> ProviderKind {
        self.conn.as_ref().map(|c| c.kind).unwrap_or(ProviderKind::Cliproxy)
    }

    /// Updates `status.json` for a profile connection after a real run: ok when at least one image was
    /// generated, else the first permission/quota/rate-limit answer; other failures keep the previous entry. Best effort.
    pub(crate) fn record_outcome(&self, ctx: &JobContext, results: &[ItemResult]) {
        let (Some(store), Some(profile_id)) = (self.opts.profiles.as_ref(), ctx.conn.profile_id.as_deref()) else { return };
        let generated = results.iter().find(|r| matches!(r.outcome, Ok(JobOk::Generated { .. })));
        let status = match generated {
            Some(r) => GenerationStatus { ok: true, at: now_ts(), model: r.model.clone(), category: None, provider_code: None },
            None => {
                let failed = results.iter().find_map(|r| match &r.outcome {
                    Err(err) if limits_generation(err) => Some((r, err)),
                    _ => None,
                });
                let Some((r, err)) = failed else { return };
                GenerationStatus {
                    ok: false,
                    at: now_ts(),
                    model: r.model.clone(),
                    category: Some(err.category),
                    provider_code: err.provider_code.clone(),
                }
            }
        };
        // status.json is a cache; failing to write it must not change the run result.
        let _ = store.record_generation(profile_id, &status);
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

    /// `resolve_path` after Python's `Path.expanduser()` (`topic_root` and reference images).
    pub(crate) fn resolve_user_path(&self, value: &str) -> PathBuf {
        self.resolve_path(&expand_user(value, self.env.as_ref()))
    }

    pub(crate) fn topic_root(&self) -> PathBuf {
        match self.get("topic_root").filter(|v| truthy(Some(v))) {
            Some(value) => self.resolve_user_path(&py_str(value)),
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
            value => timeout_from(number(value).ok_or_else(bad)?)?,
        };
        let integer = |key: &str, default: i64| match self.spec.get(key) {
            None => Ok(default),
            Some(value) => number(value).map(|n| n.trunc() as i64).ok_or_else(bad),
        };
        let default_concurrency = self.conn.as_ref().map(|c| c.concurrency_default as i64).unwrap_or(DEFAULT_CONCURRENCY);
        let concurrency = integer("concurrency", default_concurrency)?.clamp(1, MAX_CONCURRENCY) as usize;
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
            Some(value) => Fit::parse(&py_str(value))
                .ok_or_else(|| invalid(format!("invalid fit {}; expected cover, contain, or original", py_repr(value))))?,
        };
        let request = ImageRequest {
            prompt: fields.prompt,
            image_model: fields.image_model,
            size,
            quality: fields.quality,
            action,
            references: fields.references,
            background,
            mask: fields.mask,
        };
        // Kind–model mismatches and missing capabilities are refused here, before any paid call.
        validate_request(self.kind(), &request)?;
        Ok((request, fit))
    }

    /// `null`/absent → none; otherwise an array of path strings, else `invalid_request` (`location` names the key).
    pub(crate) fn references(&self, value: Option<&Value>, location: &str) -> Result<Vec<PathBuf>, EngineError> {
        let bad = || invalid(format!("{location} must be an array of image path strings"));
        match value {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(items)) => {
                items.iter().map(|v| v.as_str().map(|path| self.resolve_user_path(path)).ok_or_else(bad)).collect()
            }
            Some(_) => Err(bad()),
        }
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
        let original_prompt = self.get("original_prompt").map(py_str).map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        JobSpec {
            request,
            fit,
            output,
            record,
            lock_dir: topic_dirs(workspace, slug).1,
            resume: truthy(self.spec.get("resume")),
            timeout,
            original_prompt,
        }
    }

    pub(crate) fn connect(&self, limits: &Limits) -> Result<Arc<JobContext>, EngineError> {
        let conn = self.conn.clone()?;
        // The per-image deadline fires first so its message and record win.
        let http = Http::new(limits.timeout.map(|t| t + Duration::from_secs(5)));
        Ok(Arc::new(JobContext { http: Arc::new(http), conn }))
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
            Some(value) => {
                let n = number(value).ok_or_else(|| invalid("count must be a number"))?;
                // Python truncates and treats < 1 as 1; non-finite or huge values are refused before planning any name.
                if !n.is_finite() || n.trunc() > MAX_SINGLE_COUNT as f64 {
                    return Err(invalid(format!("count must be a finite number up to {MAX_SINGLE_COUNT} when mode=single")));
                }
                n.trunc().max(1.0) as usize
            }
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
            references: self.references(self.spec.get("reference_images"), "reference_images")?,
            background: self.get("background"),
            fit: self.get("fit"),
            mask: match self.get("mask") {
                None => None,
                Some(Value::String(path)) => Some(self.resolve_user_path(path)),
                Some(_) => return Err(invalid("mask must be an image path string")),
            },
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
        self.record_outcome(&ctx, &results);
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
        // Python sorts single-mode failures by index, not completion order.
        failures.sort_by_key(|f| f["index"].as_u64().unwrap_or_default());
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
        let default_references = self.references(self.spec.get("reference_images"), "reference_images")?;
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
                Some(Value::Null) | None => Ok(default_references.clone()),
                Some(value) => self.references(Some(value), &format!("jobs[{idx}].reference_images")),
            };
            let fields = ImageFields {
                prompt,
                size: field("size").unwrap_or(&default_size),
                quality: field("quality").map(py_str).unwrap_or_else(|| default_quality.clone()),
                image_model: field("image_model").map(py_str).unwrap_or_else(|| default_image_model.clone()),
                action: field("action").map(py_str).unwrap_or_else(|| default_action.clone()),
                references: Vec::new(),
                background: job.get("background").filter(|v| !v.is_null()).or(self.get("background")),
                fit: job.get("fit").filter(|v| !v.is_null()).or(self.get("fit")),
                mask: None,
            };
            let output = absolute(&output_dir.join(format!("{}.png", safe_topic_slug(&id))));
            match references.and_then(|references| self.build_request(ImageFields { references, ..fields })) {
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
            source = json!(ctx.conn.source.as_str());
            std::fs::create_dir_all(&output_dir)
                .map_err(|e| EngineError::new(Category::Internal, format!("cannot create {}: {e}", output_dir.display())))?;
            let results = self.execute(items, workers, limits.retries, ctx.clone(), &JOB_EVENTS, None).await;
            self.record_outcome(&ctx, &results);
            for r in results {
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
            let owner = (item.index, item.id.clone(), item.job.output.display().to_string(), item.job.request.image_model.clone());
            let handle = tasks.spawn(async move { run_item(item, semaphore, ctx, cancel, progress, backoff, retries, names, stop).await });
            owners.insert(handle.id(), owner);
        }
        let mut results = Vec::new();
        while let Some(joined) = tasks.join_next_with_id().await {
            match joined {
                Ok((_, result)) => results.push(result),
                Err(err) => {
                    let Some((index, id, output, model)) = owners.remove(&err.id()) else { continue };
                    let outcome = Err(EngineError::new(Category::Internal, format!("image task stopped unexpectedly: {err}")));
                    results.push(ItemResult { index, id, output, outcome, retried: 0, model });
                }
            }
        }
        results
    }
}

/// `job_timeout_sec` seconds: `<= 0` disables (Python), non-finite or above `MAX_JOB_TIMEOUT_SEC` is refused.
fn timeout_from(secs: f64) -> Result<Option<Duration>, EngineError> {
    let refuse = || invalid(format!("job_timeout_sec must be a finite number of seconds up to {MAX_JOB_TIMEOUT_SEC} (<=0 disables)"));
    if !secs.is_finite() || secs > MAX_JOB_TIMEOUT_SEC {
        return Err(refuse());
    }
    if secs <= 0.0 {
        return Ok(None);
    }
    Duration::try_from_secs_f64(secs).map(Some).map_err(|_| refuse())
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
    let model = item.job.request.image_model.clone();
    let finish = |outcome, retried| ItemResult {
        index: item.index,
        id: item.id.clone(),
        output: output.clone(),
        outcome,
        retried,
        model: model.clone(),
    };
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
                    _ = tokio::time::sleep(Duration::try_from_secs_f64(delay).unwrap_or(Duration::from_secs(60))) => {}
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
