#!/usr/bin/env python3
"""Generate an image through CLIProxyAPI and save it below a topic folder.

CLIProxyAPI exposes OpenAI-compatible endpoints. This wrapper calls
`POST /v1/responses` with the `image_generation` tool so the project can use
one HTTP proxy instead of shelling out to `codex responses`.

Default output layout:

    output/<topic>/<filename>.png

Environment:
    CLIPROXY_BASE_URL   Base URL. Default: http://localhost:8317/v1
                        If `/v1` is omitted, it is appended automatically.
    CLIPROXY_API_KEY    API key configured in cli-proxy-api `api-keys`.
                        Falls back to OPENAI_API_KEY.
    CLIPROXY_TIMEOUT    Per-request HTTP timeout seconds. Default: 600.
    CLIPROXY_MAIN_MODEL Mainline model used to invoke the image tool.
                        CLIPROXY_MAIN_MODE is also accepted as an alias.
    CLIPROXY_IMAGE_MODEL Image generation tool model.
"""
from __future__ import annotations

import argparse
import base64
import json
import io
import mimetypes
import os
import queue
import re
import sys
import threading
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path


DEFAULT_BASE_URL = "http://localhost:8317/v1"
DEFAULT_MODEL = "gpt-5.5"
DEFAULT_IMAGE_MODEL = "grok-imagine-image-2.0"
DEFAULT_TOPIC = "image-request"
DEFAULT_TOPIC_ROOT = str(Path(__file__).resolve().parent.parent)
STOPWORDS = {
    "the",
    "and",
    "for",
    "with",
    "without",
    "from",
    "into",
    "image",
    "scene",
    "create",
    "generate",
    "make",
    "draw",
    "please",
    "using",
    "style",
    "realistic",
    "photorealistic",
}


@dataclass(frozen=True)
class GenerateImageOptions:
    prompt: str
    output: str
    model: str
    image_model: str
    size: str
    quality: str
    action: str
    events: str | None
    reference_image: list[str]
    base_url: str
    api_key: str | None
    timeout: float
    output_format: str
    topic: str
    topic_slug: str
    topic_dir: Path


def safe_topic_slug(value: str) -> str:
    """Return a filesystem-safe slug while preserving Korean/Unicode words."""
    raw = value.strip()
    raw = re.sub(r"[^\w\s.-]+", "_", raw, flags=re.UNICODE)
    raw = re.sub(r"\s+", "_", raw, flags=re.UNICODE)
    raw = raw.strip("._- ")
    if not raw:
        return DEFAULT_TOPIC
    return raw[:80]


def infer_topic_from_prompt(prompt: str) -> str:
    """Infer a short topic from the first meaningful words of the prompt."""
    first_sentence = re.split(r"[\n.!?。！？]", prompt.strip(), maxsplit=1)[0]
    words = re.findall(r"[\w가-힣]+", first_sentence, flags=re.UNICODE)
    selected: list[str] = []
    for word in words:
        lowered = word.lower()
        if lowered in STOPWORDS:
            continue
        if len(word) < 2:
            continue
        selected.append(word)
        if len(selected) >= 6:
            break
    if not selected:
        return DEFAULT_TOPIC
    return "_".join(selected)


def resolve_base_url(value: str | None) -> str:
    base = (value or os.environ.get("CLIPROXY_BASE_URL") or DEFAULT_BASE_URL).strip()
    if not base:
        base = DEFAULT_BASE_URL
    base = base.rstrip("/")
    if not base.endswith("/v1"):
        base = f"{base}/v1"
    return base


def _windows_user_environment(name: str) -> str | None:
    if os.name != "nt":
        return None
    try:
        import winreg

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Environment") as environment:
            value = winreg.QueryValueEx(environment, name)[0]
    except (ImportError, FileNotFoundError, OSError):
        return None
    return value if isinstance(value, str) and value else None


def resolve_api_key(value: str | None) -> str | None:
    if value:
        return value
    return (
        os.environ.get("CLIPROXY_API_KEY")
        or os.environ.get("OPENAI_API_KEY")
        or _windows_user_environment("CLIPROXY_API_KEY")
        or _windows_user_environment("OPENAI_API_KEY")
    )


