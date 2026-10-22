//! Connection screen services: profile list, add/edit/remove, default, connection check and model list.
//! Keys go only to the `SecretStore`; views carry `key_configured` and nothing else about a key.

use std::fmt;
use std::time::Duration;

use image_core::adapter::{list_models, model_supported, reference_limit, supports_native_transparency, supports_references};
use image_core::catalog::{backend, is_listable_image_model, Backend};
use image_core::check::{check_connection, ConnectionReport};
use image_core::connection::{normalize_base_url, resolve, Connection, Resolver};
use image_core::error::{redact, Category};
use image_core::http::Http;
use image_core::profiles::{transport_warning, GenerationStatus, Profile, ProfilePatch, ProfileStore, ProviderKind};
use serde::{Deserialize, Serialize};

use super::generate::MAX_REFERENCES;
use super::{Ctx, ServiceError};

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub kind_label: String,
    pub base_url: String,
    pub concurrency: Option<u32>,
    pub is_default: bool,
    pub key_configured: bool,
    pub warning: Option<String>,
    pub last_generation: Option<GenerationStatus>,
}

/// The `CLIPROXY_*` environment connection, shown read-only.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvConnectionView {
    pub base_url: String,
    /// `"process_env"` or `"user_env"`.
    pub source: String,
    pub key_configured: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsView {
    pub profiles: Vec<ProfileView>,
    pub env: Option<EnvConnectionView>,
    /// Name of the connection a run without an explicit profile would use, `"env"`, or `None`.
    pub active: Option<String>,
}

/// Form input. `id: None` adds a profile. `key`: `None` keeps the stored key, blank deletes it, anything else replaces it.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub concurrency: Option<u32>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub make_default: bool,
}

impl fmt::Debug for ProfileInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = match &self.key {
            None => "keep",
            Some(k) if k.trim().is_empty() => "delete",
            Some(_) => "replace",
        };
        f.debug_struct("ProfileInput")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("concurrency", &self.concurrency)
            .field("key", &key)
            .field("make_default", &self.make_default)
            .finish()
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub id: String,
    pub provider: String,
    pub references: bool,
    /// References the screen may attach: the model's limit, at most the run limit (0 = none).
    pub max_references: u32,
    pub transparency: bool,
}

pub(super) fn store(ctx: &Ctx) -> ProfileStore {
    ProfileStore::open(ctx.home.clone())
}

fn view(ctx: &Ctx, profile: &Profile, default: Option<&str>, status: Option<&GenerationStatus>) -> ProfileView {
    ProfileView {
        id: profile.id.clone(),
        name: profile.name.clone(),
        kind: profile.kind,
        kind_label: profile.kind.label().to_string(),
        base_url: profile.base_url.clone(),
        concurrency: profile.concurrency,
        is_default: default == Some(profile.id.as_str()),
        key_configured: ctx.secrets.get(&profile.id).ok().flatten().is_some(),
        warning: transport_warning(&profile.base_url),
        last_generation: status.cloned(),
    }
}

fn view_of(ctx: &Ctx, store: &ProfileStore, id: &str) -> Result<ProfileView, ServiceError> {
    let file = store.load()?;
    let profile = file
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| ServiceError::new(Category::NotFound, format!("프로필을 찾을 수 없습니다: {id}")))?;
    let status = store.generation_status().unwrap_or_default();
    Ok(view(ctx, profile, file.default.as_deref(), status.get(id)))
}

/// Environment connection as `resolve` would see it (process, then user `CLIPROXY_BASE_URL`).
fn env_connection(ctx: &Ctx) -> Option<EnvConnectionView> {
    let env = ctx.env.as_ref();
    let (raw, source) = match env.process("CLIPROXY_BASE_URL") {
        Some(value) => (value, "process_env"),
        None => (env.user("CLIPROXY_BASE_URL")?, "user_env"),
    };
    let key_configured = ["CLIPROXY_API_KEY", "OPENAI_API_KEY"]
        .iter()
        .flat_map(|name| [env.process(name), env.user(name)])
        .flatten()
        .any(|key| !key.trim().is_empty());
    let base_url = normalize_base_url(&raw).unwrap_or_else(|_| raw.trim().to_string());
    Some(EnvConnectionView { base_url, source: source.to_string(), key_configured })
}

pub fn connections(ctx: &Ctx) -> Result<ConnectionsView, ServiceError> {
    let store = store(ctx);
    let file = store.load()?;
    let status = store.generation_status().unwrap_or_default();
    let profiles: Vec<ProfileView> = file.profiles.iter().map(|p| view(ctx, p, file.default.as_deref(), status.get(&p.id))).collect();
    let env = env_connection(ctx);
    // Resolution order (spec §5.2) without an explicit profile: environment first, then the default profile.
    let active = if env.is_some() { Some("env".to_string()) } else { profiles.iter().find(|p| p.is_default).map(|p| p.name.clone()) };
    Ok(ConnectionsView { profiles, env, active })
}

/// Blank address: the kind's official address, or an error for kinds that have none.
fn address(kind: ProviderKind, base_url: Option<&str>) -> Result<String, ServiceError> {
    match base_url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => Ok(url.to_string()),
        None => kind.default_base_url().map(str::to_string).ok_or_else(|| {
            ServiceError::new(Category::InvalidRequest, format!("{} 프로필은 주소(base_url)를 입력해야 합니다.", kind.label()))
        }),
    }
}

