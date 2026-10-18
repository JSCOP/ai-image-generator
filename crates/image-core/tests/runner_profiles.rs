use image_core::profiles::{ProfileStore, ProviderKind};
use image_core::runner::{run_spec_with, Progress, RunOptions, RunOutcome};
use image_core::secrets::{MemoryStore, SecretStore};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use test_fixture::{openai_image_reply, png_bytes, FakeProvider, MapEnv, Reply};
use tokio_util::sync::CancellationToken;

struct Setup {
    work: tempfile::TempDir,
    home: tempfile::TempDir,
    store: Arc<ProfileStore>,
    secrets: Arc<MemoryStore>,
}

impl Setup {
    fn new() -> Setup {
        let home = tempfile::tempdir().unwrap();
        let store = Arc::new(ProfileStore::open(home.path().to_path_buf()));
        Setup { work: tempfile::tempdir().unwrap(), home, store, secrets: Arc::new(MemoryStore::new()) }
    }

    fn add(&self, name: &str, kind: ProviderKind, url: Option<&str>, key: Option<&str>) -> String {
        let p = self.store.add(name, kind, url, None).unwrap();
        if let Some(key) = key {
            self.secrets.set(&p.id, key).unwrap();
        }
        p.id
    }

    async fn go(&self, mut spec: Value, env: MapEnv) -> RunOutcome {
        spec["topic_root"] = json!(self.work.path().display().to_string());
        let progress: Progress = Arc::new(|_| {});
        let mut opts = RunOptions::new(0.0, self.work.path().to_path_buf());
        opts.profiles = Some(self.store.clone());
        opts.secrets = self.secrets.clone();
        run_spec_with(spec, Arc::new(env), progress, CancellationToken::new(), opts).await
    }
}

