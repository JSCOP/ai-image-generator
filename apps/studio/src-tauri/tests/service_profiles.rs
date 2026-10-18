//! Settings and connection-profile services against temp homes, `MemoryStore`, `MapEnv` and `FakeProvider`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image_core::error::Category;
use image_core::profiles::{GenerationStatus, ProfileStore, ProviderKind};
use image_core::secrets::{MemoryStore, SecretStore};
use studio_lib::service::profiles::{check_profile, connections, models, remove_profile, save_profile, set_default_profile, ProfileInput};
use studio_lib::service::settings::{default_work_dir, load_settings, save_settings, Settings};
use studio_lib::service::Ctx;
use test_fixture::{FakeProvider, MapEnv, Reply};

const KEY: &str = "sk-test-secret-key-0123456789";

fn ctx_with(home: &Path, env: MapEnv) -> (Ctx, Arc<MemoryStore>) {
    let secrets = Arc::new(MemoryStore::new());
    let ctx = Ctx { home: home.to_path_buf(), env: Arc::new(env), secrets: secrets.clone() };
    (ctx, secrets)
}

fn input(name: &str, kind: ProviderKind, base_url: Option<&str>, key: Option<&str>) -> ProfileInput {
    ProfileInput {
        id: None,
        name: name.to_string(),
        kind,
        base_url: base_url.map(str::to_string),
        concurrency: None,
        key: key.map(str::to_string),
        make_default: false,
    }
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn settings_default_work_dir_is_pictures_image_studio() {
    let home = tempfile::tempdir().unwrap();
    let settings = load_settings(home.path());
    let pictures = dirs::picture_dir().expect("pictures folder");
    assert_eq!(settings.work_dir, pictures.join("Image Studio"));
    assert_eq!(default_work_dir(), pictures.join("Image Studio"));
    assert!(settings.gallery_folders.is_empty());
    assert_eq!(settings.last_profile, None);
    assert_eq!(settings.last_model, None);
    // Loading defaults writes nothing.
    assert!(!home.path().join("settings.json").exists());

    let saved = Settings {
        work_dir: home.path().join("work"),
        gallery_folders: vec![home.path().join("project")],
        last_profile: Some("local".into()),
        last_model: Some("gpt-image-2.5".into()),
    };
    save_settings(home.path(), &saved).unwrap();
    let text = read(home.path().join("settings.json"));
    assert!(text.contains("\"workDir\"") && text.contains("\"galleryFolders\"") && text.contains("\"lastProfile\""), "{text}");
    assert_eq!(load_settings(home.path()), saved);
}

#[test]
fn corrupt_settings_are_moved_aside_and_defaults_used() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("settings.json"), "{ not json").unwrap();
    let settings = load_settings(home.path());
    assert_eq!(settings, Settings::default());
    assert!(!home.path().join("settings.json").exists());
    let moved: Vec<String> = std::fs::read_dir(home.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("settings.json.bad-"))
        .collect();
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert_eq!(read(home.path().join(&moved[0])), "{ not json");

    // Wrong types are corrupt too; missing fields are not.
    std::fs::write(home.path().join("settings.json"), r#"{"workDir": 5}"#).unwrap();
    assert_eq!(load_settings(home.path()), Settings::default());
    std::fs::write(home.path().join("settings.json"), r#"{"lastModel": "gpt-image-2"}"#).unwrap();
    let partial = load_settings(home.path());
    assert_eq!(partial.last_model.as_deref(), Some("gpt-image-2"));
    assert_eq!(partial.work_dir, default_work_dir());
    assert!(home.path().join("settings.json").exists());
}

#[test]
fn save_settings_refuses_a_relative_work_dir() {
    let home = tempfile::tempdir().unwrap();
    let bad = Settings { work_dir: PathBuf::from("relative\\dir"), ..Settings::default() };
    let err = save_settings(home.path(), &bad).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(!home.path().join("settings.json").exists());
}

#[test]
fn save_profile_stores_key_only_in_secret_store() {
    let home = tempfile::tempdir().unwrap();
    let (ctx, secrets) = ctx_with(home.path(), MapEnv::new(&[], &[]));
    let mut new = input("local", ProviderKind::Cliproxy, Some("http://127.0.0.1:8317"), Some(KEY));
    new.make_default = true;
    let view = save_profile(&ctx, new).unwrap();

    assert!(view.key_configured);
    assert!(view.is_default);
    assert_eq!(view.base_url, "http://127.0.0.1:8317/v1");
    assert_eq!(view.kind_label, "CLIProxyAPI");
    assert_eq!(view.warning, None);
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains(KEY), "{json}");
    assert!(json.contains("\"keyConfigured\":true") && json.contains("\"kindLabel\""), "{json}");
    assert!(!read(home.path().join("profiles.json")).contains(KEY));
    assert_eq!(secrets.get(&view.id).unwrap().unwrap().expose(), KEY);
    // No other file in the home holds the key either.
    for entry in std::fs::read_dir(home.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            assert!(!std::fs::read(&path).unwrap().windows(KEY.len()).any(|w| w == KEY.as_bytes()), "{}", path.display());
        }
    }

    // Updating without a key keeps the stored one; the name and address change.
    let mut edit = input("집 프록시", ProviderKind::Cliproxy, Some("http://100.98.54.122:8317/v1"), None);
    edit.id = Some(view.id.clone());
    edit.concurrency = Some(8);
    let edited = save_profile(&ctx, edit).unwrap();
    assert_eq!(edited.id, view.id);
    assert_eq!(edited.name, "집 프록시");
    assert_eq!(edited.concurrency, Some(8));
    assert!(edited.key_configured && edited.is_default);
    assert_eq!(secrets.get(&view.id).unwrap().unwrap().expose(), KEY);

    // A public http address carries the transport warning.
    let public = save_profile(&ctx, input("public", ProviderKind::OpenaiCompatible, Some("http://example.com"), None)).unwrap();
    assert!(public.warning.is_some());
    assert!(!public.key_configured && !public.is_default);

    // The kind of a saved profile cannot change.
    let mut change_kind = input("집 프록시", ProviderKind::Openai, None, None);
    change_kind.id = Some(view.id.clone());
    assert_eq!(save_profile(&ctx, change_kind).unwrap_err().category, Category::InvalidRequest);

    // A failed add (duplicate name) leaves no key behind.
    let dup = save_profile(&ctx, input("PUBLIC", ProviderKind::Cliproxy, Some("http://127.0.0.1:1"), Some("other-key"))).unwrap_err();
    assert_eq!(dup.category, Category::Conflict);
    let file = ProfileStore::open(home.path().to_path_buf()).load().unwrap();
    assert_eq!(file.profiles.len(), 2);
    for profile in &file.profiles {
        assert_ne!(secrets.get(&profile.id).unwrap().map(|k| k.expose().to_string()).as_deref(), Some("other-key"));
    }
}

