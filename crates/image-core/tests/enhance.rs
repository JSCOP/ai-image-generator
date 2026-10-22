use image_core::connection::{Connection, Secret};
use image_core::enhance::{enhance_prompt, is_text_model, text_models, EnhanceRequest};
use image_core::error::Category;
use image_core::http::Http;
use image_core::profiles::ProviderKind;
use std::time::Duration;
use test_fixture::{FakeProvider, Reply};

fn http() -> Http {
    Http::new(Some(Duration::from_secs(5)))
}

fn chat_reply(content: &str) -> Reply {
    let body = serde_json::json!({"model": "gpt-6-luna", "choices": [{"message": {"role": "assistant", "content": content}}]});
    Reply::json(200, &body.to_string())
}

fn request<'a>(prompt: &'a str, avoid: &'a str) -> EnhanceRequest<'a> {
    EnhanceRequest { prompt, avoid, image_model: Some("gpt-image-2.5"), text_model: "gpt-6-luna" }
}

#[tokio::test]
async fn enhance_posts_chat_completion_and_returns_clean_prompt() {
    let p = FakeProvider::start(|_| chat_reply("```\n\"A cat in a rainy Seoul alley at night.\"\n```"));
    let conn = Connection::cliproxy(p.base_url(), Some(Secret::new("k")), "process_env");
    let out = enhance_prompt(&http(), &conn, &request("비 오는 밤 서울 골목 고양이", "글자")).await.unwrap();
    assert_eq!(out.prompt, "A cat in a rainy Seoul alley at night.");
    assert_eq!(out.response_model.as_deref(), Some("gpt-6-luna"));

    let sent = &p.requests()[0];
    assert_eq!(sent.method, "POST");
    assert!(sent.path.ends_with("/v1/chat/completions"), "{}", sent.path);
    assert_eq!(sent.header("authorization"), Some("Bearer k"));
    let body = sent.json();
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!(body["messages"][0]["role"], "system");
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(user.contains("비 오는 밤 서울 골목 고양이") && user.contains("글자") && user.contains("gpt-image-2.5"), "{user}");
}

#[tokio::test]
async fn enhance_rejects_bad_input_before_any_request() {
    let p = FakeProvider::start(|_| chat_reply("x"));
    let conn = Connection::cliproxy(p.base_url(), None, "process_env");
    let long = "가".repeat(4001);
    for (prompt, model) in [("   ", "gpt-6-luna"), (long.as_str(), "gpt-6-luna"), ("cat", "bad model/../x"), ("cat", "")] {
        let req = EnhanceRequest { prompt, avoid: "", image_model: None, text_model: model };
        let err = enhance_prompt(&http(), &conn, &req).await.unwrap_err();
        assert_eq!(err.category, Category::InvalidRequest, "{model}: {}", err.message);
    }
    assert!(p.requests().is_empty());

    let gemini = Connection { kind: ProviderKind::Gemini, ..Connection::cliproxy(p.base_url(), None, "x") };
    let err = enhance_prompt(&http(), &gemini, &request("cat", "")).await.unwrap_err();
    assert_eq!(err.category, Category::Unsupported);
    assert!(p.requests().is_empty());
}

#[tokio::test]
async fn enhance_maps_provider_errors_and_empty_answers() {
    let p = FakeProvider::start(|_| Reply::json(404, r#"{"error":{"message":"model not found"}}"#));
    let conn = Connection::cliproxy(p.base_url(), Some(Secret::new("sk-secret-123456")), "process_env");
    let err = enhance_prompt(&http(), &conn, &request("cat", "")).await.unwrap_err();
    assert_eq!(err.category, Category::NotFound);
    assert!(err.message.contains("model not found"));

    let p = FakeProvider::start(|_| chat_reply("   "));
    let conn = Connection::cliproxy(p.base_url(), None, "process_env");
    let err = enhance_prompt(&http(), &conn, &request("cat", "")).await.unwrap_err();
    assert_eq!(err.category, Category::Provider);

    let p = FakeProvider::start(|_| Reply::json(200, "not json"));
    let conn = Connection::cliproxy(p.base_url(), None, "process_env");
    let err = enhance_prompt(&http(), &conn, &request("cat", "")).await.unwrap_err();
    assert_eq!(err.category, Category::Provider);
}

#[test]
fn text_model_filter_skips_image_audio_and_embedding_models() {
    for id in ["gpt-6-luna", "claude-sonnet-5-5", "gemini-3.5-flash-lite", "grok-4.7", "opencode-go/kimi-k3"] {
        assert!(is_text_model(id), "{id}");
    }
    for id in [
        "gpt-image-2.5",
        "gemini-3.1-flash-image",
        "grok-imagine-image",
        "grok-tts",
        "grok-voice-tts-1.0",
        "text-embedding-3",
        "whisper-1",
        "dall-e-3",
        "codex-auto-review",
    ] {
        assert!(!is_text_model(id), "{id}");
    }
}

#[tokio::test]
async fn text_models_lists_chat_models_of_a_proxy() {
    let p = FakeProvider::start(|_| {
        Reply::json(
            200,
            r#"{"data":[{"id":"gpt-image-2.5"},{"id":"gpt-6-luna"},{"id":"grok-tts"},{"id":"claude-sonnet-5-5"},{"id":"gpt-6-luna"}]}"#,
        )
    });
    let conn = Connection::cliproxy(p.base_url(), None, "process_env");
    assert_eq!(text_models(&http(), &conn).await.unwrap(), vec!["gpt-6-luna", "claude-sonnet-5-5"]);

    let gemini = Connection { kind: ProviderKind::Gemini, ..Connection::cliproxy(p.base_url(), None, "x") };
    assert!(text_models(&http(), &gemini).await.unwrap().is_empty());
}
