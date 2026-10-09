#!/usr/bin/env python3
"""Agent-free local Image Studio backend (loopback only).

로컬 이미지 스튜디오 백엔드. 127.0.0.1 전용, 단일 활성 작업, 실생성 전용.

실행:
  python tools/image_studio.py [--port 8766] [--open]

엔드포인트:
  GET  /                        정적 index.html (tools/image_studio_web/)
  GET  /app.css, /app.js        정적 파일 (고정 경로만 서빙)
  GET  /api/health              {"app","ok","root","port","output_dir"} (공개, 비밀 없음)
  GET  /api/config              csrf_token, base_url, key_configured, models,
                                defaults, limits, output_dir
  POST /api/connection          {"base_url","api_key"?} -> 프록시 /models 확인
  POST /api/jobs                {"prompt","positive","negative","image_model",
                                "size","quality","references"} -> 202 job (단일 활성)
  GET  /api/jobs                최신순 job 목록
  GET  /api/jobs/<id>           job 1건
  GET  /files/<jobid>/<name>    결과 이미지 바이트 (job 기록에 있는 파일만)
  POST /api/shutdown            서버 종료 (활성 작업 중이면 409 거부)

프롬프트 합성 (deterministic, 에이전트 없음):
  base = prompt + positive 이어 붙임, negative는 "\nAvoid: ..." 추가
"""

from __future__ import annotations

import argparse
import base64
import io
import json
import mimetypes
import os
import posixpath
import re
import secrets
import shutil
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request
import uuid
import webbrowser
from datetime import datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
import gen_image

from PIL import Image

APP = "ai-image-studio"
HOST = "127.0.0.1"
DEFAULT_PORT = 8766
DEFAULT_BASE_URL = gen_image.default_base_url()
CONFIG_REL = Path("config") / "image-studio.local.json"

ROOT = Path(__file__).resolve().parent.parent
WEB_DIR = ROOT / "tools" / "image_studio_web"
OUTPUT_ROOT = ROOT / "ImageGallery" / "output"
METADATA_ROOT = ROOT / "ImageGallery" / "metadata"
CONFIG_PATH = ROOT / CONFIG_REL
AI_IMAGE = ROOT / "tools" / "ai_image.py"

# --- 카탈로그 (docs/SSOT.md 기준 6개 모델) ---
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
DEFAULTS = {"image_model": gen_image.DEFAULT_IMAGE_MODEL, "size": "1920x1080", "quality": "high"}
LIMITS = {"reference_count": 4, "reference_bytes": 10485760}  # 개수, 파일당 바이트

FINAL_SIZE_NOTE = (
    "최종 파일은 요청한 크기로 정확히 저장됩니다(제공자 원본과 다르면 크롭/리샘플). "
    "quality는 제공자별 해석이 다르며 최종 픽셀 크기를 바꾸지 않습니다."
)
NEGATIVE_NOTE = "negative는 프롬프트 끝에 'Avoid: ...' 한 줄로 그대로 추가됩니다. 가중치 문법은 해석되지 않을 수 있습니다."

MODELS: list[dict[str, Any]] = [
    {
        "id": "gpt-image-2.5",
        "label": "GPT Image 2.5 (최신)",
        "provider": "OpenAI",
        "reference_images": True,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "최신 GPT 이미지 모델. 참조가 있으면 자동으로 edit 호출. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "gpt-image-2.5-flare",
        "label": "GPT Image 2.5 Flare",
        "provider": "OpenAI",
        "reference_images": True,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "GPT Image 2.5 변형. 참조가 있으면 자동으로 edit 호출. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "gpt-image-2.5-sunburst",
        "label": "GPT Image 2.5 Sunburst",
        "provider": "OpenAI",
        "reference_images": True,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "GPT Image 2.5 변형. 참조가 있으면 자동으로 edit 호출. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "gpt-image-2",
        "label": "GPT Image 2",
        "provider": "OpenAI",
        "reference_images": True,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "이전 세대 모델. 참조가 있으면 자동으로 edit 호출. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "gpt-image-1.5",
        "label": "GPT Image 1.5",
        "provider": "OpenAI",
        "reference_images": True,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "참조 편집은 자동으로 edit 호출. 최근 실생성 미검증. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "gemini-3.1-flash-image",
        "label": "Gemini Flash Image",
        "provider": "Google Gemini",
        "reference_images": True,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": (
            "요청 size를 aspect/imageSize 버킷으로 변환해 요청하고 "
            "최종 파일은 요청 픽셀로 저장. quality 설정은 무시될 수 있음. " + FINAL_SIZE_NOTE
        ),
    },
    {
        "id": "grok-imagine-image-2.0",
        "label": "Grok Imagine 2.0 (기본값)",
        "provider": "xAI Grok",
        "reference_images": False,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "참조 이미지를 받으면 요청 전에 거부. 크기는 16px 단위로 패딩 요청. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "grok-imagine-image-quality",
        "label": "Grok Imagine Quality",
        "provider": "xAI Grok",
        "reference_images": False,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "참조 이미지를 받으면 요청 전에 거부. 크기는 16px 단위로 패딩 요청. " + FINAL_SIZE_NOTE,
    },
    {
        "id": "grok-imagine-image",
        "label": "Grok Imagine",
        "provider": "xAI Grok",
        "reference_images": False,
        "qualities": list(QUALITIES),
        "sizes": [{"value": v, "label": SIZE_LABELS[v]} for v in SIZES],
        "note": "참조 이미지를 받으면 요청 전에 거부. 크기는 16px 단위로 패딩 요청. " + FINAL_SIZE_NOTE,
    },
]
MODEL_IDS = {m["id"] for m in MODELS}
MODEL_SUPPORTS_REF = {m["id"]: bool(m["reference_images"]) for m in MODELS}
GPT_EDIT_MODELS = {m["id"] for m in MODELS if m["id"].startswith("gpt-image-")}

