//! Where requests go and which key they carry. No machine-specific defaults (spec §5.2).

use crate::error::{Category, EngineError};
use crate::http::{Auth, AuthScheme};
use crate::profiles::{Profile, ProfileStore, ProviderKind};
use crate::secrets::{MemoryStore, SecretStore};
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
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<Secret>,
    /// `"process_env"`, `"user_env"` or `"profile:<name>"`: where the connection came from.
    pub source: String,
    pub profile_id: Option<String>,
    /// Worker count when the spec sets no `concurrency`.
    pub concurrency_default: usize,
}

impl Connection {
    /// CLIProxyAPI connection outside any profile (environment variables, tests).
    pub fn cliproxy(base_url: String, api_key: Option<Secret>, source: &str) -> Connection {
        let kind = ProviderKind::Cliproxy;
        Connection {
            kind,
            base_url,
            api_key,
            source: source.to_string(),
            profile_id: None,
            concurrency_default: kind.default_concurrency(),
        }
    }

    /// Header scheme follows the kind: Gemini official uses `x-goog-api-key`, the rest Bearer.
    pub fn auth(&self) -> Auth<'_> {
        let scheme = match self.kind {
            ProviderKind::Gemini => AuthScheme::GoogApiKey,
            _ => AuthScheme::Bearer,
        };
        Auth { scheme, key: self.api_key.as_ref() }
    }
}

/// Same rules as `tools/image_core.normalize_base_url`, without host rewriting.
pub fn normalize_base_url(raw: &str) -> Result<String, String> {
    let mut base = validate_base_url(raw)?;
    if !base.ends_with("/v1") {
        base.push_str("/v1");
    }
    Ok(base)
}

/// `normalize_base_url` checks (no spaces, credentials or query) without adding `/v1`.
pub fn validate_base_url(raw: &str) -> Result<String, String> {
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
    Ok(base)
}

const NOT_CONFIGURED: &str = "연결 설정이 없습니다. CLIPROXY_BASE_URL(예: http://127.0.0.1:8317/v1)과 CLIPROXY_API_KEY를 설정하거나, \
`image-cli profile add`로 연결 프로필을 만들고 기본 프로필로 지정해 주세요.";

/// Inputs for `resolve`. Without a profile store only the environment is consulted.
pub struct Resolver<'a> {
    pub env: &'a dyn EnvSource,
    pub profiles: Option<&'a ProfileStore>,
    pub secrets: &'a dyn SecretStore,
}

/// Spec §5.2 order: explicit profile → process env → user env → default profile → error.
/// An explicit profile that is missing or incomplete is an error; nothing is borrowed from other sources.
pub fn resolve(r: &Resolver, explicit_profile: Option<&str>) -> Result<Connection, EngineError> {
    if let Some(wanted) = explicit_profile {
        let store = r.profiles.ok_or_else(|| {
            EngineError::new(
                Category::InvalidRequest,
                format!("프로필을 찾을 수 없습니다: {} (프로필 폴더를 정할 수 없습니다)", wanted.trim()),
            )
        })?;
        let profile = store.find(wanted).map_err(|e| match e.category {
            Category::NotFound => EngineError::new(Category::InvalidRequest, e.message),
            _ => e,
        })?;
        return from_profile(&profile, r.secrets);
    }
    if let Some(conn) = from_env(r.env)? {
        return Ok(conn);
    }
    if let Some(store) = r.profiles {
        let file = store.load()?;
        if let Some(profile) = file.default.as_ref().and_then(|id| file.profiles.iter().find(|p| &p.id == id)) {
            return from_profile(profile, r.secrets);
        }
    }
    Err(EngineError::new(Category::InvalidRequest, NOT_CONFIGURED))
}

fn from_profile(profile: &Profile, secrets: &dyn SecretStore) -> Result<Connection, EngineError> {
    // The file may have been edited by hand; re-check the address before any key is sent to it.
    let base_url = profile
        .kind
        .normalize_base_url(&profile.base_url)
        .map_err(|e| EngineError::new(Category::InvalidRequest, format!("프로필 {}의 주소: {}", profile.name, e.message)))?;
    let api_key = secrets.get(&profile.id)?;
    if api_key.is_none() && profile.kind.key_required() {
        return Err(EngineError::new(
            Category::InvalidRequest,
            format!(
                "키가 없습니다(프로필 {}). `image-cli profile update \"{}\" --key-stdin`으로 키를 저장해 주세요.",
                profile.name, profile.name
            ),
        ));
    }
    let concurrency_default = profile.concurrency.map(|c| c as usize).unwrap_or_else(|| profile.kind.default_concurrency());
    Ok(Connection {
        kind: profile.kind,
        base_url,
        api_key,
        source: format!("profile:{}", profile.name),
        profile_id: Some(profile.id.clone()),
        concurrency_default,
    })
}

/// Environment-only resolution (no profiles), kept for callers that predate profiles.
pub fn resolve_connection(env: &dyn EnvSource) -> Result<Connection, EngineError> {
    resolve(&Resolver { env, profiles: None, secrets: &MemoryStore::new() }, None)
}

/// Base URL: process then user `CLIPROXY_BASE_URL`. Key: same order as `gen_image.resolve_api_key`.
fn from_env(env: &dyn EnvSource) -> Result<Option<Connection>, EngineError> {
    let (raw, source) = match env.process("CLIPROXY_BASE_URL") {
        Some(value) => (value, "process_env"),
        None => match env.user("CLIPROXY_BASE_URL") {
            Some(value) => (value, "user_env"),
            None => return Ok(None),
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
    Ok(Some(Connection::cliproxy(base_url, api_key, source)))
}