def resolve_topic(prompt: str, topic: str | None, topic_root: str) -> tuple[str, str, Path]:
    resolved_topic = topic.strip() if topic and topic.strip() else infer_topic_from_prompt(prompt)
    topic_slug = safe_topic_slug(resolved_topic)
    return resolved_topic, topic_slug, Path(topic_root) / "output" / topic_slug


def resolve_output_path(output: str | None, topic_dir: Path, output_format: str) -> Path:
    default_extension = "jpg" if output_format == "jpeg" else output_format
    if not output:
        return topic_dir / f"image.{default_extension}"

    path = Path(output)
    if path.is_absolute() or path.parent != Path("."):
        return path
    return topic_dir / path.name


def image_to_data_url(path: Path) -> str:
    mime_type = mimetypes.guess_type(path.name)[0] or "image/png"
    image_b64 = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:{mime_type};base64,{image_b64}"


def parse_size(size: str) -> tuple[int, int]:
    try:
        width_text, height_text = size.lower().split("x", 1)
        width, height = int(width_text), int(height_text)
    except (AttributeError, TypeError, ValueError) as exc:
        raise ValueError(f"invalid size {size!r}; expected WIDTHxHEIGHT") from exc
    if width <= 0 or height <= 0:
        raise ValueError(f"invalid size {size!r}; dimensions must be positive")
    return width, height


