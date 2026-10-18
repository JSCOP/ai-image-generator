//! One output = one image + one record, published together (spec §5.4). No database.
//!
//! Order: lock (create-new) → pending record → `.part` image → verify → link to the final
//! name (never overwrites) → complete record → unlock.

use crate::error::{Category, EngineError};
use crate::normalize::{AlphaStats, Fit};
use crate::provider::ImageRequest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Per-image record. The first eight fields are the Python record; the rest are optional additions.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Record {
    pub prompt: String,
    pub image_model: String,
    pub size: String,
    pub quality: String,
    pub action: String,
    pub reference_images: Vec<String>,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<AlphaStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_source: Option<String>,
    /// Profile id when the connection came from a profile (spec §5.2 "선택 프로필 id").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<EngineError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Identity of a request for resume: prompt, model, size, quality, action, reference contents, background, fit.
pub fn request_hash(req: &ImageRequest, fit: Fit) -> Result<String, EngineError> {
    let references = req
        .references
        .iter()
        .map(|p| {
            std::fs::read(p)
                .map(|bytes| sha256_hex(&bytes))
                .map_err(|e| EngineError::new(Category::InvalidRequest, format!("reference image not readable: {}: {e}", p.display())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Keys are written in a fixed order, so this serialization is canonical.
    let identity = json!({
        "prompt": req.prompt, "image_model": req.image_model, "size": req.size.to_string(),
        "quality": req.quality, "action": req.action.as_str(), "references": references,
        "background": req.background.as_str(), "fit": fit.as_str(),
    });
    Ok(sha256_hex(identity.to_string().as_bytes()))
}

pub fn write_json_atomic(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!("{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let result = std::fs::write(&temporary, text).and_then(|_| std::fs::rename(&temporary, path));
    let _ = std::fs::remove_file(&temporary);
    result
}

pub enum Existing {
    Absent,
    Reusable,
    /// Image and a pending/failed record for this request: a crash between publishing and
    /// recording. Reuse it after rewriting the record as complete (`Reservation::finalize_existing`).
    NeedsFinalize,
    Mismatch(String),
    Corrupt(String),
}

/// `Ok(None)` only when the record file is absent. A record that cannot be read, is not a JSON
/// object, or has a non-string `status` is an error: it must never pass as a legacy Python record.
fn read_record(path: &Path) -> Result<Option<Value>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("existing record is not readable; fix or remove it, or use a new topic: {}: {e}", path.display())),
    };
    let unusable = || format!("existing record is damaged; fix or remove it, or use a new topic: {}", path.display());
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| unusable())?;
    let status_ok = value.get("status").is_none_or(|s| s.is_string() || s.is_null());
    if !value.is_object() || !status_ok {
        return Err(unusable());
    }
    Ok(Some(value))
}

fn decodes(path: &Path) -> bool {
    image::ImageReader::open(path).and_then(|r| r.with_guessed_format()).ok().and_then(|r| r.decode().ok()).is_some()
}

pub fn inspect_existing(output: &Path, record: Option<&Path>, request_hash: &str) -> Existing {
    if !output.is_file() {
        return Existing::Absent;
    }
    let corrupt = || Existing::Corrupt(format!("existing output is not a valid image: {}", output.display()));
    let saved = match record.map(read_record).transpose() {
        Ok(saved) => saved.flatten(),
        Err(message) => return Existing::Corrupt(message),
    };
    let Some((saved, status)) = saved.and_then(|v| v["status"].as_str().map(str::to_string).map(|s| (v, s))) else {
        // No record, or a record written by the Python CLI: the image itself is the evidence.
        return if decodes(output) { Existing::Reusable } else { corrupt() };
    };
    if saved["request_hash"].as_str() != Some(request_hash) {
        return Existing::Mismatch(format!("existing output was made from a different request; use a new topic: {}", output.display()));
    }
    if status == "complete" {
        if let Some(expected) = saved["sha256"].as_str() {
            let actual = std::fs::read(output).map(|b| sha256_hex(&b)).unwrap_or_default();
            if actual != expected {
                return Existing::Corrupt(format!("existing output does not match its record: {}", output.display()));
            }
        }
    }
    if !decodes(output) {
        return corrupt();
    }
    if status == "complete" {
        Existing::Reusable
    } else {
        Existing::NeedsFinalize
    }
}

#[derive(Debug)]
pub struct Reservation {
    pub output: PathBuf,
    pub record: Option<PathBuf>,
    pub request_id: String,
    lock: PathBuf,
    part: PathBuf,
    released: std::sync::atomic::AtomicBool,
}

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64()
}

fn io_error(context: &str, path: &Path, err: std::io::Error) -> EngineError {
    EngineError::new(Category::Internal, format!("{context}: {}: {err}", path.display()))
}

pub fn reserve(output: &Path, record: Option<&Path>, lock_dir: &Path) -> Result<Reservation, EngineError> {
    std::fs::create_dir_all(lock_dir).map_err(|e| io_error("cannot create metadata folder", lock_dir, e))?;
    let name = output.file_name().unwrap_or_default().to_string_lossy().to_string();
    let lock = lock_dir.join(format!("{name}.lock"));
    let request_id = uuid::Uuid::new_v4().to_string();
    let content = json!({"pid": std::process::id(), "request_id": request_id, "created_at": now_secs()}).to_string();
    for attempt in 0..2 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(mut file) => {
                file.write_all(content.as_bytes()).map_err(|e| io_error("cannot write lock", &lock, e))?;
                let part = output.with_file_name(format!(".{name}.{}.part", uuid::Uuid::new_v4().simple()));
                return Ok(Reservation {
                    output: output.to_path_buf(),
                    record: record.map(Path::to_path_buf),
                    request_id,
                    lock,
                    part,
                    released: Default::default(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => match stale_lock_content(&lock) {
                Some(judged) => {
                    take_over_stale_lock(&lock, &judged);
                }
                None => break,
            },
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => break,
            Err(e) => return Err(io_error("cannot create lock", &lock, e)),
        }
    }
    Err(EngineError::new(Category::Conflict, format!("output is being written by another run: {}", output.display())))
}

/// The lock's bytes when its owner is gone, so a takeover can check nothing changed meanwhile.
fn stale_lock_content(lock: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(lock).ok()?;
    let parsed: Option<Value> = serde_json::from_slice(&bytes).ok();
    let stale = match parsed.and_then(|v| v["pid"].as_u64()) {
        Some(pid) => !process_alive(pid),
        // Unreadable lock: a writer may be between create and write, so only reclaim old ones.
        None => std::fs::metadata(lock)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(10)),
    };
    stale.then_some(bytes)
}

/// Remove a lock judged stale, but only if it still holds the judged bytes. Moving it aside
/// first means two runs racing for the same stale lock cannot delete each other's new lock.
#[doc(hidden)]
pub fn take_over_stale_lock(lock: &Path, judged: &[u8]) -> bool {
    let aside = lock.with_extension(format!("{}.stale", uuid::Uuid::new_v4().simple()));
    if std::fs::rename(lock, &aside).is_err() {
        return false;
    }
    let same = std::fs::read(&aside).is_ok_and(|bytes| bytes == judged);
    if !same {
        // We moved a live run's lock: put it back (never over a newer one).
        let _ = std::fs::hard_link(&aside, lock);
    }
    let _ = std::fs::remove_file(&aside);
    same
}

#[cfg(windows)]
fn process_alive(pid: u64) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    const STILL_ACTIVE: u32 = 259;
    let Ok(pid) = u32::try_from(pid) else { return false };
    // SAFETY: plain Win32 calls on a handle we own and close.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}

#[cfg(unix)]
fn process_alive(pid: u64) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false };
    // SAFETY: signal 0 only checks for existence.
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

impl Reservation {
    /// Refuses when an image already sits at the output: its record belongs to whoever published it.
    pub fn write_pending(&self, rec: &Record) -> Result<(), EngineError> {
        if self.output.exists() {
            return Err(EngineError::new(
                Category::Conflict,
                format!("Output already exists; use a new topic or resume: {}", self.output.display()),
            ));
        }
        let Some(path) = &self.record else { return Ok(()) };
        let mut rec = rec.clone();
        rec.status = Some("pending".into());
        rec.request_id = Some(self.request_id.clone());
        write_json_atomic(path, &rec).map_err(|e| io_error("cannot write record", path, e))
    }

    pub fn publish(&self, png: &[u8], mut rec: Record) -> Result<Record, EngineError> {
        if let Some(parent) = self.output.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error("cannot create output folder", parent, e))?;
        }
        let exists =
            || EngineError::new(Category::Conflict, format!("Output already exists; use a new topic or resume: {}", self.output.display()));
        if self.output.exists() {
            return Err(exists());
        }
        write_new(&self.part, png)?;
        let written = std::fs::read(&self.part).map_err(|e| io_error("cannot read back image", &self.part, e))?;
        if written != png || image::load_from_memory(&written).is_err() {
            return Err(EngineError::new(Category::Internal, format!("saved image failed verification: {}", self.part.display())));
        }
        match std::fs::hard_link(&self.part, &self.output) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(exists()),
            // File systems without hard links: create-new still never overwrites.
            Err(_) => write_new(&self.output, png).map_err(|e| if self.output.exists() { exists() } else { e })?,
        }
        rec.status = Some("complete".into());
        rec.request_id = Some(self.request_id.clone());
        rec.sha256 = Some(sha256_hex(png));
        rec.error = None;
        if let Some(path) = &self.record {
            write_json_atomic(path, &rec).map_err(|e| io_error("cannot write record", path, e))?;
        }
        self.release();
        Ok(rec)
    }

    /// Records the failure, unless an image now sits at the output: then the record belongs to
    /// whoever published it (another run, or ours if only the record write failed).
    pub fn fail(&self, mut rec: Record, err: &EngineError) {
        if let Some(path) = self.record.as_ref().filter(|_| !self.output.exists()) {
            rec.status = Some("failed".into());
            rec.request_id = Some(self.request_id.clone());
            rec.error = Some(err.clone());
            let _ = write_json_atomic(path, &rec);
        }
        self.release();
    }

    /// Reuse an image whose record was left pending/failed: rewrite the record as complete.
    /// Re-inspects under this lock, so only a record of the same request is ever finalized.
    pub fn finalize_existing(&self, request_hash: &str) -> Result<Record, EngineError> {
        let conflict = |message: String| EngineError::new(Category::Conflict, message);
        match inspect_existing(&self.output, self.record.as_deref(), request_hash) {
            Existing::NeedsFinalize => {}
            Existing::Mismatch(message) | Existing::Corrupt(message) => return Err(conflict(message)),
            Existing::Absent | Existing::Reusable => {
                return Err(conflict(format!("existing output changed while waiting for its lock: {}", self.output.display())))
            }
        }
        let bytes = std::fs::read(&self.output).map_err(|e| io_error("cannot read existing image", &self.output, e))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|e| conflict(format!("existing output is not a valid image: {}: {e}", self.output.display())))?;
        let mut rec: Record = match &self.record {
            Some(path) => {
                let bytes = std::fs::read(path).map_err(|e| io_error("cannot read record", path, e))?;
                serde_json::from_slice(&bytes).map_err(|_| {
                    conflict(format!("existing record is damaged; fix or remove it, or use a new topic: {}", path.display()))
                })?
            }
            None => Record::default(),
        };
        rec.status = Some("complete".into());
        rec.sha256 = Some(sha256_hex(&bytes));
        rec.saved_size = Some(format!("{}x{}", image.width(), image.height()));
        rec.error = None;
        if let Some(path) = &self.record {
            write_json_atomic(path, &rec).map_err(|e| io_error("cannot write record", path, e))?;
        }
        self.release();
        Ok(rec)
    }

    /// Once only, and only our own lock: a stale-lock takeover may have given the name to another run.
    fn release(&self) {
        if self.released.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let _ = std::fs::remove_file(&self.part);
        let owner: Option<Value> = std::fs::read(&self.lock).ok().and_then(|b| serde_json::from_slice(&b).ok());
        if owner.is_some_and(|v| v["request_id"].as_str() == Some(self.request_id.as_str())) {
            let _ = std::fs::remove_file(&self.lock);
        }
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let mut file =
        std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| io_error("cannot create image", path, e))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| io_error("cannot write image", path, e))
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.release();
    }
}
