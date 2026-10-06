---
name: ai-image-generator
description: Use for generating, editing, restyling, upscaling, or batch-creating images through the local CLIProxyAPI image generator, including reference images, ImageGallery, blueprints, floorplans, CAD maps, airport/T2IN visuals, Gemini, and GPT Image requests.
---

# AI Image Generator

## Before generating: destination and model

1. Resolve the destination from the user's request. An explicitly named project, or a new project the user asks you to create, is the destination root. Use the active workspace only when no project destination was requested. Announce the absolute project root and planned image path before generation; run this repository's tools with that root explicitly in `topic_root`.
2. Query the current proxy with `python tools/ai_image.py --list-models` from the repository root. This returns only supported image IDs and their reference-image support; an advertised model can still fail because of quota/provider errors. If discovery fails, report the connection error instead of presenting the static provider notes below as an available list.
3. On the first generation request in a conversation without a chosen model, show the available exact IDs (including `-flare` and `-sunburst`) and ask which to use. For reference work, identify GPT/Gemini as compatible and Grok as text-only. A provider name such as "GPT" narrows the choices but does not select a variant. Use `request_user_input_async` or the available question tool with exact model IDs as choices; wait for the user's submitted answer before calling a generation API. If no question tool is available, ask in the final response and resume generation after the answer. Elapsed time is not a selection. Prompt preparation and dry-run planning can continue while waiting.
4. An exact model in the request, or a previous user selection in this conversation, supplies the choice. Explicit delegation such as "알아서", "추천 모델로", or "기본값으로" also permits choosing a suitable available model. Reuse the choice for follow-ups; ask again only when requested or when it cannot handle the task. Keep this preference in conversation context, without creating onboarding/settings files.
5. Pass the selected ID unchanged as `image_model` in every executed spec (and in per-job overrides when applicable). For example, choosing `gpt-image-2.5-sunburst` must send that complete ID rather than `gpt-image-2.5`. A missing/unavailable choice requires a new selection; do not silently substitute another variant.

## Local installation

Always use the local checkout and its `tools/ai_image.py` entry point. Resolve this loaded `SKILL.md` to its real path through the directory link; its third parent is the repository root (`<repo>/skills/ai-image-generator/SKILL.md`). Confirm `tools/ai_image.py` exists there. Use the user's explicit checkout when supplied, after verifying the same entry point. Run with `cwd` set to that root; no drive letter, username, or hostname is required.

Use `<repo>/.venv/Scripts/python.exe` on Windows or `<repo>/.venv/bin/python` on macOS/Linux. The `python` commands below mean that interpreter. For first installation, a missing/broken environment, updates, or link repair, read `<repo>/INSTALL.md` and run `scripts/setup.py` using a working Python 3.11+. Explicit `--agent codex`, `--agent claude`, and `--agent omp` create the selected skill roots on a fresh PC. Verify with the same arguments plus `--check`. Keep the checkout at a stable path so its skill links stay valid.

Do not call `scripts/gen_image.py` directly for agent work; `tools/ai_image.py` provides stable JSON input/output, edit-action forwarding, per-job timeouts, resume support, and batch orchestration.

The repository is the single source of truth. Agent skill folders are directory links into `<repo>/skills/`; never copy this file elsewhere. Machine topology, sync procedure, and verified provider status live in `docs/SSOT.md`.

## CLIProxyAPI configuration

- Use the proxy URL configured by the user through `CLIPROXY_BASE_URL`; the repository installs the image client, not a proxy/provider account. New PCs must have a reachable proxy and its API key. For connection setup and troubleshooting read `<repo>/INSTALL.md`.
- Use `scripts.gen_image.resolve_base_url(None)` from the repository root for connection checks; CLI, Studio, and the PowerShell menu share this resolver. It honors explicit/process/Windows user settings before the historical defaults documented in `docs/SSOT.md`. The known proxy host normalizes its own historical Tailscale address to loopback. Check the actual hostname only when using that existing two-PC topology.
- Read `CLIPROXY_API_KEY` from the process or Windows user environment only.
- Never write the API key into the repository, specs, prompts, logs, or output metadata.
- Before diagnosing model behavior, verify the authenticated `/v1/models` request succeeds.

## Providers

- Direct CLI calls fall back to `grok-imagine-image-2.0`; skill use follows the selection workflow above and explicitly sets `image_model`. Grok rejects reference images; use a selected GPT image model with `action: "edit"` or Gemini for reference work.
- `grok-imagine-image-2.0`, `grok-imagine-image-quality`, `grok-imagine-image`: xAI text-to-image. Verified working 2026-08-28. Reference editing is not implemented by this CLI.
- `gemini-3.1-flash-image`: Gemini native generation with reference images. Verified working 2026-08-28.
- `gpt-image-2.5`: latest GPT image model. Direct generation and reference editing through the Image API verified working 2026-09-30. Prefer it for GPT requests unless the user names another model.
- `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`: GPT Image 2.5 variants. Direct generation verified working 2026-09-30; they use the same Image API generation/edit path.
- `gpt-image-2`: previous OpenAI image model (snapshot `gpt-image-2-2026-04-21`). Direct generation and reference editing verified working 2026-09-04.
- `gpt-image-1.5`: previous OpenAI image model. The direct Image API path is supported but has not been re-verified since replacing the broken Responses path.
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

