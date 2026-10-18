use image::{ImageFormat, Rgba, RgbaImage};
use image_core::connection::{Connection, Secret};
use image_core::error::Category;
use image_core::generate::{generate_one, JobContext, JobOk, JobSpec};
use image_core::http::Http;
use image_core::normalize::Fit;
use image_core::provider::{Action, Background, ImageRequest};
use image_core::size::parse_size;
use serde_json::Value;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use test_fixture::{openai_image_reply, png_bytes, FakeProvider, Reply};
use tokio_util::sync::CancellationToken;

struct Setup {
    tmp: tempfile::TempDir,
    ctx: JobContext,
}

fn setup(provider: &FakeProvider) -> Setup {
    let conn = Connection { base_url: provider.base_url(), api_key: Some(Secret::new("fake-token")), source: "process_env" };
    Setup {
        tmp: tempfile::tempdir().unwrap(),
        ctx: JobContext { http: Arc::new(Http::new(Some(Duration::from_secs(10)))), conn: Arc::new(conn) },
    }
}

impl Setup {
    fn out(&self) -> PathBuf {
        self.tmp.path().join("ImageGallery/output/t/a.png")
    }
    fn rec(&self) -> PathBuf {
        self.tmp.path().join("ImageGallery/metadata/t/a.png.json")
    }
    fn lock_dir(&self) -> PathBuf {
        self.tmp.path().join("ImageGallery/metadata/t")
    }
    fn job(&self, request: ImageRequest, fit: Fit, resume: bool) -> JobSpec {
        JobSpec { request, fit, output: self.out(), record: Some(self.rec()), lock_dir: self.lock_dir(), resume, timeout: None }
    }
    fn record(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.rec()).unwrap()).unwrap()
    }
    fn assert_clean(&self) {
        assert!(!self.out().exists());
        assert!(!self.lock_dir().join("a.png.lock").exists());
        if let Ok(entries) = std::fs::read_dir(self.out().parent().unwrap()) {
            assert_eq!(entries.count(), 0, "no .part files remain");
        }
    }
}

fn request(model: &str, size: &str) -> ImageRequest {
    ImageRequest {
        prompt: "a mug".into(),
        image_model: model.into(),
        size: parse_size(size).unwrap(),
        quality: "low".into(),
        action: Action::Generate,
        references: vec![],
        background: Background::Auto,
    }
}

fn dims(path: &Path) -> (u32, u32) {
    let img = image::open(path).unwrap();
    (img.width(), img.height())
}

#[tokio::test]
async fn generates_exact_size_png_and_complete_record() {
    let png = png_bytes(1536, 1024, [20, 40, 60, 255]);
    let p = FakeProvider::start(move |_| openai_image_reply(&png));
    let s = setup(&p);
    let ok =
        generate_one(&s.ctx, &s.job(request("gpt-image-2.5", "1920x1080"), Fit::Cover, false), &CancellationToken::new()).await.unwrap();
    assert!(matches!(ok, JobOk::Generated { .. }));
    assert_eq!(dims(&s.out()), (1920, 1080));
    let rec = s.record();
    assert_eq!(rec["status"], "complete");
    assert_eq!(rec["provider_size"], "1536x1024");
    assert_eq!(rec["saved_size"], "1920x1080");
    assert_eq!(rec["fit"], "cover");
    assert_eq!(rec["response_model"], "gpt-image-test");
    assert_eq!(rec["image_model"], "gpt-image-2.5");
    assert_eq!(rec["connection_source"], "process_env");
    assert!(!std::fs::read_to_string(s.rec()).unwrap().contains("fake-token"));
    assert_eq!(p.requests()[0].path, "/v1/images/generations");
}

#[tokio::test]
async fn transparent_request_uses_contain_and_records_alpha() {
    let mut img = RgbaImage::from_pixel(1374, 1145, Rgba([0, 0, 0, 0]));
    for x in 400..1374 {
        for y in 300..800 {
            img.put_pixel(x, y, Rgba([0, 0, 255, 255]));
        }
    }
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut buf, ImageFormat::Png).unwrap();
    let png = buf.into_inner();
    let p = FakeProvider::start(move |_| openai_image_reply(&png));
    let s = setup(&p);
    let mut req = request("gpt-image-2.5-sunburst", "1024x1024");
    req.background = Background::Transparent;
    generate_one(&s.ctx, &s.job(req, Fit::Contain, false), &CancellationToken::new()).await.unwrap();
    assert_eq!(dims(&s.out()), (1024, 1024));
    let rec = s.record();
    assert_eq!(rec["fit"], "contain");
    assert_eq!(rec["background"], "transparent");
    assert!(rec["alpha"]["transparent_ratio"].as_f64().unwrap() > 0.0);
    assert_eq!(p.requests()[0].json()["background"], "transparent");
}

