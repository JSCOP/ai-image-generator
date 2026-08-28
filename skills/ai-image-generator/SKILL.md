---
name: ai-image-generator
description: Use for generating, editing, restyling, upscaling, or batch-creating images through the local CLIProxyAPI image generator, including reference images, ImageGallery, blueprints, floorplans, CAD maps, airport/T2IN visuals, Gemini, and GPT Image requests.
---

# AI Image Generator

## Required entry point

- Project: `C:/Users/jisung/workspaces/ai-image-generator`
- Command: `python tools/ai_image.py`
- Run the command with `cwd` set to the project root.
- Never call old one-off scripts or write generation artifacts to the project `output/` or `runs/` directories.
- Use the local CLI for covered image generation; do not use OMP native image generation.

## Mandatory storage layout

Every request must include the current conversation/workspace folder as `workspace_root`.

If the conversation is working in `E:/abc`, the final images must be stored as:

`E:/abc/ImageGallery/<YYYY-MM-DD>/<image-generation-title>/*.png`

Rules:

- `workspace_root` is the folder where the current conversation is working, not the generator project folder.
- `date` uses local `YYYY-MM-DD`; omit it only when today's date is correct.
- `topic` is the image-generation title and becomes the final title folder.
- The title folder must contain images only. Never place prompts, specs, manifests, logs, events, contact sheets, scripts, or reference files there.
- The CLI stores the reproducible request record under `C:/Users/jisung/workspaces/ai-image-generator/inputs/<YYYY-MM-DD>/<title>/request.json`.
- Reference images are copied under that input folder before generation. Use the archived copies for later reruns.
- Do not create loose temporary specs. Prefer stdin or `--json`. If a temporary helper file is unavoidable, keep it inside the generator project and delete it after use.
- Never overwrite an existing title folder. Use a new title, or set `resume: true` only to fill missing images.

## Default request

```json
{
  "mode": "single",
  "prompt": "Complete final prompt",
  "topic": "image-generation-title",
  "workspace_root": "E:/abc",
  "date": "2026-07-13",
  "count": 1,
  "size": "1920x1080",
  "quality": "high",
  "image_model": "gpt-image-2",
  "action": "generate",
  "reference_images": []
}
```

## Modes

- One prompt: `mode: "single"`; default exactly one image.
- Different listed items: `mode: "jobs"`; “각각”, “한 장씩”, or “one each” means one job per item.
- Preset dataset: `mode: "batch"`; always run `dry_run: true` first, inspect the plan, then run the real batch.
- Reference-preserving GPT edit: `image_model: "gpt-image-2"`, `action: "edit"`.
- Partial recovery: identical title/date/count plus `resume: true`.

## Provider and quality rules

- Default image model: `gpt-image-2`; `gpt-image-1.5` is an alternative.
- `gemini-3.1-flash-image` supports fast image generation with references.
- Default final output: `1920x1080`, `quality: "high"` unless the user explicitly requests draft quality or another size.
- Read `CLIPROXY_API_KEY` from the process or Windows user environment only. Never write it to any file.
- Default proxy: `CLIPROXY_BASE_URL=http://100.98.54.122:8317/v1`.

## Exact-structure edits

For floorplans, blueprints, CAD maps, and airport/T2IN structure retention:

1. Use the structure image as the only layout reference.
2. Use `gpt-image-2` with `action: "edit"`.
3. Encode style in text when a style reference could leak architecture.
4. Lock perimeter, partitions, openings, facility count/order/spacing, circulation, and rotation.
5. For strict flat maps require exact 90-degree orthographic view, zero visible height, flat fills, uniform strokes, and no shadows, depth gradients, extrusion, bevel, perspective, or photorealism.

## Verification

After every real run:

1. Require final JSON `ok: true` and an empty `failures` list.
2. Confirm every reported path exists and the count matches the request.
3. Confirm the title folder contains image files only.
4. Inspect saved images before claiming visual correctness.
5. Report exact final image paths; do not report project input-record files as deliverables.
