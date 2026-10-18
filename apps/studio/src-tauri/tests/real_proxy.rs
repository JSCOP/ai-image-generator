//! Real provider run through the app's generation service (stage 3 Task 9). Ignored by default:
//! `cargo test -p studio --test real_proxy -- --ignored --test-threads=1` makes ONE real low-quality image.
//!
//! Uses the user's existing `local` profile (profiles.json copied read-only into a temp app home, key read
//! from Credential Manager). The real app home and settings are never written; the image goes to
//! `<repo>/ImageGallery/output/studio-app-smoke-20261018/` and an existing folder there is never touched.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use image_core::connection::SystemEnv;
use image_core::paths::{absolute, metadata_path, topic_dirs};
use image_core::profiles::ProfileStore;
use image_core::runner::{run_spec_with, Progress, RunOptions};
use image_core::secrets::KeyringStore;
use serde_json::{json, Value};
use studio_lib::service::generate::{build_spec, run_generation, GenerateInput, RunEvent};
use studio_lib::service::settings::{load_settings, save_settings, Settings};
use studio_lib::service::Ctx;
use test_fixture::MapEnv;
use tokio_util::sync::CancellationToken;

const PROFILE: &str = "local";
const TOPIC: &str = "studio-app-smoke-20261018";
const MODEL: &str = "gpt-image-2.5";

fn repo_root() -> PathBuf {
    absolute(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

fn input() -> GenerateInput {
    GenerateInput {
        profile: Some(PROFILE.into()),
        prompt: "A small ceramic teacup on a wooden desk by a sunny window, soft morning light".into(),
        avoid: "text, watermark".into(),
        image_model: MODEL.into(),
        size: "1024x1024".into(),
        quality: "low".into(),
        count: 1,
        references: Vec::new(),
        background: "auto".into(),
        topic: Some(TOPIC.into()),
        resume: false,
    }
}

/// Temp app home holding a copy of the real profiles.json and settings whose work folder is this repository.
fn setup() -> (tempfile::TempDir, Ctx) {
    let real_home = ProfileStore::default_dir(&SystemEnv).expect("APPDATA or IMAGE_STUDIO_HOME");
    let profiles = std::fs::read(real_home.join("profiles.json")).expect("existing profiles.json with the `local` profile");
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("profiles.json"), profiles).unwrap();
    ProfileStore::open(home.path().to_path_buf()).find(PROFILE).expect("profile `local`");
    save_settings(home.path(), &Settings { work_dir: repo_root(), ..Settings::default() }).unwrap();
    let secrets = Arc::new(KeyringStore::system(&SystemEnv).expect("Credential Manager"));
    // Only the named profile is used: no environment connection, CLI default retry backoff.
    let env = MapEnv::new(&[], &[]);
    let ctx = Ctx { home: home.path().to_path_buf(), env: Arc::new(env), secrets };
    (home, ctx)
}

fn output_dir() -> PathBuf {
    topic_dirs(&repo_root(), TOPIC).0
}

#[tokio::test]
#[ignore = "real provider: offline dry run of the smoke request"]
async fn real_proxy_dry_run() {
    let (_home, ctx) = setup();
    let settings = load_settings(&ctx.home);
    let (mut spec, topic) = build_spec(&input(), &settings, chrono::Local::now()).unwrap();
    assert_eq!(topic, TOPIC);
    spec["dry_run"] = json!(true);
    let mut opts = RunOptions::new(5.0, settings.work_dir.clone());
    opts.profiles = Some(Arc::new(ProfileStore::open(ctx.home.clone())));
    opts.secrets = ctx.secrets.clone();
    let quiet: Progress = Arc::new(|_: Value| {});
    let result = run_spec_with(spec, ctx.env.clone(), quiet, CancellationToken::new(), opts).await.result;
    println!("dry run: {}", serde_json::to_string_pretty(&result).unwrap());
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(result["ok"], json!(true), "{result}");
}

#[tokio::test]
#[ignore = "real provider: makes one low-quality 1024x1024 image"]
async fn real_proxy_one_image() {
    assert!(!output_dir().exists(), "{} exists; never overwrite earlier results", output_dir().display());
    let (_home, ctx) = setup();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let summary = run_generation(&ctx, input(), "run-real-proxy".into(), move |e| sink.lock().unwrap().push(e), CancellationToken::new())
        .await
        .expect("generation starts");
    let events = events.lock().unwrap().clone();
    println!("summary: {}", serde_json::to_string_pretty(&summary).unwrap());
    assert!(summary.ok, "{summary:?}");
    assert_eq!(summary.outputs.len(), 1);
    assert!(matches!(events.first(), Some(RunEvent::Started { .. })), "{events:?}");
    assert!(events.iter().any(|e| matches!(e, RunEvent::ImageFinished { .. })), "{events:?}");
    assert!(matches!(events.last(), Some(RunEvent::Finished { .. })), "{events:?}");

    let image = PathBuf::from(&summary.outputs[0]);
    assert!(image.starts_with(output_dir()), "{}", image.display());
    let decoded = image::open(&image).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1024, 1024));
    let record_path = metadata_path(&image, &repo_root(), TOPIC);
    let record: Value = serde_json::from_str(&std::fs::read_to_string(&record_path).unwrap()).unwrap();
    println!("record image_model={} response_model={} status={}", record["image_model"], record["response_model"], record["status"]);
    assert_eq!(record["image_model"], json!(MODEL));
    assert_eq!(record["status"], json!("complete"));
}
