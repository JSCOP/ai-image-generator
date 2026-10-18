use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Output, Stdio};
use test_fixture::{openai_image_reply, png_bytes, FakeProvider, Reply};

const BIN: &str = env!("CARGO_BIN_EXE_image-cli");

fn cli(base_url: &str, dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(dir)
        .env("CLIPROXY_BASE_URL", base_url)
        .env("CLIPROXY_API_KEY", "fake-token")
        .env("AI_IMAGE_RETRY_BACKOFF_SEC", "0")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CLIPROXY_IMAGE_MODEL")
        .env_remove("CLIPROXY_TIMEOUT");
    cmd
}

fn run_stdin(mut cmd: Command, input: &str) -> Output {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn last_json(out: &Output) -> Value {
    let stdout = String::from_utf8(out.stdout.clone()).unwrap();
    serde_json::from_str(stdout.lines().last().expect("a JSON line")).unwrap()
}

fn ok_provider() -> FakeProvider {
    let png = png_bytes(16, 16, [3, 3, 3, 255]);
    FakeProvider::start(move |_| openai_image_reply(&png))
}

fn spec(dir: &std::path::Path, topic: &str) -> String {
    json!({"prompt": "a mug", "topic": topic, "size": "16x16", "quality": "low", "topic_root": dir.display().to_string()}).to_string()
}

#[test]
fn stdout_has_exactly_one_json_line_and_progress_goes_to_stderr() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let out = run_stdin(cli(&p.base_url(), tmp.path()), &spec(tmp.path(), "t"));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert_eq!(last_json(&out)["ok"], true);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.lines().count() >= 2);
    for line in stderr.lines() {
        let event: Value = serde_json::from_str(line).expect("stderr lines are JSON events");
        assert!(event["event"].is_string());
    }
}

#[test]
fn exit_codes() {
    let p = FakeProvider::start(|_| Reply::json(400, "{}"));
    let tmp = tempfile::tempdir().unwrap();
    let failed = run_stdin(cli(&p.base_url(), tmp.path()), &spec(tmp.path(), "t"));
    assert_eq!(failed.status.code(), Some(2));
    let broken = run_stdin(cli(&p.base_url(), tmp.path()), "{not json");
    assert_eq!(broken.status.code(), Some(1));
    assert!(last_json(&broken)["error"].as_str().unwrap().starts_with("invalid JSON spec: "));
    let empty = run_stdin(cli(&p.base_url(), tmp.path()), "  ");
    assert_eq!(empty.status.code(), Some(1));
    assert_eq!(last_json(&empty)["error"], "no spec on stdin and neither --spec nor --json given");
    let inline = cli(&p.base_url(), tmp.path()).args(["--json", "[1]"]).output().unwrap();
    assert_eq!((inline.status.code(), last_json(&inline)["error"].clone()), (Some(1), json!("spec must be a JSON object")));
    let missing = cli(&p.base_url(), tmp.path()).args(["--spec", "nope.json"]).output().unwrap();
    assert!(last_json(&missing)["error"].as_str().unwrap().starts_with("cannot read spec: "));
    let both = cli(&p.base_url(), tmp.path()).args(["--schema", "--example"]).output().unwrap();
    assert_eq!(both.status.code(), Some(2));
}

#[test]
fn accepts_utf8_bom_spec_file() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("spec.json");
    std::fs::write(&path, format!("\u{feff}{}", spec(tmp.path(), "bom"))).unwrap();
    let out = cli(&p.base_url(), tmp.path()).arg("--spec").arg(&path).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn schema_has_python_keys_plus_new_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(BIN).current_dir(tmp.path()).arg("--schema").output().unwrap();
    let schema: Value = serde_json::from_slice(&out.stdout).unwrap();
    for key in [
        "mode",
        "prompt",
        "topic",
        "count",
        "size",
        "quality",
        "model",
        "image_model",
        "action",
        "reference_images",
        "topic_root",
        "preset",
        "jobs",
        "prompt_prefix",
        "prompt_suffix",
        "concurrency",
        "retries",
        "resume",
        "dry_run",
        "seed",
        "job_timeout_sec",
        "background",
        "fit",
        "mask",
        "remove_background",
        "keep_outside_mask",
        "profile",
    ] {
        assert!(schema.get(key).is_some(), "schema lacks {key}");
    }
    assert!(schema["mask"]["description"].as_str().unwrap().contains("stage 4"));
    assert!(schema["profile"]["description"].as_str().unwrap().contains("image-cli profile"));
    let cwd = image_core::paths::absolute(tmp.path()).display().to_string();
    assert_eq!(schema["topic_root"]["default"], cwd);
    let examples = Command::new(BIN).arg("--example").output().unwrap();
    let examples: Value = serde_json::from_slice(&examples.stdout).unwrap();
    assert!(examples["single_minimal"]["prompt"].is_string());
}

