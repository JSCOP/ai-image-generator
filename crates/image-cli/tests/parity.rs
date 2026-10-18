//! Same spec, same fake provider: Python `tools/ai_image.py` and Rust `image-cli` must agree.
//! Intended differences are stripped in `normalize` and listed in `docs/RUST_PARITY.md`.
//! Skipped when the checkout has no `.venv`.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use test_fixture::{openai_image_reply, png_bytes, FakeProvider, Recorded, Reply};

const BIN: &str = env!("CARGO_BIN_EXE_image-cli");

/// Keys only one side has, or whose text legitimately differs (see docs/RUST_PARITY.md).
const DROPPED_KEYS: [&str; 7] = ["elapsed_sec", "warnings", "connection_source", "error_category", "stdout_tail", "stderr_tail", "error"];
const DROPPED_FAILURE_KEYS: [&str; 2] = ["reason", "error"];

fn repo() -> PathBuf {
    image_core::paths::absolute(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn python() -> Option<PathBuf> {
    [".venv/Scripts/python.exe", ".venv/bin/python"].iter().map(|p| repo().join(p)).find(|p| p.is_file())
}

struct Side {
    root: PathBuf,
    _tmp: tempfile::TempDir,
    provider: FakeProvider,
}

fn provider() -> FakeProvider {
    let png = png_bytes(1536, 1024, [30, 60, 90, 255]);
    FakeProvider::start(move |r: &Recorded| {
        if r.body.windows(4).any(|w| w == b"FAIL") {
            Reply::json(400, r#"{"error":{"message":"rejected"}}"#)
        } else {
            openai_image_reply(&png)
        }
    })
}

fn side() -> Side {
    let tmp = tempfile::tempdir().unwrap();
    Side { root: image_core::paths::absolute(tmp.path()), _tmp: tmp, provider: provider() }
}

fn run(side: &Side, python: Option<&Path>, mut spec: Value) -> (Option<i32>, Value) {
    spec["topic_root"] = json!(side.root.display().to_string());
    let mut cmd = match python {
        Some(py) => {
            let mut c = Command::new(py);
            c.arg(repo().join("tools/ai_image.py"));
            c
        }
        None => Command::new(BIN),
    };
    let out = cmd
        .arg("--json")
        .arg(spec.to_string())
        .current_dir(repo())
        .env("CLIPROXY_BASE_URL", side.provider.base_url())
        .env("CLIPROXY_API_KEY", "fake-token")
        .env("AI_IMAGE_RETRY_BACKOFF_SEC", "0")
        .env("PYTHONUTF8", "1")
        // Never the developer's %APPDATA%\ImageStudio profiles or real Credential Manager entries; the sibling path is never created.
        .env("IMAGE_STUDIO_HOME", side.root.with_extension("studio"))
        .env("IMAGE_STUDIO_KEYRING_SERVICE", "ImageStudioTest-parity")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CLIPROXY_IMAGE_MODEL")
        .env_remove("CLIPROXY_TIMEOUT")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let last = stdout.lines().last().unwrap_or_else(|| panic!("no output: {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.code(), serde_json::from_str(last).unwrap())
}

fn normalize(value: &Value, root: &Path) -> Value {
    let root = root.display().to_string();
    match value {
        Value::String(s) => json!(s.replace(&root, "<ROOT>").replace('\\', "/")),
        Value::Array(items) => Value::Array(items.iter().map(|v| normalize(v, Path::new(&root))).collect()),
        Value::Object(map) => {
            let failures = map.contains_key("index") && map.contains_key("reason");
            Value::Object(
                map.iter()
                    .filter(|(k, _)| !DROPPED_KEYS.contains(&k.as_str()) || (*k == "error" && map.get("ok") == Some(&json!(false))))
                    .filter(|(k, _)| !(failures && DROPPED_FAILURE_KEYS.contains(&k.as_str())))
                    .filter(|(k, _)| !(map.get("mode") == Some(&json!("batch")) && matches!(k.as_str(), "concurrency" | "retried")))
                    .map(|(k, v)| (k.clone(), normalize(v, Path::new(&root))))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

fn files(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(path.strip_prefix(root).unwrap().display().to_string().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn bodies(p: &FakeProvider) -> Vec<String> {
    let mut list: Vec<String> = p.requests().iter().map(|r| format!("{} {}", r.path, normalize_body(r.json()))).collect();
    list.sort();
    list
}

fn normalize_body(mut body: Value) -> String {
    if let Value::Object(map) = &mut body {
        map.sort_keys();
    }
    body.to_string()
}

#[test]
fn rust_cli_matches_python_cli() {
    let Some(py) = python() else {
        eprintln!("skipping parity: no .venv in {}", repo().display());
        return;
    };
    let reference = tempfile::tempdir().unwrap();
    let ref_path = image_core::paths::absolute(reference.path()).join("ref.png");
    std::fs::write(&ref_path, png_bytes(32, 32, [200, 10, 10, 255])).unwrap();
    let preset = repo().join("presets/extraction-rpg.json").display().to_string();
    let specs = vec![
        json!({"prompt": "a blue mug", "topic": "single", "count": 2, "size": "64x48", "quality": "low"}),
        json!({"mode": "jobs", "topic": "jobs", "size": "64x48", "quality": "low", "prompt_prefix": "PRE", "prompt_suffix": "POST",
               "jobs": [{"id": "01 plain", "prompt": "first"},
                        {"id": "02/edit", "prompt": "second", "action": "edit", "reference_images": [ref_path.display().to_string()]}]}),
        json!({"prompt": "a mug", "topic": "dry", "count": 3, "dry_run": true}),
        json!({"mode": "jobs", "dry_run": true, "jobs": [{"id": "x", "prompt": "p"}, {"id": "y", "prompt": "q", "size": "bad"}]}),
        json!({"mode": "batch", "preset": preset, "count": 3, "seed": 42, "dry_run": true}),
        json!({"prompt": "FAIL please", "topic": "fails", "size": "64x48", "quality": "low", "retries": 0}),
        json!({"prompt": "a blue mug", "topic": "single", "count": 2, "size": "64x48", "quality": "low", "resume": true}),
        json!({"topic": "noprompt"}),
    ];
    let (py_side, rs_side) = (side(), side());
    for spec in specs {
        let (py_code, py_result) = run(&py_side, Some(&py), spec.clone());
        let (rs_code, rs_result) = run(&rs_side, None, spec.clone());
        assert_eq!(rs_code, py_code, "exit code for {spec}\npython: {py_result}\nrust: {rs_result}");
        assert_eq!(normalize(&rs_result, &rs_side.root), normalize(&py_result, &py_side.root), "result for {spec}");
    }
    assert_eq!(files(&rs_side.root), files(&py_side.root));
    for name in files(&py_side.root) {
        if name.ends_with(".png") {
            let (a, b) = (image::open(py_side.root.join(&name)).unwrap(), image::open(rs_side.root.join(&name)).unwrap());
            assert_eq!((a.width(), a.height()), (b.width(), b.height()), "{name}");
        } else if name.ends_with(".json") {
            let read = |root: &Path| -> Value { serde_json::from_slice(&std::fs::read(root.join(&name)).unwrap()).unwrap() };
            let (py_rec, rs_rec) = (normalize(&read(&py_side.root), &py_side.root), normalize(&read(&rs_side.root), &rs_side.root));
            for (key, value) in py_rec.as_object().unwrap() {
                assert_eq!(rs_rec.get(key), Some(value), "record {name} key {key}");
            }
        }
    }
    assert_eq!(bodies(&rs_side.provider), bodies(&py_side.provider));
}

/// Like `run`, but keeps the spec's `topic_root` and points the home folder at `home`.
fn run_home(provider: &FakeProvider, home: &Path, python: Option<&Path>, spec: &Value) -> (Option<i32>, Value) {
    let mut cmd = match python {
        Some(py) => {
            let mut c = Command::new(py);
            c.arg(repo().join("tools/ai_image.py"));
            c
        }
        None => Command::new(BIN),
    };
    let out = cmd
        .arg("--json")
        .arg(spec.to_string())
        .current_dir(repo())
        .env("CLIPROXY_BASE_URL", provider.base_url())
        .env("CLIPROXY_API_KEY", "fake-token")
        .env("AI_IMAGE_RETRY_BACKOFF_SEC", "0")
        .env("PYTHONUTF8", "1")
        .env("USERPROFILE", home)
        .env("HOME", home)
        .env("IMAGE_STUDIO_HOME", home.join("studio"))
        .env("IMAGE_STUDIO_KEYRING_SERVICE", "ImageStudioTest-parity-home")
        .env_remove("HOMEDRIVE")
        .env_remove("HOMEPATH")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CLIPROXY_IMAGE_MODEL")
        .env_remove("CLIPROXY_TIMEOUT")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let last = stdout.lines().last().unwrap_or_else(|| panic!("no output: {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.code(), serde_json::from_str(last).unwrap())
}

/// Review F9 (`~` topic_root) and F10 (single failures sorted by index, whatever the completion order).
#[test]
fn rust_cli_matches_python_on_home_paths_and_failure_order() {
    let Some(py) = python() else {
        eprintln!("skipping parity: no .venv in {}", repo().display());
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let home = image_core::paths::absolute(tmp.path()).join("home");
    std::fs::create_dir_all(&home).unwrap();
    let arrivals = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let provider = FakeProvider::start(move |_: &Recorded| {
        let n = arrivals.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u64;
        // Within each run of three, later arrivals answer sooner.
        Reply::json(400, r#"{"error":{"message":"rejected"}}"#).delayed(std::time::Duration::from_millis(900 - (n % 3) * 400))
    });
    let specs = [
        json!({"prompt": "a mug", "topic": "path-review", "topic_root": "~/Pictures", "dry_run": true}),
        json!({"mode": "jobs", "topic_root": "~", "dry_run": true, "jobs": [{"id": "x", "prompt": "p"}]}),
        json!({"prompt": "FAIL", "topic": "order", "topic_root": "~/out", "count": 3, "concurrency": 3, "retries": 0,
               "size": "64x48", "quality": "low"}),
    ];
    for spec in specs {
        let (py_code, py_result) = run_home(&provider, &home, Some(&py), &spec);
        let (rs_code, rs_result) = run_home(&provider, &home, None, &spec);
        assert_eq!(rs_code, py_code, "exit code for {spec}\npython: {py_result}\nrust: {rs_result}");
        assert_eq!(normalize(&rs_result, &home), normalize(&py_result, &home), "result for {spec}");
        assert!(rs_result["output_dir"].as_str().unwrap().starts_with(&home.display().to_string()), "{rs_result}");
    }
}
