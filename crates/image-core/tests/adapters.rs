use image::ImageFormat;
use image_core::adapter::{build_call, list_models, model_supported, supports_native_transparency, supports_references};
use image_core::connection::{Connection, Secret};
use image_core::error::{Category, EngineError};
use image_core::generate::{generate_one, JobContext, JobOk, JobSpec};
use image_core::http::Http;
use image_core::normalize::Fit;
use image_core::profiles::ProviderKind;
use image_core::provider::{Action, Background, ImageRequest};
use image_core::size::parse_size;
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use test_fixture::{b64, png_bytes, FakeProvider, Reply};
use tokio_util::sync::CancellationToken;

fn conn(kind: ProviderKind, base_url: String) -> Connection {
    Connection {
        kind,
        base_url,
        api_key: Some(Secret::new("fake-token")),
        source: format!("profile:{}", kind.as_str()),
        profile_id: None,
        concurrency_default: kind.default_concurrency(),
    }
}

/// Fake provider root without `/v1` (Gemini official addresses have no `/v1`).
fn root(p: &FakeProvider) -> String {
    p.base_url().trim_end_matches("/v1").to_string()
}

fn req(model: &str, size: &str, references: Vec<PathBuf>) -> ImageRequest {
    ImageRequest {
        prompt: "p".into(),
        image_model: model.into(),
        size: parse_size(size).unwrap(),
        quality: "high".into(),
        action: Action::Generate,
        references,
        background: Background::Auto,
    }
}

fn refs(dir: &tempfile::TempDir, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|i| {
            let path = dir.path().join(format!("ref{i}.png"));
            std::fs::write(&path, png_bytes(8, 8, [1, 2, 3, 255])).unwrap();
            path
        })
        .collect()
}

fn data_url() -> String {
    format!("data:image/png;base64,{}", b64(&png_bytes(8, 8, [1, 2, 3, 255])))
}

struct Run {
    tmp: tempfile::TempDir,
    result: Result<JobOk, EngineError>,
}

impl Run {
    fn out(&self) -> PathBuf {
        self.tmp.path().join("out.png")
    }
    fn record(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.tmp.path().join("out.png.json")).unwrap()).unwrap()
    }
}

async fn generate(conn: Connection, request: ImageRequest) -> Run {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = JobContext { http: Arc::new(Http::new(Some(Duration::from_secs(10)))), conn: Arc::new(conn) };
    let job = JobSpec {
        request,
        fit: Fit::Cover,
        output: tmp.path().join("out.png"),
        record: Some(tmp.path().join("out.png.json")),
        lock_dir: tmp.path().to_path_buf(),
        resume: false,
        timeout: None,
    };
    let result = generate_one(&ctx, &job, &CancellationToken::new()).await;
    Run { tmp, result }
}

fn official_openai_reply(png: &[u8]) -> Reply {
    Reply::json(200, &json!({"created": 1, "data": [{"b64_json": b64(png)}]}).to_string())
}

fn xai_reply(bytes: &[u8], mime: &str) -> Reply {
    Reply::json(200, &json!({"model": "grok-served", "data": [{"b64_json": b64(bytes), "mime_type": mime}]}).to_string())
}

fn centre_pixel(path: &std::path::Path) -> [u8; 4] {
    let img = image::open(path).unwrap().to_rgba8();
    img.get_pixel(img.width() / 2, img.height() / 2).0
}

#[tokio::test]
async fn openai_generation_and_edit_bodies() {
    let png = png_bytes(8, 8, [10, 20, 30, 255]);
    let p = FakeProvider::start(move |_| official_openai_reply(&png));
    let run = generate(conn(ProviderKind::Openai, p.base_url()), req("gpt-image-2.5-sunburst", "1000x1000", vec![])).await;
    assert!(matches!(run.result, Ok(JobOk::Generated { .. })), "{:?}", run.result);
    // The official response has no `model` field.
    assert!(run.record()["response_model"].is_null());
    let r = &p.requests()[0];
    assert_eq!((r.method.as_str(), r.path.as_str()), ("POST", "/v1/images/generations"));
    assert_eq!(r.header("Authorization"), Some("Bearer fake-token"));
    assert_eq!(
        r.json(),
        json!({"model":"gpt-image-2.5-sunburst","prompt":"p","n":1,"size":"1008x1008","quality":"high","output_format":"png"})
    );

    let dir = tempfile::tempdir().unwrap();
    let mut edit = req("gpt-image-2.5-sunburst", "1024x1024", refs(&dir, 2));
    edit.action = Action::Edit;
    edit.background = Background::Transparent;
    let run = generate(conn(ProviderKind::Openai, p.base_url()), edit).await;
    // The fake reply is opaque, so the transparent request is sent but its result is refused.
    let err = run.result.as_ref().unwrap_err();
    assert_eq!((err.category, err.retryable), (Category::Provider, false), "{err:?}");
    let r = &p.requests()[1];
    assert_eq!(r.path, "/v1/images/edits");
    let body = r.json();
    assert_eq!(body["images"], json!([{"image_url": data_url()}, {"image_url": data_url()}]));
    assert_eq!(body["background"], "transparent");
    assert_eq!(body["output_format"], "png");
}

