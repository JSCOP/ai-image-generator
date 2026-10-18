//! `--schema` and `--example` output. Keys and defaults follow `tools/ai_image.py`.

use crate::paths::absolute;
use crate::runner::{DEFAULT_CONCURRENCY, DEFAULT_IMAGE_MODEL, DEFAULT_RETRIES, MAX_CONCURRENCY, MAX_RETRIES, MAX_SINGLE_COUNT};
use serde_json::{json, Value};

pub fn schema_json() -> Value {
    let cwd = absolute(&std::env::current_dir().unwrap_or_default()).display().to_string();
    json!({
        "mode": {"type": "string", "enum": ["single", "jobs", "batch"], "default": "single"},
        "prompt": {"type": "string", "required_when": "mode=single"},
        "topic": {"type": "string", "default": "ai-image-request"},
        "count": {"type": "integer", "default": 1, "min": 1, "max_when_single": MAX_SINGLE_COUNT},
        "size": {"type": "string", "format": "WIDTHxHEIGHT (positive integers)", "default": "1920x1080"},
        "quality": {"type": "string", "enum": ["low", "medium", "high"], "default": "high"},
        "model": {"type": "string", "description": "ignored with a warning: the legacy Responses API path was removed"},
        "image_model": {
            "type": "string", "default": DEFAULT_IMAGE_MODEL, "description": "gpt-image-*, gemini-*-image, or grok-imagine-*",
            "examples": ["gpt-image-2.5", "gpt-image-2.5-flare", "gpt-image-2.5-sunburst", "gpt-image-2", "gemini-3.1-flash-image",
                         "grok-imagine-image-2.0", "grok-imagine-image-quality", "grok-imagine-image"],
        },
        "action": {"type": "string", "enum": ["auto", "generate", "edit"], "default": "generate", "description": "image tool action; use edit for GPT reference-image restyling"},
        "reference_images": {"type": "array<string>", "description": "image paths", "default": []},
        "topic_root": {"type": "string", "description": "workspace or ImageGallery root", "default": cwd},
        "preset": {"type": "string", "required_when": "mode=batch", "description": "path to preset JSON"},
        "jobs": {"type": "array<object>", "required_when": "mode=jobs",
                 "description": "per-image specs; each item: {id, prompt, reference_images?, size?, quality?, action?, image_model?, background?, fit?}"},
        "prompt_prefix": {"type": "string", "description": "shared preamble prepended to each job's prompt (mode=jobs)"},
        "prompt_suffix": {"type": "string", "description": "shared suffix appended to each job's prompt (mode=jobs)"},
        "concurrency": {"type": "integer", "default": DEFAULT_CONCURRENCY, "min": 1, "max": MAX_CONCURRENCY},
        "retries": {"type": "integer", "default": DEFAULT_RETRIES, "min": 0, "max": MAX_RETRIES,
                    "description": "extra attempts after HTTP 408/425/429/5xx, connection loss, or timeout"},
        "resume": {"type": "boolean", "default": false, "description": "skip outputs whose record matches this request; a different request is reported, never overwritten"},
        "dry_run": {"type": "boolean", "default": false},
        "seed": {"type": "integer", "optional": true},
        "job_timeout_sec": {"type": "number", "default": 600, "description": "per-image wall-clock timeout seconds; <=0 disables"},
        "background": {"type": "string", "enum": ["auto", "opaque", "transparent"], "default": "auto",
                       "description": "transparent needs gpt-image-2.5-sunburst or gpt-image-2.5-flare; checked before any request"},
        "fit": {"type": "string", "enum": ["cover", "contain"],
                "description": "how provider output becomes the exact size: cover crops the centre, contain pads with transparency; default contain for transparent, cover otherwise"},
        "mask": {"type": "string", "description": "not supported yet (planned for stage 4); rejected before any request"},
        "keep_outside_mask": {"type": "boolean", "description": "not supported yet (planned for stage 4); rejected before any request"},
        "remove_background": {"type": "boolean", "description": "not supported yet (planned for stage 4); true is rejected before any request"},
        "profile": {"type": "string", "description": "connection profile name or id (see `image-cli profile list`; same as `image-cli --profile`); used instead of CLIPROXY_* environment variables; top level only"},
    })
}

pub fn examples_json() -> Value {
    json!({
        "single_minimal": {"prompt": "고양이가 우주를 유영하는 시네마틱 사진", "topic": "cat-in-space"},
        "single_grok": {
            "mode": "single", "prompt": "A centered matte green triangle on a clean white background, no text or watermark.",
            "topic": "grok-image-test", "image_model": "grok-imagine-image-2.0", "size": "1024x1024", "quality": "low",
        },
        "single_transparent": {
            "prompt": "A blue ceramic mug with a gold handle, isolated product cutout", "topic": "mug-cutout",
            "image_model": "gpt-image-2.5-sunburst", "background": "transparent", "size": "1024x1024",
        },
        "single_full": {
            "mode": "single", "prompt": "wizard casting fireball in a torch-lit dungeon", "topic": "wizard-test", "count": 3,
            "size": "1920x1088", "quality": "high", "action": "edit",
            "reference_images": ["E:/ai-image-generator/ImageGallery/output/extraction-rpg-dad/ui_combat_hud/extraction-rpg-dad_000001.png"],
        },
        "batch_resume": {"mode": "batch", "preset": "presets/extraction-rpg.json", "count": 50, "concurrency": 16, "resume": true},
        "jobs_with_per_image_refs": {
            "mode": "jobs", "topic": "airport-topview-eagle-batch10", "size": "1920x1088", "quality": "high", "concurrency": 10,
            "prompt_prefix": "Strict vertical orthographic top-down view of the EXACT physical airport space shown in the reference photo.",
            "prompt_suffix": "Constraints: no invented columns, no extra walls, no unrelated furniture.",
            "jobs": [
                {"id": "01_checkin_hall", "prompt": "wide departure check-in hall ...", "reference_images": ["D:/Eagle/CityAI.library/images/.../photo.jpg"]},
                {"id": "02_info_island", "prompt": "information desk island with circular help counter ...", "reference_images": ["D:/Eagle/CityAI.library/images/.../photo.jpg"]},
            ],
        },
    })
}