# --- 제한 ---
MAX_BODY = 64 * 1024 * 1024  # POST JSON 전체 상한
MAX_REF_DIM = 8192  # 참조 이미지 한 변 상한
MAX_REF_PIXELS = 50_000_000  # 참조 이미지 픽셀 상한
REF_MIME_RE = re.compile(r"^data:(image/(png|jpeg|jpg|webp));base64,(.*)$", re.DOTALL)
JOB_ID_RE = re.compile(r"[0-9a-f]{16,64}")
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp"}
FORMAT_TO_SUFFIX = {"PNG": ".png", "JPEG": ".jpg", "WEBP": ".webp"}
CONNECTION_TIMEOUT = 15.0

# --- 전역 상태 (LOCK 아래에서만 접근) ---
_LOCK = threading.Lock()
_JOBS: dict[str, dict[str, Any]] = {}
_STARTS: dict[str, float] = {}
_CSRF = secrets.token_urlsafe(32)
_BASE_URL = DEFAULT_BASE_URL
_SESSION_KEY: str | None = None
_PORT = DEFAULT_PORT
_STOPPING = False


def now_iso() -> str:
    return datetime.now().isoformat(timespec="seconds")


def job_timeout() -> float:
    try:
        return float(os.environ.get("CLIPROXY_TIMEOUT", "600") or 600)
    except (TypeError, ValueError):
        return 600.0


def _atomic_write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.tmp-{os.getpid()}-{threading.get_ident()}")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


# --- 설정: base_url만 디스크 저장, 키는 절대 저장 안 함 ---
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
        raw = CONFIG_PATH.read_text(encoding="utf-8-sig")
    except FileNotFoundError:
        raw = ""
    except OSError as exc:
        sys.stderr.write(f"image-studio: 로컬 설정을 읽지 못해 환경설정을 씁니다: {exc}\n")
        raw = ""
    if raw.strip():
        try:
            return normalize_base_url(json.loads(raw).get("base_url"))
        except (ValueError, AttributeError):
            sys.stderr.write("image-studio: 로컬 설정 형식이 깨져 환경설정을 씁니다.\n")
    base = os.environ.get("CLIPROXY_BASE_URL") or gen_image._windows_user_environment("CLIPROXY_BASE_URL")
    try:
        return normalize_base_url(gen_image.resolve_base_url(base))
    except ValueError:
        sys.stderr.write("image-studio: CLIPROXY_BASE_URL 형식이 잘못되어 기본값을 씁니다.\n")
        return DEFAULT_BASE_URL

def save_base_url(base_url: str) -> None:
    _atomic_write_text(
        CONFIG_PATH,
        json.dumps({"base_url": base_url}, ensure_ascii=False, indent=2) + "\n",
    )




def resolve_api_key() -> str | None:
    """세션 키 우선, 이후 scripts/gen_image.py와 같은 해결 순서."""
    with _LOCK:
        session = _SESSION_KEY
    if session:
        return session
    return gen_image.resolve_api_key(None)


# --- 프롬프트 합성 ---

def compose_prompt(prompt: str, positive: str, negative: str) -> str:
    base = "\n".join(p for p in (prompt.strip(), positive.strip()) if p)
    neg = negative.strip()
    if neg:
        return f"{base}\nAvoid: {neg}" if base else f"Avoid: {neg}"
    return base


# --- 참조 이미지 검증 (Pillow, PNG/JPEG/WebP만) ---

def decode_reference(index: int, item: Any) -> tuple[str, bytes]:
    if not isinstance(item, dict):
        raise ValueError(f"참조 이미지 #{index + 1} 형식이 올바르지 않습니다.")
    data_url = item.get("data_url")
    if not isinstance(data_url, str):
        raise ValueError(f"참조 이미지 #{index + 1}의 data_url이 없습니다.")
    match = REF_MIME_RE.match(data_url.strip())
    if not match:
        raise ValueError(f"참조 이미지 #{index + 1}은 PNG/JPEG/WebP data URL만 가능합니다.")
    try:
        raw = base64.b64decode(match.group(3), validate=True)
    except Exception:
        raise ValueError(f"참조 이미지 #{index + 1}을 base64로 풀 수 없습니다.")
    if len(raw) > LIMITS["reference_bytes"]:
        raise ValueError(f"참조 이미지 #{index + 1} 용량이 큽니다(파일당 최대 10MB).")
    try:
        with Image.open(io.BytesIO(raw)) as im:
            fmt = im.format
            width, height = im.size
            if fmt not in FORMAT_TO_SUFFIX:
                raise ValueError(f"참조 이미지 #{index + 1}은 PNG/JPEG/WebP 파일만 가능합니다.")
            if width < 1 or height < 1 or width > MAX_REF_DIM or height > MAX_REF_DIM:
                raise ValueError(f"참조 이미지 #{index + 1} 크기가 범위를 벗어났습니다(한 변 최대 8192px).")
            if width * height > MAX_REF_PIXELS:
                raise ValueError(f"참조 이미지 #{index + 1} 픽셀 수가 큽니다.")
            try:
                im.load()
            except Exception:
                raise ValueError(f"참조 이미지 #{index + 1} 파일을 읽을 수 없습니다(깨진 파일).")
    except ValueError:
        raise
    except Exception:
        raise ValueError(f"참조 이미지 #{index + 1}은 PNG/JPEG/WebP 파일만 가능합니다.")
    return FORMAT_TO_SUFFIX[fmt], raw


