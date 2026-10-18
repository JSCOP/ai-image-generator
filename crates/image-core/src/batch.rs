//! Preset batch mode (`scripts/gen_batch.py`).

use crate::runner::Runner;
use serde_json::{json, Value};

pub(crate) async fn run_batch(_run: &Runner) -> Value {
    json!({"ok": false, "error": "batch mode is not implemented yet"})
}
