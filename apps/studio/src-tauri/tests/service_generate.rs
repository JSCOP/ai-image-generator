//! Generation service against temp homes, `MemoryStore`, `MapEnv` and `FakeProvider`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::TimeZone;
use image_core::error::Category;
use image_core::secrets::MemoryStore;
use studio_lib::service::generate::{build_spec, run_generation, GenerateInput, RunEvent, MAX_COUNT, MAX_REFERENCES};
use studio_lib::service::settings::{save_settings, Settings};
use studio_lib::service::Ctx;
use test_fixture::{openai_image_reply, png_bytes, FakeProvider, MapEnv, Reply};
use tokio_util::sync::CancellationToken;

fn input(prompt: &str) -> GenerateInput {
    GenerateInput {
        profile: None,
        prompt: prompt.to_string(),
        avoid: String::new(),
        image_model: "gpt-image-2.5".into(),
        size: "16x16".into(),
        quality: "low".into(),
        count: 1,
        references: Vec::new(),
        background: "auto".into(),
        topic: None,
        resume: false,
    }
}

fn settings(work: &Path) -> Settings {
    Settings { work_dir: work.to_path_buf(), ..Settings::default() }
}

fn now() -> chrono::DateTime<chrono::Local> {
    chrono::Local.with_ymd_and_hms(2026, 10, 18, 14, 5, 9).unwrap()
}

struct Setup {
    home: tempfile::TempDir,
    work: tempfile::TempDir,
    ctx: Ctx,
}

impl Setup {
    fn new(base_url: &str) -> Setup {
        let home = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        save_settings(home.path(), &settings(work.path())).unwrap();
        let env =
            MapEnv::new(&[("CLIPROXY_BASE_URL", base_url), ("CLIPROXY_API_KEY", "fake-token"), ("AI_IMAGE_RETRY_BACKOFF_SEC", "0")], &[]);
        let ctx = Ctx { home: home.path().to_path_buf(), env: Arc::new(env), secrets: Arc::new(MemoryStore::new()) };
        Setup { home, work, ctx }
    }

    async fn run(
        &self,
        input: GenerateInput,
        cancel: CancellationToken,
    ) -> (Result<studio_lib::service::generate::RunSummary, studio_lib::service::ServiceError>, Vec<RunEvent>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let result = run_generation(&self.ctx, input, "run-test".into(), move |e| sink.lock().unwrap().push(e), cancel).await;
        let events = events.lock().unwrap().clone();
        (result, events)
    }

    fn files_under(&self, relative: &str) -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.work.path().join(relative), &mut out);
        out
    }
}

fn ok_provider(delay_ms: u64) -> FakeProvider {
    let png = png_bytes(16, 16, [9, 9, 9, 255]);
    FakeProvider::start(move |_| openai_image_reply(&png).delayed(Duration::from_millis(delay_ms)))
}

#[test]
fn build_spec_composes_avoid_and_topic() {
    let work = tempfile::tempdir().unwrap();
    let mut req = input("아늑한 독서 공간, 오후 햇살 ☀️");
    req.avoid = "  text  ".into();
    req.count = 3;
    let (spec, topic) = build_spec(&req, &settings(work.path()), now()).unwrap();
    assert_eq!(topic, "아늑한_독서_공간_오후_햇살-20261018-140509");
    assert!(topic.chars().count() <= 80);
    assert_eq!(spec["topic"], topic.as_str());
    assert_eq!(spec["mode"], "single");
    let prompt = spec["prompt"].as_str().unwrap();
    assert_eq!(prompt.lines().collect::<Vec<_>>(), ["아늑한 독서 공간, 오후 햇살 ☀️", "Avoid: text"]);
    assert_eq!(spec["topic_root"], work.path().display().to_string());
    assert_eq!(spec["count"], 3);
    assert_eq!(spec["size"], "16x16");
    assert_eq!(spec["quality"], "low");
    assert_eq!(spec["image_model"], "gpt-image-2.5");
    assert_eq!(spec["background"], "auto");
    assert_eq!(spec["resume"], false);
    assert_eq!(spec["reference_images"], serde_json::json!([]));
    assert!(spec.get("profile").is_none());

    // No avoid line when avoid is blank; a profile is passed through.
    let mut plain = input("한 줄");
    plain.profile = Some("local".into());
    let (spec, _) = build_spec(&plain, &settings(work.path()), now()).unwrap();
    assert_eq!(spec["prompt"], "한 줄");
    assert_eq!(spec["profile"], "local");

    // A very long Korean prompt with line breaks and emoji still gives a short, safe name.
    let long = format!("{}\n두 번째 줄 🎨🎨 그리고/역슬래시\\콜론: 끝", "가".repeat(300));
    let (_, topic) = build_spec(&input(&long), &settings(work.path()), now()).unwrap();
    assert!(topic.chars().count() <= 80, "{topic}");
    assert!(topic.ends_with("-20261018-140509"), "{topic}");
    assert!(!topic.contains(['/', '\\', ':', '\n', ' ']), "{topic}");
    assert_eq!(topic, image_core::paths::safe_topic_slug(&topic));

    // Nothing usable in the prompt: a fallback word, never an empty name.
    let (_, topic) = build_spec(&input("🎨 ☀️ !!"), &settings(work.path()), now()).unwrap();
    assert_eq!(topic, "image-20261018-140509");
}

