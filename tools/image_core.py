"""Shared model catalog, proxy connection check and prompt composition.

CLI(`tools/ai_image.py --list-models`)와 MCP 서버가 함께 쓰는 공용 모듈.
UI 코드는 두지 않는다.
"""

from __future__ import annotations

import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
import gen_image

DEFAULT_BASE_URL = gen_image.default_base_url()

# --- 카탈로그 (docs/SSOT.md 기준) ---
QUALITIES = ["low", "medium", "high"]
SIZES = [
    "1024x1024",
    "1536x1024",
    "1024x1536",
    "1920x1080",
    "1080x1920",
    "2048x2048",
    "3840x2160",
]
SIZE_LABELS = {
    "1024x1024": "1024×1024 정사각형",
    "1536x1024": "1536×1024 가로",
    "1024x1536": "1024×1536 세로",
    "1920x1080": "1920×1080 와이드",
    "1080x1920": "1080×1920 세로 와이드",
    "2048x2048": "2048×2048 고해상도 정사각형",
    "3840x2160": "3840×2160 4K 와이드",
}
LIMITS = {"reference_count": 4, "reference_bytes": 10485760, "max_count": 50}  # 개수, 파일당 바이트, 작업당 장수

FINAL_SIZE_NOTE = (
    "최종 파일은 요청한 크기로 정확히 저장됩니다(제공자 원본과 다르면 크롭/리샘플). "
    "quality는 제공자별 해석이 다르며 최종 픽셀 크기를 바꾸지 않습니다."
)


def _model(model_id: str, label: str, provider: str, reference_images: bool, note: str) -> dict[str, Any]:
    return {
        "id": model_id,
        "label": label,
        "provider": provider,
        "reference_images": reference_images,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": note + FINAL_SIZE_NOTE,
    }


_GROK_NOTE = "참조 이미지를 받으면 요청 전에 거부. 크기는 16px 단위로 패딩 요청. "
MODELS: list[dict[str, Any]] = [
    _model("gpt-image-2.5", "GPT Image 2.5 (최신)", "OpenAI", True,
           "최신 GPT 이미지 모델. 참조가 있으면 자동으로 edit 호출. "),
    _model("gpt-image-2.5-flare", "GPT Image 2.5 Flare", "OpenAI", True,
           "GPT Image 2.5 변형. 참조가 있으면 자동으로 edit 호출. "),
    _model("gpt-image-2.5-sunburst", "GPT Image 2.5 Sunburst", "OpenAI", True,
           "GPT Image 2.5 변형. 참조가 있으면 자동으로 edit 호출. "),
    _model("gpt-image-2", "GPT Image 2", "OpenAI", True,
           "이전 세대 모델. 참조가 있으면 자동으로 edit 호출. "),
    _model("gpt-image-1.5", "GPT Image 1.5", "OpenAI", True,
           "참조 편집은 자동으로 edit 호출. 최근 실생성 미검증. "),
    _model("gemini-3.1-flash-image", "Gemini Flash Image", "Google Gemini", True,
           "요청 size를 aspect/imageSize 버킷으로 변환해 요청하고 "
           "최종 파일은 요청 픽셀로 저장. quality 설정은 무시될 수 있음. "),
    _model("grok-imagine-image-2.0", "Grok Imagine 2.0", "xAI Grok", False, _GROK_NOTE),
    _model("grok-imagine-image-quality", "Grok Imagine Quality", "xAI Grok", False, _GROK_NOTE),
    _model("grok-imagine-image", "Grok Imagine", "xAI Grok", False, _GROK_NOTE),
]

IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp"}
CONNECTION_TIMEOUT = 15.0