#[tokio::test]
async fn openai_rejects_gemini_model_before_call() {
    let p = FakeProvider::start(|_| Reply::json(500, "{}"));
    let run = generate(conn(ProviderKind::Openai, p.base_url()), req("gemini-3.1-flash-image", "1024x1024", vec![])).await;
    let err = run.result.as_ref().unwrap_err().clone();
    assert_eq!(err.category, Category::Unsupported);
    assert!(err.message.contains("OpenAI 공식"), "{}", err.message);
    assert!(p.requests().is_empty());
    assert!(!run.out().exists());
}

#[tokio::test]
async fn gemini_uses_goog_key_and_skips_thought_parts() {
    let thought = png_bytes(8, 8, [255, 0, 0, 255]);
    let final_png = png_bytes(8, 8, [0, 0, 255, 255]);
    let reply = json!({
        "modelVersion": "gemini-3.1-flash-image",
        "candidates": [{"content": {"parts": [
            {"text": "planning", "thought": true},
            {"inlineData": {"mimeType": "image/png", "data": b64(&thought)}, "thought": true},
            {"text": "here you go"},
            {"inlineData": {"mimeType": "image/png", "data": b64(&final_png)}}
        ]}, "finishReason": "STOP"}]
    })
    .to_string();
    let p = FakeProvider::start(move |_| Reply::json(200, &reply));
    let run = generate(conn(ProviderKind::Gemini, root(&p)), req("gemini-3.1-flash-image", "1920x1080", vec![])).await;
    assert!(run.result.is_ok(), "{:?}", run.result);
    assert_eq!(centre_pixel(&run.out()), [0, 0, 255, 255]);
    assert_eq!(run.record()["response_model"], "gemini-3.1-flash-image");
    let r = &p.requests()[0];
    assert_eq!(r.path, "/v1beta/models/gemini-3.1-flash-image:generateContent");
    assert_eq!(r.header("x-goog-api-key"), Some("fake-token"));
    assert_eq!(r.header("Authorization"), None);
    let body = r.json();
    assert_eq!(body["generationConfig"]["imageConfig"], json!({"aspectRatio":"16:9","imageSize":"2K"}));
    assert_eq!(body["contents"][0]["parts"][0], json!({"text": "p"}));
}

#[tokio::test]
async fn gemini_safety_block_is_not_retryable() {
    let reply =
        json!({"candidates": [{"content": {"parts": [{"text": "cannot"}]}, "finishReason": "IMAGE_SAFETY", "finishMessage": "blocked"}]})
            .to_string();
    let p = FakeProvider::start(move |_| Reply::json(200, &reply));
    let run = generate(conn(ProviderKind::Gemini, root(&p)), req("gemini-3.1-flash-image", "1024x1024", vec![])).await;
    let err = run.result.as_ref().unwrap_err().clone();
    assert_eq!(err.category, Category::Provider);
    assert_eq!(err.provider_code.as_deref(), Some("IMAGE_SAFETY"));
    assert!(!err.retryable);
    assert!(!run.out().exists());
    // A plain empty answer stays a retryable provider error.
    let p = FakeProvider::start(|_| Reply::json(200, r#"{"candidates":[{"content":{"parts":[{"text":"hm"}]},"finishReason":"STOP"}]}"#));
    let err = generate(conn(ProviderKind::Gemini, root(&p)), req("gemini-3.1-flash-image", "1024x1024", vec![])).await.result.unwrap_err();
    assert_eq!((err.category, err.retryable), (Category::Provider, true));
}

#[tokio::test]
async fn gemini_model_list_follows_pages() {
    let p = FakeProvider::start(|r| {
        if r.path.contains("pageToken=t2") {
            Reply::json(
                200,
                r#"{"models":[
                    {"name":"models/gemini-3-pro-image","supportedGenerationMethods":["generateContent","countTokens"]},
                    {"name":"models/gemini-image-tuner","supportedGenerationMethods":["createTunedModel"]}
                ]}"#,
            )
        } else {
            Reply::json(
                200,
                r#"{"models":[
                    {"name":"models/gemini-3.1-flash-image","supportedGenerationMethods":["generateContent"]},
                    {"name":"models/gemini-2.5-pro","supportedGenerationMethods":["generateContent"]}
                ],"nextPageToken":"t2"}"#,
            )
        }
    });
    let ids = list_models(&Http::new(Some(Duration::from_secs(5))), &conn(ProviderKind::Gemini, root(&p))).await.unwrap();
    assert_eq!(ids, vec!["gemini-3.1-flash-image", "gemini-3-pro-image"]);
    let requests = p.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].path, "/v1beta/models?pageSize=1000");
    assert_eq!(requests[1].path, "/v1beta/models?pageSize=1000&pageToken=t2");
    assert!(requests.iter().all(|r| r.method == "GET" && r.header("x-goog-api-key") == Some("fake-token")));
}

