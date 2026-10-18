//! Preset batch mode (`scripts/gen_batch.py`). Plans match CPython's `random.Random` so
//! existing datasets keep the same prompt per index.

use crate::error::{Category, EngineError};
use crate::paths::{absolute, safe_topic_slug, topic_dirs};
use crate::runner::{error_result, py_str, truthy, ImageFields, Item, Runner, JOB_EVENTS};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha512};
use std::path::{Path, PathBuf};
use std::time::Instant;

const N: usize = 624;
const MAX_FAILURES: usize = 20;

/// CPython `random.Random` (MT19937, seeded from a string with version 2).
pub struct PyRandom {
    mt: [u32; N],
    index: usize,
}

impl PyRandom {
    fn from_key(key: &[u32]) -> PyRandom {
        let mut r = PyRandom { mt: [0; N], index: N };
        r.mt[0] = 19650218;
        for i in 1..N {
            let prev = r.mt[i - 1];
            r.mt[i] = 1812433253u32.wrapping_mul(prev ^ (prev >> 30)).wrapping_add(i as u32);
        }
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            let prev = r.mt[i - 1];
            r.mt[i] = (r.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1664525)).wrapping_add(key[j]).wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                r.mt[0] = r.mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let prev = r.mt[i - 1];
            r.mt[i] = (r.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                r.mt[0] = r.mt[N - 1];
                i = 1;
            }
        }
        r.mt[0] = 0x8000_0000;
        r
    }

    /// `random.Random(text)`: seed = int.from_bytes(text + sha512(text), "big").
    pub fn from_str_seed(text: &str) -> PyRandom {
        let mut bytes = text.as_bytes().to_vec();
        bytes.extend_from_slice(&Sha512::digest(text.as_bytes()));
        // 32-bit words, least significant first, without high zero words.
        let mut key: Vec<u32> = bytes
            .rchunks(4)
            .map(|chunk| chunk.iter().fold(0u32, |acc, b| (acc << 8) | *b as u32))
            .collect();
        while key.len() > 1 && key.last() == Some(&0) {
            key.pop();
        }
        PyRandom::from_key(&key)
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for k in 0..N {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % N] & 0x7fff_ffff);
                let mut v = self.mt[(k + 397) % N] ^ (y >> 1);
                if y & 1 != 0 {
                    v ^= 0x9908_b0df;
                }
                self.mt[k] = v;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    fn getrandbits(&mut self, k: u32) -> u64 {
        if k <= 32 {
            return (self.next_u32() >> (32 - k)) as u64;
        }
        let (mut result, mut shift, mut remaining) = (0u64, 0u32, k);
        while remaining > 0 {
            let mut word = self.next_u32();
            if remaining < 32 {
                word >>= 32 - remaining;
            }
            result |= (word as u64) << shift;
            shift += 32;
            remaining = remaining.saturating_sub(32);
        }
        result
    }

    /// `Random._randbelow(n)` (rejection sampling), used by `randrange` and `choice`.
    pub fn randbelow(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        let k = 64 - n.leading_zeros();
        loop {
            let r = self.getrandbits(k);
            if r < n {
                return r;
            }
        }
    }
}

pub struct PresetCategory {
    pub name: String,
    pub weight: u64,
    pub templates: Vec<String>,
}

pub struct Preset {
    pub topic: String,
    pub size: String,
    pub quality: String,
    pub image_model: Option<String>,
    pub global_style: String,
    pub negative: String,
    pub categories: Vec<PresetCategory>,
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(Category::InvalidRequest, message)
}