def provider_request_size(size: str) -> str:
    width, height = parse_size(size)
    padded_width = ((width + 15) // 16) * 16
    padded_height = ((height + 15) // 16) * 16
    return f"{padded_width}x{padded_height}"


def build_payload(args: GenerateImageOptions) -> dict[str, object]:
    tool: dict[str, object] = {
        "type": "image_generation",
        "size": provider_request_size(args.size),
        "quality": args.quality,
        "model": args.image_model,
        "output_format": args.output_format,
    }
    if args.action:
        tool["action"] = args.action

    content: list[dict[str, object]] = [{"type": "input_text", "text": args.prompt}]
    for image_path in args.reference_image:
        content.append(
            {
                "type": "input_image",
                "image_url": image_to_data_url(Path(image_path)),
            }
        )

    return {
        "model": args.model,
        "instructions": "Use the image_generation tool to create the requested image. Return the image generation result.",
        "input": [{"role": "user", "content": content}],
        "tools": [tool],
        "tool_choice": {"type": "image_generation"},
        "store": False,
        "stream": True,
    }

def image_backend(image_model: str) -> str:
    model = image_model.lower()
    if model.startswith("gemini-") and "image" in model:
        return "gemini"
    if model.startswith(("grok-imagine-", "gpt-image-")):
        return "openai-images"
    return "responses"


def _root_base_url(base_url: str) -> str:
    return base_url[:-3] if base_url.endswith("/v1") else base_url


def _gemini_aspect_ratio(size: str) -> str:
    width, height = parse_size(size)
    ratio = width / height
    supported = {
        "1:1": 1.0,
        "2:3": 2 / 3,
        "3:2": 3 / 2,
        "3:4": 3 / 4,
        "4:3": 4 / 3,
        "4:5": 4 / 5,
        "5:4": 5 / 4,
        "9:16": 9 / 16,
        "16:9": 16 / 9,
        "21:9": 21 / 9,
    }
    return min(supported, key=lambda key: abs(supported[key] - ratio))

def _gemini_image_size(size: str) -> str:
    width, height = parse_size(size)
    longest_edge = max(width, height)
    if longest_edge <= 768:
        return "512"
    if longest_edge <= 1536:
        return "1K"
    if longest_edge <= 3072:
        return "2K"
    return "4K"


def build_gemini_payload(args: GenerateImageOptions) -> dict[str, object]:
    parts: list[dict[str, object]] = [{"text": args.prompt}]
    for image_path in args.reference_image:
        path = Path(image_path)
        mime_type = mimetypes.guess_type(path.name)[0] or "image/png"
        parts.append(
            {
                "inlineData": {
                    "mimeType": mime_type,
                    "data": base64.b64encode(path.read_bytes()).decode("ascii"),
                }
            }
        )
    image_size = _gemini_image_size(args.size)
    return {
        "contents": [{"role": "user", "parts": parts}],
        "generationConfig": {
            "responseModalities": ["IMAGE", "TEXT"],
            "imageConfig": {
                "aspectRatio": _gemini_aspect_ratio(args.size),
                "imageSize": image_size,
            },
        },
    }


def build_openai_images_payload(args: GenerateImageOptions) -> dict[str, object]:
    is_gpt_image = args.image_model.lower().startswith("gpt-image-")
    if args.reference_image and not is_gpt_image:
        raise RuntimeError(
            f"{args.image_model} reference-image editing is not yet supported by this CLI; use a Gemini image model or gpt-image model"
        )
    if is_gpt_image and args.action == "edit" and not args.reference_image:
        raise RuntimeError("GPT image edits require at least one reference image")

    payload: dict[str, object] = {
        "model": args.image_model,
        "prompt": args.prompt,
        "n": 1,
        "size": provider_request_size(args.size),
        "quality": args.quality,
    }
    if is_gpt_image:
        payload["output_format"] = args.output_format
        if args.reference_image:
            payload["images"] = [
                {"image_url": image_to_data_url(Path(image_path))}
                for image_path in args.reference_image
            ]
    else:
        payload["response_format"] = "b64_json"
    return payload


def _call_json_blocking(url: str, payload: dict[str, object], args: GenerateImageOptions) -> tuple[bytes, int]:
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    if args.api_key:
        headers["Authorization"] = f"Bearer {args.api_key}"
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        method="POST",
        headers=headers,
    )
    try:
        with urllib.request.urlopen(request, timeout=args.timeout) as response:
            return response.read(), response.status
    except urllib.error.HTTPError as exc:
        return exc.read() or b"", exc.code
    except urllib.error.URLError as exc:
        raise RuntimeError(f"Cannot connect to CLIProxyAPI at {url}: {exc}") from exc
    except TimeoutError as exc:
        raise RuntimeError(f"Timed out waiting for CLIProxyAPI at {url} after {args.timeout:g}s") from exc


def call_native_image_backend(args: GenerateImageOptions) -> tuple[bytes, int, str]:
    backend = image_backend(args.image_model)
    if backend == "gemini":
        url = f"{_root_base_url(args.base_url)}/v1beta/models/{args.image_model}:generateContent"
        payload = build_gemini_payload(args)
    elif backend == "openai-images":
        endpoint = (
            "edits"
            if args.image_model.lower().startswith("gpt-image-") and args.reference_image
            else "generations"
        )
        url = f"{args.base_url}/images/{endpoint}"
        payload = build_openai_images_payload(args)
    else:
        raise RuntimeError(f"No native image backend for {args.image_model}")
    return (*_call_json_blocking(url, payload, args), url)


def extract_native_image(body: bytes, backend: str) -> tuple[bytes | None, str | None]:
    try:
        response = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None, None
    if backend == "gemini":
        for candidate in response.get("candidates", []):
            for part in candidate.get("content", {}).get("parts", []):
                inline = part.get("inlineData") or part.get("inline_data")
                if isinstance(inline, dict) and isinstance(inline.get("data"), str):
                    return base64.b64decode(inline["data"]), inline.get("mimeType") or inline.get("mime_type")
        return None, None
    for item in response.get("data", []):
        encoded = item.get("b64_json") if isinstance(item, dict) else None
        if isinstance(encoded, str) and encoded:
            return base64.b64decode(encoded), "image/png"
    return None, None


def normalize_image_bytes(
    image_bytes: bytes,
    output_format: str,
    target_size: str,
) -> bytes:
    target_width, target_height = parse_size(target_size)
    try:
        from PIL import Image, ImageOps
    except ImportError as exc:
        raise RuntimeError(
            "Pillow is required to convert provider output to the exact requested dimensions"
        ) from exc

    save_format = "JPEG" if output_format == "jpeg" else output_format.upper()

    with Image.open(io.BytesIO(image_bytes)) as image:
        source_matches = image.size == (target_width, target_height)
        format_matches = (image.format or "").upper() == save_format
        if source_matches and format_matches:
            return image_bytes

        fitted = ImageOps.fit(
            image,
            (target_width, target_height),
            method=Image.Resampling.LANCZOS,
            centering=(0.5, 0.5),
        )

        if save_format == "JPEG" and fitted.mode not in ("RGB", "L"):
            fitted = fitted.convert("RGB")
        converted = io.BytesIO()
        fitted.save(converted, format=save_format)
        return converted.getvalue()


def parse_response_events(body: bytes) -> list[dict[str, object]]:
    """Parse a CLIProxyAPI response body as either JSON or text/event-stream."""
    events: list[dict[str, object]] = []
    text = body.decode("utf-8", errors="replace").replace("\r\n", "\n")
    stripped = text.strip()
    if not stripped:
        return events

    try:
        direct = json.loads(stripped)
    except json.JSONDecodeError:
        direct = None
    if isinstance(direct, dict):
        return [direct]

    for raw_block in text.split("\n\n"):
        block = raw_block.strip()
        if not block:
            continue
        data_lines: list[str] = []
        for line in block.splitlines():
            line = line.strip()
            if line.startswith("data:"):
                data_lines.append(line[len("data:") :].strip())
        if not data_lines:
            continue
        joined = "\n".join(data_lines)
        if joined == "[DONE]":
            continue
        try:
            event = json.loads(joined)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict):
            events.append(event)
    return events


