//! Input validation before any paid call, and Python contract details (review F5, F7, F8, F9, F10).

use image_core::connection::EnvSource;
use image_core::runner::{run_spec_with, Progress, RunOptions, RunOutcome};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use test_fixture::{gemini_image_reply, openai_image_reply, png_bytes, FakeProvider, MapEnv, Reply};
use tokio_util::sync::CancellationToken;

async fn go(root: &Path, spec: Value, env: impl EnvSource + 'static) -> RunOutcome {
    go_with(root, spec, env, 0.0).await
}

async fn go_with(root: &Path, mut spec: Value, env: impl EnvSource + 'static, backoff: f64) -> RunOutcome {
    if spec.get("topic_root").is_none() {
        spec["topic_root"] = json!(root.display().to_string());
    }
    let progress: Progress = Arc::new(|_| {});
    let opts = RunOptions::new(backoff, root.to_path_buf());
    run_spec_with(spec, Arc::new(env), progress, CancellationToken::new(), opts).await
}

fn ok_provider() -> FakeProvider {
    let png = png_bytes(16, 16, [9, 9, 9, 255]);
    FakeProvider::start(move |r| {
        if r.path.contains("generateContent") {
            gemini_image_reply(&png, "gemini-3.1-flash-image")
        } else {
            openai_image_reply(&png)
        }
    })
}

fn spec(extra: Value) -> Value {
    let mut base = json!({"prompt": "a mug", "topic": "t", "size": "16x16", "quality": "low"});
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn assert_invalid(out: &RunOutcome) {
    assert_eq!(out.result["ok"], false, "{}", out.result);
    assert_eq!(out.result["error_category"], "invalid_request", "{}", out.result);
    assert!(out.result["error"].is_string(), "{}", out.result);
}

// F5: reference_images must be an array of strings.

#[tokio::test]
async fn single_reference_images_string_is_refused_without_a_request() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    for bad in [json!("missing.png"), json!([1]), json!(["a.png", null]), json!({"path": "a.png"}), json!(true)] {
        let out = go(tmp.path(), spec(json!({"reference_images": bad, "action": "edit"})), MapEnv::proxy(&p.base_url())).await;
        assert_invalid(&out);
        assert!(out.result["error"].as_str().unwrap().contains("reference_images"), "{}", out.result);
        let out = go(tmp.path(), spec(json!({"reference_images": bad})), MapEnv::proxy(&p.base_url())).await;
        assert_invalid(&out);
    }
    assert_eq!(p.requests().len(), 0);
}

#[tokio::test]
async fn jobs_reference_images_must_be_arrays() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let top = json!({"mode": "jobs", "size": "16x16", "quality": "low", "reference_images": "missing.png",
                     "jobs": [{"id": "a", "prompt": "p"}]});
    let out = go(tmp.path(), top, MapEnv::proxy(&p.base_url())).await;
    assert_invalid(&out);
    let per_job = json!({"mode": "jobs", "size": "16x16", "quality": "low",
                         "jobs": [{"id": "a", "prompt": "p", "reference_images": "missing.png"}]});
    let out = go(tmp.path(), per_job, MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["ok"], false, "{}", out.result);
    let failures = out.result["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1, "{}", out.result);
    assert_eq!(failures[0]["error"]["category"], "invalid_request");
    assert_eq!(p.requests().len(), 0);
}

