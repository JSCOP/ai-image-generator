//! Connection profiles in `profiles.json` (kind + address, never keys) and last generation results in `status.json`.

use crate::connection::{normalize_base_url, validate_base_url, EnvSource};
use crate::error::{Category, EngineError};
use crate::runner::MAX_CONCURRENCY;
use crate::store::write_json_atomic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FILE_VERSION: u32 = 1;
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_RETRY: Duration = Duration::from_millis(50);
const MAX_NAME_CHARS: usize = 64;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Cliproxy,
    Openai,
    Gemini,
    Xai,
    OpenaiCompatible,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 5] =
        [ProviderKind::Cliproxy, ProviderKind::Openai, ProviderKind::Gemini, ProviderKind::Xai, ProviderKind::OpenaiCompatible];

    pub fn parse(text: &str) -> Option<ProviderKind> {
        ProviderKind::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Cliproxy => "cliproxy",
            ProviderKind::Openai => "openai",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Xai => "xai",
            ProviderKind::OpenaiCompatible => "openai_compatible",
        }
    }

    pub fn default_base_url(self) -> Option<&'static str> {
        match self {
            ProviderKind::Openai => Some("https://api.openai.com/v1"),
            ProviderKind::Gemini => Some("https://generativelanguage.googleapis.com"),
            ProviderKind::Xai => Some("https://api.x.ai/v1"),
            ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => None,
        }
    }

    pub fn key_required(self) -> bool {
        matches!(self, ProviderKind::Openai | ProviderKind::Gemini | ProviderKind::Xai)
    }

    pub fn default_concurrency(self) -> usize {
        match self {
            ProviderKind::Cliproxy | ProviderKind::OpenaiCompatible => 30,
            ProviderKind::Openai | ProviderKind::Gemini | ProviderKind::Xai => 4,
        }
    }

    /// Display name for screens.
    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Cliproxy => "CLIProxyAPI",
            ProviderKind::Openai => "OpenAI 공식",
            ProviderKind::Gemini => "Gemini 공식",
            ProviderKind::Xai => "xAI 공식",
            ProviderKind::OpenaiCompatible => "OpenAI 호환(기타)",
        }
    }

    /// Validated address; `/v1` is added for OpenAI-style kinds only (spec §6.1).
    pub fn normalize_base_url(self, raw: &str) -> Result<String, EngineError> {
        let base = match self {
            ProviderKind::Gemini => validate_base_url(raw),
            _ => normalize_base_url(raw),
        }
        .map_err(invalid)?;
        // The adapter adds `/v1beta/...` itself; a pasted API path would be doubled, and Gemini addresses are never rewritten.
        if self == ProviderKind::Gemini {
            let path = reqwest::Url::parse(&base).map(|u| u.path().to_ascii_lowercase()).unwrap_or_default();
            if path.ends_with("/v1beta") || path.contains("/v1beta/") {
                return Err(invalid(
                    "Gemini 주소에는 /v1beta 경로를 넣지 않습니다. 호스트 주소만 입력해 주세요(예: https://generativelanguage.googleapis.com).",
                ));
            }
        }
        Ok(base)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProfileFile {
    pub version: u32,
    pub default: Option<String>,
    pub profiles: Vec<Profile>,
}

impl Default for ProfileFile {
    fn default() -> ProfileFile {
        ProfileFile { version: FILE_VERSION, default: None, profiles: Vec::new() }
    }
}

impl ProfileFile {
    fn position(&self, name_or_id: &str) -> Option<usize> {
        let wanted = name_or_id.trim();
        self.profiles.iter().position(|p| p.id == wanted).or_else(|| {
            let folded = wanted.to_lowercase();
            self.profiles.iter().position(|p| p.name.to_lowercase() == folded)
        })
    }

    fn index(&self, name_or_id: &str) -> Result<usize, EngineError> {
        self.position(name_or_id)
            .ok_or_else(|| EngineError::new(Category::NotFound, format!("프로필을 찾을 수 없습니다: {}", name_or_id.trim())))
    }

    fn check_name_free(&self, name: &str, except_id: Option<&str>) -> Result<(), EngineError> {
        let folded = name.to_lowercase();
        if self.profiles.iter().any(|p| Some(p.id.as_str()) != except_id && p.name.to_lowercase() == folded) {
            return Err(EngineError::new(Category::Conflict, format!("같은 이름의 프로필이 이미 있습니다: {name}")));
        }
        Ok(())
    }
}

/// `concurrency: Some(None)` clears the profile override.
#[derive(Default, Clone, Debug)]
pub struct ProfilePatch {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub concurrency: Option<Option<u32>>,
}

/// Outcome of the last real generation through a profile (spec §6.2 "실제 생성 가능").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GenerationStatus {
    pub ok: bool,
    /// Seconds since the Unix epoch.
    pub at: f64,
    pub model: String,
    pub category: Option<Category>,
    pub provider_code: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct StatusFile {
    version: u32,
    generations: HashMap<String, GenerationStatus>,
}

pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    /// The folder need not exist yet.
    pub fn open(dir: PathBuf) -> ProfileStore {
        ProfileStore { dir }
    }

    /// `IMAGE_STUDIO_HOME`, else `%APPDATA%\ImageStudio` (process environment).
    pub fn default_dir(env: &dyn EnvSource) -> Option<PathBuf> {
        if let Some(home) = env.process("IMAGE_STUDIO_HOME").filter(|v| !v.trim().is_empty()) {
            return Some(PathBuf::from(home));
        }
        env.process("APPDATA").filter(|v| !v.trim().is_empty()).map(|appdata| PathBuf::from(appdata).join("ImageStudio"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn profiles_path(&self) -> PathBuf {
        self.dir.join("profiles.json")
    }

    fn status_path(&self) -> PathBuf {
        self.dir.join("status.json")
    }

    /// A missing file is an empty list; an unreadable one is an error and is never overwritten.
    pub fn load(&self) -> Result<ProfileFile, EngineError> {
        let path = self.profiles_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ProfileFile::default()),
            Err(e) => return Err(unreadable(&path, &e.to_string())),
        };
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| unreadable(&path, &e.to_string()))?;
        match value.get("version").and_then(|v| v.as_u64()) {
            Some(v) if v == u64::from(FILE_VERSION) => {}
            Some(v) => return Err(unreadable(&path, &format!("프로필 파일 버전 {v}은(는) 지원하지 않습니다(지원: {FILE_VERSION})"))),
            None => return Err(unreadable(&path, "version 항목이 없습니다")),
        }
        let mut file: ProfileFile = serde_json::from_value(value).map_err(|e| unreadable(&path, &e.to_string()))?;
        // The file may be edited by hand: every entry must pass the same rules as `add`.
        for (index, profile) in file.profiles.iter().enumerate() {
            check_entry(&file.profiles[..index], profile).map_err(|e| {
                unreadable(&path, &format!("{}번째 프로필(id {:?}, 이름 {:?}): {}", index + 1, profile.id, profile.name, e.message))
            })?;
        }
        // A default naming no listed profile means "no default", so the other entries stay usable.
        if file.default.as_ref().is_some_and(|default| !file.profiles.iter().any(|p| &p.id == default)) {
            file.default = None;
        }
        Ok(file)
    }

    pub fn find(&self, name_or_id: &str) -> Result<Profile, EngineError> {
        let file = self.load()?;
        Ok(file.profiles[file.index(name_or_id)?].clone())
    }

    pub fn add(&self, name: &str, kind: ProviderKind, base_url: Option<&str>, concurrency: Option<u32>) -> Result<Profile, EngineError> {
        self.add_with(name, kind, base_url, concurrency, false, |_| Ok(()))
    }

    /// `add` that can also become the default in the same write. `before_write` runs under the lock with the
    /// new profile (e.g. to save its key); if it fails, profiles.json is not touched.
    pub fn add_with(
        &self,
        name: &str,
        kind: ProviderKind,
        base_url: Option<&str>,
        concurrency: Option<u32>,
        make_default: bool,
        before_write: impl FnOnce(&Profile) -> Result<(), EngineError>,
    ) -> Result<Profile, EngineError> {
        let name = clean_name(name)?;
        let raw_url = base_url.filter(|u| !u.trim().is_empty()).or(kind.default_base_url()).ok_or_else(|| {
            EngineError::new(Category::InvalidRequest, format!("{} 프로필은 주소(base_url)를 입력해야 합니다.", kind.label()))
        })?;
        let base_url = kind.normalize_base_url(raw_url)?;
        check_concurrency(concurrency)?;
        self.modify(|file| {
            file.check_name_free(&name, None)?;
            let id = loop {
                let candidate = format!("p-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
                if !file.profiles.iter().any(|p| p.id == candidate) {
                    break candidate;
                }
            };
            let profile = Profile { id, name: name.clone(), kind, base_url: base_url.clone(), concurrency };
            before_write(&profile)?;
            file.profiles.push(profile.clone());
            if make_default {
                file.default = Some(profile.id.clone());
            }
            Ok(profile)
        })
    }

    pub fn update(&self, name_or_id: &str, patch: ProfilePatch) -> Result<Profile, EngineError> {
        let name = patch.name.as_deref().map(clean_name).transpose()?;
        if let Some(concurrency) = patch.concurrency {
            check_concurrency(concurrency)?;
        }
        self.modify(|file| {
            let index = file.index(name_or_id)?;
            let current = file.profiles[index].clone();
            if let Some(name) = &name {
                file.check_name_free(name, Some(&current.id))?;
            }
            let base_url = match &patch.base_url {
                Some(raw) => current.kind.normalize_base_url(raw)?,
                None => current.base_url.clone(),
            };
            let profile = Profile {
                name: name.clone().unwrap_or(current.name),
                base_url,
                concurrency: patch.concurrency.unwrap_or(current.concurrency),
                ..current
            };
            file.profiles[index] = profile.clone();
            Ok(profile)
        })
    }

    /// Also clears the default when it pointed here. The caller deletes the stored key.
    pub fn remove(&self, name_or_id: &str) -> Result<Profile, EngineError> {
        let removed = self.modify(|file| {
            let removed = file.profiles.remove(file.index(name_or_id)?);
            if file.default.as_deref() == Some(removed.id.as_str()) {
                file.default = None;
            }
            Ok(removed)
        })?;
        // status.json is only a cache; a failure here must not undo the removal.
        let _ = self.modify_status(|status| {
            status.generations.remove(&removed.id);
        });
        Ok(removed)
    }

    pub fn set_default(&self, name_or_id: Option<&str>) -> Result<(), EngineError> {
        self.modify(|file| {
            file.default = match name_or_id {
                Some(wanted) => Some(file.profiles[file.index(wanted)?].id.clone()),
                None => None,
            };
            Ok(())
        })
    }

    pub fn record_generation(&self, profile_id: &str, outcome: &GenerationStatus) -> Result<(), EngineError> {
        self.modify_status(|status| {
            status.generations.insert(profile_id.to_string(), outcome.clone());
        })
    }

    /// Unreadable `status.json` is reported; `record_generation` replaces it.
    pub fn generation_status(&self) -> Result<HashMap<String, GenerationStatus>, EngineError> {
        let path = self.status_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<StatusFile>(&text)
                .map(|file| file.generations)
                .map_err(|e| EngineError::new(Category::Internal, format!("생성 상태 파일을 읽을 수 없습니다: {}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(EngineError::new(Category::Internal, format!("생성 상태 파일을 읽을 수 없습니다: {}: {e}", path.display()))),
        }
    }

    fn modify<T>(&self, change: impl FnOnce(&mut ProfileFile) -> Result<T, EngineError>) -> Result<T, EngineError> {
        let path = self.profiles_path();
        let _lock = FileLock::acquire(&self.dir, &path)?;
        let mut file = self.load()?;
        let out = change(&mut file)?;
        write_json_atomic(&path, &file).map_err(|e| io_error("프로필 파일을 저장할 수 없습니다", &path, e))?;
        Ok(out)
    }

    fn modify_status(&self, change: impl FnOnce(&mut StatusFile)) -> Result<(), EngineError> {
        let path = self.status_path();
        let _lock = FileLock::acquire(&self.dir, &path)?;
        let mut status = self.generation_status().map(|generations| StatusFile { version: FILE_VERSION, generations }).unwrap_or_default();
        status.version = FILE_VERSION;
        change(&mut status);
        write_json_atomic(&path, &status).map_err(|e| io_error("생성 상태 파일을 저장할 수 없습니다", &path, e))
    }
}

/// `<file>.lock` created exclusively; removed on drop.
struct FileLock {
    path: PathBuf,
}

impl FileLock {
    fn acquire(dir: &Path, file: &Path) -> Result<FileLock, EngineError> {
        std::fs::create_dir_all(dir).map_err(|e| io_error("프로필 폴더를 만들 수 없습니다", dir, e))?;
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let path = file.with_file_name(format!("{name}.lock"));
        let started = Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(FileLock { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if started.elapsed() >= LOCK_WAIT {
                        return Err(EngineError::new(
                            Category::Conflict,
                            format!(
                                "다른 작업이 프로필 파일을 수정 중입니다. 잠시 후 다시 시도하고, 계속되면 {}을(를) 지워 주세요.",
                                path.display()
                            ),
                        ));
                    }
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(e) => return Err(io_error("프로필 잠금 파일을 만들 수 없습니다", &path, e)),
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Warning text when `base_url` is plain `http://` to a public-internet host (spec §6.1).
pub fn transport_warning(base_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(base_url.trim()).ok()?;
    if url.scheme() != "http" {
        return None;
    }
    let host = url.host_str()?.trim_start_matches('[').trim_end_matches(']').to_lowercase();
    let trusted = match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback() || ip.is_private() || (a == 100 && (64..=127).contains(&b))
        }
        // Tailscale IPv6 addresses are fd7a:115c:a1e0::/48.
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
        // A single-label name (no dot, e.g. MagicDNS `proxy-pc`) only resolves inside the local or tailnet DNS.
        Err(_) => !host.contains('.') || host.ends_with(".localhost") || host.ends_with(".ts.net"),
    };
    if trusted {
        return None;
    }
    Some(format!(
        "{host}은(는) 암호화되지 않은 http:// 주소입니다. 공용 인터넷에서는 키와 이미지가 그대로 노출될 수 있으니 https 주소를 쓰거나 Tailscale·사설망 안에서만 사용해 주세요."
    ))
}

fn clean_name(raw: &str) -> Result<String, EngineError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(invalid("프로필 이름이 비어 있습니다."));
    }
    if name.chars().any(char::is_control) {
        return Err(invalid("프로필 이름에 제어 문자를 쓸 수 없습니다."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!("프로필 이름이 너무 깁니다(최대 {MAX_NAME_CHARS}자).")));
    }
    if looks_like_id(name) {
        return Err(invalid("프로필 이름을 id 형식(p-와 8자리 16진수)으로 지을 수 없습니다."));
    }
    Ok(name.to_string())
}

/// Rules for one stored entry; `earlier` are the entries before it (ids and names must stay unique).
fn check_entry(earlier: &[Profile], profile: &Profile) -> Result<(), EngineError> {
    if profile.id.trim().is_empty() || profile.id.chars().any(char::is_control) {
        return Err(invalid("id가 비어 있거나 제어 문자가 있습니다."));
    }
    if earlier.iter().any(|p| p.id == profile.id) {
        return Err(invalid("같은 id의 프로필이 이미 있습니다."));
    }
    let name = clean_name(&profile.name)?;
    if name != profile.name {
        return Err(invalid("프로필 이름 앞뒤에 공백이 있습니다."));
    }
    let folded = name.to_lowercase();
    if earlier.iter().any(|p| p.name.to_lowercase() == folded) {
        return Err(invalid("같은 이름의 프로필이 이미 있습니다."));
    }
    profile.kind.normalize_base_url(&profile.base_url)?;
    check_concurrency(profile.concurrency)
}

fn looks_like_id(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.len() == 10 && lower.starts_with("p-") && lower[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn check_concurrency(concurrency: Option<u32>) -> Result<(), EngineError> {
    match concurrency {
        Some(n) if n == 0 || i64::from(n) > MAX_CONCURRENCY => Err(invalid(format!("동시 실행 수는 1~{MAX_CONCURRENCY} 사이여야 합니다."))),
        _ => Ok(()),
    }
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(Category::InvalidRequest, message)
}

fn unreadable(path: &Path, detail: &str) -> EngineError {
    invalid(format!("프로필 파일을 읽을 수 없습니다: {}: {detail}. 파일을 고치거나 옮긴 뒤 다시 시도해 주세요.", path.display()))
}

fn io_error(context: &str, path: &Path, err: std::io::Error) -> EngineError {
    EngineError::new(Category::Internal, format!("{context}: {}: {err}", path.display()))
}
