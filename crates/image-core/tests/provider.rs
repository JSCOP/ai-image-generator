use image_core::catalog::Backend;
use image_core::connection::{Connection, Secret};
use image_core::error::Category;
use image_core::http::Http;
use image_core::profiles::ProviderKind;
use image_core::provider::{build_call, classify_http_error, extract_image, list_models, Action, Background, ImageRequest};
use image_core::size::{aspect_line, parse_size};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use test_fixture::{b64, gemini_image_reply, openai_image_reply, png_bytes, FakeProvider, Reply};

fn req(model: &str, size: &str, references: Vec<PathBuf>) -> ImageRequest {
    ImageRequest {
        prompt: "p".into(),
        image_model: model.into(),
        size: parse_size(size).unwrap(),
        quality: "high".into(),
        action: Action::Generate,
        references,
        background: Background::Auto,
        mask: None,
    }
}

/// Stage 1 CLIProxyAPI connection; these tests pin its unchanged behaviour.
fn proxy() -> Connection {
    Connection::cliproxy("http://h/v1".into(), None, "process_env")
}

fn png_ref(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("ref.png");
    std::fs::write(&path, png_bytes(8, 8, [1, 2, 3, 255])).unwrap();
    path
}

#[test]
fn openai_generation_payload_matches_python() {
    let call = build_call(&proxy(), &req("gpt-image-2.5", "1000x1000", vec![])).unwrap();
    assert_eq!(call.url, "http://h/v1/images/generations");
    let prompt = format!(
        "p

{}",
        aspect_line(parse_size("1000x1000").unwrap())
    );
    assert_eq!(call.body, json!({"model":"gpt-image-2.5","prompt":prompt,"n":1,"size":"1008x1008","quality":"high","output_format":"png"}));
    assert_eq!(call.backend, Backend::OpenAiImages);
}

/// Behind CLIProxyAPI the GPT model picks the shape from the prompt and ignores `size` (a character prompt at
/// 1920x1088 came back 1024x1536 three times out of three, 2026-10-19); one line naming the shape fixed it 3/3.
#[test]
fn gpt_prompts_name_the_requested_shape() {
    let line = |size: &str| aspect_line(parse_size(size).unwrap());
    assert_eq!(line("1920x1080"), "Image format: wide landscape 16:9 (1920x1080). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("1080x1920"), "Image format: tall portrait 9:16 (1080x1920). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("1024x1024"), "Image format: square 1:1 (1024x1024). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("1536x1024"), "Image format: landscape 3:2 (1536x1024). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("3840x2160"), "Image format: wide landscape 16:9 (3840x2160). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("1366x768"), "Image format: wide landscape 16:9 (1366x768). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("1000x700"), "Image format: landscape 10:7 (1000x700). Compose the scene for exactly this aspect ratio.");
    assert_eq!(line("1234x567"), "Image format: wide landscape 2.18:1 (1234x567). Compose the scene for exactly this aspect ratio.");
    // Grok and Gemini get the shape as parameters; their prompts stay as written.
    let grok = build_call(&proxy(), &req("grok-imagine-image-2.0", "1920x1080", vec![])).unwrap();
    assert_eq!(grok.body["prompt"], "p");
}

#[test]
fn gpt_edit_uses_images_data_urls() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = req("gpt-image-2.5", "1024x1024", vec![png_ref(&dir)]);
    r.action = Action::Edit;
    let call = build_call(&proxy(), &r).unwrap();
    assert_eq!(call.url, "http://h/v1/images/edits");
    let url = call.body["images"][0]["image_url"].as_str().unwrap();
    assert_eq!(url, format!("data:image/png;base64,{}", b64(&png_bytes(8, 8, [1, 2, 3, 255]))));
}

#[test]
fn gpt_edit_without_reference_is_rejected() {
    let mut r = req("gpt-image-2.5", "1024x1024", vec![]);
    r.action = Action::Edit;
    let err = build_call(&proxy(), &r).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert_eq!(err.message, "GPT image edits require at least one reference image");
}

#[test]
fn grok_payload_uses_b64_json_and_rejects_references() {
    let call = build_call(&proxy(), &req("grok-imagine-image-2.0", "1024x1024", vec![])).unwrap();
    assert_eq!(call.url, "http://h/v1/images/generations");
    assert_eq!(call.body["response_format"], "b64_json");
    assert!(call.body.get("output_format").is_none());
    let dir = tempfile::tempdir().unwrap();
    let err = build_call(&proxy(), &req("grok-imagine-image-2.0", "1024x1024", vec![png_ref(&dir)])).unwrap_err();
    assert_eq!(err.category, Category::Unsupported);
}

#[test]
fn transparent_background_is_sent_only_when_requested() {
    let mut r = req("gpt-image-2.5-sunburst", "1024x1024", vec![]);
    assert!(build_call(&proxy(), &r).unwrap().body.get("background").is_none());
    r.background = Background::Transparent;
    assert_eq!(build_call(&proxy(), &r).unwrap().body["background"], "transparent");
    r.background = Background::Opaque;
    assert_eq!(build_call(&proxy(), &r).unwrap().body["background"], "opaque");
    let mut g = req("gemini-3.1-flash-image", "1024x1024", vec![]);
    g.background = Background::Opaque;
    assert!(build_call(&proxy(), &g).unwrap().body.get("background").is_none());
}

