---
name: ai-image-generator
description: Use for generating, editing, restyling, upscaling, or batch-creating images through the local CLIProxyAPI image generator, including reference images, ImageGallery, blueprints, floorplans, CAD maps, airport/T2IN visuals, Gemini, and GPT Image requests.
---

# AI Image Generator

## Local installation

Always use the local checkout of the `ai-image-generator` repository and its `tools/ai_image.py` entry point. The checkout path differs per machine:

| Machine | Project root |
|---|---|
| `DESKTOP-SB818KQ` (Tailscale `100.98.54.122`) | `E:/ai-image-generator` |
| `JS` (Tailscale `100.67.54.25`) | `C:/Users/jisung/workspaces/ai-image-generator` |

Resolve the root by testing those paths in order and using the first that exists; never hardcode the other machine's path. Run with `cwd` set to that root.

Do not call `scripts/gen_image.py` directly for agent work; `tools/ai_image.py` provides stable JSON input/output, edit-action forwarding, per-job timeouts, resume support, and batch orchestration.

The repository is the single source of truth. Agent skill folders are directory links into `<repo>/skills/`; never copy this file elsewhere. Machine topology, sync procedure, and verified provider status live in `docs/SSOT.md`.

## CLIProxyAPI configuration

- CLIProxyAPI runs only on `DESKTOP-SB818KQ`, bound to `0.0.0.0:8317`.
- On `DESKTOP-SB818KQ`: `CLIPROXY_BASE_URL=http://127.0.0.1:8317/v1`.
- On every other machine: `CLIPROXY_BASE_URL=http://100.98.54.122:8317/v1` over Tailscale.
- Read `CLIPROXY_API_KEY` from the process or Windows user environment only.
- Never write the API key into the repository, specs, prompts, logs, or output metadata.
- Before diagnosing model behavior, verify the authenticated `/v1/models` request succeeds.

## Providers

- Default `image_model` is `grok-imagine-image-2.0`. Grok rejects reference images, so reference work uses `gemini-3.1-flash-image`.
- `grok-imagine-image-2.0`, `grok-imagine-image-quality`, `grok-imagine-image`: xAI text-to-image. Verified working 2026-08-28. Reference editing is not implemented by this CLI.
- `gemini-3.1-flash-image`: Gemini native generation with reference images. Verified working 2026-08-28. Use this for every reference-image job.
- `gpt-image-2`, `gpt-image-1.5`: currently broken upstream. The Responses main model returns malformed text instead of an `image_generation_call`, so the CLI reports "No image_generation_call result found in response." Do not route reference edits here until `docs/SSOT.md` marks them working again.
- `gemini-3.1-pro`: reasoning/vision model, not the image generator.

## Hard rules

- Default to exactly one image unless the user gives a count.
- “각각”, “한 장씩”, or “one each” means one output per listed item; use `mode: "jobs"`.
- Default final output to `1920x1080`, `quality: "high"` unless the user requests a draft or another size.
- Never delete or overwrite an existing image unless explicitly requested. Use a new topic or `resume: true`.
- Batch mode must run with `dry_run: true` first, then run the inspected plan.
- Never claim success until JSON returns `ok: true`, `failures` is empty, every output exists, and the output count matches.
- Inspect saved images before describing visual correctness.
- Do not use OMP native image generation for covered requests.

## Output roots

For T2IN work, prefer:

`E:/CityAI/IncheonProject/t2in-dev/ImageGallery/<request-slug>`

If the `E:` drive is unavailable, do not fail repeatedly. Use the existing workspace gallery:

`C:/Users/jisung/workspaces/cityai/t2in-dev/ImageGallery/<request-slug>`

For other projects, prefer `<workspace>/ImageGallery/<request-slug>` when that convention exists.

## Modes and key fields