def extract_image_from_events(events: list[dict[str, object]]) -> str | None:
    """Find an image_generation_call result base64 string."""
    image_b64: str | None = None
    for event in events:
        event_type = event.get("type")
        if event_type == "response.completed":
            response = event.get("response")
            if isinstance(response, dict):
                image_b64 = extract_image_from_output(response.get("output")) or image_b64
        elif event_type == "response.output_item.done":
            item = event.get("item")
            if isinstance(item, dict) and item.get("type") == "image_generation_call":
                result = item.get("result")
                if isinstance(result, str) and result:
                    image_b64 = result
        elif event.get("object") == "response":
            image_b64 = extract_image_from_output(event.get("output")) or image_b64
    return image_b64


def extract_image_from_output(output: object) -> str | None:
    if not isinstance(output, list):
        return None
    image_b64: str | None = None
    for item in output:
        if not isinstance(item, dict):
            continue
        if item.get("type") == "image_generation_call":
            result = item.get("result")
            if isinstance(result, str) and result:
                image_b64 = result
    return image_b64


def _call_cliproxy_blocking(args: GenerateImageOptions) -> tuple[bytes, int]:
    """POST /v1/responses to CLIProxyAPI. Returns (body_bytes, status_code)."""
    payload = build_payload(args)
    body = json.dumps(payload).encode("utf-8")

    url = f"{args.base_url}/responses"
    headers = {
        "Content-Type": "application/json",
        "Accept": "text/event-stream, application/json",
    }
    if args.api_key:
        headers["Authorization"] = f"Bearer {args.api_key}"

    request = urllib.request.Request(url, data=body, method="POST", headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=args.timeout) as response:
            return response.read(), response.status
    except urllib.error.HTTPError as exc:
        return exc.read() or b"", exc.code
    except urllib.error.URLError as exc:
        raise RuntimeError(f"Cannot connect to CLIProxyAPI at {url}: {exc}") from exc
    except TimeoutError as exc:
        raise RuntimeError(f"Timed out waiting for CLIProxyAPI at {url} after {args.timeout:g}s") from exc