#[test]
fn build_spec_validates_count_and_references() {
    let work = tempfile::tempdir().unwrap();
    let s = settings(work.path());
    for count in [0, MAX_COUNT + 1] {
        let mut req = input("cat");
        req.count = count;
        assert_eq!(build_spec(&req, &s, now()).unwrap_err().category, Category::InvalidRequest, "count {count}");
    }
    let mut req = input("cat");
    req.count = MAX_COUNT;
    assert!(build_spec(&req, &s, now()).is_ok());

    let small = work.path().join("ref.png");
    std::fs::write(&small, png_bytes(4, 4, [1, 2, 3, 255])).unwrap();
    let mut req = input("cat");
    req.references = vec![small.clone(); MAX_REFERENCES];
    let (spec, _) = build_spec(&req, &s, now()).unwrap();
    assert_eq!(spec["reference_images"].as_array().unwrap().len(), MAX_REFERENCES);
    req.references = vec![small.clone(); MAX_REFERENCES + 1];
    assert_eq!(build_spec(&req, &s, now()).unwrap_err().category, Category::InvalidRequest);

    let big = work.path().join("big.png");
    std::fs::File::create(&big).unwrap().set_len(11 * 1024 * 1024).unwrap();
    req.references = vec![big];
    let err = build_spec(&req, &s, now()).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("10MB"), "{}", err.message);

    req.references = vec![work.path().join("missing.png")];
    assert_eq!(build_spec(&req, &s, now()).unwrap_err().category, Category::InvalidRequest);

    let mut req = input("   ");
    assert_eq!(build_spec(&req, &s, now()).unwrap_err().category, Category::InvalidRequest);
    req.prompt = "cat".into();
    req.background = "opaque-ish".into();
    assert_eq!(build_spec(&req, &s, now()).unwrap_err().category, Category::InvalidRequest);
}

#[test]
fn resume_keeps_topic() {
    let work = tempfile::tempdir().unwrap();
    let mut req = input("완전히 다른 프롬프트");
    req.topic = Some("아늑한_독서_공간-20261017-090000".into());
    req.resume = true;
    let (spec, topic) = build_spec(&req, &settings(work.path()), now()).unwrap();
    assert_eq!(topic, "아늑한_독서_공간-20261017-090000");
    assert_eq!(spec["topic"], topic.as_str());
    assert_eq!(spec["resume"], true);
}

#[tokio::test]
async fn run_emits_events_in_order_and_summary() {
    let p = ok_provider(150);
    let s = Setup::new(&p.base_url());
    let mut req = input("a mug");
    req.count = 3;
    req.topic = Some("mugs".into());
    let (summary, events) = s.run(req, CancellationToken::new()).await;
    let summary = summary.unwrap();
    assert!(summary.ok && !summary.cancelled, "{summary:?}");
    assert_eq!(summary.outputs.len(), 3);
    assert!(summary.failures.is_empty());
    assert_eq!(summary.topic, "mugs");
    assert!(summary.output_dir.ends_with("mugs"), "{}", summary.output_dir);
    assert_eq!(p.requests().len(), 3);

    let names: Vec<&str> = events
        .iter()
        .map(|e| match e {
            RunEvent::Started { .. } => "started",
            RunEvent::ImageStarted { .. } => "image_started",
            RunEvent::ImageFinished { .. } => "image_finished",
            RunEvent::Finished { .. } => "finished",
            _ => "other",
        })
        .collect();
    assert_eq!(
        names,
        ["started", "image_started", "image_started", "image_started", "image_finished", "image_finished", "image_finished", "finished"]
    );
    match &events[0] {
        RunEvent::Started { run_id, total, output_dir, topic } => {
            assert_eq!(run_id, "run-test");
            assert_eq!(*total, 3);
            assert_eq!(output_dir, &summary.output_dir);
            assert_eq!(topic, "mugs");
        }
        other => panic!("{other:?}"),
    }
    let mut indexes: Vec<u32> =
        events.iter().filter_map(|e| if let RunEvent::ImageFinished { index, .. } = e { Some(*index) } else { None }).collect();
    indexes.sort();
    assert_eq!(indexes, [1, 2, 3]);
    match events.last().unwrap() {
        RunEvent::Finished { summary: last } => assert_eq!(last, &summary),
        other => panic!("{other:?}"),
    }

    // Wire format the screen reads: {"event": "imageFinished", "data": {"index", "output", "elapsedSec"}}.
    let wire = serde_json::to_value(&events[4]).unwrap();
    assert_eq!(wire["event"], "imageFinished");
    assert!(wire["data"]["elapsedSec"].is_number() && wire["data"]["output"].is_string(), "{wire}");
    let wire = serde_json::to_value(&events[0]).unwrap();
    assert_eq!(wire["data"]["runId"], "run-test");
    assert_eq!(wire["data"]["outputDir"], summary.output_dir.as_str());

    // The same topic with resume skips what is already complete.
    let mut again = input("a mug");
    again.count = 3;
    again.topic = Some("mugs".into());
    again.resume = true;
    let (summary, events) = s.run(again, CancellationToken::new()).await;
    assert!(summary.unwrap().ok);
    assert_eq!(p.requests().len(), 3);
    assert_eq!(events.iter().filter(|e| matches!(e, RunEvent::ImageSkipped { .. })).count(), 3);
    assert!(s.home.path().join("settings.json").is_file());
}