#[test]
fn save_profile_with_empty_key_deletes_key() {
    let home = tempfile::tempdir().unwrap();
    let (ctx, secrets) = ctx_with(home.path(), MapEnv::new(&[], &[]));
    let view = save_profile(&ctx, input("openai", ProviderKind::Openai, None, Some(KEY))).unwrap();
    assert_eq!(view.base_url, "https://api.openai.com/v1");
    assert!(view.key_configured);

    let mut clear = input("openai", ProviderKind::Openai, None, Some("  "));
    clear.id = Some(view.id.clone());
    let cleared = save_profile(&ctx, clear).unwrap();
    assert!(!cleared.key_configured);
    assert!(secrets.get(&view.id).unwrap().is_none());

    // A new key replaces nothing else; an empty key on a new profile just means "no key".
    let mut replace = input("openai", ProviderKind::Openai, None, Some(" new-key \n"));
    replace.id = Some(view.id.clone());
    assert!(save_profile(&ctx, replace).unwrap().key_configured);
    assert_eq!(secrets.get(&view.id).unwrap().unwrap().expose(), "new-key");
    let other = save_profile(&ctx, input("xai", ProviderKind::Xai, None, Some(""))).unwrap();
    assert!(!other.key_configured);

    // Removing a profile removes its key.
    remove_profile(&ctx, &view.id).unwrap();
    assert!(secrets.get(&view.id).unwrap().is_none());
    assert_eq!(remove_profile(&ctx, &view.id).unwrap_err().category, Category::NotFound);
}

