---
name: ai-image-generator
description: Use for generating, editing, restyling, upscaling, or batch-creating images through the local CLIProxyAPI image generator, including reference images, ImageGallery, blueprints, floorplans, CAD maps, airport/T2IN visuals, Gemini, and GPT Image requests.
---

# AI Image Generator

## Local installation

Always use this repository and entry point:

- Project: `C:/Users/jisung/workspaces/ai-image-generator`
- Entry point: `C:/Users/jisung/workspaces/ai-image-generator/tools/ai_image.py`
- Run with `cwd` set to the project root.

Do not assume `E:/ai-image-generator`. Do not call `scripts/gen_image.py` directly for agent work; `tools/ai_image.py` provides stable JSON input/output, edit-action forwarding, per-job timeouts, resume support, and batch orchestration.

## CLIProxyAPI configuration

- `CLIPROXY_BASE_URL=http://100.98.54.122:8317/v1`
- Read `CLIPROXY_API_KEY` from the process or Windows user environment only.
- Never write the API key into the repository, specs, prompts, logs, or output metadata.
- Before diagnosing model behavior, verify the authenticated `/v1/models` request succeeds.

## Providers

- `gpt-image-2`: default and recommended, especially for reference-image editing.
- `gpt-image-1.5`: available alternative.
- `gemini-3.1-flash-image`: fast Gemini image generation with reference images.
- `gemini-3.1-pro`: reasoning/vision model, not the image generator.
- `grok-imagine-*`: use only if `/v1/models` confirms availability; reference editing is not implemented by this CLI.

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
| GPT reference edit | `image_model: "gpt-image-2"`, `action: "edit"` |
| New composition | `action: "generate"` |
| Partial-run recovery | same topic/count plus `resume: true` |

`action` accepts `auto`, `generate`, or `edit`. It is supported at the top level for `single` and `jobs`, and each job may override it. Use `edit` for GPT reference restyling where source preservation matters.

## Exact-structure floorplan workflow

Generative models do not guarantee CAD-coordinate identity. For the best structure retention:

1. Use the structure/blueprint image as the only reference.
2. Use `gpt-image-2` with `action: "edit"`.
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
  "image_model": "gpt-image-2",
  "action": "edit",
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