def sanitize_failure(returncode: int, stderr_tail: str, api_key: str | None = None) -> str:
    tail = (stderr_tail or "").strip()
    lower = tail.lower()
    if returncode == 124 or "timed out" in lower or "timeout" in lower:
        return "생성 시간이 초과되었습니다. 잠시 후 다시 시도해 주세요."
    if "connect" in lower or "refused" in lower or "failed to establish" in lower or "max retries" in lower:
        return "CLIProxyAPI에 연결할 수 없습니다. base_url과 서버 실행 상태를 확인해 주세요."
    match = re.search(r"HTTP\s+(\d{3})", tail)
    if match:
        return (
            f"CLIProxyAPI 오류(HTTP {match.group(1)})가 발생했습니다. "
            "프록시 서버 상태와 모델 지원을 확인해 주세요."
        )
    if "no image" in lower:
        return "프록시 응답에 이미지가 없습니다. 모델과 프롬프트를 확인하고 다시 시도해 주세요."
    # The CLI returns the provider's error as a failure reason. Keep it useful,
    # but never retain the credential used by this request or inline image data.
    try:
        payload = json.loads(tail)
        error = payload.get("error", payload) if isinstance(payload, dict) else payload
        if isinstance(error, dict):
            tail = str(error.get("message") or error.get("code") or "")
        elif isinstance(error, str):
            tail = error
    except (ValueError, TypeError):
        pass
    if api_key:
        tail = tail.replace(api_key, "[REDACTED]")
    tail = re.sub(r"(?i)Bearer\s+\S+", "Bearer [REDACTED]", tail)
    tail = re.sub(r"data:image/\S+", "[image data]", tail)
    tail = " ".join(tail.split())[:400]
    return f"이미지 생성 실패: {tail}" if tail else "이미지 생성에 실패했습니다. 프록시 서버 상태를 확인해 주세요."


# --- job 기록 (비밀 없음: 키/data URL 저장 금지) ---

def job_dir(job_id: str) -> Path:
    return OUTPUT_ROOT / job_id


def record_dir(job_id: str) -> Path:
    return METADATA_ROOT / job_id


def public_job(rec: dict[str, Any]) -> dict[str, Any]:
    with _LOCK:
        started = _STARTS.get(rec["id"])
    elapsed = rec.get("elapsed_sec")
    if rec["status"] in ("queued", "running") and started is not None:
        elapsed = round(time.time() - started, 2)
    out = {
        "id": rec["id"],
        "status": rec["status"],
        "created_at": rec["created_at"],
        "prompt": rec["prompt"],
        "positive": rec["positive"],
        "negative": rec["negative"],
        "image_model": rec["image_model"],
        "size": rec["size"],
        "quality": rec["quality"],
        "reference_count": rec["reference_count"],
        "elapsed_sec": elapsed,
        "images": rec.get("images", []),
        "progress": rec.get("progress", {"stage": rec["status"]}),
    }
    if rec.get("error"):
        out["error"] = rec["error"]
    return out


def save_job(rec: dict[str, Any]) -> None:
    payload = {k: v for k, v in rec.items() if not k.startswith("_")}
    _atomic_write_text(record_dir(rec["id"]) / "job.json", json.dumps(payload, ensure_ascii=False, indent=2))


def update_job(job_id: str, **fields: Any) -> None:
    with _LOCK:
        rec = _JOBS.get(job_id)
        if rec is None:
            return
        rec.update(fields)
        snapshot = dict(rec)
    save_job(snapshot)


def active_job_id() -> str | None:
    with _LOCK:
        for job_id, rec in _JOBS.items():
            if rec["status"] in ("queued", "running"):
                return job_id
    return None


def load_history() -> None:
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    loaded: list[dict[str, Any]] = []
    for path in sorted(METADATA_ROOT.glob("*/job.json")):
        if not JOB_ID_RE.fullmatch(path.parent.name):
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if not isinstance(data, dict) or data.get("id") != path.parent.name:
            continue
        if data.get("status") in ("queued", "running"):
            data["status"] = "failed"
            data["error"] = "서버 재시작으로 중단되었습니다. 다시 생성해 주세요."
            data["progress"] = {"stage": "failed"}
            try:
                _atomic_write_text(path, json.dumps(data, ensure_ascii=False, indent=2))
            except OSError:
                pass
        loaded.append(data)
    with _LOCK:
        for data in loaded:
            _JOBS[data["id"]] = data