#[test]
fn connections_view_shows_env_and_active() {
    let home = tempfile::tempdir().unwrap();
    let env = MapEnv::new(&[("CLIPROXY_BASE_URL", "http://127.0.0.1:8317"), ("CLIPROXY_API_KEY", KEY)], &[]);
    let (ctx, _) = ctx_with(home.path(), env);
    let mut local = input("local", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None);
    local.make_default = true;
    let saved = save_profile(&ctx, local).unwrap();

    let view = connections(&ctx).unwrap();
    let env = view.env.as_ref().expect("env connection");
    assert_eq!(env.base_url, "http://127.0.0.1:8317/v1");
    assert_eq!(env.source, "process_env");
    assert!(env.key_configured);
    assert_eq!(view.active.as_deref(), Some("env"));
    assert_eq!(view.profiles.len(), 1);
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains(KEY), "{json}");

    // No env: the default profile is the active one.
    let (ctx, _) = ctx_with(home.path(), MapEnv::new(&[], &[]));
    let view = connections(&ctx).unwrap();
    assert!(view.env.is_none());
    assert_eq!(view.active.as_deref(), Some("local"));
    assert!(view.profiles[0].is_default);

    // Status from the last real generation shows on the profile.
    let status = GenerationStatus { ok: true, at: 1.0, model: "gpt-image-2.5".into(), category: None, provider_code: None };
    ProfileStore::open(home.path().to_path_buf()).record_generation(&saved.id, &status).unwrap();
    assert_eq!(connections(&ctx).unwrap().profiles[0].last_generation, Some(status));

    // No default and no env: nothing is active. User-level env counts as env.
    set_default_profile(&ctx, None).unwrap();
    assert_eq!(connections(&ctx).unwrap().active, None);
    let (ctx, _) = ctx_with(home.path(), MapEnv::new(&[], &[("CLIPROXY_BASE_URL", "http://127.0.0.1:8317/v1")]));
    let view = connections(&ctx).unwrap();
    assert_eq!(view.env.as_ref().map(|e| e.source.as_str()), Some("user_env"));
    assert!(!view.env.unwrap().key_configured);
    assert_eq!(view.active.as_deref(), Some("env"));
    set_default_profile(&ctx, Some("local")).unwrap();
    assert!(connections(&ctx).unwrap().profiles[0].is_default);
}

fn models_reply() -> Reply {
    let ids = [
        "gpt-image-2.5",
        "gpt-image-2.5-sunburst",
        "gpt-image-2",
        "gemini-3.1-flash-image",
        "grok-imagine-image-2.0",
        "gpt-4o",
        "gpt-image-2",
    ];
    let data: Vec<_> = ids.iter().map(|id| serde_json::json!({"id": id})).collect();
    Reply::json(200, &serde_json::json!({"data": data}).to_string())
}