#[test]
fn gemini_payload_and_url() {
    let dir = tempfile::tempdir().unwrap();
    let call = build_call(&proxy(), &req("gemini-3.1-flash-image", "1920x1080", vec![png_ref(&dir)])).unwrap();
    assert_eq!(call.url, "http://h/v1beta/models/gemini-3.1-flash-image:generateContent");
    assert_eq!(call.body["generationConfig"]["imageConfig"], json!({"aspectRatio":"16:9","imageSize":"2K"}));
    assert_eq!(call.body["generationConfig"]["responseModalities"], json!(["IMAGE", "TEXT"]));
    assert_eq!(call.body["contents"][0]["parts"][0], json!({"text": "p"}));
    assert_eq!(call.body["contents"][0]["parts"][1]["inlineData"]["mimeType"], "image/png");
    assert_eq!(call.backend, Backend::Gemini);
}

#[test]
fn unsupported_model_is_rejected() {
    let err = build_call(&proxy(), &req("gpt-5.5", "1024x1024", vec![])).unwrap_err();
    assert_eq!(err.category, Category::Unsupported);
    assert!(err.message.contains("Responses API path was removed"));
}

#[test]
fn extracts_both_shapes() {
    let png = png_bytes(4, 4, [9, 9, 9, 255]);
    let (bytes, model) = extract_image(ProviderKind::Cliproxy, &Backend::OpenAiImages, &openai_image_reply(&png).body).unwrap();
    assert_eq!((bytes, model.as_deref()), (png.clone(), Some("gpt-image-test")));
    let (bytes, model) = extract_image(ProviderKind::Cliproxy, &Backend::Gemini, &gemini_image_reply(&png, "gemini-x").body).unwrap();
    assert_eq!((bytes, model.as_deref()), (png.clone(), Some("gemini-x")));
    let snake = json!({"candidates":[{"content":{"parts":[{"text":"hi"},{"inline_data":{"mime_type":"image/png","data": b64(&png)}}]}}]});
    assert_eq!(extract_image(ProviderKind::Cliproxy, &Backend::Gemini, snake.to_string().as_bytes()).unwrap().0, png);
}

#[test]
fn non_image_200_is_retryable_provider_error() {
    for body in [&b"<html>oops</html>"[..], br#"{"data":[]}"#, br#"{"data":[{"b64_json":"%%%not base64"}]}"#] {
        let err = extract_image(ProviderKind::Cliproxy, &Backend::OpenAiImages, body).unwrap_err();
        assert_eq!(err.category, Category::Provider);
        assert!(err.retryable);
    }
}

#[test]
fn classifies_http_errors() {
    assert_eq!(classify_http_error(401, b"{}", None).category, Category::Auth);
    let e = classify_http_error(403, br#"{"error":{"code":"personal-team-blocked:spending-limit","message":"limit"}}"#, None);
    assert_eq!(
        (e.category, e.provider_code.as_deref(), e.retryable),
        (Category::Quota, Some("personal-team-blocked:spending-limit"), false)
    );
    assert_eq!(e.http_status, Some(403));
    assert_eq!(classify_http_error(403, b"{}", None).category, Category::Permission);
    assert_eq!(classify_http_error(402, b"{}", None).category, Category::Quota);
    assert_eq!(classify_http_error(429, b"{}", None).category, Category::RateLimit);
    assert!(classify_http_error(429, b"{}", None).retryable);
    assert_eq!(classify_http_error(429, br#"{"error":{"code":"insufficient_quota"}}"#, None).category, Category::Quota);
    assert_eq!(classify_http_error(404, b"", None).category, Category::NotFound);
    assert!(classify_http_error(503, b"", None).retryable);
    assert!(!classify_http_error(501, b"", None).retryable);
    assert_eq!(classify_http_error(408, b"", None).category, Category::Timeout);
    assert_eq!(classify_http_error(400, b"", None).category, Category::InvalidRequest);
    let key = Secret::new("sk-live");
    let e = classify_http_error(400, br#"{"error":{"message":"bad key sk-live"}}"#, Some(&key));
    assert!(!e.message.contains("sk-live"));
}

#[tokio::test]
async fn list_models_reads_data_ids_with_bearer() {
    let p = FakeProvider::start(|_| Reply::json(200, r#"{"data":[{"id":"gpt-image-2.5"},{"id":"gemini-3.1-flash-image"},{"x":1}]}"#));
    let conn = Connection::cliproxy(p.base_url(), Some(Secret::new("fake-token")), "process_env");
    let ids = list_models(&Http::new(Some(Duration::from_secs(5))), &conn).await.unwrap();
    assert_eq!(ids, vec!["gpt-image-2.5", "gemini-3.1-flash-image"]);
    let r = &p.requests()[0];
    assert_eq!((r.method.as_str(), r.path.as_str()), ("GET", "/v1/models"));
    assert_eq!(r.header("Authorization"), Some("Bearer fake-token"));
    let bad = FakeProvider::start(|_| Reply::json(401, "{}"));
    let conn = Connection::cliproxy(bad.base_url(), None, "process_env");
    assert_eq!(list_models(&Http::new(None), &conn).await.unwrap_err().category, Category::Auth);
}