# --- 생성 워커 (단일 활성, ai_image.py 서브프로세스) ---

def fail_job_memory(job_id: str, error: str, elapsed: float) -> None:
    with _LOCK:
        rec = _JOBS.get(job_id)
        if rec is not None:
            rec["status"] = "failed"
            rec["error"] = error
            rec["elapsed_sec"] = elapsed
            rec["progress"] = {"stage": "failed"}


def run_generation(job_id: str) -> None:
    with _LOCK:
        rec = _JOBS.get(job_id)
        if rec is None:
            return
        _STARTS[job_id] = time.time()
        rec["status"] = "running"
        rec["progress"] = {"stage": "running"}
        snapshot = dict(rec)
    try:
        save_job(snapshot)
    except OSError:
        fail_job_memory(job_id, "작업 기록을 저장하지 못했습니다. 디스크 상태를 확인해 주세요.", 0.0)
        with _LOCK:
            _STARTS.pop(job_id, None)
        return

    started = time.time()
    try:
        _execute(job_id, snapshot, started)
    except Exception:
        try:
            update_job(
                job_id,
                status="failed",
                error="내부 오류로 생성을 마치지 못했습니다. 다시 시도해 주세요.",
                elapsed_sec=round(time.time() - started, 2),
                progress={"stage": "failed"},
            )
        except OSError:
            fail_job_memory(job_id, "내부 오류로 생성을 마치지 못했습니다. 다시 시도해 주세요.",
                            round(time.time() - started, 2))
    finally:
        with _LOCK:
            _STARTS.pop(job_id, None)


def _execute(job_id: str, rec: dict[str, Any], started: float) -> None:
    directory = job_dir(job_id)
    refs_directory = record_dir(job_id) / "refs"
    refs = sorted(refs_directory.glob("ref-*")) if refs_directory.is_dir() else []
    ref_paths = [str(p.resolve()) for p in refs if p.is_file()]
    composed = compose_prompt(rec["prompt"], rec["positive"], rec["negative"])
    action = "edit" if (ref_paths and rec["image_model"] in GPT_EDIT_MODELS) else "generate"
    update_job(job_id, action=action, reference_images=ref_paths,
               composed_prompt=composed, output=str(directory / "image.png"))
    timeout_sec = job_timeout()
    spec = {
        "mode": "single",
        "prompt": composed,
        "topic": job_id,
        "size": rec["size"],
        "quality": rec["quality"],
        "image_model": rec["image_model"],
        "action": action,
        "reference_images": ref_paths,
        "topic_root": str(ROOT),
        "record_metadata": False,
        "job_timeout_sec": timeout_sec,
    }
    with _LOCK:
        base_url = _BASE_URL
    env = os.environ.copy()
    env["CLIPROXY_BASE_URL"] = base_url
    env["PYTHONUTF8"] = "1"
    key = resolve_api_key()
    if key:
        env["CLIPROXY_API_KEY"] = key
    else:
        env.pop("CLIPROXY_API_KEY", None)
    try:
        proc = subprocess.run(
            [sys.executable, str(AI_IMAGE)],
            input=json.dumps(spec, ensure_ascii=False),
            capture_output=True,
            text=True,
            encoding="utf-8",
            cwd=str(ROOT),
            env=env,
            timeout=timeout_sec + 120,
        )
    except subprocess.TimeoutExpired:
        update_job(
            job_id,
            status="failed",
            error="생성 시간이 초과되었습니다. 잠시 후 다시 시도해 주세요.",
            elapsed_sec=round(time.time() - started, 2),
            progress={"stage": "failed"},
        )
        return
    except Exception:
        update_job(
            job_id,
            status="failed",
            error="내부 오류로 생성을 시작하지 못했습니다. 다시 시도해 주세요.",
            elapsed_sec=round(time.time() - started, 2),
            progress={"stage": "failed"},
        )
        return

    outputs: list[str] = []
    try:
        result = json.loads((proc.stdout or "").strip().splitlines()[-1])
        if isinstance(result, dict):
            outputs = [str(p) for p in (result.get("outputs") or []) if isinstance(p, str)]
    except (IndexError, json.JSONDecodeError, AttributeError):
        result = {}
    elapsed = round(time.time() - started, 2)
    source = next((Path(p) for p in outputs if Path(p).is_file()), None)
    failure_reasons = [
        failure.get("reason", "") for failure in result.get("failures", [])
        if isinstance(failure, dict) and isinstance(failure.get("reason"), str)
    ]
    failure_text = "\n".join(failure_reasons) or result.get("error", "")
    if proc.returncode == 0 and result.get("ok") and source is not None:
        try:
            dest = directory / "image.png"
            if source.resolve() != dest.resolve():
                shutil.move(str(source), str(dest))
            with Image.open(dest) as im:
                width, height = im.size
            update_job(
                job_id,
                status="succeeded",
                elapsed_sec=elapsed,
                images=[{
                    "url": f"/files/{job_id}/image.png",
                    "name": "image.png",
                    "width": width,
                    "height": height,
                }],
                progress={"stage": "succeeded"},
                composed_prompt=composed,
            )
        except Exception:
            update_job(
                job_id,
                status="failed",
                error="결과 파일을 저장하지 못했습니다. 다시 시도해 주세요.",
                elapsed_sec=elapsed,
                progress={"stage": "failed"},
            )
    else:
        update_job(
            job_id,
            status="failed",
            error=sanitize_failure(proc.returncode, failure_text, key),
            elapsed_sec=elapsed,
            progress={"stage": "failed"},
        )