#[tokio::test]
async fn null_or_empty_reference_images_still_generate() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let out = go(tmp.path(), spec(json!({"reference_images": null})), MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    let out = go(tmp.path(), spec(json!({"reference_images": [], "topic": "u"})), MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    assert_eq!(p.requests().len(), 2);
}

// F7: model ids cannot change the request path.

const BAD_MODELS: [&str; 10] = [
    "gemini-image/../../../admin/restart#",
    "gemini-x-image/../../admin",
    "gemini-3.1-flash-image#frag",
    "gemini-3.1-flash-image?x=1",
    "gemini-3.1-flash-image%2F..%2Fadmin",
    "gemini-3.1-flash image",
    "gemini-3.1-flash-image:streamGenerateContent",
    "gemini-..-image",
    "gpt-image-2/../../admin",
    "grok-imagine-image\n",
];

#[tokio::test]
async fn path_changing_model_ids_make_no_request() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    for model in BAD_MODELS {
        let out = go(tmp.path(), spec(json!({"image_model": model})), MapEnv::proxy(&p.base_url())).await;
        assert_invalid(&out);
        let dry = go(tmp.path(), spec(json!({"image_model": model, "dry_run": true})), MapEnv::proxy(&p.base_url())).await;
        assert_invalid(&dry);
        let jobs = json!({"mode": "jobs", "size": "16x16", "quality": "low", "jobs": [{"id": "a", "prompt": "p", "image_model": model}]});
        let out = go(tmp.path(), jobs, MapEnv::proxy(&p.base_url())).await;
        assert_eq!(out.result["failures"][0]["error"]["category"], "invalid_request", "{model}: {}", out.result);
    }
    assert_eq!(p.requests().len(), 0, "{:?}", p.requests().iter().map(|r| r.path.clone()).collect::<Vec<_>>());
}

#[tokio::test]
async fn normal_model_ids_keep_their_paths() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let out = go(tmp.path(), spec(json!({"image_model": "gemini-3.1-flash-image", "topic": "g"})), MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    let out = go(tmp.path(), spec(json!({"image_model": "gpt-image-2.5", "topic": "o"})), MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    let paths: Vec<String> = p.requests().iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, vec!["/v1beta/models/gemini-3.1-flash-image:generateContent", "/v1/images/generations"]);
}

// F8: out-of-range seconds are a JSON error, never a panic.

#[tokio::test]
async fn job_timeout_out_of_range_is_invalid_request() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    for bad in [json!(1e100), json!(86401), json!("NaN"), json!("inf"), json!("-inf")] {
        for dry in [true, false] {
            let out = go(tmp.path(), spec(json!({"job_timeout_sec": bad, "dry_run": dry})), MapEnv::proxy(&p.base_url())).await;
            assert_invalid(&out);
            assert!(out.result["error"].as_str().unwrap().contains("job_timeout_sec"), "{}", out.result);
        }
        let jobs = json!({"mode": "jobs", "job_timeout_sec": bad, "jobs": [{"id": "a", "prompt": "p"}]});
        assert_invalid(&go(tmp.path(), jobs, MapEnv::proxy(&p.base_url())).await);
    }
    let env = MapEnv::new(&[("CLIPROXY_BASE_URL", &p.base_url()), ("CLIPROXY_API_KEY", "k"), ("CLIPROXY_TIMEOUT", "1e100")], &[]);
    assert_invalid(&go(tmp.path(), spec(json!({"dry_run": true})), env).await);
    assert_eq!(p.requests().len(), 0);
}

#[tokio::test]
async fn job_timeout_zero_or_negative_disables_like_python() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    for (i, value) in [json!(0), json!(-5), json!(86400)].into_iter().enumerate() {
        let out = go(tmp.path(), spec(json!({"job_timeout_sec": value, "topic": format!("t{i}")})), MapEnv::proxy(&p.base_url())).await;
        assert_eq!(out.result["ok"], true, "{}", out.result);
    }
}

#[tokio::test]
async fn backoff_out_of_range_is_refused_before_any_request() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    for bad in [f64::NAN, f64::INFINITY, -1.0, 1e300] {
        let out = go_with(tmp.path(), spec(json!({})), MapEnv::proxy(&p.base_url()), bad).await;
        assert_eq!(out.result["ok"], false, "{}", out.result);
        assert!(out.result["error"].as_str().unwrap().contains("AI_IMAGE_RETRY_BACKOFF_SEC"), "{}", out.result);
        assert_eq!(out.exit_code, 1);
    }
    assert_eq!(p.requests().len(), 0);
}

