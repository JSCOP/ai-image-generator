use image_core::connection::EnvSource;
use image_core::connection::{resolve, Resolver};
use image_core::error::Category;
use image_core::http::Http;
use image_core::profiles::{ProfileStore, ProviderKind};
use image_core::secrets::{MemoryStore, SecretStore};
use serde_json::json;
use std::time::Duration;
use test_fixture::{FakeProvider, MapEnv, Reply};

fn temp_store() -> (tempfile::TempDir, ProfileStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ProfileStore::open(dir.path().to_path_buf());
    (dir, store)
}

fn env_proxy() -> MapEnv {
    MapEnv::new(&[("CLIPROXY_BASE_URL", "http://127.0.0.1:8317"), ("CLIPROXY_API_KEY", "env-key")], &[])
}

fn resolver<'a>(env: &'a dyn EnvSource, store: &'a ProfileStore, secrets: &'a dyn SecretStore) -> Resolver<'a> {
    Resolver { env, profiles: Some(store), secrets }
}

#[test]
fn explicit_profile_wins_over_env() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let p = store.add("gem", ProviderKind::Gemini, None, None).unwrap();
    secrets.set(&p.id, "gem-key").unwrap();
    let env = env_proxy();
    let c = resolve(&resolver(&env, &store, &secrets), Some("gem")).unwrap();
    assert_eq!(c.kind, ProviderKind::Gemini);
    assert_eq!(c.source, "profile:gem");
    assert_eq!(c.base_url, "https://generativelanguage.googleapis.com");
    assert_eq!(c.profile_id.as_deref(), Some(p.id.as_str()));
    assert_eq!(c.api_key.as_ref().unwrap().expose(), "gem-key");
    // Lookup by id works too.
    assert_eq!(resolve(&resolver(&env, &store, &secrets), Some(&p.id)).unwrap().source, "profile:gem");
}

#[test]
fn env_beats_default_profile() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let p = store.add("official", ProviderKind::Openai, None, None).unwrap();
    secrets.set(&p.id, "sk-profile").unwrap();
    store.set_default(Some("official")).unwrap();
    let env = env_proxy();
    let c = resolve(&resolver(&env, &store, &secrets), None).unwrap();
    assert_eq!(c.source, "process_env");
    assert_eq!(c.kind, ProviderKind::Cliproxy);
    assert_eq!(c.base_url, "http://127.0.0.1:8317/v1");
    assert_eq!(c.api_key.as_ref().unwrap().expose(), "env-key");
    assert_eq!(c.profile_id, None);
    let user_env = MapEnv::new(&[], &[("CLIPROXY_BASE_URL", "http://b:2")]);
    assert_eq!(resolve(&resolver(&user_env, &store, &secrets), None).unwrap().source, "user_env");
}

#[test]
fn env_still_works_when_profile_file_is_corrupt() {
    let (dir, store) = temp_store();
    std::fs::write(dir.path().join("profiles.json"), "{not json").unwrap();
    let secrets = MemoryStore::new();
    let env = env_proxy();
    assert_eq!(resolve(&resolver(&env, &store, &secrets), None).unwrap().source, "process_env");
    let empty = MapEnv::new(&[], &[]);
    let err = resolve(&resolver(&empty, &store, &secrets), None).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("프로필 파일을 읽을 수 없습니다"), "{}", err.message);
}

#[test]
fn default_profile_used_when_env_missing() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let p = store.add("집 PC", ProviderKind::Cliproxy, Some("http://100.64.0.10:8317"), None).unwrap();
    store.set_default(Some(&p.id)).unwrap();
    let env = MapEnv::new(&[], &[]);
    let c = resolve(&resolver(&env, &store, &secrets), None).unwrap();
    assert_eq!(c.source, "profile:집 PC");
    assert_eq!(c.kind, ProviderKind::Cliproxy);
    assert_eq!(c.base_url, "http://100.64.0.10:8317/v1");
    assert_eq!(c.profile_id.as_deref(), Some(p.id.as_str()));
    assert!(c.api_key.is_none());
}

#[test]
fn missing_profile_is_an_error_without_fallback() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let p = store.add("other", ProviderKind::Cliproxy, Some("http://127.0.0.1:1"), None).unwrap();
    store.set_default(Some(&p.id)).unwrap();
    let env = env_proxy();
    let err = resolve(&resolver(&env, &store, &secrets), Some("nope")).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("프로필을 찾을 수 없습니다: nope"), "{}", err.message);
    // No profile store at all: still an error, never the environment.
    let none = Resolver { env: &env, profiles: None, secrets: &secrets };
    let err = resolve(&none, Some("nope")).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("nope"), "{}", err.message);
}

