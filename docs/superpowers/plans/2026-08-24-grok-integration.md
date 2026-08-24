# Grok Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose all three available Grok image models through every image-generator entry point and make `cliproxy-xai/grok-4.6:xhigh` the working Oh My Pi default.

**Architecture:** Keep CLIProxyAPI as the existing provider seam. Extend only its consumers: the image generator continues using the `/v1/images/generations` adapter, while Oh My Pi adds Grok 4.6 to the existing `cliproxy-xai` provider registry. Do not add duplicate CPA aliases.

**Tech Stack:** Python 3 `unittest`, PowerShell 7, CLIProxyAPI OpenAI-compatible endpoints, Oh My Pi 18.0.3 YAML model registry.

**Spec:** `docs/superpowers/specs/2026-08-24-grok-integration-design.md`

## Global Constraints

- Supported image IDs are exactly `grok-imagine-image-2.0`, `grok-imagine-image`, and `grok-imagine-image-quality`.
- Keep image default `gpt-image-2`; recommend Grok 2.0 only when Grok is selected.
- Grok image generation remains text-to-image only; reject reference images before a network call.
- Register only text model `grok-4.6` in Oh My Pi; keep Grok 4.3 and 4.5.
- Make `cliproxy-xai/grok-4.6:xhigh` the Oh My Pi default.
- Do not modify `E:/cliproxyapi/release/config.yaml` unless verification disproves the observed live contract.
- Never print or persist credentials from CPA config/auth files.
- Live text and image requests require point-of-risk approval immediately before execution.

## Reference Map

- `scripts/gen_image.py:image_backend`, `build_openai_images_payload`, `call_native_image_backend` — existing Grok provider adapter and request contract.
- `scripts/gen_batch.py:Preset`, `parse_args`, `main` — preset model source and required CLI override seam.
- `tools/ai_image.py:SCHEMA`, `run_single`, `run_jobs`, `run_batch` — agent-facing JSON interface; batch currently drops `image_model`.
- `tools/Image-Menu.ps1:Run-Single`, `Run-Batch` — manual entry points that currently cannot select an image model.
- `tests/test_generator_observability.py:GeneratorObservabilityTests` — existing provider and subprocess contract tests.
- `C:/Users/js/.omp/agent/models.yml:equivalence.overrides`, `providers.cliproxy-xai.models` — custom OMP model registry.
- `C:/Users/js/.omp/agent/config.yml:modelRoles.default` — currently points to an unregistered provider selector.
- `E:/cliproxyapi/release/config.yaml:disable-image-generation`, `oauth-excluded-models.xai` — current live endpoint/exposure policy; verification reference only.

---

### Task 1: Lock Grok Image Contracts

**Files:**
- Modify: `tests/test_generator_observability.py`

**Interfaces:**
- Consumes: `image_backend(image_model)`, `build_openai_images_payload(args)`, `call_native_image_backend(args)`, `run_batch(spec)`.
- Produces: regression coverage for three Grok IDs and a failing contract for batch `image_model` propagation.

- [ ] **Step 1: Add the failing batch override test**

Patch `tools.ai_image.subprocess.run`, invoke `run_batch` with a temporary valid preset and `image_model: "grok-imagine-image-2.0"`, then assert the captured command contains:

```text
--image-model grok-imagine-image-2.0
```

The production change that makes this pass is `run_batch` forwarding the spec-level override.

- [ ] **Step 2: Add Grok adapter characterization cases**

For each supported Grok image ID, assert:

- `image_backend(id) == "openai-images"`
- payload `model` preserves the exact ID
- payload contains the prompt and one requested image
- native endpoint ends in `/v1/images/generations`
- a reference image raises the existing text-to-image-only error before `urlopen`

- [ ] **Step 3: Verify RED**

Run:

```powershell
python tests/test_generator_observability.py GeneratorObservabilityTests.test_ai_batch_passes_image_model_override -v
```

Expected: FAIL because `tools/ai_image.py:run_batch` omits `--image-model`.

### Task 2: Propagate Model Selection Through Every Entry Point

**Files:**
- Modify: `scripts/gen_batch.py`
- Modify: `tools/ai_image.py`
- Modify: `tools/Image-Menu.ps1`

**Interfaces:**
- Produces: `scripts/gen_batch.py --image-model MODEL`; JSON mode=batch honors `image_model`; manual single/batch flows pass the chosen model.
- Preserves: preset `image_model` when no override is supplied and global default `gpt-image-2`.

- [ ] **Step 1: Add `gen_batch.py` override**

Add optional `--image-model`. After `load_preset`, use `dataclasses.replace` only when the flag is supplied so all downstream plans and subprocesses see the effective model without mutating the frozen `Preset`.

- [ ] **Step 2: Forward JSON batch override**

In `tools/ai_image.py:run_batch`, append `--image-model <value>` when `spec.image_model` is present. Update `SCHEMA` and examples to list the three Grok image IDs and show `grok-imagine-image-2.0`.

- [ ] **Step 3: Add manual model selection**

Add one `Read-ImageModel` function in `tools/Image-Menu.ps1` with these choices:

1. `gpt-image-2` — existing default
2. `gemini-3.1-flash-image`
3. `grok-imagine-image-2.0` — recommended Grok
4. `grok-imagine-image-quality`
5. `grok-imagine-image`
6. custom model ID