fn spec(extra: Value) -> Value {
    let mut base = json!({"prompt": "a mug", "topic": "t", "size": "16x16", "quality": "low"});
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn ok_provider(delay_ms: u64) -> FakeProvider {
    let png = png_bytes(16, 16, [9, 9, 9, 255]);
    FakeProvider::start(move |_| openai_image_reply(&png).delayed(Duration::from_millis(delay_ms)))
}

#[tokio::test]
async fn spec_profile_selects_connection_and_records_success() {
    let p = ok_provider(0);
    let s = Setup::new();
    let id = s.add("집 PC", ProviderKind::Cliproxy, Some(&p.base_url()), Some("profile-key"));
    // The environment points elsewhere; the named profile must win.
    let env = MapEnv::proxy(&test_fixture::closed_base_url());
    let out = s.go(spec(json!({"profile": "집 PC"})), env).await;
    assert_eq!(out.exit_code, 0, "{}", out.result);
    assert_eq!(out.result["connection_source"], "profile:집 PC");
    assert_eq!(p.requests().len(), 1);
    assert_eq!(p.requests()[0].header("Authorization"), Some("Bearer profile-key"));
    let status = s.store.generation_status().unwrap();
    let last = status.get(&id).expect("status recorded");
    assert!(last.ok);
    assert_eq!(last.model, "gpt-image-2.5");
    assert!(last.at > 0.0);
    assert_eq!(last.category, None);
    // Environment connections are not profiles and record nothing.
    let other = ok_provider(0);
    let out = s.go(spec(json!({"topic": "env"})), MapEnv::proxy(&other.base_url())).await;
    assert_eq!(out.result["connection_source"], "process_env");
    assert_eq!(s.store.generation_status().unwrap().len(), 1);
    assert!(s.home.path().join("status.json").is_file());
}

#[tokio::test]
async fn quota_failure_is_recorded_as_last_generation() {
    let p = FakeProvider::start(|_| Reply::json(429, r#"{"error":{"code":"insufficient_quota","message":"You exceeded your quota"}}"#));
    let s = Setup::new();
    let id = s.add("grok", ProviderKind::Cliproxy, Some(&p.base_url()), None);
    s.store.set_default(Some("grok")).unwrap();
    // No environment URL: the default profile is used.
    let out = s.go(spec(json!({"image_model": "grok-imagine-image-2.0", "count": 2})), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 2, "{}", out.result);
    assert_eq!(out.result["connection_source"], "profile:grok");
    let last = s.store.generation_status().unwrap().remove(&id).expect("status recorded");
    assert!(!last.ok);
    assert_eq!(last.model, "grok-imagine-image-2.0");
    assert_eq!(serde_json::to_value(last.category).unwrap(), json!("quota"));
    assert_eq!(last.provider_code.as_deref(), Some("insufficient_quota"));
}

#[tokio::test]
async fn kind_model_mismatch_fails_before_call() {
    let p = ok_provider(0);
    let s = Setup::new();
    let id = s.add("gem", ProviderKind::Gemini, Some(&p.base_url()), Some("gem-key"));
    let out = s.go(spec(json!({"profile": "gem", "image_model": "gpt-image-2.5"})), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.result["error_category"], "unsupported", "{}", out.result);
    assert!(out.result["error"].as_str().unwrap().contains("Gemini 공식"), "{}", out.result);
    // Jobs mode reports it per job, still before any call; nothing is recorded.
    let jobs = json!({"mode": "jobs", "profile": "gem", "topic": "j", "size": "16x16",
        "jobs": [{"id": "a", "prompt": "x", "image_model": "gpt-image-2.5"}]});
    let out = s.go(jobs, MapEnv::new(&[], &[])).await;
    assert_eq!(out.result["failures"][0]["error"]["category"], "unsupported", "{}", out.result);
    // A dry run checks the same thing.
    let out = s.go(spec(json!({"profile": "gem", "image_model": "gpt-image-2.5", "dry_run": true})), MapEnv::new(&[], &[])).await;
    assert_eq!(out.result["error_category"], "unsupported", "{}", out.result);
    assert!(p.requests().is_empty());
    assert!(!s.store.generation_status().unwrap().contains_key(&id));
}

#[tokio::test]
async fn missing_spec_profile_is_an_error_without_fallback() {
    let p = ok_provider(0);
    let s = Setup::new();
    for extra in [json!({"profile": "nope"}), json!({"profile": "nope", "dry_run": true})] {
        let out = s.go(spec(extra), MapEnv::proxy(&p.base_url())).await;
        assert_eq!(out.exit_code, 2);
        assert_eq!(out.result["error_category"], "invalid_request", "{}", out.result);
        assert!(out.result["error"].as_str().unwrap().contains("nope"), "{}", out.result);
    }
    // A per-job profile is refused: one run uses one connection.
    let jobs = json!({"mode": "jobs", "topic": "j", "size": "16x16", "jobs": [{"id": "a", "prompt": "x", "profile": "other"}]});
    let out = s.go(jobs, MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["error_category"], "unsupported", "{}", out.result);
    assert!(p.requests().is_empty());
}

#[tokio::test]
async fn official_profile_defaults_to_four_workers() {
    let p = ok_provider(300);
    let s = Setup::new();
    s.add("openai", ProviderKind::Openai, Some(&p.base_url()), Some("sk-test"));
    let out = s.go(spec(json!({"profile": "openai", "count": 6, "image_model": "gpt-image-2.5-flare"})), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 0, "{}", out.result);
    assert_eq!(out.result["concurrency"], 4);
    assert_eq!(p.peak_concurrency(), 4);
    assert_eq!(p.requests().len(), 6);
    // The spec's own concurrency still wins.
    let out = s
        .go(
            spec(json!({"profile": "openai", "count": 3, "concurrency": 2, "topic": "c2", "image_model": "gpt-image-2"})),
            MapEnv::new(&[], &[]),
        )
        .await;
    assert_eq!(out.result["concurrency"], 2, "{}", out.result);
}

#[tokio::test]
async fn openai_profile_refuses_the_bare_default_model_offline() {
    let p = ok_provider(0);
    let s = Setup::new();
    let id = s.add("openai", ProviderKind::Openai, Some(&p.base_url()), Some("sk-test"));
    // No image_model: the runner default gpt-image-2.5 does not exist on the official API.
    for extra in [json!({"profile": "openai"}), json!({"profile": "openai", "image_model": "gpt-image-2.5", "dry_run": true})] {
        let out = s.go(spec(extra), MapEnv::new(&[], &[])).await;
        assert_eq!(out.exit_code, 2, "{}", out.result);
        assert_eq!(out.result["error_category"], "unsupported", "{}", out.result);
        let message = out.result["error"].as_str().unwrap();
        assert!(message.contains("gpt-image-2.5-sunburst") && message.contains("gpt-image-2.5-flare"), "{message}");
    }
    assert!(p.requests().is_empty());
    assert!(!s.store.generation_status().unwrap().contains_key(&id));
}

#[tokio::test]
async fn record_carries_the_profile_id_only_for_profiles() {
    let p = ok_provider(0);
    let s = Setup::new();
    let id = s.add("local", ProviderKind::Cliproxy, Some(&p.base_url()), None);
    let read_record = |out: &RunOutcome| -> Value {
        let dir = std::path::PathBuf::from(out.result["metadata_dir"].as_str().unwrap());
        let file =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).find(|p| p.extension().is_some_and(|e| e == "json")).unwrap();
        serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
    };
    let out = s.go(spec(json!({"profile": "local"})), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 0, "{}", out.result);
    let record = read_record(&out);
    assert_eq!(record["profile_id"], json!(id));
    assert_eq!(record["connection_source"], "profile:local");
    let out = s.go(spec(json!({"topic": "env"})), MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.exit_code, 0, "{}", out.result);
    let record = read_record(&out);
    assert!(record.get("profile_id").is_none(), "{record}");
}

#[tokio::test]
async fn last_generation_ignores_transport_and_server_failures() {
    let ok = ok_provider(0);
    let s = Setup::new();
    let id = s.add("p", ProviderKind::Cliproxy, Some(&ok.base_url()), None);
    let run = |topic: &str| spec(json!({"profile": "p", "topic": topic, "image_model": "gpt-image-2"}));
    let out = s.go(run("ok"), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 0, "{}", out.result);
    let first = s.store.generation_status().unwrap().remove(&id).unwrap();
    assert!(first.ok);

    // 5xx, 404, 401 and an unreachable server say nothing about "can really generate".
    for reply in [(503, r#"{"error":{"message":"overloaded"}}"#), (404, "not found"), (401, r#"{"error":{"message":"bad"}}"#)] {
        let bad = FakeProvider::start(move |_| Reply::json(reply.0, reply.1));
        s.store.update("p", image_core::profiles::ProfilePatch { base_url: Some(bad.base_url()), ..Default::default() }).unwrap();
        let out = s.go(run(&format!("h{}", reply.0)), MapEnv::new(&[], &[])).await;
        assert_eq!(out.exit_code, 2, "{}", out.result);
        assert_eq!(s.store.generation_status().unwrap().remove(&id).unwrap(), first, "HTTP {}", reply.0);
    }
    let closed = test_fixture::closed_base_url();
    s.store.update("p", image_core::profiles::ProfilePatch { base_url: Some(closed), ..Default::default() }).unwrap();
    let out = s.go(run("net"), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 2, "{}", out.result);
    assert_eq!(s.store.generation_status().unwrap().remove(&id).unwrap(), first, "network failure");

    // A permission / spending answer does update it.
    let blocked = FakeProvider::start(|_| Reply::json(403, r#"{"error":{"code":"personal-team-blocked","message":"policy"}}"#));
    s.store.update("p", image_core::profiles::ProfilePatch { base_url: Some(blocked.base_url()), ..Default::default() }).unwrap();
    let out = s.go(run("perm"), MapEnv::new(&[], &[])).await;
    assert_eq!(out.exit_code, 2, "{}", out.result);
    let last = s.store.generation_status().unwrap().remove(&id).unwrap();
    assert!(!last.ok);
    assert_eq!(serde_json::to_value(last.category).unwrap(), json!("permission"));
    assert_eq!(last.provider_code.as_deref(), Some("personal-team-blocked"));
}
