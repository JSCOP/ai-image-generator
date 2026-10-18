use image_core::connection::EnvSource;
use image_core::runner::{run_spec_with, Progress, RunOptions, RunOutcome};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use test_fixture::{closed_base_url, openai_image_reply, png_bytes, FakeProvider, MapEnv, Reply};
use tokio_util::sync::CancellationToken;

struct Run {
    tmp: tempfile::TempDir,
    events: Arc<Mutex<Vec<Value>>>,
}

impl Run {
    fn new() -> Run {
        Run { tmp: tempfile::tempdir().unwrap(), events: Arc::new(Mutex::new(Vec::new())) }
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    async fn go(&self, mut spec: Value, env: impl EnvSource + 'static, cancel: CancellationToken) -> RunOutcome {
        if spec.get("topic_root").is_none() {
            spec["topic_root"] = json!(self.root().display().to_string());
        }
        let sink = self.events.clone();
        let progress: Progress = Arc::new(move |event| sink.lock().unwrap().push(event));
        let opts = RunOptions { backoff_base_sec: 0.0, cwd: self.root().to_path_buf() };
        run_spec_with(spec, Arc::new(env), progress, cancel, opts).await
    }

    fn events(&self, name: &str) -> Vec<Value> {
        self.events.lock().unwrap().iter().filter(|e| e["event"] == name).cloned().collect()
    }

    fn files_under(&self, dir: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.root().join(dir))
            .map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }
}

fn ok_provider(delay_ms: u64) -> FakeProvider {
    let png = png_bytes(16, 16, [9, 9, 9, 255]);
    FakeProvider::start(move |_| openai_image_reply(&png).delayed(Duration::from_millis(delay_ms)))
}