def call_cliproxy(args: GenerateImageOptions) -> tuple[bytes, int]:
    """Call CLIProxyAPI with a wall-clock timeout, not just socket timeouts."""
    result_queue: queue.Queue[tuple[bytes, int] | BaseException] = queue.Queue(maxsize=1)

    def worker() -> None:
        try:
            result_queue.put(_call_cliproxy_blocking(args))
        except BaseException as exc:
            result_queue.put(exc)

    thread = threading.Thread(target=worker, daemon=True)
    thread.start()
    try:
        result = result_queue.get(timeout=args.timeout)
    except queue.Empty as exc:
        raise RuntimeError(
            f"Timed out waiting for CLIProxyAPI at {args.base_url}/responses after {args.timeout:g}s"
        ) from exc
    if isinstance(result, BaseException):
        raise result
    return result


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate an image through CLIProxyAPI and save it under output/<topic>/."
    )
    _ = parser.add_argument("prompt", help="Image prompt")
    _ = parser.add_argument("-o", "--output", default=None, help="Output image path. Bare filenames are saved under output/<topic>/.")
    _ = parser.add_argument("--topic", default=None, help="Topic folder name. Defaults to an inferred prompt topic.")
    _ = parser.add_argument("--topic-root", default=DEFAULT_TOPIC_ROOT, help="Root directory that contains topic folders")
    _ = parser.add_argument("--model", default=os.environ.get("CLIPROXY_MAIN_MODE") or os.environ.get("CLIPROXY_MAIN_MODEL") or DEFAULT_MODEL, help="Mainline model used to call the tool")
    _ = parser.add_argument("--image-model", default=os.environ.get("CLIPROXY_IMAGE_MODEL", DEFAULT_IMAGE_MODEL), help="Image generation tool model")
    _ = parser.add_argument("--size", default="1920x1080", help="Exact output size; provider requests are padded or bucketed automatically")
    _ = parser.add_argument("--quality", default="high", help="Image quality: low, medium, high")
    _ = parser.add_argument("--action", choices=("auto", "generate", "edit"), default="generate", help="Image tool action")
    _ = parser.add_argument("--events", help="Optional path to save the raw CLIProxyAPI response body")
    _ = parser.add_argument("--reference-image", action="append", default=[], help="Optional reference image path. Can be passed more than once.")
    _ = parser.add_argument("--base-url", default=None, help="CLIProxyAPI base URL (e.g. http://localhost:8317/v1)")
    _ = parser.add_argument("--api-key", default=None, help="CLIProxyAPI API key. Falls back to process or Windows user environment CLIPROXY_API_KEY/OPENAI_API_KEY.")
    _ = parser.add_argument("--timeout", type=float, default=float(os.environ.get("CLIPROXY_TIMEOUT", "600")), help="HTTP timeout seconds")
    _ = parser.add_argument("--output-format", default="png", choices=("png", "jpeg", "webp"), help="Image output format requested from upstream")
    namespace = parser.parse_args()

    topic, topic_slug, topic_dir = resolve_topic(
        namespace.prompt,
        namespace.topic,
        namespace.topic_root,
    )
    output_path = resolve_output_path(namespace.output, topic_dir, namespace.output_format)

    args = GenerateImageOptions(
        prompt=namespace.prompt,
        output=str(output_path),
        model=namespace.model,
        image_model=namespace.image_model,
        size=namespace.size,
        quality=namespace.quality,
        action=namespace.action,
        events=namespace.events,
        reference_image=list(namespace.reference_image),
        base_url=resolve_base_url(namespace.base_url),
        api_key=resolve_api_key(namespace.api_key),
        timeout=namespace.timeout,
        output_format=namespace.output_format,
        topic=topic,
        topic_slug=topic_slug,
        topic_dir=topic_dir,
    )

    for image_path in args.reference_image:
        if not Path(image_path).is_file():
            _ = sys.stderr.write(f"Reference image not found: {image_path}\n")
            return 2

    backend = image_backend(args.image_model)
    request_url = f"{args.base_url}/responses"
    try:
        if backend == "responses":
            body_bytes, status = call_cliproxy(args)
        else:
            body_bytes, status, request_url = call_native_image_backend(args)
    except RuntimeError as exc:
        _ = sys.stderr.write(f"{exc}\n")
        return 1

    if args.events:
        events_path = Path(args.events)
        events_path.parent.mkdir(parents=True, exist_ok=True)
        _ = events_path.write_bytes(body_bytes)

    if status != 200:
        _ = sys.stderr.write(
            f"CLIProxyAPI request failed: HTTP {status}\n"
            f"URL: {request_url}\n"
        )
        try:
            _ = sys.stderr.write(body_bytes.decode("utf-8", errors="replace") + "\n")
        except Exception:
            pass
        return 1

    if backend == "responses":
        events = parse_response_events(body_bytes)
        image_b64 = extract_image_from_events(events)
        if not image_b64:
            _ = sys.stderr.write("No image_generation_call result found in response.\n")
            if not args.events:
                _ = sys.stderr.write("Re-run with --events <path> to inspect the raw response body.\n")
            return 1
        image_bytes = base64.b64decode(image_b64)
    else:
        image_bytes, _ = extract_native_image(body_bytes, backend)
        if not image_bytes:
            _ = sys.stderr.write(f"No image data found in {backend} response.\n")
            return 1

    try:
        image_bytes = normalize_image_bytes(image_bytes, args.output_format, args.size)
    except (RuntimeError, ValueError) as exc:
        _ = sys.stderr.write(f"{exc}\n")
        return 1

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    _ = output_path.write_bytes(image_bytes)
    print(f"Topic: {args.topic_slug}")
    print(f"Saved {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