pub fn load_preset(path: &Path) -> Result<Preset, EngineError> {
    let text = std::fs::read_to_string(path).map_err(|e| invalid(format!("cannot read preset {}: {e}", path.display())))?;
    let data: Value = serde_json::from_str(&text).map_err(|e| invalid(format!("invalid preset JSON {}: {e}", path.display())))?;
    let text_of = |key: &str, default: &str| data.get(key).filter(|v| !v.is_null()).map(py_str).unwrap_or_else(|| default.into());
    let raw = data["categories"].as_array().cloned().unwrap_or_default();
    if raw.is_empty() {
        return Err(invalid(format!("Preset {} has no categories.", path.display())));
    }
    let mut categories = Vec::new();
    for c in raw {
        let name = c.get("name").map(py_str).ok_or_else(|| invalid("preset category without name"))?;
        let weight = c.get("weight").and_then(Value::as_f64).unwrap_or(1.0).trunc().max(1.0) as u64;
        let templates: Vec<String> = c["templates"].as_array().into_iter().flatten().map(py_str).collect();
        if templates.is_empty() {
            return Err(invalid(format!("Category '{name}' has no templates.")));
        }
        categories.push(PresetCategory { name, weight, templates });
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    Ok(Preset {
        topic: if truthy(data.get("topic")) { py_str(&data["topic"]) } else { stem },
        size: text_of("size", "1920x1080"),
        quality: text_of("quality", "high"),
        image_model: data.get("image_model").filter(|v| truthy(Some(v))).map(py_str),
        global_style: text_of("global_style", ""),
        negative: text_of("negative", ""),
        categories,
    })
}

#[derive(Serialize, Clone, Debug)]
pub struct BatchPlan {
    pub id: String,
    pub prompt: String,
    pub category: String,
    pub image_model: String,
    pub output: String,
}

fn plan_one(preset: &Preset, rng: &mut PyRandom) -> (String, String) {
    let total: u64 = preset.categories.iter().map(|c| c.weight).sum();
    let pick = rng.randbelow(total);
    let mut cursor = 0;
    let category = preset
        .categories
        .iter()
        .find(|c| {
            cursor += c.weight;
            pick < cursor
        })
        .unwrap_or(preset.categories.last().unwrap());
    let template = &category.templates[rng.randbelow(category.templates.len() as u64) as usize];
    let mut parts = vec![template.trim().to_string()];
    if !preset.global_style.trim().is_empty() {
        parts.push(preset.global_style.trim().to_string());
    }
    if !preset.negative.trim().is_empty() {
        parts.push(format!("Constraints: {}", preset.negative.trim()));
    }
    (category.name.clone(), parts.join(" "))
}

fn batch_topic(preset: &Preset, topic: Option<&str>) -> String {
    let topic = topic.map(str::trim).filter(|t| !t.is_empty()).unwrap_or(&preset.topic);
    safe_topic_slug(topic)
}

/// Same ids, prompts and categories as `gen_batch.py --dry-run` for the same seed.
pub fn plan_batch(preset_path: &Path, count: u32, seed: i64, topic: Option<&str>, image_model: Option<&str>, workspace: &Path) -> Result<Vec<BatchPlan>, EngineError> {
    let preset = load_preset(preset_path)?;
    Ok(plans_for(&preset, count, seed, topic, image_model, workspace))
}

fn plans_for(preset: &Preset, count: u32, seed: i64, topic: Option<&str>, image_model: Option<&str>, workspace: &Path) -> Vec<BatchPlan> {
    let slug = batch_topic(preset, topic);
    let (output_dir, _) = topic_dirs(workspace, &slug);
    let model = image_model.map(str::to_string).or_else(|| preset.image_model.clone()).unwrap_or_else(|| crate::runner::DEFAULT_IMAGE_MODEL.into());
    (1..=count)
        .map(|index| {
            let mut rng = PyRandom::from_str_seed(&format!("{seed}:{slug}:{index}"));
            let (category, prompt) = plan_one(preset, &mut rng);
            let id = format!("{slug}_{index:06}");
            let output = output_dir.join(format!("{id}.png")).display().to_string();
            BatchPlan { id, prompt, category, image_model: model.clone(), output }
        })
        .collect()
}

/// Relative presets resolve against the working directory, then the checkout that holds this executable.
fn resolve_preset(run: &Runner, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return path;
    }
    let from_cwd = run.resolve_path(value);
    if from_cwd.is_file() {
        return from_cwd;
    }
    let exe = std::env::current_exe().ok();
    exe.iter()
        .flat_map(|e| e.ancestors())
        .map(|dir| dir.join(&path))
        .find(|candidate| candidate.is_file())
        .map(|p| absolute(&p))
        .unwrap_or(from_cwd)
}

pub(crate) async fn run_batch(run: &Runner) -> Value {
    match run_batch_inner(run).await {
        Ok(value) => value,
        Err(err) => error_result(&err),
    }
}