## Output layout

Always set `topic_root` to the destination project/workspace, even though the command runs from this repository. For example, ordinary work in `E:/workspaces/test` uses `topic_root: "E:/workspaces/test"`. If the user asks for a new project there, create an appropriately named project directory and set `topic_root` to that directory instead. Follow the workspace's existing project convention (for example, `projects/<project>`), but keep its gallery inside that project. A shared gallery is supported when the user explicitly chooses `E:/workspaces/ImageGallery`.

All generation modes use one layout:

```text
<destination-project-or-workspace>/ImageGallery/
  output/<topic>/         images only; this directory can be zipped by itself
  metadata/<topic>/       one <image-filename>.json per image: prompt, exact requested image_model, size, quality, action, references, output, and response_model when supplied by the provider
```

`topic_root` may also be the `ImageGallery` directory itself. Pass the project/workspace or gallery, rather than constructing nested topic/output roots. The CLI defaults to its caller's current directory; agent commands must explicitly carry the resolved destination. In particular, `E:/workspaces/blendermcp/projects/<project>/ImageGallery/` is the destination for an image project created under `projects/`; the tool repository and the parent studio are execution locations, not substitute output roots.

Use absolute reference paths. Keep any task-specific scripts, input specifications, reference copies, documents, and inspection records in `metadata/<topic>/` only when needed. Generation already records the prompt; a second prompt.txt, spec copy, raw provider response, log, contact sheet, or upscale is created only when the task needs it. A simple image request does not need a new project README, manifest, job.json, references.json, or verification.json unless the workspace requires one or the user asks. Plan with `dry_run: true` in any CLI mode; planning does not call a provider or create files. Studio keeps one job.json and optional uploaded references in metadata; MCP keeps the minimal state needed for cancellation/recovery under metadata/_mcp.

For T2IN work, use the user's actual T2IN checkout as the destination rather than assuming another PC's path. Preserve existing successful images with `resume: true`; materially changed prompts use a new topic. Existing output files are rejected rather than overwritten.

## Modes and key fields

| Need | Specification |
|---|---|
| One/repeated prompt | `mode: "single"`, `prompt`, optional `count` |
| Explicit different jobs | `mode: "jobs"`, `jobs: [{id, prompt, ...}]` |
| Preset dataset | `mode: "batch"`, `preset`, `count`, first `dry_run: true` |
| References | `reference_images: ["absolute/path.png"]` |
| Reference restyle | `image_model: "gpt-image-2.5"`, `action: "edit"`; Gemini remains available for comparison |
| New composition | `action: "generate"` |
| Partial-run recovery | same topic/count plus `resume: true` |
| Standard size | `1920x1080`; the CLI pads/buckets provider requests and saves the exact requested dimensions |
| Square size | `1024x1024` or another positive `WIDTHxHEIGHT`; dimensions no longer need to be divisible by 16 |
| Gemini/Antigravity | `image_model: "gemini-3.1-flash-image"`; supports `reference_images`; native buckets `512`, `1K`, `2K`, `4K`, then exact-size conversion |
| Grok/xAI | default `image_model: "grok-imagine-image-2.0"`; text-to-image only; requires available xAI credits |
| Native image error/history pollution | Stop using native image calls; rerun through `tools/ai_image.py` with file outputs |

`action` accepts `auto`, `generate`, or `edit`. It is supported at the top level for `single` and `jobs`, and each job may override it. For `gpt-image-*`, reference inputs use `/v1/images/edits`; `action: "edit"` requires at least one reference image.

## Exact-structure floorplan workflow

Generative models do not guarantee CAD-coordinate identity. For the best structure retention:

1. Use the structure/blueprint image as the only reference.
2. Use `gpt-image-2.5` with `action: "edit"`; compare 1–2 samples with Gemini when structure retention is critical.
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
  "topic_root": "C:/Users/jisung/workspaces/cityai/t2in-dev",
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
5. Inspect saved images; make a contact sheet only when it helps compare a large set.
6. Compare top candidates directly against the structure source.
7. Report observed deviations honestly; do not call generative output CAD-exact without evidence.

## Required model disclosure

Every final response when using this skill must state the exact requested `image_model`, including its variant (for example, `사용 모델(요청 ID): gpt-image-2.5-sunburst`). Read it from the saved image metadata or executed specification, not from an assumed default; resumed images use their existing metadata. If `response_model` is present, also report the provider's returned identifier, especially if it differs. An alias such as `gpt-image-2.5` is not proof that `sunburst` ran internally: when the proxy omits the resolved model, say the internal variant is unverified if the user asks. For multiple models, map each output/group to its model. If generation fails, state the attempted model and failure; if no model was invoked, explicitly say generation did not run and label any requested model as planned. Include this disclosure even when the response otherwise only links to or displays an image, and link the saved file with an absolute path so the destination is clear.
