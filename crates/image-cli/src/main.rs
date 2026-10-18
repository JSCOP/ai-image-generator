//! AI-friendly image generation entry point. JSON in → one JSON line out (same contract as
//! `tools/ai_image.py`). Progress events go to stderr as JSON lines.
//!
//! Usage: see `USAGE`. With no option the spec is read from stdin.
//! Exit codes: 0 ok, 2 spec ran with failures or usage error, 1 fatal.

use image_core::adapter::{model_supported, supports_references};
use image_core::catalog::{backend, is_listable_image_model, Backend};
use image_core::check::check_connection;
use image_core::connection::{resolve, Connection, EnvSource, Resolver, Secret, SystemEnv};
use image_core::error::{Category, EngineError};
use image_core::http::Http;
use image_core::profiles::{transport_warning, Profile, ProfilePatch, ProfileStore, ProviderKind};
use image_core::provider::list_models;
use image_core::runner::{run_spec, Progress};
use image_core::schema::{examples_json, schema_json};
use image_core::secrets::{KeyringStore, MemoryStore, SecretStore};
use serde_json::{json, Value};
use std::io::{BufRead, IsTerminal, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const USAGE: &str = "usage: image-cli [--profile NAME] [--spec FILE | --json STRING | --list-models]
       image-cli --schema | --example
       image-cli profile list
       image-cli profile add NAME --kind KIND [--url URL] [--concurrency N] [--key-stdin | --key-env VAR] [--default]
       image-cli profile update NAME [--name NEW] [--url URL] [--concurrency N|none] [--key-stdin | --key-env VAR]
       image-cli profile remove NAME
       image-cli profile default NAME | --none
       image-cli profile check [NAME]
KIND: cliproxy, openai, gemini, xai, openai_compatible. Keys are never taken as arguments:
--key-stdin reads the first line of piped stdin (an interactive terminal is refused: it would echo the key),
--key-env reads the named environment variable.";

enum Command {
    Spec(String),
    Json(String),
    Stdin,
    Schema,
    Example,
    ListModels,
    Profile(ProfileCommand),
}

/// Where a new key comes from. Never the command line (shell history).
#[derive(Clone, PartialEq, Eq)]
enum KeySource {
    Keep,
    Stdin,
    Env(String),
}

enum ProfileCommand {
    List,
    Add { name: String, kind: ProviderKind, url: Option<String>, concurrency: Option<u32>, key: KeySource, make_default: bool },
    Update { name: String, patch: ProfilePatch, key: KeySource },
    Remove { name: String },
    Default { name: Option<String> },
    Check { name: Option<String> },
}

struct Invocation {
    command: Command,
    profile: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Invocation, String> {
    if args.first().map(String::as_str) == Some("profile") {
        let command = Command::Profile(parse_profile(&args[1..])?);
        return Ok(Invocation { command, profile: None });
    }
    let mut command = None;
    let mut profile = None;
    let mut i = 0;
    while i < args.len() {
        let next = match args[i].as_str() {
            "-h" | "--help" => {
                println!("{USAGE}\n\nImage generation through CLIProxyAPI or a connection profile. See --schema for spec keys.");
                std::process::exit(0);
            }
            "--profile" => {
                let value = args.get(i + 1).ok_or("--profile needs a value")?.clone();
                if profile.replace(value).is_some() {
                    return Err("--profile may be given only once".into());
                }
                i += 2;
                continue;
            }
            "--spec" | "--json" => {
                let value = args.get(i + 1).ok_or_else(|| format!("{} needs a value", args[i]))?.clone();
                i += 1;
                if args[i - 1] == "--spec" {
                    Command::Spec(value)
                } else {
                    Command::Json(value)
                }
            }
            "--schema" => Command::Schema,
            "--example" => Command::Example,
            "--list-models" => Command::ListModels,
            other => return Err(format!("unrecognized argument: {}", flag_name(other))),
        };
        if command.replace(next).is_some() {
            return Err("only one of --spec, --json, --schema, --example, --list-models may be given".into());
        }
        i += 1;
    }
    let command = command.unwrap_or(Command::Stdin);
    if profile.is_some() && matches!(command, Command::Schema | Command::Example) {
        return Err("--profile cannot be combined with --schema or --example".into());
    }
    Ok(Invocation { command, profile })
}

/// Argument text safe to echo: a flag without any `=value` part, never a positional value (it may be a key).
fn flag_name(arg: &str) -> String {
    if arg.starts_with('-') {
        arg.split('=').next().unwrap_or(arg).to_string()
    } else {
        "a value in an unexpected position".to_string()
    }
}

fn parse_profile(args: &[String]) -> Result<ProfileCommand, String> {
    let (sub, rest) = args.split_first().ok_or("profile needs a command: list, add, update, remove, default or check")?;
    let mut name: Option<String> = None;
    let mut values: Vec<(&'static str, String)> = Vec::new();
    let mut switches: Vec<&'static str> = Vec::new();
    let value_flags: &[&'static str] = match sub.as_str() {
        "add" => &["--kind", "--url", "--concurrency", "--key-env"],
        "update" => &["--name", "--url", "--concurrency", "--key-env"],
        _ => &[],
    };
    let switch_flags: &[&'static str] = match sub.as_str() {
        "add" => &["--key-stdin", "--default"],
        "update" => &["--key-stdin"],
        "default" => &["--none"],
        _ => &[],
    };
    let takes_name = !matches!(sub.as_str(), "list");
    let mut i = 0;
    while i < rest.len() {
        let arg = rest[i].as_str();
        if let Some(flag) = value_flags.iter().find(|f| **f == arg) {
            let value = rest.get(i + 1).ok_or_else(|| format!("{flag} needs a value"))?.clone();
            if values.iter().any(|(f, _)| f == flag) {
                return Err(format!("{flag} may be given only once"));
            }
            values.push((flag, value));
            i += 2;
            continue;
        }
        if let Some(flag) = switch_flags.iter().find(|f| **f == arg) {
            if switches.contains(flag) {
                return Err(format!("{flag} may be given only once"));
            }
            switches.push(flag);
        } else if arg.starts_with('-') {
            let flag = flag_name(arg);
            if flag == "--key" {
                return Err("keys are not accepted as arguments (they stay in shell history); use --key-stdin or --key-env VAR".into());
            }
            return Err(format!("unrecognized argument for profile {sub}: {flag}"));
        } else if takes_name && name.is_none() {
            name = Some(arg.to_string());
        } else {
            return Err(format!("profile {sub}: unexpected extra argument"));
        }
        i += 1;
    }
    let value = |flag: &str| values.iter().find(|(f, _)| *f == flag).map(|(_, v)| v.clone());
    let has = |flag: &str| switches.contains(&flag);
    let key = match (has("--key-stdin"), value("--key-env")) {
        (true, Some(_)) => return Err("use only one of --key-stdin and --key-env".into()),
        (true, None) => KeySource::Stdin,
        (false, Some(var)) => KeySource::Env(var),
        (false, None) => KeySource::Keep,
    };
    let need_name = || name.clone().ok_or_else(|| format!("profile {sub} needs a profile NAME"));
    let concurrency =
        |text: &str| text.trim().parse::<u32>().map_err(|_| format!("--concurrency must be a positive integer, not {text:?}"));
    Ok(match sub.as_str() {
        "list" => ProfileCommand::List,
        "add" => {
            let kind_text = value("--kind").ok_or("profile add needs --kind")?;
            let kind = ProviderKind::parse(kind_text.trim()).ok_or_else(|| {
                let kinds: Vec<&str> = ProviderKind::ALL.iter().map(|k| k.as_str()).collect();
                format!("unknown --kind {kind_text:?}; use one of {}", kinds.join(", "))
            })?;
            ProfileCommand::Add {
                name: need_name()?,
                kind,
                url: value("--url"),
                concurrency: value("--concurrency").map(|c| concurrency(&c)).transpose()?,
                key,
                make_default: has("--default"),
            }
        }
        "update" => {
            let concurrency = match value("--concurrency") {
                None => None,
                Some(c) if c.trim().eq_ignore_ascii_case("none") => Some(None),
                Some(c) => Some(Some(concurrency(&c)?)),
            };
            let patch = ProfilePatch { name: value("--name"), base_url: value("--url"), concurrency };
            ProfileCommand::Update { name: need_name()?, patch, key }
        }
        "remove" => ProfileCommand::Remove { name: need_name()? },
        "default" => match (name.clone(), has("--none")) {
            (Some(_), true) | (None, false) => return Err("profile default needs either NAME or --none".into()),
            (name, _) => ProfileCommand::Default { name },
        },
        "check" => ProfileCommand::Check { name },
        other => return Err(format!("unknown profile command {other:?}; use list, add, update, remove, default or check")),
    })
}

fn emit(value: &Value, code: i32) -> i32 {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
    code
}

fn error_json(err: &EngineError) -> Value {
    json!({"ok": false, "error": err.message, "error_category": err.category})
}

fn read_spec(command: Command) -> Result<Value, (Value, i32)> {
    let fatal = |message: String| (json!({"ok": false, "error": message}), 1);
    let text = match command {
        Command::Spec(path) => std::fs::read_to_string(&path).map_err(|e| fatal(format!("cannot read spec: {e}")))?,
        Command::Json(text) => text,
        _ => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).map_err(|e| fatal(format!("cannot read spec: {e}")))?;
            if text.trim().trim_start_matches('\u{feff}').is_empty() {
                return Err(fatal("no spec on stdin and neither --spec nor --json given".into()));
            }
            text
        }
    };
    serde_json::from_str(text.trim().trim_start_matches('\u{feff}')).map_err(|e| fatal(format!("invalid JSON spec: {e}")))
}

/// Puts `--profile` into the spec. A spec that names a different profile is refused: one run uses one connection.
fn apply_profile_flag(spec: &mut Value, profile: Option<&str>) -> Result<(), (Value, i32)> {
    let (Some(wanted), Value::Object(object)) = (profile, spec) else { return Ok(()) };
    match object.get("profile") {
        None | Some(Value::Null) => {
            object.insert("profile".into(), json!(wanted));
            Ok(())
        }
        Some(Value::String(named)) if named.trim().to_lowercase() == wanted.trim().to_lowercase() => Ok(()),
        Some(other) => {
            let named = other.as_str().map(str::to_string).unwrap_or_else(|| other.to_string());
            let message = format!("--profile {wanted}와(과) spec의 profile {named}이(가) 다릅니다. 둘 중 하나만 지정해 주세요.");
            Err((json!({"ok": false, "error": message, "error_category": Category::InvalidRequest}), 1))
        }
    }
}

/// Profile folder from `IMAGE_STUDIO_HOME` or `%APPDATA%\ImageStudio`.
fn profile_store(env: &dyn EnvSource) -> Result<ProfileStore, EngineError> {
    ProfileStore::default_dir(env).map(ProfileStore::open).ok_or_else(|| {
        EngineError::new(
            Category::InvalidRequest,
            "프로필 폴더를 정할 수 없습니다. IMAGE_STUDIO_HOME 또는 APPDATA 환경변수를 확인해 주세요.",
        )
    })
}

/// Same sources as `run_spec`: profile folder (if any) and the Credential Manager.
fn resolve_for_cli(env: &dyn EnvSource, explicit: Option<&str>) -> Result<(Connection, Option<ProfileStore>), EngineError> {
    let store = ProfileStore::default_dir(env).map(ProfileStore::open);
    let keyring = KeyringStore::system(env).ok();
    let memory = MemoryStore::new();
    let secrets: &dyn SecretStore = match &keyring {
        Some(keyring) => keyring,
        None => &memory,
    };
    let conn = resolve(&Resolver { env, profiles: store.as_ref(), secrets }, explicit)?;
    Ok((conn, store))
}

fn profile_name(conn: &Connection) -> Option<&str> {
    conn.source.strip_prefix("profile:")
}

async fn models(profile: Option<&str>) -> (Value, i32) {
    let conn = match resolve_for_cli(&SystemEnv, profile) {
        Ok((conn, _)) => conn,
        Err(err) => {
            // Same keys as success; what the failed resolution could not tell is null.
            let result = json!({"ok": false, "base_url": null, "key_configured": false, "profile": null, "kind": null,
                "available_models": [], "models": [], "error": err.message, "error_category": err.category});
            return (result, 1);
        }
    };
    let base = json!({"base_url": conn.base_url, "key_configured": conn.api_key.is_some(),
                      "profile": profile_name(&conn), "kind": conn.kind});
    let mut result = match list_models(&Http::new(Some(Duration::from_secs(15))), &conn).await {
        Err(err) => json!({"ok": false, "available_models": [], "models": [], "error": err.message, "error_category": err.category}),
        Ok(ids) => {
            let mut available: Vec<String> = Vec::new();
            for id in ids.into_iter().filter(|id| is_listable_image_model(id) && model_supported(conn.kind, id).is_ok()) {
                if !available.contains(&id) {
                    available.push(id);
                }
            }
            let models: Vec<Value> = available
                .iter()
                .map(|id| {
                    let provider = if backend(id) == Backend::Gemini {
                        "Google Gemini"
                    } else if id.to_lowercase().starts_with("gpt-image-") {
                        "OpenAI"
                    } else {
                        "xAI Grok"
                    };
                    json!({"id": id, "provider": provider, "reference_images": supports_references(conn.kind, id)})
                })
                .collect();
            let mut ok = json!({"ok": true, "available_models": available, "models": models});
            if available.is_empty() {
                ok["ok"] = json!(false);
                ok["error"] = json!(match conn.kind {
                    ProviderKind::Cliproxy => "프록시 목록에 이 CLI가 지원하는 이미지 모델이 없습니다.",
                    _ => "모델 목록에 이 CLI가 지원하는 이미지 모델이 없습니다.",
                });
            }
            ok
        }
    };
    if let (Value::Object(result), Value::Object(base)) = (&mut result, base) {
        for (k, v) in base {
            result.insert(k, v);
        }
    }
    let code = if result["ok"] == true { 0 } else { 1 };
    (result, code)
}

/// The new key, read before anything is written. Empty input is an error.
fn read_key(source: &KeySource) -> Result<Option<Secret>, EngineError> {
    let stdin = std::io::stdin();
    let is_terminal = stdin.is_terminal();
    read_key_from(source, is_terminal, &mut stdin.lock())
}

fn read_key_from(source: &KeySource, stdin_is_terminal: bool, stdin: &mut dyn BufRead) -> Result<Option<Secret>, EngineError> {
    let raw = match source {
        KeySource::Keep => return Ok(None),
        // A console would echo the typed key on screen (and into terminal scrollback).
        KeySource::Stdin if stdin_is_terminal => {
            return Err(EngineError::new(
                Category::InvalidRequest,
                "--key-stdin은 키를 파이프로 받습니다. 터미널에 직접 입력하면 키가 화면에 보이므로 받지 않습니다. 예: \
                 $k = Read-Host 'API key' -AsSecureString; [System.Net.NetworkCredential]::new('', $k).Password | image-cli profile ... --key-stdin \
                 (또는 --key-env 변수)",
            ))
        }
        KeySource::Stdin => {
            let mut line = String::new();
            stdin
                .read_line(&mut line)
                .map_err(|e| EngineError::new(Category::InvalidRequest, format!("표준입력에서 키를 읽을 수 없습니다: {e}")))?;
            line
        }
        KeySource::Env(var) => SystemEnv
            .process(var)
            .ok_or_else(|| EngineError::new(Category::InvalidRequest, format!("환경변수 {var}이(가) 없거나 비어 있습니다.")))?,
    };
    let key = raw.trim_start_matches('\u{feff}').trim();
    if key.is_empty() {
        return Err(EngineError::new(Category::InvalidRequest, "키가 비어 있습니다."));
    }
    Ok(Some(Secret::new(key)))
}

/// Profile as shown to users: never the key, only whether one is stored.
fn profile_view(profile: &Profile, keyring: Option<&KeyringStore>) -> Value {
    let key_configured = keyring.and_then(|k| k.get(&profile.id).ok()).flatten().is_some();
    json!({
        "id": profile.id, "name": profile.name, "kind": profile.kind, "label": profile.kind.label(), "base_url": profile.base_url,
        "concurrency": profile.concurrency, "key_configured": key_configured, "warning": transport_warning(&profile.base_url),
    })
}

/// Saves the profile, its key and the default flag; a failure leaves nothing behind.
fn add_profile(
    store: &ProfileStore,
    key: Option<(&dyn SecretStore, &Secret)>,
    name: &str,
    kind: ProviderKind,
    url: Option<&str>,
    concurrency: Option<u32>,
    make_default: bool,
) -> Result<Profile, EngineError> {
    // Profile and default go in one write; the key is saved under the lock just before it.
    let mut saved_key_for: Option<String> = None;
    let result = store.add_with(name, kind, url, concurrency, make_default, |profile| {
        if let Some((key_store, key)) = key {
            key_store.set(&profile.id, key.expose())?;
            saved_key_for = Some(profile.id.clone());
        }
        Ok(())
    });
    if let (Err(_), Some(id), Some((key_store, _))) = (&result, &saved_key_for, key) {
        // The file write failed after the key was saved: no orphan key.
        let _ = key_store.delete(id);
    }
    result
}

/// Changes the profile and/or its key; a failure leaves both as they were.
fn update_profile(
    store: &ProfileStore,
    key: Option<(&dyn SecretStore, &Secret)>,
    name: &str,
    patch: ProfilePatch,
) -> Result<Profile, EngineError> {
    let current = store.find(name)?;
    let unchanged = patch.name.is_none() && patch.base_url.is_none() && patch.concurrency.is_none();
    let Some((key_store, key)) = key else {
        return if unchanged { Ok(current) } else { store.update(&current.id, patch) };
    };
    // Key first: a failed key save leaves profiles.json untouched.
    let old_key = key_store.get(&current.id)?;
    key_store.set(&current.id, key.expose())?;
    let result = if unchanged { store.find(&current.id) } else { store.update(&current.id, patch) };
    if result.is_err() {
        let _ = match &old_key {
            Some(old) => key_store.set(&current.id, old.expose()),
            None => key_store.delete(&current.id),
        };
    }
    result
}

async fn profile_command(command: ProfileCommand) -> Result<Value, EngineError> {
    let env = SystemEnv;
    if let ProfileCommand::Check { name } = command {
        let (conn, store) = resolve_for_cli(&env, name.as_deref())?;
        let report = check_connection(&Http::new(Some(Duration::from_secs(15))), &conn, store.as_ref()).await;
        let ok = report.error.is_none();
        let mut value = serde_json::to_value(&report).map_err(|e| EngineError::new(Category::Internal, e.to_string()))?;
        value["ok"] = json!(ok);
        return Ok(value);
    }
    let store = profile_store(&env)?;
    let keyring = KeyringStore::system(&env).ok();
    let need_keyring =
        || KeyringStore::system(&env).map_err(|e| EngineError::new(e.category, format!("키를 저장할 수 없습니다: {}", e.message)));
    match command {
        ProfileCommand::List => {
            let file = store.load()?;
            let profiles: Vec<Value> = file.profiles.iter().map(|p| profile_view(p, keyring.as_ref())).collect();
            Ok(json!({"ok": true, "default": file.default, "profiles": profiles}))
        }
        ProfileCommand::Add { name, kind, url, concurrency, key, make_default } => {
            let key = read_key(&key)?;
            let key_store = key.as_ref().map(|_| need_keyring()).transpose()?;
            let new_key = key.as_ref().zip(key_store.as_ref()).map(|(k, s)| (s as &dyn SecretStore, k));
            let profile = add_profile(&store, new_key, &name, kind, url.as_deref(), concurrency, make_default)?;
            Ok(json!({"ok": true, "profile": profile_view(&profile, keyring.as_ref()), "default": make_default}))
        }
        ProfileCommand::Update { name, patch, key } => {
            let key = read_key(&key)?;
            let key_store = key.as_ref().map(|_| need_keyring()).transpose()?;
            let new_key = key.as_ref().zip(key_store.as_ref()).map(|(k, s)| (s as &dyn SecretStore, k));
            let profile = update_profile(&store, new_key, &name, patch)?;
            Ok(json!({"ok": true, "profile": profile_view(&profile, keyring.as_ref())}))
        }
        ProfileCommand::Remove { name } => {
            let removed = store.remove(&name)?;
            let key_store = keyring.as_ref().ok_or_else(|| {
                EngineError::new(Category::Unsupported, format!("프로필 {}을(를) 지웠지만 저장된 키를 지울 수 없습니다.", removed.name))
            })?;
            key_store.delete(&removed.id).map_err(|e| {
                EngineError::new(e.category, format!("프로필 {}을(를) 지웠지만 키를 지우지 못했습니다: {}", removed.name, e.message))
            })?;
            Ok(json!({"ok": true, "removed": {"id": removed.id, "name": removed.name}}))
        }
        ProfileCommand::Default { name } => {
            store.set_default(name.as_deref())?;
            Ok(json!({"ok": true, "default": store.load()?.default}))
        }
        ProfileCommand::Check { .. } => unreachable!("handled above"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Invocation { command, profile } = match parse_args(&args) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("{USAGE}\nimage-cli: error: {message}");
            std::process::exit(2);
        }
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    let code = match command {
        Command::Schema => emit_pretty(&schema_json()),
        Command::Example => emit_pretty(&examples_json()),
        Command::ListModels => {
            let (result, code) = runtime.block_on(models(profile.as_deref()));
            emit(&result, code)
        }
        Command::Profile(command) => match runtime.block_on(profile_command(command)) {
            Ok(result) => {
                let code = if result["ok"] == true { 0 } else { 1 };
                emit(&result, code)
            }
            Err(err) => emit(&error_json(&err), 1),
        },
        other => match read_spec(other).and_then(|mut spec| apply_profile_flag(&mut spec, profile.as_deref()).map(|()| spec)) {
            Err((result, code)) => emit(&result, code),
            Ok(spec) => runtime.block_on(async {
                let cancel = CancellationToken::new();
                let on_ctrl_c = cancel.clone();
                tokio::spawn(async move {
                    if tokio::signal::ctrl_c().await.is_ok() {
                        on_ctrl_c.cancel();
                    }
                });
                let stderr = Arc::new(Mutex::new(()));
                let progress: Progress = Arc::new(move |event| {
                    let _guard = stderr.lock();
                    let mut err = std::io::stderr().lock();
                    let _ = writeln!(err, "{event}");
                    let _ = err.flush();
                });
                let outcome = run_spec(spec, Arc::new(SystemEnv), progress, cancel.clone()).await;
                let code = if cancel.is_cancelled() && outcome.exit_code == 0 { 2 } else { outcome.exit_code };
                emit(&outcome.result, code)
            }),
        },
    };
    std::process::exit(code);
}

fn emit_pretty(value: &Value) -> i32 {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MemoryStore that can refuse `set` or run a hook first (e.g. another process grabbing the profile lock).
    struct FaultyStore {
        inner: MemoryStore,
        fail_set: bool,
        before_set: Box<dyn Fn() + Send + Sync>,
    }

    impl FaultyStore {
        fn failing() -> FaultyStore {
            FaultyStore { inner: MemoryStore::new(), fail_set: true, before_set: Box::new(|| {}) }
        }

        fn with_hook(hook: impl Fn() + Send + Sync + 'static) -> FaultyStore {
            FaultyStore { inner: MemoryStore::new(), fail_set: false, before_set: Box::new(hook) }
        }
    }

    impl SecretStore for FaultyStore {
        fn get(&self, id: &str) -> Result<Option<Secret>, EngineError> {
            self.inner.get(id)
        }

        fn set(&self, id: &str, key: &str) -> Result<(), EngineError> {
            (self.before_set)();
            if self.fail_set {
                return Err(EngineError::new(Category::Internal, "자격 증명 관리자 오류: 테스트 실패"));
            }
            self.inner.set(id, key)
        }

        fn delete(&self, id: &str) -> Result<(), EngineError> {
            self.inner.delete(id)
        }
    }

    #[test]
    fn key_stdin_from_an_interactive_terminal_is_refused_without_reading() {
        let mut typed: &[u8] = b"sk-would-echo\n";
        let err = read_key_from(&KeySource::Stdin, true, &mut typed).err().expect("refused");
        assert_eq!(err.category, Category::InvalidRequest);
        assert!(err.message.contains("--key-stdin") && err.message.contains("Read-Host"), "{}", err.message);
        assert!(!err.message.contains("sk-would-echo"));
        assert_eq!(typed, b"sk-would-echo\n", "the terminal line must not be consumed");
        let mut piped: &[u8] = b"sk-piped\n";
        assert_eq!(read_key_from(&KeySource::Stdin, false, &mut piped).unwrap().unwrap().expose(), "sk-piped");
        // --key-env does not care about the terminal.
        let mut unused: &[u8] = b"";
        assert!(read_key_from(&KeySource::Keep, true, &mut unused).unwrap().is_none());
    }

    fn stored(store: &FaultyStore, id: &str) -> Option<String> {
        store.inner.get(id).unwrap().map(|k| k.expose().to_string())
    }

    #[test]
    fn add_with_default_is_all_or_nothing_when_the_profile_file_is_locked_after_the_key_save() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(tmp.path().to_path_buf());
        let lock = tmp.path().join("profiles.json.lock");
        let hook_lock = lock.clone();
        let keys = FaultyStore::with_hook(move || {
            let _ = std::fs::write(&hook_lock, b"");
        });
        let key = Secret::new("sk-test-add-default");
        let result = add_profile(&store, Some((&keys, &key)), "p1", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None, true);
        let _ = std::fs::remove_file(&lock);
        let file = store.load().unwrap();
        match result {
            Ok(profile) => {
                assert_eq!(file.default.as_deref(), Some(profile.id.as_str()));
                assert_eq!(stored(&keys, &profile.id).as_deref(), Some("sk-test-add-default"));
            }
            Err(err) => {
                assert!(file.profiles.is_empty(), "{}: profile left behind", err.message);
                let ids: Vec<String> = file.profiles.iter().map(|p| p.id.clone()).collect();
                assert!(ids.iter().all(|id| stored(&keys, id).is_none()));
            }
        }
    }

    #[test]
    fn add_whose_key_save_fails_leaves_no_profile_and_no_default() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(tmp.path().to_path_buf());
        let key = Secret::new("sk-test-add-fail");
        let keys = FaultyStore::failing();
        assert!(add_profile(&store, Some((&keys, &key)), "p1", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None, true).is_err());
        let file = store.load().unwrap();
        assert!(file.profiles.is_empty());
        assert_eq!(file.default, None);
    }

    #[test]
    fn update_whose_key_save_fails_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(tmp.path().to_path_buf());
        store.add("old", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None).unwrap();
        let before = std::fs::read(tmp.path().join("profiles.json")).unwrap();
        let key = Secret::new("sk-test-update-fail");
        let patch = ProfilePatch { name: Some("new".into()), base_url: Some("http://127.0.0.1:10".into()), concurrency: None };
        assert!(update_profile(&store, Some((&FaultyStore::failing(), &key)), "old", patch).is_err());
        assert_eq!(std::fs::read(tmp.path().join("profiles.json")).unwrap(), before);
    }

    #[test]
    fn update_whose_profile_change_fails_keeps_the_old_key() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(tmp.path().to_path_buf());
        let target = store.add("a", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None).unwrap();
        store.add("b", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None).unwrap();
        let keys = FaultyStore::with_hook(|| {});
        keys.inner.set(&target.id, "sk-old").unwrap();
        let empty = FaultyStore::with_hook(|| {});
        let other = store.add("c", ProviderKind::Cliproxy, Some("http://127.0.0.1:9"), None).unwrap();
        let new_key = Secret::new("sk-new");
        // Renaming onto an existing name fails inside the profile write.
        let patch = ProfilePatch { name: Some("b".into()), base_url: None, concurrency: None };
        assert!(update_profile(&store, Some((&keys, &new_key)), "a", patch).is_err());
        assert_eq!(stored(&keys, &target.id).as_deref(), Some("sk-old"));
        // A profile with no key before: the failed update must not leave the new one.
        let patch = ProfilePatch { name: Some("b".into()), base_url: None, concurrency: None };
        assert!(update_profile(&store, Some((&empty, &new_key)), "c", patch).is_err());
        assert_eq!(stored(&empty, &other.id), None);
    }
}