pub fn save_profile(ctx: &Ctx, input: ProfileInput) -> Result<ProfileView, ServiceError> {
    let store = store(ctx);
    let new_key = input.key.as_deref().map(str::trim);
    let Some(id) = input.id.as_deref().map(str::trim).filter(|id| !id.is_empty()) else {
        let url = address(input.kind, input.base_url.as_deref())?;
        let mut saved_key_for: Option<String> = None;
        // Profile, default flag and key are saved together; the key is stored under the profile lock just before the write.
        let result = store.add_with(&input.name, input.kind, Some(&url), input.concurrency, input.make_default, |profile| {
            if let Some(key) = new_key.filter(|k| !k.is_empty()) {
                ctx.secrets.set(&profile.id, key)?;
                saved_key_for = Some(profile.id.clone());
            }
            Ok(())
        });
        if let (Err(_), Some(id)) = (&result, &saved_key_for) {
            let _ = ctx.secrets.delete(id);
        }
        let profile = result?;
        return view_of(ctx, &store, &profile.id);
    };

    let current = store.find(id)?;
    if current.kind != input.kind {
        return Err(ServiceError::new(
            Category::InvalidRequest,
            format!("연결 종류는 바꿀 수 없습니다({} → {}). 새 연결을 추가해 주세요.", current.kind.label(), input.kind.label()),
        ));
    }
    let patch = ProfilePatch {
        name: Some(input.name.clone()),
        base_url: Some(address(current.kind, input.base_url.as_deref())?),
        concurrency: Some(input.concurrency),
    };
    // Key first, then the profile; a failed profile change puts the old key back.
    let old_key = match new_key {
        Some(_) => Some(ctx.secrets.get(&current.id)?),
        None => None,
    };
    match new_key {
        Some("") => ctx.secrets.delete(&current.id)?,
        Some(key) => ctx.secrets.set(&current.id, key)?,
        None => {}
    }
    if let Err(err) = store.update(&current.id, patch) {
        if let Some(old) = old_key {
            let _ = match old {
                Some(old) => ctx.secrets.set(&current.id, old.expose()),
                None => ctx.secrets.delete(&current.id),
            };
        }
        return Err(err.into());
    }
    if input.make_default {
        store.set_default(Some(&current.id))?;
    }
    view_of(ctx, &store, &current.id)
}

/// Removes the profile, then its stored key.
pub fn remove_profile(ctx: &Ctx, id: &str) -> Result<(), ServiceError> {
    let removed = store(ctx).remove(id)?;
    ctx.secrets.delete(&removed.id).map_err(|e| {
        ServiceError::new(e.category, format!("프로필 {}을(를) 지웠지만 저장된 키를 지우지 못했습니다: {}", removed.name, e.message))
    })
}

pub fn set_default_profile(ctx: &Ctx, id: Option<&str>) -> Result<(), ServiceError> {
    Ok(store(ctx).set_default(id)?)
}

/// `name: None` follows the resolution order (environment, then default profile).
pub(super) fn connection(ctx: &Ctx, store: &ProfileStore, name: Option<&str>) -> Result<Connection, ServiceError> {
    let resolver = Resolver { env: ctx.env.as_ref(), profiles: Some(store), secrets: ctx.secrets.as_ref() };
    Ok(resolve(&resolver, name.map(str::trim).filter(|n| !n.is_empty()))?)
}

/// Server → key → model list stages; never generates (spec §6.2).
pub async fn check_profile(ctx: &Ctx, name: Option<&str>) -> Result<ConnectionReport, ServiceError> {
    let store = store(ctx);
    let conn = connection(ctx, &store, name)?;
    let mut report = check_connection(&Http::new(Some(CHECK_TIMEOUT)), &conn, Some(&store)).await;
    // The engine already redacts; this keeps the guarantee local to what the screen receives.
    if let Some(err) = report.error.as_mut() {
        err.message = redact(&err.message, conn.api_key.as_ref());
        err.provider_code = err.provider_code.as_deref().map(|code| redact(code, conn.api_key.as_ref()));
    }
    Ok(report)
}

fn provider_label(id: &str) -> &'static str {
    if backend(id) == Backend::Gemini {
        "Google Gemini"
    } else if id.to_lowercase().starts_with("gpt-image-") {
        "OpenAI"
    } else {
        "xAI Grok"
    }
}

/// Image models the connection lists and can call, in list order, with what each can do there.
pub async fn models(ctx: &Ctx, name: Option<&str>) -> Result<Vec<ModelView>, ServiceError> {
    let store = store(ctx);
    let conn = connection(ctx, &store, name)?;
    let ids = list_models(&Http::new(Some(CHECK_TIMEOUT)), &conn).await.map_err(|e| ServiceError::from_engine(e, conn.api_key.as_ref()))?;
    let mut out: Vec<ModelView> = Vec::new();
    for id in ids {
        if !is_listable_image_model(&id) || model_supported(conn.kind, &id).is_err() || out.iter().any(|m| m.id == id) {
            continue;
        }
        let references = supports_references(conn.kind, &id);
        let max_references = if references { reference_limit(conn.kind, &id).min(MAX_REFERENCES) as u32 } else { 0 };
        out.push(ModelView {
            provider: provider_label(&id).to_string(),
            references,
            max_references,
            transparency: supports_native_transparency(conn.kind, &id),
            id,
        });
    }
    Ok(out)
}