# --- 설정: base_url은 환경설정에서만 해석, 키는 절대 저장 안 함 ---
def normalize_base_url(value: Any) -> str:
    raw = value if isinstance(value, str) else ""
    if not raw.strip():
        raise ValueError("base_url이 비어 있습니다.")
    if re.search(r"[\x00-\x20\x7f]", raw):
        raise ValueError("base_url에 공백·제어 문자를 쓸 수 없습니다.")
    raw = raw.strip()
    try:
        parts = urllib.parse.urlsplit(raw)
    except ValueError:
        raise ValueError("base_url 형식이 올바르지 않습니다. http(s)://host:port 형태를 사용해 주세요.")
    if parts.scheme not in ("http", "https"):
        raise ValueError("base_url은 http 또는 https만 가능합니다.")
    if not parts.hostname:
        raise ValueError("base_url 형식이 올바르지 않습니다. http(s)://host:port 형태를 사용해 주세요.")
    if parts.username or parts.password:
        raise ValueError("base_url에 인증 정보를 넣을 수 없습니다.")
    if parts.query or parts.fragment:
        raise ValueError("base_url에 쿼리·프래그먼트를 넣을 수 없습니다.")
    try:
        port = parts.port
    except ValueError:
        raise ValueError("base_url 포트가 올바르지 않습니다.")
    if port is not None and not 1 <= port <= 65535:
        raise ValueError("base_url 포트가 올바르지 않습니다.")
    host = parts.hostname
    netloc = f"[{host}]" if ":" in host else host
    if port:
        netloc += f":{port}"
    base = f"{parts.scheme}://{netloc}{parts.path.rstrip('/')}"
    if not base.endswith("/v1"):
        base = f"{base}/v1"
    return gen_image.resolve_base_url(base)


def load_base_url() -> str:
    try:
        return normalize_base_url(gen_image.resolve_base_url(None))
    except ValueError:
        sys.stderr.write("image-core: CLIPROXY_BASE_URL 형식이 잘못되어 기본값을 씁니다.\n")
        return DEFAULT_BASE_URL


def resolve_api_key() -> str | None:
    """scripts/gen_image.py와 같은 해결 순서(프로세스 → Windows 사용자 환경변수)."""
    return gen_image.resolve_api_key(None)


# --- 프롬프트 합성 ---
def compose_prompt(prompt: str, positive: str, negative: str) -> str:
    base = "\n".join(p for p in (prompt.strip(), positive.strip()) if p)
    neg = negative.strip()
    if neg:
        return f"{base}\nAvoid: {neg}" if base else f"Avoid: {neg}"
    return base


# --- 연결 확인 ---
def check_connection(base_url: str, api_key: str | None) -> dict[str, Any]:
    key = (api_key or "").strip() or resolve_api_key()

    def failure(error: str, key_configured: bool = True) -> dict[str, Any]:
        return {
            "ok": False,
            "base_url": base_url,
            "key_configured": key_configured,
            "available_models": [],
            "error": error,
        }

    if not key:
        return failure(
            "API 키가 없습니다. api_key를 함께 보내거나 CLIPROXY_API_KEY 환경변수를 설정해 주세요.",
            key_configured=False,
        )
    url = base_url.rstrip("/") + "/models"
    req = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {key}", "Accept": "application/json"},
        method="GET",
    )
    try:
        with gen_image.open_no_redirect(req, CONNECTION_TIMEOUT) as resp:
            status = resp.status
            body = resp.read(2 * 1024 * 1024)
    except urllib.error.HTTPError as exc:
        if exc.code in (401, 403):
            return failure(f"API 키가 거부되었습니다(HTTP {exc.code}). 키 값을 확인해 주세요.")
        if exc.code in (301, 302, 303, 307, 308):
            return failure("프록시가 다른 주소로 리다이렉트했습니다. base_url이 정확한지 확인해 주세요.")
        return failure(f"프록시가 HTTP {exc.code}를 반환했습니다. base_url과 API 키를 확인해 주세요.")
    except (urllib.error.URLError, OSError, ValueError):
        return failure("CLIProxyAPI에 연결할 수 없습니다. base_url과 서버 실행 상태를 확인해 주세요.")
    if status != 200:
        return failure(f"프록시가 HTTP {status}를 반환했습니다. base_url과 API 키를 확인해 주세요.")
    try:
        payload = json.loads(body.decode("utf-8"))
        items = payload.get("data", []) if isinstance(payload, dict) else []
        models = [str(m.get("id")) for m in items if isinstance(m, dict) and m.get("id")][:1000]
    except (json.JSONDecodeError, UnicodeDecodeError, AttributeError):
        return failure("프록시 응답을 해석하지 못했습니다. 서버 상태를 확인해 주세요.")
    return {
        "ok": True,
        "base_url": base_url,
        "key_configured": True,
        "available_models": models,
    }
