//! Where requests go and which key they carry. No machine-specific defaults (spec §5.2).

use crate::error::{Category, EngineError};
use std::fmt;

/// API key that never prints itself.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Secret {
        Secret(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

pub trait EnvSource: Send + Sync {
    fn process(&self, key: &str) -> Option<String>;
    fn user(&self, key: &str) -> Option<String>;
}

/// Process environment, then the Windows user environment (`HKCU\Environment`).
pub struct SystemEnv;

impl EnvSource for SystemEnv {
    fn process(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    #[cfg(windows)]
    fn user(&self, key: &str) -> Option<String> {
        let env = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Environment").ok()?;
        env.get_value::<String, _>(key).ok().filter(|v| !v.is_empty())
    }

    #[cfg(not(windows))]
    fn user(&self, _key: &str) -> Option<String> {
        None
    }
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub base_url: String,
    pub api_key: Option<Secret>,
    /// `"process_env"` or `"user_env"`: where the base URL came from.
    pub source: &'static str,
}

/// Same rules as `tools/image_core.normalize_base_url`, without host rewriting.
pub fn normalize_base_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("base_url이 비어 있습니다.".into());
    }
    if raw.chars().any(|c| c <= ' ' || c == '\u{7f}') {
        return Err("base_url에 공백·제어 문자를 쓸 수 없습니다.".into());
    }
    let bad_form = || "base_url 형식이 올바르지 않습니다. http(s)://host:port 형태를 사용해 주세요.".to_string();
    let url = reqwest::Url::parse(raw).map_err(|_| bad_form())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("base_url은 http 또는 https만 가능합니다.".into());
    }
    let host = url.host_str().ok_or_else(bad_form)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("base_url에 인증 정보를 넣을 수 없습니다.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("base_url에 쿼리·프래그먼트를 넣을 수 없습니다.".into());
    }
    let mut base = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        base.push_str(&format!(":{port}"));
    }
    base.push_str(url.path().trim_end_matches('/'));
    if !base.ends_with("/v1") {
        base.push_str("/v1");
    }
    Ok(base)
}

const NOT_CONFIGURED: &str = "연결 설정이 없습니다. CLIPROXY_BASE_URL(예: http://127.0.0.1:8317/v1)과 CLIPROXY_API_KEY를 설정해 주세요.";

/// Base URL: process then user `CLIPROXY_BASE_URL`. Key: same order as `gen_image.resolve_api_key`.
pub fn resolve_connection(env: &dyn EnvSource) -> Result<Connection, EngineError> {
    let (raw, source) = match env.process("CLIPROXY_BASE_URL") {
        Some(value) => (value, "process_env"),
        None => match env.user("CLIPROXY_BASE_URL") {
            Some(value) => (value, "user_env"),
            None => return Err(EngineError::new(Category::InvalidRequest, NOT_CONFIGURED)),
        },
    };
    let base_url = normalize_base_url(&raw).map_err(|e| EngineError::new(Category::InvalidRequest, format!("CLIPROXY_BASE_URL: {e}")))?;
    let api_key = env
        .process("CLIPROXY_API_KEY")
        .or_else(|| env.process("OPENAI_API_KEY"))
        .or_else(|| env.user("CLIPROXY_API_KEY"))
        .or_else(|| env.user("OPENAI_API_KEY"))
        // A pasted key often carries a newline; it would otherwise fail as a bogus header.
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .map(Secret::new);
    Ok(Connection { base_url, api_key, source })
}
