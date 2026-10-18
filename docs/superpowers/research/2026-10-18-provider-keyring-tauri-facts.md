# xai
- [doc] Generation endpoint is POST https://api.x.ai/v1/images/generations. The request body is JSON with these fields: model (string|null), prompt (string), n (integer|null), response_format (string|null, 'url' or 'b64_json'), aspect_ratio (enum), resolution (enum), deferred (boolean|null), output.upload_urls, storage_options {expires_after, filename, public_url}, and user. There is no 'size' field.
- [doc] n can be 1 to 10 and defaults to 1. response_format defaults to 'url'. aspect_ratio defaults to 'auto'. resolution defaults to '1k'. The guide lists only '1k' and '2k' as resolutions, while the REST reference also lists '1.5k'.
- [doc] An optional 'quality' field ('low' | 'medium' | 'auto') is supported only on grok-imagine-image-2.0. 'auto' is the default: it currently serves 'low' for generation and 'medium' for edits. The quality field is missing from the REST reference's request schema, and the models reference lists quality tiers as low/medium/high with medium as the default, so the two pages disagree.
- [doc] The edit endpoint is POST https://api.x.ai/v1/images/edits and accepts JSON only, not multipart. A single source image goes in 'image': {url | file_id}. url can be a public URL or a base64 data URL (JPEG/PNG/WebP), and the 'image_url' key is also accepted for compatibility. The doc examples add "type": "image_url". prompt is required. The edit body otherwise takes the same fields as generation (model, n, response_format, aspect_ratio, resolution, deferred, output, storage_options, user).
- [doc] The OpenAI SDK's images.edit() does not work with xAI because it sends multipart/form-data.
- [doc] For multiple source images, use an 'images' array of {url | file_id} objects instead of 'image' (the two cannot be combined). Up to 5 images are allowed, and URL, data URI and file_id sources can be mixed. The prompt refers to them as <IMAGE_0>, <IMAGE_1>, and so on. By default the output aspect ratio follows the first image.
- [not] Mask (inpainting) parameters are not documented. No 'mask' field appears in the edits request schema or the guides.
- [doc] The response is {data: [{url?, b64_json?, mime_type?, file_output?, storage_error?}], usage: {cost_in_usd_ticks, input_tokens, input_tokens_details, output_tokens, output_tokens_details, total_tokens}}. b64_json is raw base64 without a data-URI prefix. URLs are temporary. Generation and edits return the same shape. revised_prompt is not documented.
- [doc] The response may include a top-level 'model' field naming the model that served the request. This is stated only in the migration guide and is absent from the REST response schema.
- [doc] Deferred mode: with deferred:true the call returns {request_id} immediately. Poll GET /v1/images/{request_id}: it returns 202 while status is 'pending'. When status is 'done' it returns 200 with data and usage. When status is 'failed' it also returns 200, with error {code, message}. Only response_format 'url' works in deferred mode, and results are kept for 24 hours.
- [doc] Auth uses the header 'Authorization: Bearer $XAI_API_KEY' with 'Content-Type: application/json'. The base URL is https://api.x.ai/v1.
- [doc] GET /v1/models returns {object: 'list', data: [{id, aliases, created, object: 'model', owned_by, image_price?, pricing?: [{price_per_image, quality, resolution}], context_length?, ...token prices}]}. Image models appear in this list (the example shows 'grok-imagine-image' with image_price 200000000).
- [doc] GET /v1/image-generation-models (and /v1/image-generation-models/{model_id}) returns {models: [{id, aliases, created, fingerprint, image_price, input_modalities, output_modalities, max_prompt_length, object, owned_by, pricing?, version}]}. The top-level key is 'models', not 'data'. The doc's example response omits some fields its own schema marks as required (for example input_modalities and image_price) and includes token-price fields that are not in the schema.
- [doc] Current image models and prices: grok-imagine-image-2.0 ($0.04/image), grok-imagine-image (1.0, $0.02/image), grok-imagine-image-quality ($0.05/image). grok-imagine-image-quality will be retired on 2026-11-02 and redirected to grok-imagine-image-2.0 with quality 'low'. grok-imagine-image-pro already redirects to grok-imagine-image-quality. Video models: grok-imagine-video-1.5, grok-imagine-video-1.5-lite, grok-imagine-video.
- [inf] Only grok-imagine-image-2.0 supports the 21:9 and 5:2 aspect ratios, multi-image edits (up to 5 images) and the quality parameter. Older image models presumably accept only a single source image and the older set of aspect ratios.
  ? The docs never mention revised_prompt. The response schema has no revised_prompt field (only url, b64_json, mime_type, file_output, storage_error), so code should treat it as optional or absent.
  ? Masks are not documented for /v1/images/edits.
  ? There is no 'size' (WxH) parameter. Output dimensions are controlled with aspect_ratio plus resolution (1k/2k, and '1.5k' per the REST reference only).
  ? The docs disagree on quality: the generation guide gives low/medium/auto with auto as the default, while the models reference gives low/medium/high with medium as the default. The REST images schema leaves quality out entirely.
  ? The edit examples include "type": "image_url" inside the image object, but the REST schema does not list a 'type' field. It is probably ignored or optional; this needs a live call to confirm.
  ? The docs do not say whether the response carries a top-level 'model' or 'created' field beyond the migration-guide remark about 'model'.
  ? The /v1/image-generation-models example omits several fields its schema marks as required (image_price, input_modalities, output_modalities). Parse leniently.
  ? The docs do not give a per-image size limit for input images. They only say JPEG, PNG or WebP.
## corrections
! Only grok-imagine-image-2.0 supports multi-image edits. Older image models presumably accept only a single source image (fact 15, marked inferred). => grok-imagine-image-quality also accepts the 'images' array for multi-image edits (documented example). grok-imagine-image-2.0 raises the limit to 5 source images and adds 'quality' plus the 21:9 and 5:2 ratios. The per-model image limit for the older models is undocumented, so do not assume they take a single image only.
! Code sample 4: 'Multi-image edit body built from the REST schema (no verbatim curl sample on the docs for this)'. => Cite the documented sample from the 'Editing with multiple stored images' section on the Files API inputs page (it uses grok-imagine-image-quality; swap the model for grok-imagine-image-2.0).
notes: Method: WebFetch got HTTP 404 on every docs.x.ai .md URL, so I downloaded the pages with curl (all returned 200) and read them in full: rest-api-reference/inference/images.md, images/generation.md, editing.md, multi-image-editing.md, rest-api-reference/inference/models.md, models.md, pricing.md, models/grok-imagine-image-2.0.md, migration/imagine-image-quality-nov-2.md, migration/may-15-retirement.md, imagine.md, imagine/files/inputs.md, prompting-guide.md and llms.txt.

What checked out exactly:
- Endpoints, plus every enum value for aspect_ratio and resolution (1k, 2k, 1.5k).
- n is 1–10 with default 1. response_format defaults to url, aspect_ratio to auto, resolution to 1k.
- quality is low/medium/auto with auto as the default (auto serves low for generation, medium for edits). It works only on 2.0 and is missing from the REST schema. The models reference says low/medium/high with medium as the default, so the two pages really do disagree.
- Edits take JSON only. image takes {url | file_id}, image_url is accepted for compatibility, and prompt is required.
- The images array is mutually exclusive with image, prompts refer to <IMAGE_n>, and the limit is 5.
- The response schema matches, and no field for 'mask' or 'revised_prompt' exists anywhere.
- Deferred polling returns 202 while pending and 200 when done or failed. Errors carry code and message. Only url output is supported, and results are kept 24h.
- Both models schemas and their examples match (the image-generation-models example really does leave out fields its schema marks as required).
- Prices: 2.0 is $0.04, 1.0 is $0.02, quality is $0.05. The 2026-11-02 redirect to 2.0 with quality low is correct.