#[test]
fn list_models_filters_image_ids() {
    let p = FakeProvider::start(|_| {
        Reply::json(
            200,
            r#"{"data":[{"id":"gpt-image-2.5"},{"id":"gpt-5.5"},{"id":"gemini-3.1-flash-image"},{"id":"grok-imagine-image-2.0"},{"id":"gpt-image-2.5"}]}"#,
        )
    });
    let tmp = tempfile::tempdir().unwrap();
    let out = cli(&p.base_url(), tmp.path()).arg("--list-models").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let res = last_json(&out);
    assert_eq!(res["available_models"], json!(["gpt-image-2.5", "gemini-3.1-flash-image", "grok-imagine-image-2.0"]));
    assert_eq!(res["base_url"], p.base_url());
    assert_eq!(res["key_configured"], true);
    let grok = res["models"].as_array().unwrap().iter().find(|m| m["id"] == "grok-imagine-image-2.0").unwrap();
    assert_eq!((grok["provider"].clone(), grok["reference_images"].clone()), (json!("xAI Grok"), json!(false)));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn list_models_with_bad_connection_config_fails_offline() {
    let tmp = tempfile::tempdir().unwrap();
    let out = cli("ftp://nowhere", tmp.path()).arg("--list-models").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let res = last_json(&out);
    assert_eq!(res["ok"], false);
    assert!(res["error"].as_str().unwrap().contains("CLIPROXY_BASE_URL"));
}

#[test]
fn key_never_appears_in_any_output() {
    let p = FakeProvider::start(|r| {
        let echoed = r.header("Authorization").unwrap_or_default().to_string();
        Reply::json(403, &json!({"error": {"message": format!("rejected {echoed} fake-token")}}).to_string())
    });
    let tmp = tempfile::tempdir().unwrap();
    let out = run_stdin(cli(&p.base_url(), tmp.path()), &spec(tmp.path(), "k"));
    let everything = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!everything.contains("fake-token"), "{everything}");
    let record = std::fs::read_to_string(tmp.path().join("ImageGallery/metadata/k/k.png.json")).unwrap();
    assert!(!record.contains("fake-token"));
    assert!(record.contains("\"failed\""));
}

#[test]
fn huge_single_count_is_invalid_request_not_a_panic() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let run = |count: Value, dry: bool| {
        let spec = json!({"prompt": "x", "topic": "t", "size": "16x16", "quality": "low", "dry_run": dry, "count": count,
                          "topic_root": tmp.path().display().to_string()});
        let mut cmd = cli(&p.base_url(), tmp.path());
        cmd.env("IMAGE_STUDIO_HOME", tmp.path().join("studio")).env("IMAGE_STUDIO_KEYRING_SERVICE", "ImageStudioTest-g3-count");
        run_stdin(cmd, &spec.to_string())
    };
    for value in [json!(1e100), json!(1e18), json!(1001), json!("inf"), json!("NaN")] {
        for dry in [true, false] {
            let out = run(value.clone(), dry);
            assert_eq!(out.status.code(), Some(2), "{value}: {}", String::from_utf8_lossy(&out.stderr));
            let res = last_json(&out);
            assert_eq!((res["ok"].clone(), res["error_category"].clone()), (json!(false), json!("invalid_request")), "{res}");
            assert!(res["error"].as_str().unwrap().contains("count"), "{res}");
        }
    }
    let out = run(json!(1000), true);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(last_json(&out)["planned_outputs"].as_array().unwrap().len(), 1000);
    assert_eq!(p.requests().len(), 0);
}

#[test]
fn out_of_range_seconds_give_json_errors_not_panics() {
    let p = ok_provider();
    let tmp = tempfile::tempdir().unwrap();
    let isolated = |cmd: &mut Command| {
        cmd.env("IMAGE_STUDIO_HOME", tmp.path().join("studio")).env("IMAGE_STUDIO_KEYRING_SERVICE", "ImageStudioTest-g3-seconds");
    };
    for value in [json!(1e100), json!("NaN"), json!("inf"), json!(86401)] {
        for dry in [true, false] {
            let spec = json!({"prompt": "a mug", "topic": "t", "size": "16x16", "quality": "low", "dry_run": dry,
                              "job_timeout_sec": value, "topic_root": tmp.path().display().to_string()});
            let mut cmd = cli(&p.base_url(), tmp.path());
            isolated(&mut cmd);
            let out = run_stdin(cmd, &spec.to_string());
            assert_eq!(out.status.code(), Some(2), "{value}: {}", String::from_utf8_lossy(&out.stderr));
            let res = last_json(&out);
            assert_eq!((res["ok"].clone(), res["error_category"].clone()), (json!(false), json!("invalid_request")), "{res}");
        }
    }
    for value in ["1e300", "nan", "-1", "abc"] {
        let mut cmd = cli(&p.base_url(), tmp.path());
        isolated(&mut cmd);
        cmd.env("AI_IMAGE_RETRY_BACKOFF_SEC", value);
        let out = run_stdin(cmd, &spec(tmp.path(), "b"));
        assert_eq!(out.status.code(), Some(1), "{value}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(last_json(&out)["error"].as_str().unwrap().contains("AI_IMAGE_RETRY_BACKOFF_SEC"));
    }
    assert_eq!(p.requests().len(), 0);
}
