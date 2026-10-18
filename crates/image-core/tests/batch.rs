use image_core::batch::{plan_batch, PyRandom};
use image_core::runner::{run_spec_with, Progress, RunOptions};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use test_fixture::{FakeProvider, MapEnv, Reply};
use tokio_util::sync::CancellationToken;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn pyrandom_matches_cpython() {
    // Values from CPython: random.Random(seed)._randbelow(n)
    let mut r = PyRandom::from_str_seed("42:extraction-rpg:1");
    assert_eq!([r.randbelow(10), r.randbelow(1000), r.randbelow(7), r.randbelow(1 << 40)], [2, 732, 6, 855731928170]);
    let mut r = PyRandom::from_str_seed("");
    assert_eq!([r.randbelow(10), r.randbelow(1000)], [7, 609]);
    let mut r = PyRandom::from_str_seed("공항:1");
    assert_eq!([r.randbelow(3), r.randbelow(100)], [2, 24]);
}

#[test]
fn plan_matches_gen_batch_dry_run() {
    let expected: Vec<Value> = serde_json::from_str(include_str!("fixtures/batch_extraction_rpg_5.json")).unwrap();
    let ws = tempfile::tempdir().unwrap();
    let plans = plan_batch(&repo_root().join("presets/extraction-rpg.json"), 5, 42, None, None, ws.path()).unwrap();
    assert_eq!(plans.len(), 5);
    for (plan, want) in plans.iter().zip(&expected) {
        assert_eq!(plan.id, want["id"].as_str().unwrap());
        assert_eq!(plan.prompt, want["prompt"].as_str().unwrap());
        assert_eq!(plan.category, want["category"].as_str().unwrap());
        assert_eq!(plan.image_model, "gpt-image-2.5");
        assert!(plan.output.ends_with(&format!("{}.png", plan.id)));
    }
    let overridden = plan_batch(&repo_root().join("presets/extraction-rpg.json"), 1, 42, Some("other topic"), Some("gemini-3.1-flash-image"), ws.path()).unwrap();
    assert_eq!(overridden[0].id, "other_topic_000001");
    assert_eq!(overridden[0].image_model, "gemini-3.1-flash-image");
}

#[tokio::test]
async fn batch_dry_run_and_stop_after_max_failures() {
    let p = FakeProvider::start(|_| Reply::json(400, r#"{"error":{"message":"no"}}"#));
    let ws = tempfile::tempdir().unwrap();
    let progress: Progress = Arc::new(|_| {});
    let preset = repo_root().join("presets/extraction-rpg.json").display().to_string();
    let run = |spec: Value| {
        let opts = RunOptions { backoff_base_sec: 0.0, cwd: ws.path().to_path_buf() };
        run_spec_with(spec, Arc::new(MapEnv::proxy(&p.base_url())), progress.clone(), CancellationToken::new(), opts)
    };
    let root = ws.path().display().to_string();
    let dry = run(json!({"mode": "batch", "preset": preset, "count": 3, "dry_run": true, "topic_root": root})).await;
    assert_eq!(dry.exit_code, 0, "{}", dry.result);
    assert_eq!(dry.result["mode"], "batch");
    assert_eq!(dry.result["planned_outputs"].as_array().unwrap().len(), 3);
    assert_eq!(dry.result["planned_outputs"][0]["category"], "gameplay_barbarian");
    assert!(p.requests().is_empty());
    let out = run(json!({"mode": "batch", "preset": preset, "count": 25, "concurrency": 1, "retries": 0, "topic_root": root})).await;
    assert_eq!(out.exit_code, 2);
    assert_eq!(p.requests().len(), 20);
    let failures = out.result["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 25);
    assert_eq!(failures.iter().filter(|f| f["error"]["category"] == "cancelled").count(), 5);
    let missing = run(json!({"mode": "batch", "topic_root": root})).await;
    assert_eq!(missing.result["error"], "spec.preset is required when mode=batch");
    let absent = run(json!({"mode": "batch", "preset": "nope.json", "topic_root": root})).await;
    assert!(absent.result["error"].as_str().unwrap().starts_with("preset not found: "));
}