fn spec(extra: Value) -> Value {
    let mut base = json!({"prompt": "a mug", "topic": "t", "size": "16x16", "quality": "low"});
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

#[tokio::test]
async fn single_count_three_names_and_parallelism() {
    let p = ok_provider(300);
    let r = Run::new();
    let out = r.go(spec(json!({"count": 3})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.exit_code, 0, "{}", out.result);
    let res = &out.result;
    assert_eq!(res["ok"], true);
    assert_eq!(res["mode"], "single");
    let outputs: Vec<&str> = res["outputs"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(outputs.len(), 3);
    assert!(outputs[0].ends_with("t_001.png") && outputs[2].ends_with("t_003.png"));
    assert_eq!(res["concurrency"], 3);
    assert_eq!(res["retried"], 0);
    assert_eq!(res["connection_source"], "process_env");
    assert_eq!(p.peak_concurrency(), 3);
    assert_eq!(r.events("image_started").len(), 3);
    assert_eq!(r.events("image_finished").len(), 3);
    assert!(r.events("image_finished")[0]["ts"].is_number());
    assert_eq!(r.files_under("ImageGallery/output/t"), vec!["t_001.png", "t_002.png", "t_003.png"]);
    assert_eq!(r.files_under("ImageGallery/metadata/t"), vec!["t_001.png.json", "t_002.png.json", "t_003.png.json"]);
}

#[tokio::test]
async fn thirty_images_run_at_once_and_clamp() {
    let p = ok_provider(400);
    let r = Run::new();
    let out = r.go(spec(json!({"count": 30})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["concurrency"], 30);
    assert_eq!(p.peak_concurrency(), 30);
    let r2 = Run::new();
    let out = r2.go(spec(json!({"count": 40, "concurrency": 99, "topic": "u"})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["concurrency"], 32);
    assert_eq!(out.result["outputs"].as_array().unwrap().len(), 40);
}

#[tokio::test]
async fn transient_then_success_counts_retried() {
    let png = png_bytes(16, 16, [9, 9, 9, 255]);
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let p = FakeProvider::start(move |_| if c.fetch_add(1, Ordering::SeqCst) == 0 { Reply::json(429, "{}") } else { openai_image_reply(&png) });
    let r = Run::new();
    let out = r.go(spec(json!({})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    assert_eq!(out.result["retried"], 1);
    let retry = r.events("retry");
    assert_eq!(retry.len(), 1);
    assert_eq!((retry[0]["attempt"].clone(), retry[0]["retries"].clone()), (json!(1), json!(2)));
    assert!(retry[0]["reason"].as_str().unwrap().contains("429"));
}

#[tokio::test]
async fn permanent_errors_are_not_retried() {
    let p = FakeProvider::start(|_| Reply::json(400, r#"{"error":{"message":"bad prompt"}}"#));
    let r = Run::new();
    let out = r.go(spec(json!({})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.exit_code, 2);
    assert_eq!(p.requests().len(), 1);
    let failure = &out.result["failures"][0];
    assert_eq!(failure["index"], 1);
    assert_eq!(failure["error"]["category"], "invalid_request");
    assert!(failure["reason"].as_str().unwrap().contains("bad prompt"));
    assert_eq!(r.events("image_failed").len(), 1);
}

#[tokio::test]
async fn proxy_down_fails_every_image_cleanly() {
    let r = Run::new();
    let out = r.go(spec(json!({"count": 5, "retries": 1})), MapEnv::proxy(&closed_base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["ok"], false);
    let failures = out.result["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 5);
    assert!(failures.iter().all(|f| f["error"]["category"] == "network"));
    assert_eq!(out.result["retried"], 5);
    assert!(r.files_under("ImageGallery/output/t").is_empty());
    assert!(r.files_under("ImageGallery/metadata/t").iter().all(|n| n.ends_with(".json")));
}

#[tokio::test]
async fn per_image_timeout_is_retryable() {
    let p = ok_provider(2000);
    let r = Run::new();
    let out = r.go(spec(json!({"job_timeout_sec": 0.2, "retries": 1})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["failures"][0]["reason"], "timed out after 0.2s");
    assert_eq!(out.result["retried"], 1);
}

#[tokio::test]
async fn jobs_mode_overrides_prefix_suffix_and_collisions() {
    let png = png_bytes(16, 16, [9, 9, 9, 255]);
    let p = FakeProvider::start(move |_| openai_image_reply(&png));
    let r = Run::new();
    let jobs = json!({
        "mode": "jobs", "topic": "set", "size": "16x16", "quality": "low",
        "prompt_prefix": "PRE", "prompt_suffix": "POST",
        "jobs": [
            {"id": "b/2", "prompt": "second", "image_model": "grok-imagine-image-2.0"},
            {"id": "a 1", "prompt": "first", "size": "32x16"},
        ],
    });
    let out = r.go(jobs, MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    assert_eq!(out.result["mode"], "jobs");
    let ids: Vec<&str> = out.result["outputs"].as_array().unwrap().iter().map(|o| o["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["a 1", "b/2"]);
    assert_eq!(r.files_under("ImageGallery/output/set"), vec!["a_1.png", "b_2.png"]);
    let bodies: Vec<Value> = p.requests().iter().map(|q| q.json()).collect();
    let first = bodies.iter().find(|b| b["prompt"] == "PRE\nfirst\nPOST").expect("prefix/suffix joined with newlines");
    assert_eq!(first["size"], "32x16");
    assert!(bodies.iter().any(|b| b["model"] == "grok-imagine-image-2.0"));
    let collide = json!({"mode": "jobs", "jobs": [{"id": "a/b", "prompt": "x"}, {"id": "A_b", "prompt": "y"}]});
    let out = r.go(collide, MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["error"], "job IDs collide after filename normalization");
}

#[tokio::test]
async fn dry_run_makes_no_calls_and_no_dirs() {
    let p = ok_provider(0);
    let r = Run::new();
    let out = r.go(spec(json!({"count": 2, "dry_run": true})), MapEnv::new(&[], &[]), CancellationToken::new()).await;
    assert_eq!(out.result["dry_run"], true);
    let planned = out.result["planned_outputs"].as_array().unwrap();
    assert!(planned[1].as_str().unwrap().ends_with("t_002.png"));
    let jobs = json!({"mode": "jobs", "dry_run": true, "jobs": [{"id": "x", "prompt": "p"}]});
    let out = r.go(jobs, MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["planned_outputs"][0]["id"], "x");
    assert_eq!(out.result["outputs"], json!([]));
    assert!(p.requests().is_empty());
    assert!(!r.root().join("ImageGallery").exists());
}

#[tokio::test]
async fn validation_errors_match_python_shape() {
    let p = ok_provider(0);
    let r = Run::new();
    let env = || MapEnv::proxy(&p.base_url());
    let out = r.go(json!({"topic": "t"}), env(), CancellationToken::new()).await;
    assert_eq!((out.exit_code, out.result["error"].clone()), (2, json!("spec.prompt is required when mode=single")));
    let out = r.go(json!({"mode": "x"}), env(), CancellationToken::new()).await;
    assert_eq!((out.exit_code, out.result["error"].clone()), (1, json!("unknown mode 'x'; use 'single', 'jobs' or 'batch'")));
    let out = r.go(spec(json!({"quality": "ultra"})), env(), CancellationToken::new()).await;
    assert_eq!(out.result["error"], "invalid quality 'ultra'");
    let out = r.go(spec(json!({"action": "paint"})), env(), CancellationToken::new()).await;
    assert_eq!(out.result["error"], "invalid action 'paint'; expected auto, generate, or edit");
    let out = r.go(spec(json!({"concurrency": "many"})), env(), CancellationToken::new()).await;
    assert_eq!(out.result["error"], "job_timeout_sec, concurrency, and retries must be numbers");
    for extra in [json!({"profile": "home"}), json!({"mask": "m.png"}), json!({"keep_outside_mask": true}), json!({"remove_background": true})] {
        let out = r.go(spec(extra.clone()), env(), CancellationToken::new()).await;
        assert_eq!((out.exit_code, out.result["error_category"].clone()), (2, json!("unsupported")), "{extra}");
    }
    let out = r.go(spec(json!({"remove_background": false})), env(), CancellationToken::new()).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    for extra in [json!({"background": "rainbow"}), json!({"fit": "stretch"})] {
        let out = r.go(spec(extra), env(), CancellationToken::new()).await;
        assert_eq!(out.result["error_category"], "invalid_request");
    }
    let out = r.go(spec(json!({"background": "transparent", "topic": "tr"})), env(), CancellationToken::new()).await;
    assert_eq!(out.result["error_category"], "unsupported");
    let calls = p.requests().len();
    let out = r.go(spec(json!({"topic": "noconn"})), MapEnv::new(&[], &[]), CancellationToken::new()).await;
    assert_eq!(out.exit_code, 2);
    assert!(out.result["error"].as_str().unwrap().contains("CLIPROXY_BASE_URL"));
    assert_eq!(p.requests().len(), calls);
}

#[tokio::test]
async fn relative_reference_paths_resolve_against_cwd() {
    let p = ok_provider(0);
    let r = Run::new();
    std::fs::write(r.root().join("ref.png"), png_bytes(4, 4, [1, 1, 1, 255])).unwrap();
    let out = r.go(spec(json!({"reference_images": ["ref.png"], "action": "edit"})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    assert_eq!(p.requests()[0].path, "/v1/images/edits");
    let out = r.go(spec(json!({"reference_images": ["nope.png"], "topic": "n"})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert!(out.result["error"].as_str().unwrap().starts_with("reference image not found: "));
    assert_eq!(p.requests().len(), 1);
}

#[tokio::test]
async fn model_key_is_ignored_with_warning() {
    let p = ok_provider(0);
    let r = Run::new();
    let out = r.go(spec(json!({"model": "gpt-5.5"})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(out.result["ok"], true);
    assert_eq!(out.result["warnings"].as_array().unwrap().len(), 1);
    assert!(p.requests()[0].json()["model"] == "gpt-image-2.5");
}

#[tokio::test]
async fn cancel_stops_new_work_and_keeps_finished() {
    let p = ok_provider(400);
    let r = Run::new();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        trigger.cancel();
    });
    let out = r.go(spec(json!({"count": 4, "concurrency": 2})), MapEnv::proxy(&p.base_url()), cancel).await;
    assert_eq!(out.result["outputs"].as_array().unwrap().len(), 2, "{}", out.result);
    let failures = out.result["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 2);
    assert!(failures.iter().all(|f| f["error"]["category"] == "cancelled"));
    assert_eq!(r.files_under("ImageGallery/output/t").len(), 2);
    assert!(!r.files_under("ImageGallery/metadata/t").iter().any(|n| n.ends_with(".lock")));
}

#[tokio::test]
async fn resume_after_cancel_regenerates_only_missing() {
    let p = ok_provider(0);
    let r = Run::new();
    let first = r.go(spec(json!({"count": 2})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(first.result["ok"], true);
    std::fs::remove_file(r.root().join("ImageGallery/output/t/t_002.png")).unwrap();
    let again = r.go(spec(json!({"count": 2, "resume": true})), MapEnv::proxy(&p.base_url()), CancellationToken::new()).await;
    assert_eq!(again.result["ok"], true, "{}", again.result);
    assert_eq!(p.requests().len(), 3);
    assert_eq!(r.events("image_skipped").len(), 1);
}