#[tokio::test]
async fn openai_and_xai_model_lists_keep_their_image_models() {
    let p = FakeProvider::start(|_| {
        Reply::json(200, r#"{"data":[{"id":"gpt-image-2"},{"id":"gpt-4o"},{"id":"grok-imagine-image-2.0"},{"id":"grok-imagine-video"}]}"#)
    });
    let http = Http::new(Some(Duration::from_secs(5)));
    assert_eq!(list_models(&http, &conn(ProviderKind::Openai, p.base_url())).await.unwrap(), vec!["gpt-image-2"]);
    assert_eq!(list_models(&http, &conn(ProviderKind::Xai, p.base_url())).await.unwrap(), vec!["grok-imagine-image-2.0"]);
    assert_eq!(list_models(&http, &conn(ProviderKind::Cliproxy, p.base_url())).await.unwrap().len(), 4);
    assert!(p.requests().iter().all(|r| r.path == "/v1/models"));
}

#[tokio::test]
async fn xai_generation_maps_size_to_aspect_and_resolution() {
    let png = png_bytes(8, 8, [10, 20, 30, 255]);
    let p = FakeProvider::start(move |_| xai_reply(&png, "image/png"));
    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image-2.0", "1920x1080", vec![])).await;
    assert!(run.result.is_ok(), "{:?}", run.result);
    assert_eq!(run.record()["response_model"], "grok-served");
    let r = &p.requests()[0];
    assert_eq!(r.path, "/v1/images/generations");
    assert_eq!(r.header("Authorization"), Some("Bearer fake-token"));
    assert_eq!(
        r.json(),
        json!({"model":"grok-imagine-image-2.0","prompt":"p","n":1,"response_format":"b64_json",
               "aspect_ratio":"16:9","resolution":"2k","quality":"medium"})
    );
    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image", "1024x1024", vec![])).await;
    assert!(run.result.is_ok(), "{:?}", run.result);
    assert_eq!(
        p.requests()[1].json(),
        json!({"model":"grok-imagine-image","prompt":"p","n":1,"response_format":"b64_json","aspect_ratio":"1:1","resolution":"1k"})
    );

    let xai = conn(ProviderKind::Xai, "http://h/v1".into());
    let mut low = req("grok-imagine-image-2.0", "1536x1024", vec![]);
    low.quality = "low".into();
    let body = build_call(&xai, &low).unwrap().body;
    assert_eq!(
        (body["aspect_ratio"].as_str(), body["resolution"].as_str(), body["quality"].as_str()),
        (Some("3:2"), Some("1k"), Some("low"))
    );
    // 21:9 and 5:2 exist only on 2.0; older models get the nearest older ratio.
    assert_eq!(build_call(&xai, &req("grok-imagine-image-2.0", "2520x1080", vec![])).unwrap().body["aspect_ratio"], "21:9");
    assert_eq!(build_call(&xai, &req("grok-imagine-image", "2520x1080", vec![])).unwrap().body["aspect_ratio"], "2:1");
}

#[tokio::test]
async fn xai_edit_single_and_multi() {
    let png = png_bytes(8, 8, [10, 20, 30, 255]);
    let p = FakeProvider::start(move |_| xai_reply(&png, "image/png"));
    let dir = tempfile::tempdir().unwrap();
    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image", "1024x1024", refs(&dir, 1))).await;
    assert!(run.result.is_ok(), "{:?}", run.result);
    let r = &p.requests()[0];
    assert_eq!(r.path, "/v1/images/edits");
    let body = r.json();
    assert_eq!(body["image"], json!({"url": data_url()}));
    assert!(body.get("images").is_none());
    assert_eq!(body["response_format"], "b64_json");

    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image-2.0", "1024x1024", refs(&dir, 3))).await;
    assert!(run.result.is_ok(), "{:?}", run.result);
    let body = p.requests()[1].json();
    assert_eq!(body["images"], json!([{"url": data_url()}, {"url": data_url()}, {"url": data_url()}]));
    assert!(body.get("image").is_none());

    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image", "1024x1024", refs(&dir, 3))).await;
    assert_eq!(run.result.unwrap_err().category, Category::Unsupported);
    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image-2.0", "1024x1024", refs(&dir, 6))).await;
    assert_eq!(run.result.unwrap_err().category, Category::Unsupported);
    assert_eq!(p.requests().len(), 2, "rejected edits never reach the provider");
}

#[tokio::test]
async fn xai_jpeg_output_is_saved_as_png() {
    let rgb = image::RgbImage::from_pixel(16, 16, image::Rgb([0, 200, 0]));
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rgb).write_to(&mut jpeg, ImageFormat::Jpeg).unwrap();
    let jpeg = jpeg.into_inner();
    let p = FakeProvider::start(move |_| xai_reply(&jpeg, "image/jpeg"));
    let run = generate(conn(ProviderKind::Xai, p.base_url()), req("grok-imagine-image", "1024x1024", vec![])).await;
    assert!(run.result.is_ok(), "{:?}", run.result);
    let saved = std::fs::read(run.out()).unwrap();
    assert_eq!(image::guess_format(&saved).unwrap(), ImageFormat::Png);
    let img = image::load_from_memory(&saved).unwrap();
    assert_eq!((img.width(), img.height()), (1024, 1024));
}

