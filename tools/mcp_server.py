"""MCP stdio server for the complete local AI image generator.

Run: python tools/mcp_server.py [--state-dir PATH] [--allow-root PATH ...]
The server never prints to stdout; stdout belongs exclusively to MCP JSON-RPC.
"""
from __future__ import annotations

import argparse
import base64
import json
import sys
from pathlib import Path
from typing import Annotated, Any

from mcp.server.fastmcp import FastMCP
from mcp.types import ImageContent
from pydantic import Field

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mcp_models import CropRegion, GenerationSpec, PresetSpec
from mcp_service import ImageService, ROOT


def make_server(service: ImageService) -> FastMCP:
    mcp = FastMCP("ai-image-generator", instructions=(
        "Local AI image generation through CLIProxyAPI. Use plan_generation before start_generation. "
        "Plans are deterministic and never call a provider. Poll get_job until terminal, then inspect outputs with image_info or image_preview. "
        "API credentials stay in environment variables and are never returned."
    ))

    @mcp.resource("image://models")
    def models() -> str:
        return json.dumps({"catalog": service.studio_catalog(), "environment": service.available_models()}, ensure_ascii=False)

    @mcp.resource("image://presets")
    def presets_resource() -> str:
        return json.dumps(service.list_presets(), ensure_ascii=False)

    @mcp.tool()
    def health() -> dict:
        """Return server root, configured provider endpoint, and credential presence without exposing the key."""
        return {"ok": True, "root": str(service.root), "state_dir": str(service.state),
                "base_url": service.base_url, "key_configured": bool(service.key)}

    @mcp.tool()
    def check_connection() -> dict:
        """Check CLIProxyAPI /models using the environment API key."""
        return service.check_connection()

    @mcp.tool()
    def list_models() -> dict:
        """Return the static image model catalog and the authenticated provider model IDs."""
        return {"catalog": service.studio_catalog(), "available_models": service.available_models()}

    @mcp.tool()
    def list_presets() -> list[dict]:
        """List repository JSON presets."""
        return service.list_presets()

    @mcp.tool()
    def get_preset(path: str) -> dict:
        """Read one preset JSON under an allowed root."""
        return service.get_preset(path)

    @mcp.tool()
    def save_preset(name: str, preset: PresetSpec) -> dict:
        """Create a new preset. Existing preset files are never overwritten."""
        return service.save_preset(name, preset)

    @mcp.tool()
    def plan_generation(spec: GenerationSpec) -> dict:
        """Validate and freeze a single, jobs, or preset batch request without making an API call."""
        return service.plan_generation(spec)

    @mcp.tool()
    def get_plan(plan_id: str) -> dict:
        """Read a frozen generation plan."""
        return service.get_plan(plan_id)

    @mcp.tool()
    def start_generation(plan_id: str, resume: bool = False) -> dict:
        """Start a frozen plan asynchronously; returns immediately with a durable job ID."""
        return service.start_generation(plan_id, resume=resume)

    @mcp.tool()
    def get_job(job_id: str) -> dict:
        """Read progress, bounded events, outputs, and the final generator result."""
        return service.get_job(job_id)

    @mcp.tool()
    def list_jobs(limit: int = 50) -> list[dict]:
        """List recent generation jobs."""
        return service.list_jobs(limit)

    @mcp.tool()
    def cancel_job(job_id: str) -> dict:
        """Cancel an active job and its child generator process tree."""
        return service.cancel_job(job_id)

    @mcp.tool()
    def resume_job(job_id: str) -> dict:
        """Resume a terminal or interrupted job, preserving successful output files."""
        return service.resume_job(job_id)

    @mcp.tool()
    def list_images(directory: str = "ImageGallery/output", offset: int = 0, limit: int = 50) -> dict:
        """List PNG/JPEG/WebP files under an allowed directory."""
        return service.list_images(directory, offset, limit)

    @mcp.tool()
    def image_info(path: str) -> dict:
        """Inspect image dimensions and byte size."""
        return service.image_info(path)

    @mcp.tool()
    def image_preview(path: str, max_side: int = 1536) -> ImageContent:
        """Return a rendered PNG preview for visual inspection."""
        return ImageContent(type="image", data=base64.b64encode(service.preview(path, max_side)).decode("ascii"), mimeType="image/png")

    @mcp.tool()
    def crop_image(path: str, regions: list[CropRegion], upscale: int = 1) -> dict:
        """Crop named regions deterministically, optionally upscaling each crop."""
        return service.crop_image(path, regions, upscale)

    @mcp.tool()
    def resize_image(path: str, width: int, height: int, fit: str = "contain") -> dict:
        """Create a new PNG with contain, cover, or stretch fitting; never overwrites input."""
        return service.resize_image(path, width, height, fit)

    @mcp.tool()
    def contact_sheet(paths: list[str], columns: int = 4, tile_size: int = 256) -> dict:
        """Create a numbered contact sheet from up to 100 images."""
        return service.contact_sheet(paths, columns, tile_size)

    @mcp.tool()
    def studio_history(limit: int = 50) -> list[dict]:
        """Read local Image Studio job history without secrets."""
        return service.studio_history(limit)

    return mcp


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state-dir", type=Path, help="durable state directory (one server per directory)")
    parser.add_argument("--allow-root", action="append", type=Path, default=[], help="additional file root; repeatable")
    args = parser.parse_args()
    try:
        service = ImageService(state_dir=args.state_dir, allowed_roots=args.allow_root)
    except Exception as exc:
        print(f"MCP server startup failed: {exc}", file=sys.stderr)
        return 2
    try:
        make_server(service).run(transport="stdio")
    finally:
        service.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
