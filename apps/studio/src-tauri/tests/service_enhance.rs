//! Prompt enhancement service against a temp home, `MemoryStore`, `MapEnv` and `FakeProvider`.

use std::sync::Arc;

use image_core::error::Category;
use image_core::secrets::MemoryStore;
use studio_lib::service::enhance::{enhance, text_models, EnhanceInput};
use studio_lib::service::Ctx;
use test_fixture::{FakeProvider, MapEnv, Reply};

fn ctx(home: &std::path::Path, base_url: &str) -> Ctx {
    let env = MapEnv::new(&[("CLIPROXY_BASE_URL", base_url), ("CLIPROXY_API_KEY", "sk-test-secret-key-0123456789")], &[]);
    Ctx { home: home.to_path_buf(), env: Arc::new(env), secrets: Arc::new(MemoryStore::new()) }
}

fn input(prompt: &str) -> EnhanceInput {
    EnhanceInput {
        profile: None,
        prompt: prompt.into(),
        avoid: String::new(),
        image_model: Some("gpt-image-2.5".into()),
        model: "gpt-6-luna".into(),
    }
}

#[tokio::test]
async fn enhance_returns_english_prompt_and_model() {
    let p = FakeProvider::start(|r| {
        if r.path.ends_with("/models") {
            return Reply::json(200, r#"{"data":[{"id":"gpt-image-2.5"},{"id":"gpt-6-luna"},{"id":"grok-tts"}]}"#);
        }
        Reply::json(200, r#"{"model":"gpt-6-luna","choices":[{"message":{"content":"A cat on a rainy night."}}]}"#)
    });
    let home = tempfile::tempdir().unwrap();
    let ctx = ctx(home.path(), &p.base_url());
    let view = enhance(&ctx, input("비 오는 밤 고양이")).await.unwrap();
    assert_eq!(view.prompt, "A cat on a rainy night.");
    assert_eq!(view.model, "gpt-6-luna");
    assert_eq!(text_models(&ctx, None).await.unwrap(), vec!["gpt-6-luna"]);
}

#[tokio::test]
async fn enhance_errors_never_carry_the_key() {
    let p = FakeProvider::start(|_| Reply::json(401, r#"{"error":{"message":"bad key sk-test-secret-key-0123456789"}}"#));
    let home = tempfile::tempdir().unwrap();
    let err = enhance(&ctx(home.path(), &p.base_url()), input("cat")).await.unwrap_err();
    assert_eq!(err.category, Category::Auth);
    assert!(!err.message.contains("sk-test-secret-key"), "{}", err.message);
}