| Need | Specification |
|---|---|
| One/repeated prompt | `mode: "single"`, `prompt`, optional `count` |
| Explicit different jobs | `mode: "jobs"`, `jobs: [{id, prompt, ...}]` |
| Preset dataset | `mode: "batch"`, `preset`, `count`, first `dry_run: true` |
| References | `reference_images: ["absolute/path.png"]` |
| Reference restyle | `image_model: "gemini-3.1-flash-image"`; `gpt-image-2` + `action: "edit"` only once `docs/SSOT.md` marks GPT working |
| New composition | `action: "generate"` |
| Partial-run recovery | same topic/count plus `resume: true` |
| Standard size | `1920x1080`; the CLI pads/buckets provider requests and saves the exact requested dimensions |
| Square size | `1024x1024` or another positive `WIDTHxHEIGHT`; dimensions no longer need to be divisible by 16 |
| Gemini/Antigravity | `image_model: "gemini-3.1-flash-image"`; required when using `reference_images`; native buckets `512`, `1K`, `2K`, `4K`, then exact-size conversion |
| Grok/xAI | default `image_model: "grok-imagine-image-2.0"`; text-to-image only; requires available xAI credits |
| Native image error/history pollution | Stop using native image calls; rerun through `tools/ai_image.py` with file outputs |

`action` accepts `auto`, `generate`, or `edit`. It is supported at the top level for `single` and `jobs`, and each job may override it. `edit` only changes the GPT Responses path, so it has no effect while `gpt-image-*` is broken.

## Exact-structure floorplan workflow

Generative models do not guarantee CAD-coordinate identity. For the best structure retention:

1. Use the structure/blueprint image as the only reference.
2. Use `gemini-3.1-flash-image`. Switch to `gpt-image-2` with `action: "edit"` only after `docs/SSOT.md` records it working again.
3. Describe the desired visual style in text instead of supplying a second style-layout image.
4. Explicitly lock perimeter, partitions, openings, facility count/order/spacing, circulation, and source rotation.
5. Generate 1–2 samples first and compare them directly with the source.
6. Generate the requested count only after a sample passes visual inspection.
7. Use `resume: true` after cancellation or partial failure to skip existing numbered outputs.

A style reference containing architecture can leak its rooms, counters, gates, empty-space ratios, and circulation into the result. If exact structure is more important than style matching, do not pass that style image to the model. Extract its palette, line treatment, facility vocabulary, and lighting through analysis, then encode those traits in the prompt.

## Strict flat top-view workflow

Terms such as `3D`, `2.5D`, `digital twin`, `realistic materials`, `ambient occlusion`, `bevel`, and `soft shadows` can trigger visible height or perspective. When the user forbids 3D, explicitly require:

- exact 90-degree vertical overhead view;
- true orthographic projection;
- zero wall/object height and no visible side faces;
- flat 2D vector shapes, solid fills, and uniform thin strokes;
- no shadows, gradients implying depth, extrusion, bevel, perspective, or photorealism.

For airport security maps, keep facility interpretation inside existing source footprints: X-ray conveyor belts, tray-return lanes, walk-through metal-detector frames, body-scanner circles, document readers, automated e-gates, and queue stanchions. Never add equipment outside source geometry.

## Reference edit example

```json
{
  "mode": "single",
  "prompt": "Preserve the complete source geometry and restyle it as a strict flat 90-degree airport security map. No moved, missing, duplicated, or invented structures; no 3D, perspective, shadows, text, logos, or watermark.",
  "topic": "airport-floorplan-flat-edit",
  "topic_root": "C:/Users/jisung/workspaces/cityai/t2in-dev/ImageGallery/airport-floorplan-flat-edit",
  "count": 2,
  "size": "1920x1080",
  "quality": "high",
  "image_model": "gemini-3.1-flash-image",
  "resume": true,
  "concurrency": 2,
  "reference_images": [
    "C:/absolute/path/structure-source.png"
  ]
}
```

## Performance and recovery

- Reference-heavy/high-resolution work should start with `concurrency: 2`.
- `job_timeout_sec` limits waiting; lowering it does not make generation faster.
- Long GPT runs may produce some outputs before interruption. Rerun the identical topic/count with `resume: true` rather than overwriting successful files.
- Use a fresh topic for materially changed prompts.

## Output verification

After each real run:

1. Parse the final JSON line.
2. Require `ok: true` and an empty `failures` list.
3. Confirm every listed path exists.
4. Confirm output count and dimensions.
5. Build or inspect a contact sheet for multi-image work.
6. Compare top candidates directly against the structure source.
7. Report observed deviations honestly; do not call generative output CAD-exact without evidence.