#[tokio::test]
async fn rejects_before_any_http_call() {
    let p = FakeProvider::start(|_| openai_image_reply(&png_bytes(8, 8, [0, 0, 0, 255])));
    let s = setup(&p);
    let text_file = s.tmp.path().join("notes.png");
    std::fs::write(&text_file, b"hello").unwrap();
    let missing = s.tmp.path().join("missing.png");
    let cases: Vec<(ImageRequest, Category, &str)> = vec![
        (
            {
                let mut r = request("gpt-image-2.5", "64x64");
                r.references = vec![missing.clone()];
                r
            },
            Category::InvalidRequest,
            "reference image not found",
        ),
        (
            {
                let mut r = request("gpt-image-2.5", "64x64");
                r.references = vec![text_file.clone()];
                r
            },
            Category::InvalidRequest,
            "not a readable image",
        ),
        (
            {
                let mut r = request("gpt-image-2.5", "64x64");
                r.background = Background::Transparent;
                r
            },
            Category::Unsupported,
            "sunburst",
        ),
        (
            {
                let mut r = request("grok-imagine-image-2.0", "64x64");
                r.references = vec![s.out()];
                r
            },
            Category::InvalidRequest,
            "reference image not found",
        ),
        (request("gpt-5.5", "64x64"), Category::Unsupported, "Responses API path was removed"),
        (
            {
                let mut r = request("gpt-image-2.5", "64x64");
                r.action = Action::Edit;
                r
            },
            Category::InvalidRequest,
            "require at least one reference",
        ),
    ];
    for (req, category, text) in cases {
        let err = generate_one(&s.ctx, &s.job(req, Fit::Cover, false), &CancellationToken::new()).await.unwrap_err();
        assert_eq!(err.category, category, "{}", err.message);
        assert!(err.message.contains(text), "{}", err.message);
    }
    let grok_ref = s.tmp.path().join("ok.png");
    std::fs::write(&grok_ref, png_bytes(4, 4, [0, 0, 0, 255])).unwrap();
    let mut r = request("grok-imagine-image-2.0", "64x64");
    r.references = vec![grok_ref];
    let err = generate_one(&s.ctx, &s.job(r, Fit::Cover, false), &CancellationToken::new()).await.unwrap_err();
    assert_eq!(err.category, Category::Unsupported);
    assert!(p.requests().is_empty());
    s.assert_clean();
}

#[tokio::test]
async fn http_200_without_image_fails_without_file() {
    let p = FakeProvider::start(|_| Reply::json(200, "<html>proxy login</html>"));
    let s = setup(&p);
    let err =
        generate_one(&s.ctx, &s.job(request("gpt-image-2.5", "64x64"), Fit::Cover, false), &CancellationToken::new()).await.unwrap_err();
    assert_eq!(err.category, Category::Provider);
    assert!(err.retryable);
    s.assert_clean();
    assert_eq!(s.record()["status"], "failed");
}

#[tokio::test]
async fn quota_error_is_recorded_and_not_retryable() {
    let p = FakeProvider::start(|_| Reply::json(403, r#"{"error":{"code":"personal-team-blocked:spending-limit","message":"limit"}}"#));
    let s = setup(&p);
    let err = generate_one(&s.ctx, &s.job(request("grok-imagine-image-2.0", "64x64"), Fit::Cover, false), &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(!err.retryable);
    let rec = s.record();
    assert_eq!(rec["error"]["category"], "quota");
    assert_eq!(rec["error"]["provider_code"], "personal-team-blocked:spending-limit");
}

#[tokio::test]
async fn existing_output_without_resume_is_rejected_untouched() {
    let p = FakeProvider::start(|_| openai_image_reply(&png_bytes(8, 8, [0, 0, 0, 255])));
    let s = setup(&p);
    std::fs::create_dir_all(s.out().parent().unwrap()).unwrap();
    std::fs::write(s.out(), b"old bytes").unwrap();
    let err =
        generate_one(&s.ctx, &s.job(request("gpt-image-2.5", "8x8"), Fit::Cover, false), &CancellationToken::new()).await.unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.starts_with("Output already exists; use a new topic or resume: "));
    assert_eq!(std::fs::read(s.out()).unwrap(), b"old bytes");
    assert!(p.requests().is_empty());
}

#[tokio::test]
async fn resume_skips_reusable_and_reports_mismatch() {
    let p = FakeProvider::start(|_| openai_image_reply(&png_bytes(8, 8, [0, 0, 0, 255])));
    let s = setup(&p);
    let cancel = CancellationToken::new();
    generate_one(&s.ctx, &s.job(request("gpt-image-2.5", "8x8"), Fit::Cover, true), &cancel).await.unwrap();
    let before = std::fs::read(s.out()).unwrap();
    let ok = generate_one(&s.ctx, &s.job(request("gpt-image-2.5", "8x8"), Fit::Cover, true), &cancel).await.unwrap();
    assert!(matches!(ok, JobOk::Skipped { .. }));
    assert_eq!(p.requests().len(), 1);
    let mut changed = request("gpt-image-2.5", "8x8");
    changed.prompt = "a different mug".into();
    let err = generate_one(&s.ctx, &s.job(changed, Fit::Cover, true), &cancel).await.unwrap_err();
    assert_eq!(err.category, Category::Conflict);
    assert_eq!(std::fs::read(s.out()).unwrap(), before);
    assert_eq!(p.requests().len(), 1);
}

#[tokio::test]
async fn cancel_during_request_cleans_up() {
    let p = FakeProvider::start(|_| openai_image_reply(&png_bytes(8, 8, [0, 0, 0, 255])).delayed(Duration::from_secs(5)));
    let s = setup(&p);
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let err = generate_one(&s.ctx, &s.job(request("gpt-image-2.5", "8x8"), Fit::Cover, false), &cancel).await.unwrap_err();
    assert_eq!(err.category, Category::Cancelled);
    s.assert_clean();
    assert_eq!(s.record()["error"]["category"], "cancelled");
}

#[tokio::test]
async fn job_timeout_is_recorded_as_retryable_timeout() {
    let p = FakeProvider::start(|_| openai_image_reply(&png_bytes(8, 8, [0, 0, 0, 255])).delayed(Duration::from_secs(3)));
    let s = setup(&p);
    let mut job = s.job(request("gpt-image-2.5", "8x8"), Fit::Cover, false);
    job.timeout = Some(Duration::from_millis(200));
    let err = generate_one(&s.ctx, &job, &CancellationToken::new()).await.unwrap_err();
    assert_eq!((err.category, err.retryable), (Category::Timeout, true));
    assert_eq!(err.message, "timed out after 0.2s");
    s.assert_clean();
    assert_eq!(s.record()["status"], "failed");
}
