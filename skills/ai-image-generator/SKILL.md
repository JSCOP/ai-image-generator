---
name: ai-image-generator
description: Use when a user asks to generate, edit, reinterpret, restyle, upscale, or batch-create AI images with the local CLIProxyAPI image generator, especially requests mentioning ImageGallery, reference images, blueprints, floorplans, airport/T2IN visuals, native image_generation_call errors, or E:\\ai-image-generator.
---

# AI Image Generator

## Overview

Use the local `E:\ai-image-generator` project for AI image generation through CLIProxyAPI. This is separate from any built-in image tool; do not claim an image exists until the local command returns `ok: true` with concrete output paths.

Core safety rule: covered image-generation requests MUST go through the local CLI only. Native Responses/API image generation calls (`image_generation_call`, built-in image tools, or tool outputs embedded as base64 in chat history) are unsafe here because they can pollute OMP conversation history and break later turns with errors such as `400 Unknown parameter: input[N].action`.

## When to Use

Use this skill for:
- AI image generation, image editing, style transfer, restyling, reinterpretation, or upscaling requests.
- Requests that provide or mention reference images, floorplans, blueprints, CAD/vector maps, airport/T2IN visuals, UI/dashboard art, or `ImageGallery`.
- Batch image requests using presets or multiple explicit prompt jobs.

Do not use this skill for pure image analysis when the user does not ask to create or edit an image.

## Hard Rules

- Default to exactly one image unless the user specifies a count.
- A listed set of N spaces/items with wording like `one each`, `한 장씩`, or `각각` is an explicit count; use `mode: "jobs"` and produce exactly one output per listed item unless the user asks otherwise.
- If the user explicitly asks for final/high quality (`final`, `high`, `고퀄`, `최종`, `바로 최종`, `그냥 최종본`), generate final output directly: `1920x1080`, `quality: "high"`, full useful references, no quality-choice question.
- For multi-image, reference-heavy, T2IN/ImageGallery, or expensive jobs where the user does not specify draft vs final quality, ask one concise choice before generating: fast draft review or final high-quality output.
- Use fast draft review when the user prioritizes speed, exploration, variants, or approval before final: `1280x720` or `1024x576`, `quality: "medium"`, usually `concurrency: 2`, then rerun approved images at final settings.
- Never delete or overwrite existing generated images unless explicitly asked.
- Never say the image was created until `tools/ai_image.py` returns JSON with `ok: true` and output paths.
- Never use native Responses/API image generation (`image_generation_call`) or built-in image tools for requests covered by this skill, even when the user attaches reference images.
- Never leave a generated image only as a chat/base64 result; produce concrete files under the requested output package.
- If the current workspace has an `ImageGallery/` rule, set `topic_root` to `ImageGallery/<request-slug>` so outputs land under that request package.
- For T2IN workspace requests, keep generated assets under `E:/CityAI/IncheonProject/t2in-dev/ImageGallery/<request-slug>/`, not loose workspace files.
- For batch generation, run `dry_run: true` first, inspect planned outputs, then run the real job.
- If a previous turn used native image generation or the next turn fails with `Unknown parameter: input[N].action`, treat the session history as contaminated: explain that no reliable file path exists, avoid reusing the same polluted history for generation, and rerun via `E:/ai-image-generator/tools/ai_image.py` into `ImageGallery/<request-slug>/`.

## Quick Reference

| Need | Use |
|---|---|
| One image | `mode: "single"` |
| Multiple explicit prompts | `mode: "jobs"` |
| Preset dataset | `mode: "batch"` |
| Reference images | `reference_images: ["path"]` |
| Native image error/history pollution | Stop using native image calls; rerun through `tools/ai_image.py` with file outputs |
| T2IN organized output | `topic_root: "E:/CityAI/IncheonProject/t2in-dev/ImageGallery/<request-slug>"` |
| Standard size | `1920x1080`; the CLI pads/buckets provider requests and saves the exact requested dimensions |
| Square size | `1024x1024` or another positive `WIDTHxHEIGHT`; dimensions no longer need to be divisible by 16 |
| Gemini/Antigravity | `image_model: "gemini-3.1-flash-image"`; reference images supported; native buckets `512`, `1K`, `2K`, `4K`, then exact-size conversion |
| Grok/xAI | `grok-imagine-image*`; text-to-image only in this CLI and requires available xAI credits |

## Quality / Speed Workflow

- Final/direct mode: use `1920x1080`, `quality: "high"`, and the full reference set when the user clearly asks for final/high-quality output.
- Unspecified multi-image/reference-heavy jobs: ask whether to run a fast draft pass first or go straight to final quality.
- Draft pass: use `1280x720` or `1024x576`, `quality: "medium"`, `resume: true`, and `concurrency: 2` unless the backend is known to handle more. Prefer text style guidance over extra style reference images for draft speed.
- Final pass: regenerate only approved images at `1920x1080`, `quality: "high"`, with full source/style references.
- Do not lower `job_timeout_sec` expecting faster generation; it only limits how long to wait before killing a slow job.

## Command Pattern

Run from the generator project root:

```bash
python tools/ai_image.py <<'JSON'
{
  "mode": "single",
  "prompt": "Complete final prompt here. Include subject, action, composition, lighting, style, constraints, and negative constraints.",
  "topic": "blueprint-reinterpret-v1",
  "topic_root": "E:/CityAI/IncheonProject/t2in-dev/ImageGallery/blueprint-reinterpret",
  "count": 1,
  "size": "1920x1080",
  "quality": "high",
  "reference_images": [
    "E:/CityAI/IncheonProject/t2in-dev/ImageGallery/blueprint-reinterpret/input/reference-01.png"
  ]
}
JSON
```

Use tool execution with `cwd` set to `E:/ai-image-generator`.

## Prompt Requirements

Prompts must be final, self-contained, and specific:
- Subject and source interpretation goal.
- Camera/framing or top-view/isometric requirement.
- Style direction: dark navy blueprint, crisp SVG-like vector infographic, clean CAD lines, flat colors, organized layers, low/no noise.
- Preservation constraints: keep source floorplan/map structure, scale, zones, and circulation logic unless the user asks for redesign.
- Negative constraints: no arbitrary distortion, no random dotted routes/nodes, no paper texture, no real logos, no watermarks, no unreadable brand signage.

For CAD/vector map styling, prefer: clean dark navy blueprint-style or vector infographic, crisp SVG-like lines, low/no noise, flat colors, organized layers, no paper texture or grain.

## Output Handling

After a successful run:
1. Read the returned JSON.
2. Report the exact output paths.
3. If the workspace requires `ImageGallery`, keep or move final outputs under that package and preserve prompt/run metadata in `manifest.json` or `README.md` for non-trivial requests.
4. Do not describe visual details as observed unless you actually inspect the generated image file.

## Common Mistakes

| Mistake | Correction |
|---|---|
| Using the built-in image tool after the user requested this skill | Use `E:/ai-image-generator/tools/ai_image.py` instead. |
| Native `image_generation_call` appears in history or `Unknown parameter: input[N].action` occurs | Treat OMP history as polluted and rerun with the local CLI into `ImageGallery/<request-slug>/`. |
| Saying generation succeeded before seeing output JSON | Wait for `ok: true` and paths. |
| Leaving outputs in loose workspace root files | Use `ImageGallery/<request-slug>/`. |
| Batch-generating without a plan | Run `dry_run: true` first. |
| Relying on a reference image that is only in chat | Save or locate it as a real file path before calling the CLI. |