#[test]
fn profile_without_required_key_is_an_error() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let p = store.add("work openai", ProviderKind::Openai, None, None).unwrap();
    let env = env_proxy();
    let err = resolve(&resolver(&env, &store, &secrets), Some("work openai")).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("키가 없습니다"), "{}", err.message);
    assert!(err.message.contains("work openai"), "{}", err.message);
    // Same check for a default profile whose key vanished (Review Focus 3).
    store.set_default(Some(&p.id)).unwrap();
    let empty = MapEnv::new(&[], &[]);
    let err = resolve(&resolver(&empty, &store, &secrets), None).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("키가 없습니다") && err.message.contains("work openai"), "{}", err.message);
}

#[test]
fn cliproxy_profile_may_have_no_key() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    store.add("local", ProviderKind::Cliproxy, Some("http://127.0.0.1:8317"), None).unwrap();
    store.add("compat", ProviderKind::OpenaiCompatible, Some("http://127.0.0.1:9000"), None).unwrap();
    let env = MapEnv::new(&[], &[]);
    let c = resolve(&resolver(&env, &store, &secrets), Some("local")).unwrap();
    assert!(c.api_key.is_none());
    let c = resolve(&resolver(&env, &store, &secrets), Some("compat")).unwrap();
    assert_eq!(c.kind, ProviderKind::OpenaiCompatible);
    assert!(c.api_key.is_none());
}

#[test]
fn nothing_configured_names_both_options() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let env = MapEnv::new(&[], &[]);
    for r in [resolver(&env, &store, &secrets), Resolver { env: &env, profiles: None, secrets: &secrets }] {
        let err = resolve(&r, None).unwrap_err();
        assert_eq!(err.category, Category::InvalidRequest);
        assert!(err.message.contains("CLIPROXY_BASE_URL"), "{}", err.message);
        assert!(err.message.contains("image-cli profile add"), "{}", err.message);
    }
}

#[tokio::test]
async fn gemini_auth_uses_goog_header_not_bearer() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let fake = FakeProvider::start(|_| Reply::json(200, "{}"));
    let p = store.add("gem", ProviderKind::Gemini, Some(&fake.base_url()), None).unwrap();
    secrets.set(&p.id, "k").unwrap();
    let env = MapEnv::new(&[], &[]);
    let c = resolve(&resolver(&env, &store, &secrets), Some("gem")).unwrap();
    let http = Http::new(Some(Duration::from_secs(5)));
    http.post_json(&format!("{}/v1beta/models/x:generateContent", c.base_url), &json!({}), c.auth()).await.unwrap();
    http.get(&format!("{}/v1beta/models", c.base_url), c.auth()).await.unwrap();
    for r in fake.requests() {
        assert_eq!(r.header("x-goog-api-key"), Some("k"));
        assert_eq!(r.header("Authorization"), None);
    }
    assert_eq!(fake.requests().len(), 2);
    // OpenAI-style kinds keep Bearer and send no goog header.
    let o = store.add("compat", ProviderKind::OpenaiCompatible, Some(&fake.base_url()), None).unwrap();
    secrets.set(&o.id, "k2").unwrap();
    let c = resolve(&resolver(&env, &store, &secrets), Some("compat")).unwrap();
    http.get(&format!("{}/models", c.base_url), c.auth()).await.unwrap();
    let last = fake.requests().pop().unwrap();
    assert_eq!(last.header("Authorization"), Some("Bearer k2"));
    assert_eq!(last.header("x-goog-api-key"), None);
}

#[test]
fn concurrency_default_follows_kind_and_profile() {
    let (_dir, store) = temp_store();
    let secrets = MemoryStore::new();
    let o = store.add("o", ProviderKind::Openai, None, None).unwrap();
    secrets.set(&o.id, "sk").unwrap();
    store.add("c", ProviderKind::Cliproxy, Some("http://127.0.0.1:1"), None).unwrap();
    store.add("seven", ProviderKind::Cliproxy, Some("http://127.0.0.1:2"), Some(7)).unwrap();
    let env = MapEnv::new(&[], &[]);
    let r = resolver(&env, &store, &secrets);
    assert_eq!(resolve(&r, Some("o")).unwrap().concurrency_default, 4);
    assert_eq!(resolve(&r, Some("c")).unwrap().concurrency_default, 30);
    assert_eq!(resolve(&r, Some("seven")).unwrap().concurrency_default, 7);
    let proxy = env_proxy();
    assert_eq!(resolve(&resolver(&proxy, &store, &secrets), None).unwrap().concurrency_default, 30);
}
