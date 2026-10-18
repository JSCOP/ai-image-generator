//! AI-friendly image generation entry point. JSON in → one JSON line out (same contract as
//! `tools/ai_image.py`). Progress events go to stderr as JSON lines.
//!
//! Usage: image-cli [--spec FILE | --json STRING | --schema | --example | --list-models]
//! With no option the spec is read from stdin. Exit codes: 0 ok, 2 spec ran with failures, 1 fatal.

use image_core::catalog::{backend, is_listable_image_model, Backend};
use image_core::connection::{resolve_connection, SystemEnv};
use image_core::http::Http;
use image_core::provider::list_models;
use image_core::runner::{run_spec, Progress};
use image_core::schema::{examples_json, schema_json};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const USAGE: &str = "usage: image-cli [--spec FILE | --json STRING | --schema | --example | --list-models]";

enum Command {
    Spec(String),
    Json(String),
    Stdin,
    Schema,
    Example,
    ListModels,
}

fn parse_args() -> Result<Command, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut command = None;
    let mut i = 0;
    while i < args.len() {
        let next = match args[i].as_str() {
            "-h" | "--help" => {
                println!("{USAGE}\n\nImage generation through CLIProxyAPI. See --schema for spec keys.");
                std::process::exit(0);
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
            other => return Err(format!("unrecognized argument: {other}")),
        };
        if command.replace(next).is_some() {
            return Err("only one of --spec, --json, --schema, --example, --list-models may be given".into());
        }
        i += 1;
    }
    Ok(command.unwrap_or(Command::Stdin))
}

fn emit(value: &Value, code: i32) -> i32 {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
    code
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

async fn models() -> (Value, i32) {
    let conn = match resolve_connection(&SystemEnv) {
        Ok(conn) => conn,
        Err(err) => {
            let result = json!({"ok": false, "base_url": null, "key_configured": false, "available_models": [], "models": [],
                "error": err.message, "error_category": err.category});
            return (result, 1);
        }
    };
    let base = json!({"base_url": conn.base_url, "key_configured": conn.api_key.is_some()});
    let mut result = match list_models(&Http::new(Some(Duration::from_secs(15))), &conn).await {
        Err(err) => json!({"ok": false, "available_models": [], "models": [], "error": err.message, "error_category": err.category}),
        Ok(ids) => {
            let mut available: Vec<String> = Vec::new();
            for id in ids.into_iter().filter(|id| is_listable_image_model(id)) {
                if !available.contains(&id) {
                    available.push(id);
                }
            }
            let models: Vec<Value> = available
                .iter()
                .map(|id| {
                    let gemini = backend(id) == Backend::Gemini;
                    let gpt = id.to_lowercase().starts_with("gpt-image-");
                    let provider = if gemini {
                        "Google Gemini"
                    } else if gpt {
                        "OpenAI"
                    } else {
                        "xAI Grok"
                    };
                    json!({"id": id, "provider": provider, "reference_images": gemini || gpt})
                })
                .collect();
            let mut ok = json!({"ok": true, "available_models": available, "models": models});
            if available.is_empty() {
                ok["ok"] = json!(false);
                ok["error"] = json!("프록시 목록에 이 CLI가 지원하는 이미지 모델이 없습니다.");
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

fn main() {
    let command = match parse_args() {
        Ok(command) => command,
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
            let (result, code) = runtime.block_on(models());
            emit(&result, code)
        }
        other => match read_spec(other) {
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
