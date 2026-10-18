use image_core::connection::{normalize_base_url, resolve_connection, Secret};
use image_core::error::{redact, Category};
use image_core::http::Http;
use serde_json::json;
use std::time::Duration;
use test_fixture::{closed_base_url, FakeProvider, MapEnv, Reply};

#[tokio::test]
async fn redirect_is_not_followed_and_key_stays_home() {
    let dest = FakeProvider::start(|_| Reply::json(200, "{}"));
    let target = format!("{}/images/generations", dest.base_url()).replace("127.0.0.1", "localhost");
    let src = FakeProvider::start(move |_| Reply::redirect(302, &target));
    let http = Http::new(Some(Duration::from_secs(5)));
    let key = Secret::new("fake-token");
    let err = http.post_json(&format!("{}/images/generations", src.base_url()), &json!({}), Some(&key)).await.unwrap_err();
    assert_eq!(err.category, Category::Redirect);
    assert_eq!(err.http_status, Some(302));
    assert!(!err.retryable);
    let err = http.get(&format!("{}/models", src.base_url()), Some(&key), &[]).await.unwrap_err();
    assert_eq!(err.category, Category::Redirect);
    assert!(dest.requests().is_empty());
    assert_eq!(src.requests()[0].header("Authorization"), Some("Bearer fake-token"));
}

#[tokio::test]
async fn connection_refused_is_retryable_network() {
    let http = Http::new(Some(Duration::from_secs(5)));
    let err = http.post_json(&format!("{}/images/generations", closed_base_url()), &json!({}), None).await.unwrap_err();
    assert_eq!(err.category, Category::Network);
    assert!(err.retryable);
}

#[tokio::test]
async fn slow_server_times_out() {
    let slow = FakeProvider::start(|_| Reply::json(200, "{}").delayed(Duration::from_secs(2)));
    let http = Http::new(Some(Duration::from_millis(200)));
    let err = http.get(&format!("{}/models", slow.base_url()), None, &[]).await.unwrap_err();
    assert_eq!(err.category, Category::Timeout);
    assert!(err.retryable);
}

#[tokio::test]
async fn non_redirect_status_is_returned_for_classification() {
    let p = FakeProvider::start(|_| Reply::json(429, r#"{"error":{}}"#));
    let http = Http::new(Some(Duration::from_secs(5)));
    let (status, body) = http.post_json(&format!("{}/x", p.base_url()), &json!({"a": 1}), None).await.unwrap();
    assert_eq!(status, 429);
    assert_eq!(body, br#"{"error":{}}"#);
    assert_eq!(p.requests()[0].json(), json!({"a": 1}));
    assert_eq!(p.requests()[0].header("Authorization"), None);
}

#[test]
fn base_url_rules() {
    assert_eq!(normalize_base_url("http://127.0.0.1:8317").unwrap(), "http://127.0.0.1:8317/v1");
    assert_eq!(normalize_base_url(" http://host:1/v1/ ").unwrap(), "http://host:1/v1");
    assert!(normalize_base_url("http://u:p@host/v1").is_err());
    assert!(normalize_base_url("ftp://host").is_err());
    assert!(normalize_base_url("http://host/v1?x=1").is_err());
    assert!(normalize_base_url("http://ho st/v1").is_err());
    assert!(normalize_base_url("").is_err());
}

#[test]
fn connection_resolution_order_and_no_default() {
    let err = resolve_connection(&MapEnv::new(&[], &[])).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("CLIPROXY_BASE_URL"));
    let env = MapEnv::new(&[("CLIPROXY_BASE_URL", "http://a:1")], &[("CLIPROXY_BASE_URL", "http://b:2"), ("CLIPROXY_API_KEY", "k")]);
    let c = resolve_connection(&env).unwrap();
    assert_eq!(c.base_url, "http://a:1/v1");
    assert_eq!(c.source, "process_env");
    assert_eq!(format!("{:?}", c.api_key), "Some(***)");
    assert_eq!(c.api_key.as_ref().unwrap().expose(), "k");
    let c = resolve_connection(&MapEnv::new(&[("OPENAI_API_KEY", "o")], &[("CLIPROXY_BASE_URL", "http://b:2"), ("CLIPROXY_API_KEY", "u")]))
        .unwrap();
    assert_eq!((c.base_url.as_str(), c.source, c.api_key.unwrap().expose()), ("http://b:2/v1", "user_env", "o"));
    let err = resolve_connection(&MapEnv::new(&[("CLIPROXY_BASE_URL", "ftp://x")], &[])).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
}

#[test]
fn redact_removes_key_and_bearer() {
    let s = Secret::new("sk-123");
    assert_eq!(redact("bad sk-123 Authorization: Bearer sk-999", Some(&s)), "bad *** Authorization: Bearer ***");
    assert_eq!(redact("no secrets", None), "no secrets");
}