#[tokio::test]
async fn failure_carries_category() {
    let p = FakeProvider::start(|_| Reply::json(403, r#"{"error":{"code":"personal-team-blocked:spending-limit","message":"limit"}}"#));
    let s = Setup::new(&p.base_url());
    let (summary, events) = s.run(input("a mug"), CancellationToken::new()).await;
    let summary = summary.unwrap();
    assert!(!summary.ok);
    assert_eq!(summary.failures.len(), 1);
    let failure = &summary.failures[0];
    assert_eq!(failure.index, 1);
    assert_eq!(failure.category, Some(Category::Quota));
    assert_eq!(failure.provider_code.as_deref(), Some("personal-team-blocked:spending-limit"));
    assert!(!failure.reason.is_empty());
    assert!(events.iter().any(|e| matches!(e, RunEvent::ImageFailed { index: 1, .. })));
    let wire = serde_json::to_value(&summary).unwrap();
    assert_eq!(wire["failures"][0]["category"], "quota");
    assert_eq!(wire["failures"][0]["providerCode"], "personal-team-blocked:spending-limit");
}

#[tokio::test]
async fn cancel_marks_summary_cancelled_and_leaves_no_lock() {
    let p = ok_provider(3000);
    let s = Setup::new(&p.base_url());
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let mut req = input("a mug");
    req.count = 2;
    req.topic = Some("cancelled".into());
    let started = std::time::Instant::now();
    let (summary, events) = s.run(req, cancel).await;
    let summary = summary.unwrap();
    assert!(started.elapsed() < Duration::from_millis(2500), "cancel did not stop the wait");
    assert!(summary.cancelled && !summary.ok, "{summary:?}");
    assert!(summary.outputs.is_empty());
    assert_eq!(summary.failures.len(), 2);
    assert!(summary.failures.iter().all(|f| f.category == Some(Category::Cancelled)));
    assert!(matches!(events.last(), Some(RunEvent::Finished { .. })));
    let metadata = s.files_under("ImageGallery/metadata/cancelled");
    assert!(!metadata.iter().any(|f| f.to_string_lossy().ends_with(".lock")), "{metadata:?}");
    assert!(s.files_under("ImageGallery/output/cancelled").iter().all(|f| !f.to_string_lossy().ends_with(".png")));
}

#[tokio::test]
async fn transparent_on_unsupported_model_fails_before_call() {
    let p = ok_provider(0);
    let s = Setup::new(&p.base_url());
    let mut req = input("a sticker");
    req.background = "transparent".into();
    req.image_model = "gpt-image-2".into();
    let (result, events) = s.run(req, CancellationToken::new()).await;
    let err = result.unwrap_err();
    assert_eq!(err.category, Category::Unsupported, "{err:?}");
    assert!(err.message.contains("transparent"), "{}", err.message);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(p.requests().len(), 0);
    assert!(s.files_under("ImageGallery").is_empty());
}

#[tokio::test]
async fn unknown_profile_fails_before_started() {
    let p = ok_provider(0);
    let s = Setup::new(&p.base_url());
    let mut req = input("a mug");
    req.profile = Some("없는 연결".into());
    let (result, events) = s.run(req, CancellationToken::new()).await;
    assert_eq!(result.unwrap_err().category, Category::InvalidRequest);
    assert!(events.is_empty());
    assert_eq!(p.requests().len(), 0);
}
