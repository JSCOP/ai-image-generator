use image_core::check::check_connection;
use image_core::connection::{Connection, Secret};
use image_core::error::Category;
use image_core::http::Http;
use image_core::profiles::{GenerationStatus, ProfileStore, ProviderKind};
use std::time::Duration;
use test_fixture::{closed_base_url, FakeProvider, Reply};

fn http() -> Http {
    Http::new(Some(Duration::from_secs(5)))
}

fn models_reply() -> Reply {
    Reply::json(200, r#"{"data":[{"id":"gpt-image-2.5"},{"id":"gpt-4o"},{"id":"gemini-3.1-flash-image"},{"id":"grok-imagine-image-2.0"}]}"#)
}

#[tokio::test]
async fn check_reports_each_stage() {
    // Nothing listening: not reachable, nothing further known.
    let conn = Connection::cliproxy(closed_base_url(), None, "process_env");
    let r = check_connection(&http(), &conn, None).await;
    assert!(!r.reachable && !r.authenticated && !r.models_listed, "{}", serde_json::to_string(&r).unwrap());
    assert!(!r.key_configured);
    assert_eq!(r.error.as_ref().unwrap().category, Category::Network);
    assert_eq!(r.warning, None, "loopback http needs no warning");
    assert_eq!(r.profile, None);

    // 401: the server answered but refused the key.
    let p = FakeProvider::start(|_| Reply::json(401, r#"{"error":{"message":"bad key"}}"#));
    let conn = Connection::cliproxy(p.base_url(), Some(Secret::new("k")), "process_env");
    let r = check_connection(&http(), &conn, None).await;
    assert!(r.reachable && !r.authenticated && !r.models_listed);
    assert!(r.key_configured);
    assert_eq!(r.error.as_ref().unwrap().category, Category::Auth);

    // A spending-limit answer means the key itself was accepted.
    let p = FakeProvider::start(|_| Reply::json(403, r#"{"error":{"code":"spending-limit","message":"limit reached"}}"#));
    let conn = Connection::cliproxy(p.base_url(), Some(Secret::new("k")), "process_env");
    let r = check_connection(&http(), &conn, None).await;
    assert!(r.reachable && r.authenticated && !r.models_listed);
    assert_eq!(r.error.as_ref().unwrap().category, Category::Quota);

    // 200: image models only, and the last real generation from status.json.
    let p = FakeProvider::start(|_| models_reply());
    let dir = tempfile::tempdir().unwrap();
    let store = ProfileStore::open(dir.path().to_path_buf());
    let profile = store.add("집 PC", ProviderKind::Cliproxy, Some(&p.base_url()), None).unwrap();
    let last = GenerationStatus {
        ok: false,
        at: 1.0,
        model: "grok-imagine-image-2.0".into(),
        category: Some(Category::Quota),
        provider_code: None,
    };
    store.record_generation(&profile.id, &last).unwrap();
    let mut conn = Connection::cliproxy(p.base_url(), None, "profile:집 PC");
    conn.profile_id = Some(profile.id.clone());
    let r = check_connection(&http(), &conn, Some(&store)).await;
    assert!(r.reachable && r.authenticated && r.models_listed, "{:?}", r.error);
    assert_eq!(r.image_models, vec!["gpt-image-2.5", "gemini-3.1-flash-image", "grok-imagine-image-2.0"]);
    assert_eq!(r.error, None);
    assert_eq!(r.profile.as_deref(), Some("집 PC"));
    assert_eq!(r.kind, ProviderKind::Cliproxy);
    assert_eq!(r.base_url, p.base_url());
    assert_eq!(r.last_generation, Some(last));

    // Plain http to a public host carries a warning (the host does not resolve, so nothing is sent).
    let conn = Connection::cliproxy("http://image-studio-check.invalid/v1".into(), None, "process_env");
    let r = check_connection(&http(), &conn, None).await;
    assert!(r.warning.is_some());
    assert!(!r.reachable);
}

#[tokio::test]
async fn check_never_generates() {
    let p = FakeProvider::start(|_| models_reply());
    let conn = Connection::cliproxy(p.base_url(), Some(Secret::new("k")), "process_env");
    check_connection(&http(), &conn, None).await;
    let gem = FakeProvider::start(|_| {
        Reply::json(200, r#"{"models":[{"name":"models/gemini-3.1-flash-image","supportedGenerationMethods":["generateContent"]}]}"#)
    });
    let mut conn = Connection::cliproxy(gem.base_url().trim_end_matches("/v1").to_string(), Some(Secret::new("g")), "profile:gem");
    conn.kind = ProviderKind::Gemini;
    let r = check_connection(&http(), &conn, None).await;
    assert_eq!(r.image_models, vec!["gemini-3.1-flash-image"]);
    let requests: Vec<_> = p.requests().into_iter().chain(gem.requests()).collect();
    assert_eq!(requests.len(), 2);
    for r in requests {
        assert_eq!(r.method, "GET", "{}", r.path);
        assert!(r.path.starts_with("/v1/models") || r.path.starts_with("/v1beta/models"), "{}", r.path);
    }
}