Extra details for the implementer (not errors in the findings):
1. Unit inconsistency: image_price is described as 'USD cents', but pricing[].price_per_image is in '1/100,000,000ths of a USD cent'. The example image_price of 200000000 equals $0.02 (the grok-imagine-image price) only in the second unit. Treat image_price as 1e-8-cent ticks.
2. image_price is documented as the default tier, 'medium quality at the default (1k) resolution'.
3. output.upload_urls notes that the PUT Content-Type is 'image/jpeg unless the requested quality produces PNG'. The deferred example also shows mime_type image/jpeg. So the default output format is probably JPEG; do not assume PNG.
4. The deferred done response includes request_id and status. Its example usage has an undocumented 'image_cost' field, so parse usage leniently.
5. The generation guide says the xAI SDK exposes response.model and respect_moderation. That supports, but does not settle in the REST schema, the claim that responses carry a top-level 'model' field.
6. Open question on input size limits: models.md lists 'Maximum image size: 20MiB' with jpg/jpeg/png types, but under 'Image input models' (chat image understanding), not Imagine edits. No limit is documented specifically for Imagine edit inputs.
7. The migration guide says 2.0 at low quality is $0.01 cheaper than quality 'at every resolution'. That means prices vary by resolution, even though the pricing table shows one flat per-image price.
8. Reference-to-video allows up to 14 reference images on grok-imagine-video-1.5. That is a different limit from the 5 for image edits.
## samples
curl -s https://api.x.ai/v1/images/generations \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $XAI_API_KEY" \
  -d '{
    "model": "grok-imagine-image-2.0",
    "prompt": "A serene Japanese garden",
    "response_format": "b64_json"
  }'
curl -s https://api.x.ai/v1/images/generations -H "Content-Type: application/json" -H "Authorization: Bearer $XAI_API_KEY" -d '{"model": "grok-imagine-image-2.0", "prompt": "Mountain landscape at sunrise", "aspect_ratio": "16:9"}'   # also documented separately: "resolution": "2k", "n": 4, "quality": "low"
curl -s https://api.x.ai/v1/images/edits \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $XAI_API_KEY" \
  -d '{
    "model": "grok-imagine-image-2.0",
    "prompt": "Render this as a pencil sketch with detailed shading",
    "image": {
      "url": "https://docs.x.ai/assets/api-examples/images/style-realistic.png",
      "type": "image_url"
    }
  }'
// Multi-image edit body built from the REST schema (no verbatim curl sample on the docs for this):
{"model":"grok-imagine-image-2.0","prompt":"Put the object from <IMAGE_1> into the scene of <IMAGE_0>","images":[{"url":"data:image/png;base64,..."},{"url":"https://..."}],"response_format":"b64_json"}
Response example (docs): {"data": [{"url": "..."}, {"url": "..."}]}  // b64_json variant: data[i].b64_json (raw base64, no data: prefix), data[i].mime_type; plus usage{cost_in_usd_ticks,...}
GET /v1/image-generation-models -> {"models": [{"id": "grok-imagine-image", "fingerprint": "fp_ca78641a52", "max_prompt_length": 1024, "created": 1738961600, "object": "model", "owned_by": "xai", "version": "1.0.0", "aliases": []}]}
GET /v1/models -> {"data": [{"id": "grok-imagine-image", "aliases": [], "created": 1769472000, "object": "model", "owned_by": "xai", "image_price": 200000000}], "object": "list"}
# openai
- [doc] POST /v1/images/edits accepts a JSON body where input images are `images: array of object { file_id, image_url }`, up to 16 images for GPT image models. image_url can be a full URL or a base64 data URL.
- [doc] The JSON-body mask is `mask: { file_id | image_url }` (an object, not an array), with exactly one of the two fields.
- [doc] The official JSON edit example uses Content-Type application/json with images[].image_url.
- [inf] No official sample shows a data: URL in the Images API JSON body. Data URLs (`data:image/png;base64,...`) appear only in Responses API input_image samples. The reference text says image_url accepts a "base64-encoded data URL", so this works by spec but has no sample.
- [doc] The multipart/form-data alternative uses repeated `image[]` fields for input images and a single `mask` file field. Other parameters (model, prompt, stream, etc.) are plain form fields.
- [not] The reference has no separate multipart schema, so multipart field names are known only from curl examples. Whether a single `image` field (without []) is accepted in multipart is not stated, although the Python SDK sample passes `image=open(...)`.
- [doc] Mask requirements: the mask must have the same format and size as the image, be under 50MB, and include an alpha channel. With multiple input images, the mask applies to the first one (stated in the Responses section). Masking is prompt-based.
- [doc] The edits `model` field defaults to gpt-image-2.5-sunburst. The enum is gpt-image-1.5, gpt-image-2, gpt-image-2-2026-04-21, gpt-image-2.5-sunburst, gpt-image-2.5-sunburst-2026-09-08, gpt-image-2.5-flare, gpt-image-2.5-flare-2026-09-08, gpt-image-1, gpt-image-1-mini, chatgpt-image-latest, and any string is also allowed.
- [doc] Generations requires `model` with no default ("Specify a model explicitly"). ImageModel also lists dall-e-2 and dall-e-3 (retired, legacy).
- [doc] The gpt-image-2.5 family appears only as -sunburst and -flare (plus -2026-09-08 snapshots). There is no plain `gpt-image-2.5` model ID.
- [doc] background is transparent|opaque|auto (default auto). Sunburst and Flare support opaque and transparent. For gpt-image-2 and gpt-image-2-2026-04-21, transparent is in preview. transparent requires png or webp output.
- [not] Per-model transparency support for gpt-image-1 / 1-mini / 1.5 / chatgpt-image-latest is not listed. The docs say only "available for supported GPT Image models".
- [doc] output_format is png|jpeg|webp (default png). output_compression is 0-100 and applies only to jpeg/webp, defaulting to 100.
- [doc] quality is low|medium|high|auto (default auto) for all GPT image models. xhigh and max work only on gpt-image-2.5-sunburst/flare and their snapshots. Generations also lists the legacy values standard and hd (DALL-E).
- [doc] size on gpt-image-2 and the 2.5 models accepts arbitrary WIDTHxHEIGHT. Rules: both sides divisible by 16, aspect ratio between 1:3 and 3:1, max edge 3840, total pixels from 655,360 to 8,294,400, max 3840x2160, and anything above 2560x1440 is experimental. The standard sizes 1024x1024, 1536x1024, 1024x1536 work on all GPT image models, and auto works on models that allow automatic sizing.
- [inf] The docs contradict each other on max resolution: the reference says max is 3840x2160, but the guide says max edge is 3840 with pixels up to 8,294,400 and lists 2160x3840 portrait for gpt-image-2. Read the max as either orientation.
- [inf] For gpt-image-1, 1-mini and 1.5, size is limited to 1024x1024, 1536x1024, 1024x1536 and auto. The docs imply this (arbitrary sizes are only listed for 2/2.5) but don't spell it out per model.
- [doc] input_fidelity (edits only) is high|low, default low. gpt-image-1 and gpt-image-1.5 support high and low, gpt-image-1-mini supports only low, and it must be omitted for gpt-image-2, which always uses high fidelity.
- [not] The docs do not say whether input_fidelity applies to gpt-image-2.5-sunburst/flare or chatgpt-image-latest. Treating it like gpt-image-2 and omitting it is the safe choice.
- [doc] Other edits/generations fields: moderation low|auto, n (generations: 1-10; edits: no range given), partial_images 0-3, stream bool, safety_identifier, user (deprecated). Generations also has prompt up to 32000 chars, plus legacy response_format/style, which GPT image models don't support.
- [doc] The /v1/images/generations body is JSON with these fields: model, prompt, background, moderation, n, output_compression, output_format, partial_images, quality, response_format, safety_identifier, size, stream, style, user.
- [doc] The response is an ImagesResponse with: created (unix seconds), background (transparent|opaque), data[] {b64_json, revised_prompt, url}, output_format, quality, safety_identifier, size, and usage {input_tokens, input_tokens_details{image_tokens,text_tokens}, output_tokens, total_tokens, output_tokens_details?{image_tokens,text_tokens}}. GPT image models return b64_json by default; url and revised_prompt are not returned for them.
- [doc] ImagesResponse has no `model` field. The schema lists only created, background, data, output_format, quality, safety_identifier, size, usage.
- [inf] usage is described as "For `gpt-image-1` only" in the schema, yet the official gpt-image-2.5-flare sample response includes usage. Treat usage as optional.
- [doc] Streaming emits SSE events image_generation.partial_image / image_generation.completed for generations and image_edit.partial_image / image_edit.completed for edits. Each event carries b64_json, background, created_at, output_format, quality, size, type, plus partial_image_index on partial events and usage on completed events.
- [doc] GET /v1/models returns {object:"list", data:[{id, created, object:"model", owned_by, shutdown_date (string|null)}]}.
- [doc] /v1/images/variations is retired.
- [doc] Using these models may require API Organization Verification.
  ? Does input_fidelity apply to gpt-image-2.5-sunburst/flare or chatgpt-image-latest? It is not documented; the safe choice is to omit it like gpt-image-2.
  ? Do gpt-image-1, 1-mini, 1.5 and chatgpt-image-latest support transparent backgrounds? Per-model support is not listed (only 'supported GPT Image models').
  ? Is usage always present for gpt-image-2 and 2.5? The schema says 'For gpt-image-1 only', but the 2.5-flare sample includes it, so treat it as optional.
  ? Does multipart accept a bare `image` field (no []) alongside `image[]`? The SDK passes image=...; curl samples use image[]. Unconfirmed.
  ? Do data: URLs work in images[].image_url for the Images API JSON body? The description says yes ('base64-encoded data URL') but no official sample exists. Also unknown: the max data URL/body size for JSON (multipart mask limit is <50MB).
  ? Max resolution orientation: the reference says 3840x2160 max, the guide allows 2160x3840 (max edge 3840, pixels <= 8,294,400).
  ? Edits `n` range is not stated (generations: 1-10).
  ? The platform.openai.com mirror of the reference was not checked separately; all quotes come from developers.openai.com (raw .md pages fetched 2026-10-18).
