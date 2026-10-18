//! `image-cli profile …` and `--profile`. Uses a temporary profile folder and a throwaway
//! Credential Manager service; every stored key is deleted when the test ends.
#![cfg(windows)]

use image_core::secrets::{KeyringStore, SecretStore};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use test_fixture::{closed_base_url, openai_image_reply, png_bytes, FakeProvider, Reply};

const BIN: &str = env!("CARGO_BIN_EXE_image-cli");
const KEY: &str = "sk-profile-test-0123456789";

/// Isolated profile folder + keyring service. Drop removes every key under the service.
struct Home {
    dir: tempfile::TempDir,
    service: String,
}

impl Home {
    fn new() -> Home {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let service = format!("ImageStudioTest-{}-{nanos}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
        Home { dir: tempfile::tempdir().unwrap(), service }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn profiles_json(&self) -> PathBuf {
        self.path().join("home").join("profiles.json")
    }

    fn keyring(&self) -> KeyringStore {
        KeyringStore::with_service(&self.service)
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.current_dir(self.path())
            .env("IMAGE_STUDIO_HOME", self.path().join("home"))
            .env("IMAGE_STUDIO_KEYRING_SERVICE", &self.service)
            .env("AI_IMAGE_RETRY_BACKOFF_SEC", "0")
            .env_remove("CLIPROXY_BASE_URL")
            .env_remove("CLIPROXY_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("CLIPROXY_IMAGE_MODEL")
            .env_remove("CLIPROXY_TIMEOUT");
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_stdin(args, "")
    }

    fn run_stdin(&self, args: &[&str], input: &str) -> Output {
        let mut cmd = self.cmd();
        cmd.args(args);
        run_with_stdin(cmd, input)
    }

    fn ids(&self) -> Vec<String> {
        let Ok(text) = std::fs::read_to_string(self.profiles_json()) else { return Vec::new() };
        let file: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        file["profiles"].as_array().map(|ps| ps.iter().filter_map(|p| p["id"].as_str().map(str::to_string)).collect()).unwrap_or_default()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let keyring = self.keyring();
        for id in self.ids() {
            let _ = keyring.delete(&id);
        }
    }
}

fn run_with_stdin(mut cmd: Command, input: &str) -> Output {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn last_json(out: &Output) -> Value {
    let text = stdout(out);
    let line = text.lines().last().unwrap_or_else(|| panic!("no JSON line; stderr: {}", String::from_utf8_lossy(&out.stderr)));
    serde_json::from_str(line).unwrap()
}

fn assert_ok(out: &Output) -> Value {
    assert_eq!(out.status.code(), Some(0), "stdout: {} stderr: {}", stdout(out), String::from_utf8_lossy(&out.stderr));
    let value = last_json(out);
    assert_eq!(value["ok"], true, "{value}");
    value
}

#[test]
fn add_with_key_from_stdin_then_list_shows_key_configured_without_key() {
    let home = Home::new();
    let added = assert_ok(&home.run_stdin(
        &["profile", "add", "내 프록시 v2.1", "--kind", "cliproxy", "--url", "http://127.0.0.1:9", "--key-stdin", "--default"],
        &format!("{KEY}\r\nsecond line ignored\n"),
    ));
    let profile = &added["profile"];
    assert_eq!(profile["name"], "내 프록시 v2.1");
    assert_eq!(profile["kind"], "cliproxy");
    assert_eq!(profile["base_url"], "http://127.0.0.1:9/v1");
    assert_eq!(profile["key_configured"], true);
    let id = profile["id"].as_str().unwrap().to_string();
    assert_eq!(home.keyring().get(&id).unwrap().unwrap().expose(), KEY);

    // A key-required kind without a key is stored but reported as not configured; a public http address warns.
    let bare = assert_ok(&home.run(&["profile", "add", "OpenAI", "--kind", "openai", "--concurrency", "2"]));
    assert_eq!(bare["profile"]["key_configured"], false);
    assert_eq!(bare["profile"]["base_url"], "https://api.openai.com/v1");
    assert_eq!(bare["profile"]["concurrency"], 2);
    let mut from_env = home.cmd();
    from_env.args(["profile", "add", "remote", "--kind", "openai_compatible", "--url", "http://example.com", "--key-env", "MY_TEST_KEY"]);
    from_env.env("MY_TEST_KEY", format!("  {KEY}-env \n"));
    let remote = assert_ok(&run_with_stdin(from_env, ""));
    assert_eq!(remote["profile"]["key_configured"], true);
    assert!(remote["profile"]["warning"].as_str().unwrap().contains("http://"));
    let remote_id = remote["profile"]["id"].as_str().unwrap().to_string();
    assert_eq!(home.keyring().get(&remote_id).unwrap().unwrap().expose(), format!("{KEY}-env"));

    let list_out = home.run(&["profile", "list"]);
    let list = assert_ok(&list_out);
    assert_eq!(stdout(&list_out).lines().count(), 1);
    assert_eq!(list["default"], id.as_str());
    let profiles = list["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 3);
    assert_eq!(profiles[0]["key_configured"], true);
    assert_eq!(profiles[0]["warning"], Value::Null);
    assert_eq!(profiles[1]["key_configured"], false);
    assert_eq!(profiles[1]["concurrency"], 2);
    assert!(!stdout(&list_out).contains(KEY));
    assert!(!stdout(&list_out).contains("sk-"));
    let file = std::fs::read_to_string(home.profiles_json()).unwrap();
    assert!(!file.contains(KEY), "{file}");
    assert!(!file.contains("sk-"), "{file}");

    // Update: rename, clear concurrency, replace the key; duplicates and missing keys are errors.
    let updated = assert_ok(&home.run_stdin(
        &["profile", "update", "openai", "--name", "OpenAI main", "--concurrency", "none", "--key-stdin"],
        &format!("{KEY}-new\n"),
    ));
    assert_eq!(updated["profile"]["name"], "OpenAI main");
    assert_eq!(updated["profile"]["concurrency"], Value::Null);
    assert_eq!(updated["profile"]["key_configured"], true);
    let dup = home.run(&["profile", "add", "openai MAIN", "--kind", "openai"]);
    assert_eq!(dup.status.code(), Some(1));
    assert_eq!(last_json(&dup)["error_category"], "conflict");
    let empty_key = home.run_stdin(&["profile", "add", "empty", "--kind", "openai", "--key-stdin"], "\n");
    assert_eq!(empty_key.status.code(), Some(1));
    assert_eq!(last_json(&empty_key)["ok"], false);
    assert_eq!(home.ids().len(), 3, "a failed add leaves no profile behind");

    // Default can be moved and cleared.
    assert_ok(&home.run(&["profile", "default", "OpenAI main"]));
    assert_eq!(assert_ok(&home.run(&["profile", "list"]))["default"], updated["profile"]["id"]);
    assert_eq!(assert_ok(&home.run(&["profile", "default", "--none"]))["default"], Value::Null);
    assert_eq!(assert_ok(&home.run(&["profile", "list"]))["default"], Value::Null);
}

fn fake_proxy() -> FakeProvider {
    let png = png_bytes(16, 16, [3, 3, 3, 255]);
    FakeProvider::start(move |r| {
        if r.path.ends_with("/models") {
            Reply::json(200, r#"{"data":[{"id":"gpt-image-2.5"},{"id":"gpt-5.5"},{"id":"gemini-3.1-flash-image"}]}"#)
        } else {
            openai_image_reply(&png)
        }
    })
}

#[test]
fn profile_flag_drives_generation_and_list_models() {
    let home = Home::new();
    let p = fake_proxy();
    assert_ok(&home.run_stdin(&["profile", "add", "fake", "--kind", "cliproxy", "--url", &p.base_url(), "--key-stdin"], KEY));
    let spec = json!({"prompt": "a mug", "topic": "via-profile", "size": "16x16", "quality": "low",
                      "topic_root": home.path().display().to_string()})
    .to_string();

    // The environment points at a closed port: the explicit profile must win.
    let mut run = home.cmd();
    run.env("CLIPROXY_BASE_URL", closed_base_url()).env("CLIPROXY_API_KEY", "env-token").args(["--profile", "fake", "--json", &spec]);
    let out = run_with_stdin(run, "");
    let result = assert_ok(&out);
    assert_eq!(result["outputs"].as_array().map(Vec::len), Some(1), "{result}");
    assert_eq!(result["connection_source"], "profile:fake");
    let image_calls: Vec<_> = p.requests().into_iter().filter(|r| r.path.ends_with("/images/generations")).collect();
    assert_eq!(image_calls.len(), 1);
    assert_eq!(image_calls[0].header("Authorization"), Some(format!("Bearer {KEY}").as_str()));
    let everything = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
    assert!(!everything.contains(KEY));
    let status = std::fs::read_to_string(home.path().join("home").join("status.json")).unwrap();
    assert!(status.contains("\"ok\": true") || status.contains("\"ok\":true"), "{status}");

    // `--profile` with stdin spec, and a spec naming another profile is refused before any request.
    let before = p.requests().len();
    let mut clash = home.cmd();
    clash.args(["--profile", "fake"]);
    let clash_spec = json!({"prompt": "x", "profile": "other", "topic_root": home.path().display().to_string()}).to_string();
    let refused = run_with_stdin(clash, &clash_spec);
    assert_eq!(refused.status.code(), Some(1));
    assert!(last_json(&refused)["error"].as_str().unwrap().contains("--profile"));
    assert_eq!(p.requests().len(), before);

    let mut models = home.cmd();
    models.env("CLIPROXY_BASE_URL", closed_base_url()).args(["--profile", "fake", "--list-models"]);
    let listed = assert_ok(&run_with_stdin(models, ""));
    assert_eq!(listed["available_models"], json!(["gpt-image-2.5", "gemini-3.1-flash-image"]));
    assert_eq!(listed["base_url"], p.base_url());
    assert_eq!(listed["key_configured"], true);
    assert_eq!(listed["profile"], "fake");
    assert_eq!(listed["kind"], "cliproxy");

    let missing = home.run(&["--profile", "nope", "--list-models"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(last_json(&missing)["error_category"], "invalid_request");
}

#[test]
fn check_outputs_report() {
    let home = Home::new();
    let p = fake_proxy();
    assert_ok(&home.run_stdin(&["profile", "add", "fake", "--kind", "cliproxy", "--url", &p.base_url(), "--key-stdin"], KEY));
    let out = home.run(&["profile", "check", "fake"]);
    let report = assert_ok(&out);
    assert_eq!(stdout(&out).lines().count(), 1);
    assert_eq!(report["profile"], "fake");
    assert_eq!(report["kind"], "cliproxy");
    assert_eq!(report["base_url"], p.base_url());
    assert_eq!(
        (report["key_configured"].clone(), report["reachable"].clone(), report["authenticated"].clone()),
        (json!(true), json!(true), json!(true))
    );
    assert_eq!(report["image_models"], json!(["gpt-image-2.5", "gemini-3.1-flash-image"]));
    assert_eq!(report["error"], Value::Null);
    assert!(!stdout(&out).contains(KEY));

    // Without NAME the usual order applies; the environment connection here wins over profiles.
    let mut env_check = home.cmd();
    env_check.env("CLIPROXY_BASE_URL", p.base_url()).args(["profile", "check"]);
    let env_report = assert_ok(&run_with_stdin(env_check, ""));
    assert_eq!((env_report["profile"].clone(), env_report["base_url"].clone()), (Value::Null, json!(p.base_url())));

    // An unreachable server is a report with ok false and exit code 1.
    assert_ok(&home.run(&["profile", "add", "closed", "--kind", "cliproxy", "--url", &closed_base_url()]));
    let closed = home.run(&["profile", "check", "closed"]);
    assert_eq!(closed.status.code(), Some(1));
    let closed = last_json(&closed);
    assert_eq!((closed["ok"].clone(), closed["reachable"].clone()), (json!(false), json!(false)));
    assert!(closed["error"]["category"].is_string());

    let missing = home.run(&["profile", "check", "nope"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(last_json(&missing)["error_category"], "invalid_request");
}

#[test]
fn key_argument_is_refused() {
    let home = Home::new();
    for args in [
        &["profile", "add", "x", "--kind", "openai", "--key", "sk-1"][..],
        &["profile", "add", "x", "--kind", "openai", "--key=sk-1"][..],
        &["profile", "add", "x", "sk-1", "--kind", "openai"][..],
        &["profile", "update", "x", "--key", "sk-1"][..],
    ] {
        let out = home.run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("usage"), "{stderr}");
        assert!(!stderr.contains("sk-1"), "the refused key must not be echoed: {stderr}");
        assert!(stdout(&out).is_empty());
    }
    let both = home.run(&["profile", "add", "x", "--kind", "openai", "--key-stdin", "--key-env", "V"]);
    assert_eq!(both.status.code(), Some(2));
    let bad_kind = home.run(&["profile", "add", "x", "--kind", "anthropic"]);
    assert_eq!(bad_kind.status.code(), Some(2));
    assert!(!home.profiles_json().exists());
}

#[test]
fn remove_deletes_key() {
    let home = Home::new();
    let added = assert_ok(&home.run_stdin(&["profile", "add", "gone", "--kind", "xai", "--key-stdin", "--default"], KEY));
    let id = added["profile"]["id"].as_str().unwrap().to_string();
    assert!(home.keyring().get(&id).unwrap().is_some());
    let removed = assert_ok(&home.run(&["profile", "remove", "gone"]));
    assert_eq!(removed["removed"]["id"], id.as_str());
    assert!(home.keyring().get(&id).unwrap().is_none());
    let list = assert_ok(&home.run(&["profile", "list"]));
    assert_eq!((list["default"].clone(), list["profiles"].clone()), (Value::Null, json!([])));
    let again = home.run(&["profile", "remove", "gone"]);
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(last_json(&again)["error_category"], "not_found");
}