`Run-Single` includes the selected model in summary and command. `Run-Batch` reads the preset's current `image_model` as its default, allows override, and passes `--image-model`.

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
python -m unittest discover -s tests -v
```

Expected: all tests pass, including the new batch propagation and Grok adapter cases.

- [ ] **Step 5: Exercise menu without generation**

Run `pwsh -NoProfile -File tools/Image-Menu.ps1` through captured stdin. Choose batch, one item, `grok-imagine-image-2.0`, and dry-run. Expected: generated command and `prompts.jsonl` both report `grok-imagine-image-2.0`; no image endpoint request occurs.

### Task 3: Register Grok 4.6 in Oh My Pi

**Files:**
- Modify: `C:/Users/js/.omp/agent/models.yml`
- Modify: `C:/Users/js/.omp/agent/config.yml`

**Interfaces:**
- Produces: selector `cliproxy-xai/grok-4.6` and default role `cliproxy-xai/grok-4.6:xhigh`.
- Consumes: existing `cliproxy-xai` OpenAI Responses provider at `http://127.0.0.1:8317/v1`.

- [ ] **Step 1: Preserve RED evidence**

Run:

```powershell
omp models find grok --json
```

Expected before edit: `cliproxy-xai/grok-4.3` and `grok-4.5` exist; `grok-4.6` is absent.

- [ ] **Step 2: Add equivalence and model metadata**

Add `cliproxy-xai/grok-4.6: grok-4.6` under `equivalence.overrides`. Add model metadata under `providers.cliproxy-xai.models`:

```yaml
id: grok-4.6
name: Grok 4.6 CPA
contextWindow: 500000
maxTokens: 500000
input: [text, image]
reasoning: true
thinking:
  mode: effort
  efforts: [minimal, low, medium, high, xhigh]
cost:
  input: 0
  output: 0
  cacheRead: 0
  cacheWrite: 0
```

These values match OMP's current authoritative `xai-oauth` cache entry for Grok 4.6.

- [ ] **Step 3: Fix default selector**

Replace only:

```yaml
default: xai-oauth/grok-4.6:xhigh
```

with:

```yaml
default: cliproxy-xai/grok-4.6:xhigh
```

- [ ] **Step 4: Verify OMP parsing and resolution**

Run:

```powershell
omp models cliproxy-xai --json
omp config get modelRoles.default
```

Expected: catalog includes `cliproxy-xai/grok-4.6` with `xhigh`; default prints `cliproxy-xai/grok-4.6:xhigh`.

### Task 4: Document the Supported Image Models

**Files:**
- Modify: `README.md`

**Interfaces:**
- Produces: operator-facing exact model list and JSON example.

- [ ] **Step 1: Update provider documentation**

Add `grok-imagine-image-2.0` to the provider list, mark it recommended among Grok choices, retain the text-to-image/reference restriction, and add one JSON example using `image_model`.

- [ ] **Step 2: Keep generated artifacts out of docs**

Do not add credentials, raw event bodies, generated images, or local auth/account details. Existing installed `ai-image-generator` skill already accepts `grok-imagine-image*`; no skill edit is required.

- [ ] **Step 3: Commit repository changes**

After tests pass:

```powershell
git add README.md scripts/gen_batch.py tools/ai_image.py tools/Image-Menu.ps1 tests/test_generator_observability.py
git commit -m "feat: add Grok image model selection"
```

### Task 5: Verify the Live Cross-System Path

**Files:**
- Verify only: `E:/cliproxyapi/release/config.yaml`
- Create only after approval: `ImageGallery/grok-integration-smoke/` generated package

**Interfaces:**
- Proves CPA exposure, OMP model execution, and Grok image file output.

- [ ] **Step 1: Verify CPA model exposure without mutation**

Query authenticated `/v1/models` and assert exact presence of:

```text
grok-4.6
grok-imagine-image-2.0
grok-imagine-image
grok-imagine-image-quality
```

Also verify `OPTIONS /v1/images/generations` returns HTTP 204. Do not edit release config when both checks pass.

- [ ] **Step 2: Ask point-of-risk approval**

Explain that the next two requests consume xAI credits. Request approval for exactly one short Grok 4.6 text call and one low-quality `1024x1024` Grok 2.0 image.

- [ ] **Step 3: Run OMP text smoke after approval**

```powershell
omp -p --no-session --no-tools --model cliproxy-xai/grok-4.6:xhigh "Reply exactly GROK46_OK"
```

Expected: successful response containing `GROK46_OK` through `cliproxy-xai`.

- [ ] **Step 4: Run image smoke after approval**

Call `tools/ai_image.py` with:

```json
{
  "mode": "single",
  "prompt": "A centered matte green triangle on a clean white background, flat studio graphic, crisp edges, no text, no logo, no watermark.",
  "topic": "grok-integration-smoke",
  "topic_root": "E:/ai-image-generator/ImageGallery/grok-integration-smoke",
  "count": 1,
  "size": "1024x1024",
  "quality": "low",
  "image_model": "grok-imagine-image-2.0"
}
```

Expected: JSON returns `ok: true` and one concrete PNG path under the new ImageGallery package. Inspect the file before describing its contents.

- [ ] **Step 5: Final regression check**

Run fresh:

```powershell
python -m unittest discover -s tests -v
omp models find grok --json
```

Confirm no unintended changes in `E:/cliproxyapi/release`, no existing generated images overwritten, and no credential data staged in the image-generator repository.