## corrections
! With multiple input images, the mask applies to the first one (stated in the Responses section). => The guide's general mask section says the mask is applied to the first image when you send multiple input images. This applies to the Image API edits endpoint as well as the Responses API.
! Each streaming event carries b64_json, background, created_at, output_format, quality, size, type, plus partial_image_index on partial events and usag => Partial events: b64_json, background (transparent|opaque|auto), created_at, output_format, partial_image_index, quality (low|medium|high|xhigh|max|auto), size (string or auto), type, optional safety_identifier. Completed events: the same fields without partial_image_index, plus a required usage object (input_tokens, input_tokens_details{image_tokens,text_tokens}, output_tokens, total_tokens).
notes: On 2026-10-18 I fetched the raw markdown for the images reference, the image-generation guide and the models list page from developers.openai.com and checked every quote against it. 26 of the 28 facts match the docs word for word, including all model IDs and snapshot dates, the edits default (gpt-image-2.5-sunburst), the 16-image limit, the images[]/mask object shapes, the input_fidelity text, the size rules (16-divisible, 1:3–3:1, above 2560x1440 experimental, 3840x2160 max; guide gives 655,360–8,294,400 pixels and 2160x3840 portrait), the xhigh/max limit, legacy standard/hd, the n 1–10 range for generations only, prompt max 32000, the ImagesResponse fields (no `model` field), the retired variations endpoint, Organization Verification and the shape of /v1/models. The data:image samples appear only inside responses.create input_image blocks, and the images reference has none. Additions, not errors: (1) Generations `size` also lists legacy values 256x256, 512x512, 1792x1024 and 1024x1792. (2) For the usage question: the stream event schemas say usage is \"For the GPT image models only\" and mark it required. ImagesResponse still says \"For `gpt-image-1` only\", which looks out of date. Treating usage as optional in non-streaming responses is still the right call. (3) In the edits section, output_compression only says \"Compression level for `jpeg` or `webp` output\". The 0–100 range and the default of 100 are stated only in the generations section. (4) The Python SDK multi-image edit sample passes image=[open(...), ...], a list. (5) The models page's first example shows a non-null shutdown_date (\"2019-12-27\"), and the second example includes \"2026-10-23\" for one model. Downloaded docs are in C:\Users\js\AppData\Local\Temp\claude\E--ai-image-generator\4b63e1e7-1dc6-4c80-985d-ca80de31bd65\scratchpad\docs.
## samples
# JSON edit (official reference example)
curl https://api.openai.com/v1/images/edits \
    -H 'Content-Type: application/json' \
    -H "Authorization: Bearer $OPENAI_API_KEY" \
    -d '{
          "images": [
            {
              "image_url": "https://example.com/source-image.png"
            }
          ],
          "prompt": "Add a watercolor effect to this image",
          "model": "gpt-image-1.5",
          "quality": "high",
          "size": "1024x1024"
        }'
# Multipart edit with mask (official guide)
curl -s -D >(grep -i x-request-id >&2) \
  -o >(jq -r '.data[0].b64_json' | base64 --decode > lounge.png) \
  -X POST "https://api.openai.com/v1/images/edits" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -F "model=gpt-image-2.5-sunburst" \
  -F "mask=@mask.png" \
  -F "image[]=@sunlit_lounge.png" \
  -F 'prompt=A sunlit indoor lounge area with a pool containing a flamingo'
# Multipart multi-image edit (official reference)
curl -s -X POST "https://api.openai.com/v1/images/edits" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -F "model=gpt-image-1.5" \
  -F "image[]=@body-lotion.png" \
  -F "image[]=@bath-bomb.png" \
  -F "image[]=@incense-kit.png" \
  -F "image[]=@soap.png" \
  -F 'prompt=Create a lovely gift basket with these four items in it'