#[tokio::test]
async fn cliproxy_behaviour_is_unchanged() {
    let png = png_bytes(8, 8, [10, 20, 30, 255]);
    let p = FakeProvider::start(move |_| xai_reply(&png, "image/png"));
    for kind in [ProviderKind::Cliproxy, ProviderKind::OpenaiCompatible] {
        let before = p.requests().len();
        let c = || conn(kind, p.base_url());
        assert!(generate(c(), req("gpt-image-2.5", "1000x1000", vec![])).await.result.is_ok());
        assert!(generate(c(), req("grok-imagine-image-2.0", "1024x1024", vec![])).await.result.is_ok());
        let requests = p.requests();
        assert_eq!(requests[before].path, "/v1/images/generations");
        assert_eq!(
            requests[before].json(),
            json!({"model":"gpt-image-2.5","prompt":"p","n":1,"size":"1008x1008","quality":"high","output_format":"png"})
        );
        assert_eq!(
            requests[before + 1].json(),
            json!({"model":"grok-imagine-image-2.0","prompt":"p","n":1,"size":"1024x1024","quality":"high","response_format":"b64_json"})
        );
        assert_eq!(requests[before].header("Authorization"), Some("Bearer fake-token"));
        let gemini = build_call(&c(), &req("gemini-3.1-flash-image", "1024x1024", vec![])).unwrap();
        assert_eq!(gemini.url, format!("{}/v1beta/models/gemini-3.1-flash-image:generateContent", root(&p)));
        let dir = tempfile::tempdir().unwrap();
        let err = generate(c(), req("grok-imagine-image-2.0", "1024x1024", refs(&dir, 1))).await.result.unwrap_err();
        assert_eq!(err.category, Category::Unsupported);
        let err = generate(c(), req("gpt-5.5", "1024x1024", vec![])).await.result.unwrap_err();
        assert!(err.message.contains("Responses API path was removed"));
        assert_eq!(p.requests().len(), before + 2);
    }
}

#[test]
fn capability_table() {
    use ProviderKind::*;
    assert!(supports_native_transparency(Openai, "gpt-image-2.5-sunburst-2026-09-08"));
    assert!(supports_native_transparency(Openai, "gpt-image-2.5-flare"));
    assert!(!supports_native_transparency(Openai, "gpt-image-2"));
    assert!(supports_references(Xai, "grok-imagine-image-2.0"));
    assert!(supports_references(Xai, "grok-imagine-image"));
    assert!(!supports_native_transparency(Xai, "grok-imagine-image-2.0"));
    assert!(supports_references(Gemini, "gemini-3.1-flash-image"));
    assert!(!supports_native_transparency(Gemini, "gemini-3.1-flash-image"));
    assert!(!supports_references(Cliproxy, "grok-imagine-image"));
    assert!(supports_references(Cliproxy, "gpt-image-2.5"));
    assert!(supports_native_transparency(Cliproxy, "gpt-image-2.5-sunburst"));
    assert!(model_supported(Openai, "gpt-image-1.5").is_ok());
    assert_eq!(model_supported(Openai, "grok-imagine-image").unwrap_err().category, Category::Unsupported);
    assert_eq!(model_supported(Gemini, "gpt-image-2.5").unwrap_err().category, Category::Unsupported);
    assert_eq!(model_supported(Xai, "gemini-3.1-flash-image").unwrap_err().category, Category::Unsupported);
    assert!(model_supported(Xai, "grok-imagine-image-quality").is_ok());
    assert!(model_supported(Cliproxy, "grok-imagine-image").is_ok());
    assert!(model_supported(OpenaiCompatible, "gemini-3.1-flash-image").is_ok());
    assert_eq!(model_supported(Cliproxy, "gpt-5.5").unwrap_err().category, Category::Unsupported);
}