# --- 연결 확인 ---

class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """리다이렉트를 따르지 않음: Authorization 키가 다른 호스트로 넘어가지 않게."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: N802
        return None


def check_connection(base_url: str, api_key: str | None) -> dict[str, Any]:
    key = (api_key or "").strip() or resolve_api_key()
    if not key:
        return {
            "ok": False,
            "base_url": base_url,
            "key_configured": False,
            "available_models": [],
            "error": "API 키가 없습니다. api_key를 함께 보내거나 CLIPROXY_API_KEY 환경변수를 설정해 주세요.",
        }
    url = base_url.rstrip("/") + "/models"
    req = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {key}", "Accept": "application/json"},
        method="GET",
    )
    opener = urllib.request.build_opener(_NoRedirect)
    try:
        with opener.open(req, timeout=CONNECTION_TIMEOUT) as resp:
            status = resp.status
            body = resp.read(2 * 1024 * 1024)
    except urllib.error.HTTPError as exc:
        if exc.code in (401, 403):
            error = f"API 키가 거부되었습니다(HTTP {exc.code}). 키 값을 확인해 주세요."
        elif exc.code in (301, 302, 303, 307, 308):
            error = "프록시가 다른 주소로 리다이렉트했습니다. base_url이 정확한지 확인해 주세요."
        else:
            error = f"프록시가 HTTP {exc.code}를 반환했습니다. base_url과 API 키를 확인해 주세요."
        return {
            "ok": False,
            "base_url": base_url,
            "key_configured": True,
            "available_models": [],
            "error": error,
        }
    except (urllib.error.URLError, OSError, ValueError):
        return {
            "ok": False,
            "base_url": base_url,
            "key_configured": True,
            "available_models": [],
            "error": "CLIProxyAPI에 연결할 수 없습니다. base_url과 서버 실행 상태를 확인해 주세요.",
        }
    if status != 200:
        return {
            "ok": False,
            "base_url": base_url,
            "key_configured": True,
            "available_models": [],
            "error": f"프록시가 HTTP {status}를 반환했습니다. base_url과 API 키를 확인해 주세요.",
        }
    try:
        payload = json.loads(body.decode("utf-8"))
        items = payload.get("data", []) if isinstance(payload, dict) else []
        models = [str(m.get("id")) for m in items if isinstance(m, dict) and m.get("id")][:1000]
    except (json.JSONDecodeError, UnicodeDecodeError, AttributeError):
        return {
            "ok": False,
            "base_url": base_url,
            "key_configured": True,
            "available_models": [],
            "error": "프록시 응답을 해석하지 못했습니다. 서버 상태를 확인해 주세요.",
        }
    return {
        "ok": True,
        "base_url": base_url,
        "key_configured": True,
        "available_models": models,
    }


# --- 기존 인스턴스 재사용 판별 ---

def probe_instance(port: int) -> tuple[str, dict[str, Any] | None]:
    """'own' | 'other' | 'none' + health 페이로드."""
    url = f"http://127.0.0.1:{port}/api/health"
    try:
        with urllib.request.urlopen(url, timeout=2) as resp:
            payload = json.loads(resp.read(64 * 1024).decode("utf-8"))
    except Exception:
        return "none", None
    if (
        isinstance(payload, dict)
        and payload.get("app") == APP
        and isinstance(payload.get("root"), str)
        and Path(payload["root"]).resolve() == ROOT.resolve()
    ):
        return "own", payload
    return "other", payload if isinstance(payload, dict) else None


# --- HTTP 핸들러 ---

class Handler(BaseHTTPRequestHandler):
    server_version = "ImageStudio/1.0"

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))

    # -- 공통 응답 --
    def send_json(self, data: dict[str, Any], status: HTTPStatus = HTTPStatus.OK) -> None:
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def send_bytes(self, data: bytes, content_type: str) -> None:
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Frame-Options", "DENY")
        self.send_header("Content-Security-Policy",
                         "default-src 'self'; img-src 'self' data: blob:; "
                         "frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
        self.end_headers()
        self.wfile.write(data)

    # -- 보안 경계 --
    def client_is_loopback(self) -> bool:
        return self.client_address[0] in ("127.0.0.1", "::1", "::ffff:127.0.0.1")

    def host_ok(self) -> bool:
        raw = (self.headers.get("Host") or "").strip()
        if not raw:
            return False
        if raw.startswith("["):
            end = raw.find("]")
            if end < 0:
                return False
            host, rest = raw[1:end], raw[end + 1:]
            if rest and not rest.startswith(":"):
                return False
            port = rest[1:] if rest else ""
        elif raw.count(":") == 1:
            host, _, port = raw.partition(":")
        else:
            host, port = raw, ""
        if host.lower() not in ("127.0.0.1", "localhost"):
            return False
        if port and port != str(_PORT):
            return False
        return True

    def origin_ok(self) -> bool:
        for header in ("Origin", "Referer"):
            raw = (self.headers.get(header) or "").strip()
            if not raw:
                continue
            try:
                parts = urllib.parse.urlsplit(raw)
            except ValueError:
                return False
            if parts.scheme != "http":
                return False
            if (parts.hostname or "").lower() not in ("127.0.0.1", "localhost"):
                return False
            try:
                oport = parts.port
            except ValueError:
                return False
            if oport != _PORT:
                return False
        return True

    def csrf_ok(self) -> bool:
        token = (self.headers.get("X-Studio-Token") or "")
        return bool(token) and secrets.compare_digest(token, _CSRF)

    def guard_post(self) -> bool:
        if not self.client_is_loopback():
            self.send_json({"ok": False, "error": "로컬에서만 접근할 수 있습니다."}, HTTPStatus.FORBIDDEN)
            return False
        if not self.host_ok() or not self.origin_ok():
            self.send_json({"ok": False, "error": "잘못된 요청 경로입니다."}, HTTPStatus.FORBIDDEN)
            return False
        if not self.csrf_ok():
            self.send_json(
                {"ok": False, "error": "토큰이 올바르지 않습니다. 페이지를 새로고침해 주세요."},
                HTTPStatus.FORBIDDEN,
            )
            return False
        return True

    def read_json_body(self) -> dict[str, Any]:
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except (TypeError, ValueError):
            raise ValueError("요청 본문 길이가 올바르지 않습니다.")
        if length < 0 or length > MAX_BODY:
            raise ValueError("요청 본문이 너무 큽니다.")
        raw = self.rfile.read(length) if length else b""
        if not raw:
            return {}
        try:
            data = json.loads(raw.decode("utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError):
            raise ValueError("JSON 본문이 올바르지 않습니다.")
        if not isinstance(data, dict):
            raise ValueError("JSON 본문은 객체여야 합니다.")
        return data

    # -- 라우팅 --
    def do_GET(self) -> None:  # noqa: N802
        if not self.client_is_loopback():
            self.send_json({"ok": False, "error": "로컬에서만 접근할 수 있습니다."}, HTTPStatus.FORBIDDEN)
            return
        if not self.host_ok():
            self.send_json({"ok": False, "error": "잘못된 요청 경로입니다."}, HTTPStatus.FORBIDDEN)
            return
        parsed = urllib.parse.urlparse(self.path)
        route = posixpath.normpath(parsed.path or "/")
        if route == "/api/health":
            self.send_json({
                "app": APP,
                "ok": True,
                "root": str(ROOT.resolve()),
                "port": _PORT,
                "output_dir": str(OUTPUT_ROOT.resolve()),
            })
            return
        if route == "/api/config":
            with _LOCK:
                base_url = _BASE_URL
            self.send_json({
                "csrf_token": _CSRF,
                "base_url": base_url,
                "key_configured": bool(resolve_api_key()),
                "models": MODELS,
                "defaults": DEFAULTS,
                "limits": LIMITS,
                "output_dir": str(OUTPUT_ROOT.resolve()),
            })
            return
        if route == "/api/jobs":
            with _LOCK:
                recs = list(_JOBS.values())
            recs.sort(key=lambda r: r.get("created_at", ""), reverse=True)
            self.send_json({"jobs": [public_job(r) for r in recs]})
            return
        if route.startswith("/api/jobs/"):
            job_id = route[len("/api/jobs/"):]
            with _LOCK:
                rec = _JOBS.get(job_id)
            if job_id and "/" not in job_id and rec is not None:
                self.send_json(public_job(rec))
            else:
                self.send_json({"ok": False, "error": "작업을 찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        if route == "/files" or route.startswith("/files/"):
            self.serve_file(route)
            return
        if route == "/":
            self.serve_static("index.html", "text/html; charset=utf-8")
            return
        if route in ("/app.css", "/app.js"):
            content_type = (
                "text/css; charset=utf-8" if route.endswith(".css")
                else "application/javascript; charset=utf-8"
            )
            self.serve_static(route.lstrip("/"), content_type)
            return
        self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        route = posixpath.normpath(parsed.path or "/")
        if route == "/api/connection":
            if not self.guard_post():
                return
            try:
                data = self.read_json_body()
            except ValueError as exc:
                self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
                return
            self.handle_connection(data)
            return
        if route == "/api/jobs":
            if not self.guard_post():
                return
            try:
                data = self.read_json_body()
            except ValueError as exc:
                self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
                return
            self.handle_create_job(data)
            return
        if route == "/api/shutdown":
            if not self.guard_post():
                return
            try:
                self.read_json_body()
            except ValueError as exc:
                self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
                return
            global _STOPPING
            with _LOCK:
                busy = next((j for j, r in _JOBS.items() if r["status"] in ("queued", "running")), None)
                if busy is None and not _STOPPING:
                    _STOPPING = True
                    stopping_now = True
                else:
                    stopping_now = False
            if busy is not None:
                self.send_json(
                    {"ok": False,
                     "error": "생성 중인 작업이 끝난 뒤에 종료해 주세요.",
                     "active_job": busy},
                    HTTPStatus.CONFLICT,
                )
                return
            self.send_json({"ok": True})
            if stopping_now:
                threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)

    # -- 정적 파일 (고정 3개만) --
    def serve_static(self, name: str, content_type: str) -> None:
        try:
            data = (WEB_DIR / name).read_bytes()
        except OSError:
            self.send_json(
                {"ok": False, "error": "UI 파일이 아직 없습니다."},
                HTTPStatus.NOT_FOUND,
            )
            return
        self.send_bytes(data, content_type)

    # -- 결과 파일 (job 기록에 있는 이름만, 단일 세그먼트) --
    def serve_file(self, route: str) -> None:
        parts = route.split("/")
        # ["", "files", "<jobid>", "<name>"]
        if len(parts) != 4 or not parts[2] or not parts[3]:
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        _, _, job_id, name = parts
        if not JOB_ID_RE.fullmatch(job_id):
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        if "/" in name or "\\" in name or ".." in name:
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        if Path(name).suffix.lower() not in IMAGE_SUFFIXES:
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        with _LOCK:
            rec = _JOBS.get(job_id)
        allowed = {img.get("name") for img in (rec.get("images", []) if rec else [])}
        if name not in allowed:
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        path = (job_dir(job_id) / name).resolve()
        if job_dir(job_id).resolve() not in path.parents or not path.is_file():
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        try:
            data = path.read_bytes()
        except OSError:
            self.send_json({"ok": False, "error": "찾을 수 없습니다."}, HTTPStatus.NOT_FOUND)
            return
        self.send_bytes(data, mimetypes.guess_type(name)[0] or "application/octet-stream")

    # -- 연결 확인 --
    def handle_connection(self, data: dict[str, Any]) -> None:
        global _BASE_URL, _SESSION_KEY
        with _LOCK:
            current_base = _BASE_URL
        raw_base = data.get("base_url", "")
        try:
            base_url = normalize_base_url(raw_base) if str(raw_base or "").strip() else current_base
        except ValueError as exc:
            self.send_json({
                "ok": False,
                "base_url": current_base,
                "key_configured": bool(resolve_api_key()),
                "available_models": [],
                "error": str(exc),
            }, HTTPStatus.BAD_REQUEST)
            return
        api_key = data.get("api_key")
        if api_key is not None and not isinstance(api_key, str):
            self.send_json({
                "ok": False,
                "base_url": base_url,
                "key_configured": bool(resolve_api_key()),
                "available_models": [],
                "error": "api_key 형식이 올바르지 않습니다.",
            }, HTTPStatus.BAD_REQUEST)
            return
        provided = (api_key or "").strip() if isinstance(api_key, str) else ""
        result = check_connection(base_url, provided or None)
        if result.get("ok"):
            with _LOCK:
                _BASE_URL = base_url
                if provided:
                    _SESSION_KEY = provided
            try:
                save_base_url(base_url)
            except OSError:
                result = dict(result)
                result["error"] = "연결은 성공했지만 로컬 설정 저장에 실패했습니다(재시작하면 기본값으로 돌아갑니다)."
        self.send_json(result)

    # -- 작업 생성 --
    def handle_create_job(self, data: dict[str, Any]) -> None:
        with _LOCK:
            stopping = _STOPPING
        if stopping:
            self.send_json({"ok": False, "error": "서버가 종료 중입니다."},
                           HTTPStatus.SERVICE_UNAVAILABLE)
            return
        busy = active_job_id()
        if busy is not None:
            self.send_json(
                {"ok": False, "error": "이미 생성 중인 작업이 있습니다. 완료 후 다시 시도해 주세요.",
                 "active_job": busy},
                HTTPStatus.CONFLICT,
            )
            return
        prompt = data.get("prompt", "")
        positive = data.get("positive", "")
        negative = data.get("negative", "")
        if not all(isinstance(v, str) for v in (prompt, positive, negative)):
            self.send_json({"ok": False, "error": "prompt/positive/negative는 문자열이어야 합니다."},
                           HTTPStatus.BAD_REQUEST)
            return
        if not prompt.strip() and not positive.strip():
            self.send_json({"ok": False, "error": "prompt 또는 positive 중 하나는 필요합니다."},
                           HTTPStatus.BAD_REQUEST)
            return
        image_model = str(data.get("image_model") or DEFAULTS["image_model"])
        size = str(data.get("size") or DEFAULTS["size"])
        quality = str(data.get("quality") or DEFAULTS["quality"])
        if image_model not in MODEL_IDS:
            self.send_json({"ok": False, "error": f"지원하지 않는 모델입니다: {image_model}"},
                           HTTPStatus.BAD_REQUEST)
            return
        if size not in SIZES:
            self.send_json({"ok": False, "error": f"지원하지 않는 크기입니다: {size}"},
                           HTTPStatus.BAD_REQUEST)
            return
        if quality not in QUALITIES:
            self.send_json({"ok": False, "error": f"지원하지 않는 품질입니다: {quality}"},
                           HTTPStatus.BAD_REQUEST)
            return
        refs = data.get("references", [])
        if refs is None:
            refs = []
        if not isinstance(refs, list):
            self.send_json({"ok": False, "error": "references는 배열이어야 합니다."},
                           HTTPStatus.BAD_REQUEST)
            return
        if len(refs) > LIMITS["reference_count"]:
            self.send_json({"ok": False, "error": "참조 이미지는 최대 4개까지 가능합니다."},
                           HTTPStatus.BAD_REQUEST)
            return
        if refs and not MODEL_SUPPORTS_REF[image_model]:
            self.send_json(
                {"ok": False,
                 "error": "선택한 Grok 모델은 참조 이미지를 지원하지 않습니다. Gemini나 GPT 이미지 모델을 선택해 주세요."},
                HTTPStatus.BAD_REQUEST,
            )
            return
        try:
            decoded = [decode_reference(i, item) for i, item in enumerate(refs)]
        except ValueError as exc:
            self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
            return

        job_id = uuid.uuid4().hex
        directory = record_dir(job_id)
        try:
            refs_dir = directory / "refs"
            if decoded:
                refs_dir.mkdir(parents=True, exist_ok=True)
            for i, (suffix, raw) in enumerate(decoded):
                (refs_dir / f"ref-{i}{suffix}").write_bytes(raw)
        except OSError:
            shutil.rmtree(directory, ignore_errors=True)
            self.send_json({"ok": False, "error": "작업 폴더를 만들지 못했습니다. 다시 시도해 주세요."},
                           HTTPStatus.INTERNAL_SERVER_ERROR)
            return

        rec: dict[str, Any] = {
            "id": job_id,
            "status": "queued",
            "created_at": now_iso(),
            "prompt": prompt.strip(),
            "positive": positive.strip(),
            "negative": negative.strip(),
            "composed_prompt": compose_prompt(prompt, positive, negative),
            "image_model": image_model,
            "size": size,
            "quality": quality,
            "reference_count": len(decoded),
            "elapsed_sec": 0.0,
            "images": [],
            "progress": {"stage": "queued"},
        }
        with _LOCK:
            stopping = _STOPPING
            if stopping:
                busy = None
            elif any(r["status"] in ("queued", "running") for r in _JOBS.values()):
                busy = next(j for j, r in _JOBS.items() if r["status"] in ("queued", "running"))
            else:
                _JOBS[job_id] = rec
                busy = None
        if stopping:
            shutil.rmtree(directory, ignore_errors=True)
            self.send_json({"ok": False, "error": "서버가 종료 중입니다."},
                           HTTPStatus.SERVICE_UNAVAILABLE)
            return
        if busy is not None:
            shutil.rmtree(directory, ignore_errors=True)
            self.send_json(
                {"ok": False, "error": "이미 생성 중인 작업이 있습니다. 완료 후 다시 시도해 주세요.",
                 "active_job": busy},
                HTTPStatus.CONFLICT,
            )
            return
        try:
            save_job(rec)
        except OSError:
            with _LOCK:
                _JOBS.pop(job_id, None)
            shutil.rmtree(directory, ignore_errors=True)
            self.send_json({"ok": False, "error": "작업 기록을 저장하지 못했습니다. 디스크 상태를 확인해 주세요."},
                           HTTPStatus.INTERNAL_SERVER_ERROR)
            return
        worker = threading.Thread(target=run_generation, args=(job_id,), daemon=True)
        worker.start()
        self.send_json(public_job(rec), HTTPStatus.ACCEPTED)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="로컬 이미지 스튜디오 서버 (127.0.0.1 전용)")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="HTTP 포트 (기본 8766)")
    parser.add_argument("--open", action="store_true", help="브라우저로 열기")
    return parser.parse_args()


def main() -> int:
    global _BASE_URL, _PORT
    args = parse_args()
    if not 1 <= args.port <= 65535:
        print("포트가 올바르지 않습니다 (1-65535).", file=sys.stderr)
        return 2
    _PORT = args.port

    state, _ = probe_instance(args.port)
    url = f"http://127.0.0.1:{args.port}/"
    if state == "own":
        print(json.dumps({"ok": True, "url": url, "reused": True,
                          "root": str(ROOT.resolve())}, ensure_ascii=False))
        if args.open:
            webbrowser.open(url)
        return 0
    if state == "other":
        print(f"포트 {args.port}에 다른 서버가 이미 있습니다.", file=sys.stderr)
        return 2

    _BASE_URL = load_base_url()
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    load_history()

    try:
        server = ThreadingHTTPServer((HOST, args.port), Handler)
    except OSError:
        state, _ = probe_instance(args.port)
        if state == "own":
            print(json.dumps({"ok": True, "url": url, "reused": True,
                              "root": str(ROOT.resolve())}, ensure_ascii=False))
            if args.open:
                webbrowser.open(url)
            return 0
        print(f"포트 {args.port}에 다른 서버가 이미 있습니다.", file=sys.stderr)
        return 2
    server.daemon_threads = True
    print(json.dumps({"ok": True, "url": url, "reused": False,
                      "root": str(ROOT.resolve()),
                      "output_dir": str(OUTPUT_ROOT.resolve())}, ensure_ascii=False))
    if args.open:
        threading.Timer(0.5, lambda: webbrowser.open(url)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