# Generations (official reference)
curl https://api.openai.com/v1/images/generations \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -d '{"model": "gpt-image-2.5-flare", "prompt": "A cute baby sea otter", "n": 1, "size": "1024x1024"}'
# Response:
{"created": 1713833628, "data": [{"b64_json": "..."}], "usage": {"total_tokens": 100, "input_tokens": 50, "output_tokens": 50, "input_tokens_details": {"text_tokens": 10, "image_tokens": 40}}}
# Inferred JSON edit with data URL + mask (fields documented; no official sample combines them)
{"model":"gpt-image-2","prompt":"...","images":[{"image_url":"data:image/png;base64,AAAA..."}],"mask":{"image_url":"data:image/png;base64,BBBB..."},"background":"opaque","output_format":"png","quality":"high","size":"1536x1024"}
# GET /v1/models response (official)
{"object": "list", "data": [{"id": "model-id-0", "object": "model", "created": 1686935002, "owned_by": "organization-owner", "shutdown_date": null}]}
# gemini
- [doc] BIG CONTEXT CHANGE: the official image-generation guide no longer shows generateContent at all. Every example now uses the Interactions API: POST https://generativelanguage.googleapis.com/v1beta/interactions with body {model, input, response_format:{type:'image', mime_type, aspect_ratio, image_size}}. The guide never mentions generateContent, responseModalities, imageConfig or inlineData.
- [doc] generateContent is still officially supported, but Google recommends the Interactions API for new work.
- [doc] generateContent endpoint: POST https://generativelanguage.googleapis.com/v1beta/{model=models/*}:generateContent, where model is in the form models/{model}.
- [doc] Authentication uses the x-goog-api-key header, and the migration guide's generateContent REST examples use it too. The API reference's own older curl sample still uses the ?key= query parameter.
- [doc] generationConfig.responseModalities is an array of the Modality enum: MODALITY_UNSPECIFIED, TEXT, IMAGE, AUDIO. Requested modalities must exactly match a supported combination, or the API returns an error.
- [not] The reference does not say which modality combinations each image model accepts, for example whether ['IMAGE'] alone is allowed or only ['TEXT','IMAGE'].
- [doc] generationConfig.imageConfig is {aspectRatio: string, imageSize: string}. Allowed aspectRatio values: 1:1, 1:4, 4:1, 1:8, 8:1, 2:3, 3:2, 3:4, 4:3, 4:5, 5:4, 9:16, 16:9, 21:9. Allowed imageSize values: 512, 1K, 2K, 4K, with 1K as the default.
- [doc] Support differs by model. Nano Banana 2.1 (gemini-nano-banana-2.1) and Gemini 3.1 Flash Image (gemini-3.1-flash-image) accept all 14 aspect ratios. Gemini 3 Pro Image and Gemini 2.5 Flash Image accept only 10: 1:1, 2:3, 3:2, 3:4, 4:3, 4:5, 5:4, 9:16, 16:9, 21:9. Gemini 2.5 Flash Image has a single resolution per ratio, for example 1:1 is 1024x1024 at 1290 tokens. Size 512 is only for 3.1 Flash Image, and 3.1 Flash Lite Image supports 1K only.
- [doc] The size must use an uppercase K. Lowercase values such as 1k are rejected.
- [inf] The exact wire string for the 512 size in generateContent imageConfig is '512'. The ImageConfig reference says 'Supported values are 512, 1K, 2K, 4K'. The guide's '512px (05.K)' wording and the ImageResponseFormat enum IMAGE_SIZE_FIVE_TWELVE belong to the newer response_format path.
- [doc] generationConfig also has a newer responseFormat field, ResponseFormatConfig. Its image sub-object (ImageResponseFormat) has mimeType (only IMAGE_JPEG), delivery (INLINE or URI), aspectRatio (enum such as ASPECT_RATIO_SIXTEEN_BY_NINE) and imageSize (enum IMAGE_SIZE_FIVE_TWELVE, IMAGE_SIZE_ONE_K, IMAGE_SIZE_TWO_K, IMAGE_SIZE_FOUR_K).
- [doc] To send input images for editing in generateContent, add a part {"inlineData": {"mimeType": "image/png", "data": "<base64>"}} next to a {"text": ...} part in contents[].parts. Blob also has an optional displayName.
- [doc] The guide gives these reference-image limits. 'Up to 14 reference images' is the overall mix. Gemini 2.5 Flash Image works best with up to 3 input images. Gemini 3 Pro Image handles 5 with high fidelity and up to 14 in total. Nano Banana 2.1 and 3.1 Flash Image keep up to 4 characters and 10 objects consistent. A separate table on the same page says Nano Banana 2.1 / 3.1 Flash allow 10 object and 5 character images, and Gemini 3 Pro allows 6 object and 3 style images, so the guide contradicts itself on these counts.
- [doc] generateContent response fields: candidates[], promptFeedback, usageMetadata, modelVersion, responseId and modelStatus. Each candidate has content (with parts[]), finishReason and finishMessage. A part holds exactly one data field (text, inlineData, functionCall, ...) and may also carry thought and thoughtSignature.
- [doc] Image-related finishReason values: IMAGE_SAFETY, IMAGE_PROHIBITED_CONTENT, IMAGE_OTHER, NO_IMAGE, IMAGE_RECITATION.
- [not] The docs do not say whether thinking-mode 'thought images' come back as inlineData parts with thought=true in generateContent responses, which a client would need to skip.
- [doc] models.list: GET https://generativelanguage.googleapis.com/v1beta/models with pageSize (default 50, max 1000) and pageToken. The response has models[] and nextPageToken, which is omitted on the last page.
- [doc] Model resource: name has the format models/{model}. Other fields: baseModelId, version, displayName, inputTokenLimit, outputTokenLimit, and supportedGenerationMethods, a list of method names such as 'generateContent'. models.get is GET /v1beta/{name=models/*}.
- [doc] Current image model IDs: gemini-nano-banana-2.1 (newest, recommended), gemini-3.1-flash-image (Nano Banana 2), gemini-3.1-flash-lite-image (Nano Banana 2 Lite), gemini-3-pro-image (Nano Banana Pro) and gemini-2.5-flash-image (legacy Nano Banana). Imagen is shut down. The rate-limits page still lists 'Gemini 3.1 Flash Image Preview' and 'Gemini 3 Pro Image Preview' as names.
- [doc] The current API errors page covers ONLY the Interactions API. Its error body is {"error": {"code": "<snake_case string>", "message": "..."}}, with codes such as invalid_request(400), authentication(401), payment_required(402), permission_denied(403), model_not_found(404), rate_limit_exceeded / quota_exceeded / too_many_requests(429), api_error(500), service_unavailable(503) and deadline_exceeded(504).
- [not] The current docs do not document the generateContent (v1beta models) error body, the google.rpc.Status-style {error:{code:<int HTTP>, message, status:'RESOURCE_EXHAUSTED', details:[...]}}. That this shape still applies to generateContent is an inference from Google API conventions, not something the pages say.
- [doc] Quota and rate-limit errors are 429 RESOURCE_EXHAUSTED and are retryable with exponential backoff, as are 408, 5xx and 503 UNAVAILABLE. 400, 402 and 403 should not be retried. Limits apply per project, not per API key. Image models also have an IPM (images per minute) limit.
  ? The docs do not say whether responseModalities ['IMAGE'] alone (without TEXT) is accepted by each image model in generateContent.
  ? The generateContent error body shape ({error:{code:int,message,status,details}}) is no longer documented; the current API errors page covers only the Interactions API's snake_case string codes. Clients should parse both shapes defensively.
  ? The exact imageSize string for 512px in generateContent imageConfig: the reference says '512', while the guide says '512px (05.K)'. This needs a live test.
  ? The docs do not say how thought images appear in generateContent parts (inlineData with thought=true?), so a client may need to filter by the thought flag.
  ? The docs contradict themselves on reference-image limits (4 vs 5 characters, 10 objects, 14 total). The API's actual enforcement is undocumented.
  ? Whether the 'Preview' model IDs (e.g. gemini-3.1-flash-image-preview, gemini-3-pro-image-preview) still resolve. The rate-limits page still uses Preview names, but the models page lists IDs without suffixes.
  ? Whether the new gemini-nano-banana-2.1 model supports generateContent or only the Interactions API. Check supportedGenerationMethods from models.list.
## corrections
! A separate table on the same page says Nano Banana 2.1 / 3.1 Flash allow 10 object and 5 character images, and Gemini 3 Pro allows 6 object and 3 styl => Reference-image limits per the guide's table: Flash Lite Image allows 14 objects. Nano Banana 2.1 and 3.1 Flash Image allow 10 objects and 4 characters. Gemini 3 Pro Image allows 6 objects, 5 characters and 3 style images. The prose matches: 2.5 Flash Image works best with up to 3 inputs, 3 Pro handles 5 at high fidelity and up to 14 in total, and NB 2.1/3.1 Flash keep up to 4 characters and 10 objects consistent.
! Quota/rate-limit errors are 429 RESOURCE_EXHAUSTED and retryable with exponential backoff ... 400, 402 and 403 should not be retried (cited to the rat => Per the troubleshooting page, retry 429, 408 and 5xx (including 503 UNAVAILABLE) with exponential backoff, and do not retry 400, 402 or 403. Per the rate-limits page, a spend-based limit returns 429 RESOURCE_EXHAUSTED, limits apply per project, IPM applies to image models, and RPD resets at midnight Pacific. Treat a daily-quota 429 as wait-until-reset, not as a short backoff retry.
notes: I downloaded every cited page (2026-10-18) and checked it with grep. Confirmed verbatim:
- The image guide (Last updated 2026-10-09 UTC) uses only POST /v1beta/interactions with response_format {type:'image', mime_type, aspect_ratio, image_size}. It never mentions generateContent, responseModalities, imageConfig or inlineData.
- The migration guide says 'generateContent remains fully supported, we recommend the Interactions API for all new development', and its curl examples use the x-goog-api-key header. The API reference curl samples still use ?key= with gemini-2.0-flash.
- GenerationConfig.responseModalities wording is exact, including the exact-match rule and 'empty list = text only'. The response Modality enum is MODALITY_UNSPECIFIED, TEXT, IMAGE, AUDIO. The separate ModalityTokenCount enum also has VIDEO and DOCUMENT; don't mix them up.
- ImageConfig {aspectRatio, imageSize}: the 14 ratios and 'Supported values are 512, 1K, 2K, 4K ... default value 1K' are verbatim.
- responseFormat/ResponseFormatConfig and ImageResponseFormat: mimeType is IMAGE_JPEG only, delivery is INLINE/URI, and the AspectRatio and ImageSize enum names include IMAGE_SIZE_FIVE_TWELVE. The deprecated responseSchema and _responseJsonSchema now say 'Use responseFormat instead', but imageConfig is not deprecated.
- Per-model tables: Nano Banana 2.1 and 3.1 Flash Image have 14 ratios, and 3.1 Flash has the 512px column (1:1 is 512x512 at 747 tokens). The '3.1 Pro Image' table has 10 ratios. 2.5 Flash Image has 10 ratios, each a single resolution at 1290 tokens. The page says '512px (0.5K) ... not supported on Gemini Nano Banana 2.1' and 'Flash Lite Image only supports 1K'. The uppercase-K sentence with the '05.K' typo is verbatim; elsewhere the same page writes '512px (0.5K)'. The guide also says 3.1 Flash Lite Image supports 10 ratios.
- Blob has mimeType (list as quoted), data and displayName.
- Part has thought and thoughtSignature. Candidate has finishMessage. The response has modelStatus.
- FinishReason includes IMAGE_SAFETY, IMAGE_PROHIBITED_CONTENT, IMAGE_OTHER, NO_IMAGE and IMAGE_RECITATION.
- models.list: pageSize defaults to 50 with a maximum of 1000, and nextPageToken is omitted on the last page. The supportedGenerationMethods example is 'generateMessage and generateContent'. models.get is GET /v1beta/{name=models/*}.
- Models page IDs: gemini-nano-banana-2.1, gemini-3.1-flash-image, gemini-3.1-flash-lite-image, gemini-3-pro-image, and gemini-2.5-flash-image (in the legacy-workflows section). Imagen 4 is shut down.
- The rate-limits page still uses 'Gemini 3.1 Flash Image Preview', 'Gemini 3 Pro Image Preview' and also 'Gemini 2.5 Flash Image Preview'.
- The API errors page covers only the Interactions API, with snake_case codes as listed. It also has image_safety/no_image block codes and says unlisted codes fall back to the snake_case HTTP status.
- No page documents the google.rpc.Status-style generateContent error body; 'not_documented' is correct.
- The thought-image layout for generateContent is undocumented. The guide's Interactions samples read thought images from steps with type=='thought'.
## samples
# generateContent image generation/editing request (body fields from the API reference; only the endpoint and headers are copied from a docs example)
curl -X POST "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-image:generateContent" \
  -H "Content-Type: application/json" \
  -H "x-goog-api-key: $GEMINI_API_KEY" \
  -d '{
    "contents": [{"parts": [
      {"inlineData": {"mimeType": "image/png", "data": "<base64>"}},
      {"text": "Make the sky sunset orange"}
    ]}],
    "generationConfig": {
      "responseModalities": ["TEXT", "IMAGE"],
      "imageConfig": {"aspectRatio": "16:9", "imageSize": "2K"}
    }
  }'
# Response shape (per reference): {"candidates":[{"content":{"role":"model","parts":[{"text":"..."},{"inlineData":{"mimeType":"image/png","data":"<base64>"}}]},"finishReason":"STOP"}],"usageMetadata":{...},"modelVersion":"...","responseId":"..."}
# Interactions API form used by the current image guide (verbatim structure)
curl -X POST "https://generativelanguage.googleapis.com/v1beta/interactions" \
  -H "x-goog-api-key: $GEMINI_API_KEY" -H 'Content-Type: application/json' \
  -d '{"model": "gemini-nano-banana-2.1", "input": "Create a picture of a nano banana dish in a fancy restaurant with a Gemini theme", "response_format": {"type": "image", "aspect_ratio": "16:9", "image_size": "2K"}}'
# models.list pagination
GET https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000&pageToken=<nextPageToken>
# -> {"models":[{"name":"models/gemini-3.1-flash-image","baseModelId":...,"supportedGenerationMethods":["generateContent",...]}],"nextPageToken":"..."}
# keyring
- [doc] The current latest keyring is 4.2.0, published 2026-08-29. 4.0.0 came out 2026-04-26. 4.1.3 was yanked. The last 3.x documented on docs.rs is 3.6.3.
- [doc] keyring v4 is a major restructure. The API now lives in keyring-core (current 1.0.1) plus separate per-platform store crates. keyring 4 itself is a compatibility and CLI wrapper, and its docs tell apps that want control over stores not to link it.
- [doc] keyring 4.2.0 default features are `default = ["v1"]`. `v1` enables the apple-native-keyring-store (keychain), windows-native-keyring-store and zbus-secret-service-keyring-store dependencies, so the defaults DO include stores. A separate `windows-native-keyring-store` feature exists (also default). The old `windows-native` feature name is gone.
- [doc] In keyring 3.6.3 there are no default features. The Windows store needs `features = ["windows-native"]`. Without a store feature that matches the platform, it silently falls back to the mock store.
- [doc] keyring 4 v1 API (keyring::v1::Entry) auto-selects the platform store. Signatures: new(service: &str, username: &str) -> Result<Self>, store_status() -> &'static Result<()>, set_password(&self, &str) -> Result<()>, set_secret(&self, &[u8]) -> Result<()>, get_password(&self) -> Result<String>, get_secret(&self) -> Result<Vec<u8>>, delete_credential(&self) -> Result<()>. new can return NoDefaultStore if the platform store fails to initialize.
- [doc] keyring-core 1.0.1 Entry API: new(service: &str, user: &str) -> Result<Entry>, new_with_modifiers(service, user, &HashMap<&str,&str>), new_with_credential(Arc<Credential>) -> Entry, search(&HashMap) -> Result<Vec<Entry>>, set_password(&self, &str) -> Result<()>, get_password(&self) -> Result<String>, set_secret(&self, &[u8]), get_secret(&self) -> Result<Vec<u8>>, get_attributes, update_attributes, delete_credential(&self) -> Result<()>, get_credential, get_specifiers, as_any. new, new_with_modifiers and search return NoDefaultStore if no default store is set.
- [doc] keyring-core: `pub fn set_default_store(new: Arc<CredentialStore>)` is a process-global setter. It blocks until other threads finish creating entries and is meant to be called at startup. unset_default_store also exists.
- [doc] Windows store for v4 is windows-native-keyring-store 1.1.0 (depends on keyring-core ^1). Register it with keyring_core::set_default_store(windows_native_keyring_store::Store::new().unwrap()). Store::new() -> Result<Arc<Self>> uses an empty prefix and suffix with '.' as the divider (same as earlier keyring). new_with_configuration(&HashMap) accepts prefix/divider/suffix/service_no_divider.
- [doc] Windows target name = prefix + user + divider + service + suffix, so by default it is "{user}.{service}". Different service/user pairs that contain '.' can collide unless service_no_divider is set.
- [doc] keyring-core 1.0.1 Error is #[non_exhaustive] with these variants: PlatformFailure(PlatformError), NoStorageAccess(PlatformError), NoEntry, BadEncoding(Vec<u8>), BadDataFormat(Vec<u8>, PlatformError), BadStoreFormat(String), TooLong(String, u32), Invalid(String, String), Ambiguous(Vec<Entry>), NoDefaultStore, NotSupportedByStore(String).
- [doc] keyring 3.6.3 Error is #[non_exhaustive] with these variants: PlatformFailure(Box<dyn Error+Send+Sync>), NoStorageAccess(Box<dyn Error+Send+Sync>), NoEntry, BadEncoding(Vec<u8>), TooLong(String, u32), Invalid(String, String), Ambiguous(Vec<Box<Credential>>). v4 adds BadDataFormat, BadStoreFormat, NoDefaultStore and NotSupportedByStore, and Ambiguous now holds Vec<Entry>.
- [doc] Windows limits are enforced as Error::TooLong. Password blob: at most CRED_MAX_CREDENTIAL_BLOB_SIZE = 5*512 = 2560 bytes. Passwords are stored as UTF-16LE, so get/set_password allows about 1280 UTF-16 code units. set_secret allows 2560 raw bytes. Generic target name: at most 32767 characters. User: at most CRED_MAX_USERNAME_LENGTH (513). Comment and target alias: at most CRED_MAX_STRING_LENGTH (256). At most 64 attributes. An empty target is Invalid.
- [doc] Thread safety: keyring_core::Entry (and keyring::v1::Entry) is Send + Sync but not UnwindSafe. Stores must be Send + Sync. Concurrent operations on the same credential from multiple threads are not reliably ordered, especially on Windows, so serialize access per credential.
- [doc] v4 testing without touching the real store: keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap()). The mock module needs no feature flag. keyring-core has only one feature, `sample`, and no default features. The mock is process-only. Entries with the same service/user share one Cred. Error injection works by downcasting entry.as_any() to mock::Cred and calling set_error, which fails the next call once.
- [doc] v3 testing: set_default_credential_builder(mock::default_credential_builder()) at startup. Error injection downcasts entry.get_credential() to MockCredential and calls set_error. In v3 the mock state lives only in the entry (a get before a set gives NoEntry; separate Entry::new calls do not share state).
- [inf] Because the default store is process-global, parallel cargo tests share it. Tests should use unique service/user names, or build entries directly from a dedicated mock store rather than relying on global-store ordering. The keyring::v1 path offers no documented way to inject the mock, so for testability use keyring-core directly. Recommended v4 dependencies: keyring-core = "1" and, under [target.'cfg(windows)'.dependencies], windows-native-keyring-store = "1".
  ? The windows-native-keyring-store 1.1.0 docs contradict themselves. Store::new says divider '.' with an empty prefix, but new_with_configuration says the defaults are 'keyring:' / '@'. Verify in the source before relying on a target-name format.
  ? validate_target compares UTF-8 byte length against Windows character limits (513 / 32767). Non-ASCII names hit TooLong sooner than the Windows limits would require. Not a practical concern for short ASCII names.
  ? Does the keyring 4 `v1` feature pull the apple and zbus stores only as target-specific dependencies? The features page lists them unconditionally. This matters for the size of the Windows build's dependency tree.
  ? keyring::v1 has no documented way to swap in a mock store, so using keyring-core directly is the testable path.
  ? Was the mock::Cred / as_any downcast API stable across keyring-core 1.0.x? Checked only at 1.0.1.
## corrections
! Windows limits are enforced as Error::TooLong ... At most 64 attributes. => TooLong is enforced for: the password blob (UTF-16LE) and set_secret, both over 2560 bytes; user over 513; generic target over 32767; target_alias and comment over 256. An empty target gives Invalid. There is no attribute-count check. CRED_MAX_ATTRIBUTES (64) is only a Windows API limit, and this store never reaches it.
! new_with_configuration doc says defaults are `keyring:`, `@`, empty, which conflicts with Store::new (open question: verify in source). => Both Store::new() and new_with_configuration(&HashMap::new()) produce target names of the form "{user}.{service}". You can rely on this format for 1.1.0. The 'keyring:'/'@' text in the docs is a doc bug.
notes: I checked the crates.io API and the actual published .crate sources: keyring 4.2.0 and 3.6.3, keyring-core 1.0.1, windows-native-keyring-store 1.1.0. Confirmed:
- Versions and dates. keyring 4.2.0 came out 2026-08-29 and 4.0.0 on 2026-04-26. 4.1.3 is yanked. The newest 3.x is 3.6.3 (2025-07-27). keyring-core 1.0.1 came out 2026-10-13 and 1.0.0 on 2026-04-21. windows-native-keyring-store 1.1.0 came out 2026-05-24. It depends on keyring-core "1" and windows-sys 0.61.2.
- keyring 4 features: default = ["v1"], and v1 = apple-native-keyring-store/keychain, windows-native-keyring-store, zbus-secret-service-keyring-store. windows-native-keyring-store is not declared in [features]; it is an implicit feature created by an optional dependency.
- Open question 3, resolved: all store dependencies in keyring 4 are target-specific. For example, windows-native-keyring-store sits under cfg(target_os = "windows") and zbus under cfg(unix, not macos/ios/android). A Windows build therefore does not pull in the apple or zbus stores. keyring 4 gives a compile_error if neither v1 nor cli is enabled.
- v1::Entry signatures, store_status, and NoDefaultStore all match. The store is set lazily through a LazyLock on the first Entry::new or store_status call, which then calls keyring_core::set_default_store. That would overwrite a mock set earlier. Setting a mock afterwards would replace the real store, but that is undocumented. v1::Entry has a public `inner: keyring_core::Entry` field.
- keyring-core 1.0.1: the Entry API, the Error variants (#[non_exhaustive]; PlatformError = Box<dyn Error + Send + Sync>), and set_default_store / unset_default_store / get_default_store (a static RwLock) all match. get_credential returns Result<Entry>. The only feature is `sample` and there are no defaults. mock::Store::build reuses an Arc<Cred> when service and user match, rejects non-empty modifiers with NotSupportedByStore, and reports persistence as ProcessOnly. Calling set_error makes the next call fail once.
- Credential/CredentialStore = dyn ... + Send + Sync.
- keyring 3.6.3: no default features, `windows-native` = windows-sys + byteorder, falls back to the mock when the feature is missing on Windows, and the Error variants and mock docs (set_default_credential_builder, get_credential().downcast_ref, state per entry) all match.
- The Windows thread-ordering warning is present in the windows-native-keyring-store crate docs.
- The code samples are consistent with the source.

Remaining open question: whether the mock API was stable across keyring-core 1.0.0 vs 1.0.1. Not checked; I only inspected 1.0.1. Source unpacked at C:\Users\js\AppData\Local\Temp\claude\E--ai-image-generator\4b63e1e7-1dc6-4c80-985d-ca80de31bd65\scratchpad\crates.
## samples
# Cargo.toml (v4 style, recommended)
[dependencies]
keyring-core = "1"            # 1.0.1; mock store included, no feature needed

[target.'cfg(windows)'.dependencies]
windows-native-keyring-store = "1"   # 1.1.0
// startup (production, Windows)
keyring_core::set_default_store(windows_native_keyring_store::Store::new()?);
let entry = keyring_core::Entry::new("my-service", "my-user")?; // target name "my-user.my-service"
entry.set_password("secret")?;
let pw: String = entry.get_password()?;
entry.delete_credential()?;
match entry.get_password() { Err(keyring_core::Error::NoEntry) => {}, other => panic!("{other:?}") }
// tests (v4): never touches Credential Manager
use keyring_core::{mock, Entry, Error};
keyring_core::set_default_store(mock::Store::new().unwrap());
let entry = Entry::new("service", "user").unwrap();
entry.set_password("test").unwrap();
let m: &mock::Cred = entry.as_any().downcast_ref().unwrap();
m.set_error(Error::Invalid("mock error".into(), "takes precedence".into()));
assert!(entry.get_password().is_err());
assert_eq!(entry.get_password().unwrap(), "test");
# Legacy v3 alternative
# keyring = { version = "3.6.3", features = ["windows-native"] }
# tests: keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
# keyring 4 compat wrapper (auto store, default feature v1): keyring = "4.2"  -> keyring::v1::Entry::new(service, username)
# tauri
- [doc] Current stable Rust crate versions (crates.io max_stable_version, all published 2026-10-14): tauri 2.12.4, tauri-build 2.7.3, tauri-plugin-dialog 2.8.3, tauri-plugin-opener 2.7.2, tauri-cli 2.12.3. The tauri 2.12.4 MSRV (minimum supported Rust version) is 1.90. The repo pins Rust 1.97 (rust-toolchain.toml) and rustc 1.97.1 is installed, so this is satisfied.
- [doc] Current npm versions (registry 'latest' tag): @tauri-apps/api 2.12.4, @tauri-apps/cli 2.12.3, @tauri-apps/plugin-dialog 2.8.3, @tauri-apps/plugin-opener 2.7.2, svelte 5.57.2, @sveltejs/vite-plugin-svelte 7.3.1 (peer: vite ^8.0.0, svelte ^5.46.4), vite 8.3.4, svelte-check 4.7.7 (peer typescript ^5 || ^6), typescript 7.0.2, @tsconfig/svelte 5.0.8, create-tauri-app 4.7.5.
- [doc] GOTCHA: typescript@latest is 7.0.2, but svelte-check 4.7.7 only allows typescript ^5 || ^6 as a peer, and the official create-tauri-app template pins "typescript": "~6.0.3". So pin TypeScript ~6.0.x rather than taking latest.
- [doc] The create-tauri-app template name is 'svelte-ts' (directory template-svelte-ts). It is a SvelteKit template, not plain Svelte+Vite: it uses @sveltejs/kit ^2.65.1, @sveltejs/adapter-static ^3.0.10 with fallback index.html, @sveltejs/vite-plugin-svelte ^7.1.2, svelte ^5.56.3, vite ^8.0.16, @tauri-apps/api ^2, @tauri-apps/plugin-opener ^2. Its manifest sets devUrl http://localhost:1420, frontendDist ../build, and beforeDev/BuildCommand '<pm> dev'/'<pm> build'. The invocation would be `pnpm create tauri-app --template svelte-ts` (not run here). For a plain Svelte 5 + Vite SPA without Kit, follow the official Vite guide instead (frontendDist ../dist).
- [doc] The official Vite guide recommends: devUrl http://localhost:5173 (the template uses 1420; either works if it matches vite server.port), frontendDist ../dist, server.strictPort true, host from process.env.TAURI_DEV_HOST, hmr ws on port 1421 when host is set, clearScreen false, watch.ignored ['**/src-tauri/**'], envPrefix ['VITE_','TAURI_ENV_*'], build target chrome105 on Windows, and minify/sourcemap toggled by TAURI_ENV_DEBUG.
- [doc] Official src-tauri/Cargo.toml template (v2): [lib] name = "<x>_lib", crate-type = ["staticlib","cdylib","rlib"]; [build-dependencies] tauri-build = { version = "2" }; [dependencies] tauri = { version = "2" }, tauri-plugin-opener = "2", serde (derive), serde_json. The _lib suffix is required because of a Windows bin/lib name clash (cargo#8519). build.rs is just `fn main() { tauri_build::build() }`.
- [doc] Official tauri.conf.json v2 template: "$schema": "https://schema.tauri.app/config/2", productName, version, identifier, build{beforeDevCommand, devUrl, beforeBuildCommand, frontendDist}, app{windows:[{title,width,height}], security{csp:null}}, bundle{active:true, targets:"all", icon:[...]}. A window's label defaults to "main", and capabilities target windows by that label.
- [doc] bundle.targets takes "all", a single bundle type, or a list (case-insensitive), so ["nsis"] restricts the build to the NSIS -setup.exe. bundle.windows.webviewInstallMode.type is one of downloadBootstrapper (default, silent:true, needs internet, +0MB), embedBootstrapper (~1.8MB), offlineInstaller (~127MB), fixedRuntime (path required, ~180MB), or skip (not recommended). WebView2 ships with Windows 10 (April 2018 or later) and Windows 11. The --bundles CLI flag was not confirmed on that docs page (check `tauri build --help`).
- [doc] Official capabilities/default.json template: {"$schema": "../gen/schemas/desktop-schema.json", "identifier": "default", "windows": ["main"], "permissions": ["core:default", "opener:default"]}. Add "dialog:default" when using the dialog plugin. Files in src-tauri/capabilities/ are picked up automatically.
- [doc] main.rs/lib.rs split: main.rs holds #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] and fn main() { <lib_name>::run() }. lib.rs has #[cfg_attr(mobile, tauri::mobile_entry_point)] pub fn run() { tauri::Builder::default().plugin(...).invoke_handler(tauri::generate_handler![...]).run(tauri::generate_context!()).expect(...) }.
- [doc] For progress streaming, use tauri::ipc::Channel<T> passed as a command argument; it is the recommended mechanism. Global events (Emitter::emit) suit low-volume notifications. The TS side creates `new Channel<T>()` from @tauri-apps/api/core, sets `onmessage`, and passes it as an invoke argument (camelCase arg name maps to the snake_case Rust parameter). The docs' event enum uses #[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "event", content = "data")], which gives a TS discriminated union.
- [doc] Async commands run on a separate async task, so long work does not block the UI. Sync commands run on the main thread unless marked #[tauri::command(async)]. Async commands cannot take borrowed args such as &str (use String). Everything returned, including errors, must implement serde::Serialize, so wrap image-core's thiserror errors in a Serialize impl. AppHandle, WebviewWindow and State<T> (registered via .manage()) can be injected as parameters.
- [doc] Displaying local images: convertFileSrc(filePath, protocol='asset') from @tauri-apps/api/core. It requires app.security.assetProtocol.enable = true plus a scope (FsScope: an array of globs or {allow, deny, requireLiteralLeadingDot}; default {enable:false, scope:[]}). The CSP must also allow the asset origins, e.g. img-src 'self' asset: http://asset.localhost (on Windows the URL is http://asset.localhost/...). csp is "injected on all HTML files on the built application" and also applies in dev unless devCsp is set.
- [inf] The tauri crate has a 'protocol-asset' Cargo feature (it enables 'http-range'). With assetProtocol.enable = true, declare tauri = { version = "2.12", features = ["protocol-asset"] }. Otherwise the build-time feature/config consistency check fails, or the CLI rewrites Cargo.toml for you.
- [doc] Dialog plugin setup: `pnpm tauri add dialog` (or cargo add tauri-plugin-dialog in src-tauri + pnpm add @tauri-apps/plugin-dialog), then .plugin(tauri_plugin_dialog::init()) in lib.rs and "dialog:default" in the capability. dialog:default enables all dialog types (allow-message, allow-save, allow-open, ...). JS: `import { open } from '@tauri-apps/plugin-dialog'`; open({ multiple:false, directory:true }) returns a folder path (string | null); filters [{name, extensions}] picks files.
- [doc] Opener plugin: .plugin(tauri_plugin_opener::init()) (already in the template) and "opener:default", which allows allow-open-url, allow-reveal-item-in-dir and allow-default-urls (http/https/mailto/tel). revealItemInDir(path | path[]) opens Explorer with the item selected and is covered by opener:default. openPath(folder) opens the folder itself, but requires an explicit opener:allow-open-path permission with a path scope such as {"identifier":"opener:allow-open-path","allow":[{"path":"$APPDATA/**"}]}. Rust side: use tauri_plugin_opener::OpenerExt; app.opener().open_path(p, None::<&str>).
- [doc] Workspace membership is officially supported: the src-tauri folder can be the top-level project or a member of a Rust workspace. The Tauri CLI locates the Rust project by finding tauri.conf.json.
- [inf] Workspace gotchas: (a) add "apps/studio/src-tauri" to root [workspace].members. It then shares the root Cargo.lock and the root target/ dir, so build output lands in E:\ai-image-generator\target\release\bundle\nsis\, not src-tauri/target. (b) The template's [profile.release] (lto, codegen-units=1, panic="abort", strip) is ignored when placed in a member crate; Cargo only honours profiles in the workspace root, so move it there. panic="abort" would then also apply to image-cli. (c) The template uses edition 2024 but the workspace uses 2021. Either is fine; use edition.workspace = true for consistency. (d) Depend on image-core via `image-core = { workspace = true }` (already declared in [workspace.dependencies]). (e) `pnpm tauri build` (the @tauri-apps/cli npm package, run from apps/studio where package.json lives) and `cargo tauri build` (needs `cargo install tauri-cli --version ^2.12`, run from apps/studio or src-tauri) do the same thing. Prefer the pnpm script so the CLI version is locked in package.json. (f) image-core's tokio dependency is compatible: tauri::async_runtime is tokio-based.
- [doc] Headless Rust tests of commands: enable the tauri 'test' feature (marked unstable), e.g. [dev-dependencies] tauri = { version = "2.12", features = ["test"] }. Then use tauri::test::{mock_builder, mock_context, noop_assets, mock_app, get_ipc_response, assert_ipc_response, INVOKE_KEY} with the MockRuntime. Build app = mock_builder().invoke_handler(generate_handler![...]).build(mock_context(noop_assets())), create a webview with WebviewWindowBuilder::new(&app, "main", Default::default()).build(), and send an InvokeRequest {cmd, callback: CallbackFn(0), error: CallbackFn(1), url: "http://tauri.localhost", body: InvokeBody::default()/Json, headers, invoke_key: INVOKE_KEY}. Commands that need a real runtime type should be generic over R: Runtime. In practice the simplest approach is to keep logic in plain functions (or image-core) and unit-test those directly.
- [doc] Frontend-only tests: mockIPC((cmd, args) => ...) and mockWindows() from @tauri-apps/api/mocks with Vitest. mockIPC(..., { shouldMockEvents: true }) partially mocks events. Call clearMocks() after each test. The v2 mocking page does not cover Rust-side tauri::test.
  ? Not confirmed for v2: whether tauri-build fails, or the CLI auto-adds the feature, when assetProtocol.enable is true but 'protocol-asset' is missing from Cargo.toml. Declaring it explicitly avoids the question.
  ? Exact CSP connect-src needed for IPC on Windows (ipc: http://ipc.localhost) is from general knowledge and was not re-quoted from the docs. Starting with csp:null and then tightening, while watching the devtools console, is the safe path.
  ? Whether the tauri CLI writes bundles to the workspace-root target/ dir (expected via cargo metadata) is not stated in the docs. Confirm after the first `pnpm tauri build` (look for target/release/bundle/nsis).
  ? The --bundles nsis CLI flag was not confirmed on the docs page. bundle.targets ["nsis"] in config is the documented route; confirm with `pnpm tauri build --help`.
  ? Whether the tauri::test MockRuntime test can be generic-free for commands taking AppHandle (AppHandle<MockRuntime>). Commands that take AppHandle may need to be generic over R: tauri::Runtime to be testable.
  ? Plain Svelte+Vite vs the official svelte-ts template (SvelteKit + adapter-static, frontendDist ../build): a decision for the implementer. The code samples assume plain Vite with ../dist.
## corrections
! Vite config code sample: build.minify = !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false (presented as 'per v2.tauri.app/start/frontend/vite') => Use `minify: !process.env.TAURI_ENV_DEBUG` (the default minifier) and `sourcemap: !!process.env.TAURI_ENV_DEBUG`. Do not use 'esbuild' unless esbuild is added as a devDependency.
! The --bundles CLI flag was not confirmed (open question; check `tauri build --help`) => `pnpm tauri build --bundles nsis` is valid. Setting bundle.targets: ["nsis"] in config is an equivalent, persistent alternative.
! protocol-asset feature: confidence 'inferred'; 'exact v2 error/auto-fix behaviour was not confirmed' (open question) => Declare tauri = { version = "2.12", features = ["protocol-asset"] } when app.security.assetProtocol.enable = true. Otherwise a direct cargo build errors in tauri-build's manifest check, and `tauri dev`/`tauri build` rewrite Cargo.toml. This can now be marked documented (from source).
! Headless Rust tests: enable the tauri 'test' feature (marked unstable) => Use features = ["test"]. No 'unstable' feature is needed. The module's API is documented as unstable, so it may change in minor releases.
notes: Checked live on 2026-10-18 against crates.io, the npm registry, raw create-tauri-app templates (dev branch), raw tauri-docs (v2 branch), schema.tauri.app/config/2 (whose $id resolves to 2.12.4), and the source of the published crates tauri 2.12.4, tauri-build 2.7.3, tauri-plugin-dialog 2.8.3 and tauri-plugin-opener 2.7.2. Confirmed claims:
- All crate and npm versions, including the vite-plugin-svelte and svelte-check peer ranges and the 2026-10-14 publish dates. crates.io rust_version for tauri 2.12.4 is 1.90. The repo pins Rust 1.97 and rustc 1.97.1 is installed.
- TypeScript: 7.0.2 is the latest tag, but svelte-check only allows ^5 || ^6 as a peer, and the template pins ~6.0.3.
- The svelte-ts template is SvelteKit with adapter-static. Its versions, devUrl 1420 and frontendDist ../build all match. The template also lists svelte-check ^4.6.0, which the claim leaves out.
- Cargo.toml template: _lib comment, crate-type, edition 2024 and the [profile.release] block. build.rs, main.rs and lib.rs match the template.
- tauri.conf.json template, and the window label defaulting to 'main' (schema). minWidth and minHeight exist in the schema.
- bundle.targets accepts all, a list, or one type, case-insensitive. webviewInstallMode variants: downloadBootstrapper, embedBootstrapper and offlineInstaller each take silent (default true); fixedRuntime requires path; skip. One nit: the prose table on the Windows installer page says 'fixedVersion', but the actual config value is 'fixedRuntime', as the claim says.
- Capabilities default.json.
- Channel/event guidance, the serde attribute, and the camelCase argument mapping ('Arguments should be passed as a JSON object with camelCase keys').
- Async commands use async_runtime::spawn; sync commands run on the main thread unless marked #[tauri::command(async)]; borrowed args are not allowed in async commands; returned values must be Serialize.
- convertFileSrc(filePath, protocol?). The asset URL is http://asset.localhost/ on Windows (source and doc comment suggest img-src 'self' asset: http://asset.localhost). FsScope shape, plus csp/devCsp semantics (schema).
- Dialog default permissions are exactly [allow-message, allow-save, allow-open] ('All dialog types are enabled'). The OpenDialogReturn type gives string | null for directory:true with multiple:false.
- Opener default permissions are [allow-open-url, allow-reveal-item-in-dir, allow-default-urls]. reveal_item_in_dir has no scope check. revealItemInDir(path: string | string[]). openPath needs a scoped allow-open-path. open_path(path, Option<impl Into<String>>) matches.
- Workspace membership and the 'marker' wording from project-structure.
- Mocking page: mockIPC, shouldMockEvents, mockWindows, clearMocks. It has no Rust tauri::test section.
- Repo workspace.package has version, edition 2021, license and publish, so the .workspace = true keys in the sample are valid.

Not independently verified (standard Cargo behaviour, plausibly correct):
- Member-crate profiles are ignored.
- The Tauri CLI writes bundles to the workspace-root target/.

The CSP connect-src 'ipc: http://ipc.localhost' entry was not re-checked in this pass. Starting with csp:null is still a reasonable approach.
## samples
# apps/studio/src-tauri/Cargo.toml
[package]
name = "studio"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[lib]
name = "studio_lib"            # _lib suffix avoids Windows bin/lib clash (cargo#8519)
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2.7", features = [] }

[dependencies]
tauri = { version = "2.12", features = ["protocol-asset"] }
tauri-plugin-dialog = "2.8"
tauri-plugin-opener = "2.7"
image-core = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tauri = { version = "2.12", features = ["protocol-asset", "test"] }

# root Cargo.toml: members += "apps/studio/src-tauri"; put any [profile.release] there (member profiles are ignored)
// build.rs
fn main() {
    tauri_build::build()
}
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "AI Image Studio",
  "version": "0.1.0",
  "identifier": "kr.example.ai-image-studio",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [{ "label": "main", "title": "AI Image Studio", "width": 1280, "height": 820, "minWidth": 960, "minHeight": 640 }],
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost blob: data:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost",
      "assetProtocol": { "enable": true, "scope": ["$PICTURE/**", "$DOCUMENT/**", "$APPDATA/**"] }
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
    "windows": { "webviewInstallMode": { "type": "downloadBootstrapper", "silent": true } }
  }
}
// src-tauri/capabilities/default.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "opener:default"
  ]
}
// to open a folder itself (not just reveal it), add a scoped entry:
// { "identifier": "opener:allow-open-path", "allow": [{ "path": "$PICTURE/**" }] }
// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    studio_lib::run()
}

// src/lib.rs
use serde::Serialize;
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "event", content = "data")]
pub enum GenEvent {
    Started { job_id: String, total: usize },
    Progress { job_id: String, done: usize },
    ImageSaved { job_id: String, path: String },
    Finished { job_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum CmdError { #[error("{0}")] Core(String) }
impl Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(&self.to_string()) }
}

#[tauri::command]
async fn generate(prompt: String, on_event: Channel<GenEvent>) -> Result<(), CmdError> {
    let job_id = "job-1".to_string();
    on_event.send(GenEvent::Started { job_id: job_id.clone(), total: 1 }).map_err(|e| CmdError::Core(e.to_string()))?;
    // ... call image_core here ...
    on_event.send(GenEvent::Finished { job_id }).map_err(|e| CmdError::Core(e.to_string()))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![generate])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
// frontend TS (Svelte 5)
import { invoke, Channel, convertFileSrc } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { revealItemInDir, openPath } from '@tauri-apps/plugin-opener';

type GenEvent =
  | { event: 'started'; data: { jobId: string; total: number } }
  | { event: 'progress'; data: { jobId: string; done: number } }
  | { event: 'imageSaved'; data: { jobId: string; path: string } }
  | { event: 'finished'; data: { jobId: string } };

let images = $state<string[]>([]);
const onEvent = new Channel<GenEvent>();
onEvent.onmessage = (m) => {
  if (m.event === 'imageSaved') images.push(convertFileSrc(m.data.path)); // <img src={url}>
};
await invoke('generate', { prompt: 'a cat', onEvent }); // onEvent -> on_event

const dir = await open({ directory: true, multiple: false }); // string | null
const file = await open({ multiple: false, filters: [{ name: 'Images', extensions: ['png', 'jpg', 'webp'] }] });
if (dir) await revealItemInDir(dir);   // covered by opener:default
// await openPath(dir);                // needs scoped opener:allow-open-path
// headless Rust test (tauri 'test' feature)
#[cfg(test)]
mod tests {
    use tauri::test::{mock_builder, mock_context, noop_assets, get_ipc_response, INVOKE_KEY};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::webview::InvokeRequest;

    #[tauri::command]
    fn ping(name: String) -> String { format!("pong {name}") }

    #[test]
    fn ping_works() {
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![ping])
            .build(mock_context(noop_assets()))
            .unwrap();
        let wv = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        let res = get_ipc_response(&wv, InvokeRequest {
            cmd: "ping".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::Json(serde_json::json!({ "name": "x" })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }).map(|b| b.deserialize::<String>().unwrap());
        assert_eq!(res.unwrap(), "pong x");
    }
}
// apps/studio/vite.config.ts (plain Svelte 5 + Vite, per v2.tauri.app/start/frontend/vite)
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const host = process.env.TAURI_DEV_HOST;
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420, strictPort: true, host: host || '127.0.0.1',
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target: 'chrome105',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
// package.json devDeps: @tauri-apps/cli ^2.12.3, vite ^8.3.4, @sveltejs/vite-plugin-svelte ^7.3.1, svelte ^5.57.2, svelte-check ^4.7.7, typescript ~6.0.3, @tsconfig/svelte ^5.0.8; deps: @tauri-apps/api ^2.12.4, @tauri-apps/plugin-dialog ^2.8.3, @tauri-apps/plugin-opener ^2.7.2