async fn run_batch_inner(run: &Runner) -> Result<Value, EngineError> {
    if !truthy(run.spec.get("preset")) {
        return Err(invalid("spec.preset is required when mode=batch"));
    }
    let preset_path = resolve_preset(run, &py_str(&run.spec["preset"]));
    if !preset_path.is_file() {
        return Err(invalid(format!("preset not found: {}", preset_path.display())));
    }
    let preset = load_preset(&preset_path)?;
    let number = |key: &str, default: i64| match run.get(key) {
        None => Ok(default),
        Some(v) => v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).map(|n| n.trunc() as i64).ok_or_else(|| invalid(format!("{key} must be a number"))),
    };
    let count = number("count", 10)?.max(1) as u32;
    let seed = number("seed", 42)?;
    let limits = run.limits()?;
    let workspace = run.topic_root();
    let topic = run.get("topic").map(py_str);
    let image_model = if truthy(run.spec.get("image_model")) {
        Some(py_str(&run.spec["image_model"]))
    } else if preset.image_model.is_none() {
        run.env.process("CLIPROXY_IMAGE_MODEL")
    } else {
        None
    };
    let slug = batch_topic(&preset, topic.as_deref());
    let (output_dir, metadata_dir) = topic_dirs(&workspace, &slug);
    let plans = plans_for(&preset, count, seed, topic.as_deref(), image_model.as_deref(), &workspace);
    let dirs = json!({"topic_dir": output_dir.display().to_string(), "output_dir": output_dir.display().to_string(),
        "metadata_dir": metadata_dir.display().to_string()});
    if truthy(run.spec.get("dry_run")) {
        let mut result = json!({"ok": true, "mode": "batch", "dry_run": true, "outputs": [], "failures": [], "planned_outputs": plans});
        merge(&mut result, dirs);
        return Ok(result);
    }
    let started = Instant::now();
    let size = json!(preset.size);
    let mut items = Vec::new();
    let mut failures = Vec::new();
    for (idx, plan) in plans.iter().enumerate() {
        let fields = ImageFields {
            prompt: plan.prompt.clone(),
            size: &size,
            quality: preset.quality.clone(),
            image_model: plan.image_model.clone(),
            action: "generate".into(),
            references: Vec::new(),
            background: None,
            fit: None,
        };
        match run.build_request(fields) {
            Err(err) => failures.push(json!({"id": plan.id, "reason": err.reason(), "error": err})),
            Ok((request, fit)) => {
                let output = absolute(Path::new(&plan.output));
                let mut event = Map::new();
                event.insert("mode".into(), json!("batch"));
                event.insert("id".into(), json!(plan.id));
                event.insert("index".into(), json!(idx));
                event.insert("output".into(), json!(output.display().to_string()));
                let job = run.job_spec(request, fit, output, &workspace, &slug, limits.timeout);
                items.push(Item { index: idx, id: Some(plan.id.clone()), job, event });
            }
        }
    }
    let workers = limits.concurrency.min(plans.len());
    let mut outputs = Vec::new();
    let mut retried = 0;
    if !items.is_empty() {
        let ctx = run.connect(&limits)?;
        std::fs::create_dir_all(&output_dir).map_err(|e| EngineError::new(Category::Internal, format!("cannot create {}: {e}", output_dir.display())))?;
        for r in run.execute(items, workers, limits.retries, ctx, &JOB_EVENTS, Some(MAX_FAILURES)).await {
            retried += r.retried;
            match r.outcome {
                Ok(_) => outputs.push(r.output),
                Err(err) => failures.push(json!({"id": r.id, "reason": err.reason(), "error": err})),
            }
        }
    }
    outputs.sort();
    failures.sort_by_key(|f| f["id"].as_str().unwrap_or_default().to_string());
    let mut result = json!({
        "ok": failures.is_empty() && outputs.len() == plans.len(), "mode": "batch", "outputs": outputs,
        "failures": failures, "dry_run": false, "planned_outputs": [], "concurrency": workers, "retried": retried,
        "elapsed_sec": (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
    });
    merge(&mut result, dirs);
    Ok(result)
}

fn merge(target: &mut Value, extra: Value) {
    if let (Value::Object(target), Value::Object(extra)) = (target, extra) {
        target.extend(extra);
    }
}