#[tokio::test]
async fn models_view_marks_capabilities() {
    let fake = FakeProvider::start(|req| if req.path.ends_with("/models") { models_reply() } else { Reply::json(404, "{}") });
    let home = tempfile::tempdir().unwrap();
    let (ctx, _) = ctx_with(home.path(), MapEnv::new(&[], &[]));
    let mut local = input("local", ProviderKind::Cliproxy, Some(&fake.base_url()), Some(KEY));
    local.make_default = true;
    save_profile(&ctx, local).unwrap();

    let list = models(&ctx, Some("local")).await.unwrap();
    let ids: Vec<&str> = list.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["gpt-image-2.5", "gpt-image-2.5-sunburst", "gpt-image-2", "gemini-3.1-flash-image", "grok-imagine-image-2.0"]);
    let transparent: Vec<&str> = list.iter().filter(|m| m.transparency).map(|m| m.id.as_str()).collect();
    assert_eq!(transparent, ["gpt-image-2.5-sunburst"]);
    let grok = list.iter().find(|m| m.id == "grok-imagine-image-2.0").unwrap();
    assert!(!grok.references);
    assert_eq!(grok.provider, "xAI Grok");
    assert!(list.iter().find(|m| m.id == "gemini-3.1-flash-image").unwrap().references);
    let json = serde_json::to_value(&list[0]).unwrap();
    assert_eq!(json, serde_json::json!({"id": "gpt-image-2.5", "provider": "OpenAI", "references": true, "transparency": false}));

    // The default profile is used when no name is given; the key went to the provider as a bearer token.
    assert_eq!(models(&ctx, None).await.unwrap().len(), 5);
    assert_eq!(fake.requests()[0].header("authorization"), Some(format!("Bearer {KEY}").as_str()));
    // A missing profile is an error, not a fallback.
    assert_eq!(models(&ctx, Some("nope")).await.unwrap_err().category, Category::InvalidRequest);
}

#[tokio::test]
async fn check_profile_returns_stage_report() {
    let fake = FakeProvider::start(|req| if req.path.ends_with("/models") { models_reply() } else { Reply::json(404, "{}") });
    let refusing = FakeProvider::start(|_| Reply::json(401, r#"{"error":{"message":"bad key","code":"invalid_api_key"}}"#));
    let home = tempfile::tempdir().unwrap();
    let (ctx, _) = ctx_with(home.path(), MapEnv::new(&[], &[]));
    save_profile(&ctx, input("good", ProviderKind::Cliproxy, Some(&fake.base_url()), Some(KEY))).unwrap();
    save_profile(&ctx, input("bad", ProviderKind::Cliproxy, Some(&refusing.base_url()), Some(KEY))).unwrap();

    let report = check_profile(&ctx, Some("good")).await.unwrap();
    assert_eq!(report.profile.as_deref(), Some("good"));
    assert!(report.key_configured && report.reachable && report.authenticated && report.models_listed);
    assert_eq!(report.image_models.len(), 5);
    assert!(report.error.is_none());

    let report = check_profile(&ctx, Some("bad")).await.unwrap();
    assert!(report.reachable && !report.authenticated && !report.models_listed);
    assert_eq!(report.error.as_ref().map(|e| e.category), Some(Category::Auth));
    assert!(!serde_json::to_string(&report).unwrap().contains(KEY));

    // No env and no default profile: the error says how to configure one.
    let err = check_profile(&ctx, None).await.unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    // A key-requiring kind without a key is refused before any request.
    save_profile(&ctx, input("openai", ProviderKind::Openai, None, None)).unwrap();
    assert_eq!(check_profile(&ctx, Some("openai")).await.unwrap_err().category, Category::InvalidRequest);
}

#[tokio::test]
async fn service_error_never_contains_key() {
    let fake = FakeProvider::start(|req| {
        let echoed = req.header("authorization").unwrap_or("").to_string();
        let body = serde_json::json!({"error": {"message": format!("forbidden for {echoed} and {KEY}"), "code": KEY}});
        Reply::json(403, &body.to_string())
    });
    let home = tempfile::tempdir().unwrap();
    let (ctx, _) = ctx_with(home.path(), MapEnv::new(&[], &[]));
    save_profile(&ctx, input("leaky", ProviderKind::Cliproxy, Some(&fake.base_url()), Some(KEY))).unwrap();

    let err = models(&ctx, Some("leaky")).await.unwrap_err();
    assert_eq!(err.category, Category::Permission);
    assert!(!err.message.contains(KEY), "{}", err.message);
    let json = serde_json::to_string(&err).unwrap();
    assert!(!json.contains(KEY), "{json}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&json).unwrap()["category"], "permission");

    let report = check_profile(&ctx, Some("leaky")).await.unwrap();
    assert!(!serde_json::to_string(&report).unwrap().contains(KEY));
}