// F9: "~" expands to the user's home like Python's expanduser.

#[tokio::test]
async fn tilde_topic_root_and_references_expand_to_home() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join("refs")).unwrap();
    std::fs::write(home.join("refs/r.png"), png_bytes(8, 8, [1, 2, 3, 255])).unwrap();
    let env = || {
        MapEnv::new(
            &[
                ("CLIPROXY_BASE_URL", &p.base_url()),
                ("CLIPROXY_API_KEY", "k"),
                ("USERPROFILE", &home.display().to_string()),
                ("HOME", &home.display().to_string()),
            ],
            &[],
        )
    };
    let dry = spec(json!({"topic_root": "~/Pictures", "topic": "path-review", "dry_run": true}));
    let out = go(tmp.path(), dry, env()).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    let expected = image_core::paths::absolute(&home.join("Pictures/ImageGallery/output/path-review"));
    assert_eq!(out.result["output_dir"], json!(expected.display().to_string()));
    let edit = spec(json!({"topic_root": "~", "action": "edit", "reference_images": ["~/refs/r.png"]}));
    let out = go(tmp.path(), edit, env()).await;
    assert_eq!(out.result["ok"], true, "{}", out.result);
    assert!(home.join("ImageGallery/output/t/t.png").is_file());
    assert_eq!(p.requests().len(), 1);
}

// F10: single-mode failures are sorted by index, as in Python.

#[tokio::test]
async fn single_failures_are_sorted_by_index() {
    // Each later request answers sooner, so completion order is the reverse of arrival order.
    let arrivals = Arc::new(AtomicUsize::new(0));
    let p = FakeProvider::start(move |_| {
        let n = arrivals.fetch_add(1, Ordering::SeqCst);
        Reply::json(400, r#"{"error":{"message":"rejected"}}"#).delayed(Duration::from_millis(600u64.saturating_sub(n as u64 * 250)))
    });
    let tmp = tempfile::tempdir().unwrap();
    let out = go(tmp.path(), spec(json!({"count": 3, "concurrency": 3, "retries": 0})), MapEnv::proxy(&p.base_url())).await;
    assert_eq!(out.result["ok"], false);
    let order: Vec<i64> = out.result["failures"].as_array().unwrap().iter().map(|f| f["index"].as_i64().unwrap()).collect();
    assert_eq!(order, vec![1, 2, 3], "{}", out.result);
}

#[test]
fn expand_user_follows_python() {
    use image_core::paths::expand_user;
    let none = MapEnv::new(&[], &[]);
    assert_eq!(expand_user("~/x", &none), "~/x");
    assert_eq!(expand_user("a/~/x", &none), "a/~/x");
    if cfg!(windows) {
        let env = MapEnv::new(&[("USERPROFILE", r"C:\Users\me"), ("USERNAME", "me")], &[]);
        assert_eq!(expand_user("~", &env), r"C:\Users\me");
        assert_eq!(expand_user("~/Pictures", &env), r"C:\Users\me/Pictures");
        assert_eq!(expand_user(r"~\Pictures", &env), r"C:\Users\me\Pictures");
        assert_eq!(expand_user("~me/x", &env), r"C:\Users\me/x");
        assert_eq!(expand_user("~other/x", &env), r"C:\Users\other/x");
        let odd = MapEnv::new(&[("USERPROFILE", r"D:\profile"), ("USERNAME", "me")], &[]);
        assert_eq!(expand_user("~other/x", &odd), "~other/x");
        let drive = MapEnv::new(&[("HOMEDRIVE", "C:"), ("HOMEPATH", r"\Users\me")], &[]);
        assert_eq!(expand_user("~/x", &drive), r"C:\Users\me/x");
    } else {
        let env = MapEnv::new(&[("HOME", "/home/me")], &[]);
        assert_eq!(expand_user("~/x", &env), "/home/me/x");
        assert_eq!(expand_user("~other/x", &env), "~other/x");
    }
}
