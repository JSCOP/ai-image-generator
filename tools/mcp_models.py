"""Validated, discoverable MCP inputs. Kept separate from transport and execution."""
from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)


class ImageOptions(StrictModel):
    size: str = Field(default="1920x1080", pattern=r"^[1-9][0-9]*x[1-9][0-9]*$")
    quality: Literal["low", "medium", "high"] = "high"
    image_model: str = Field(default="grok-imagine-image-2.0", min_length=1)
    model: str | None = None
    action: Literal["auto", "generate", "edit"] = "auto"
    reference_images: list[str] = Field(default_factory=list, max_length=4)


class ImageJob(StrictModel):
    id: str = Field(min_length=1, max_length=80)
    prompt: str = Field(min_length=1, max_length=100000)
    size: str | None = None
    quality: Literal["low", "medium", "high"] | None = None
    image_model: str | None = None
    model: str | None = None
    action: Literal["auto", "generate", "edit"] | None = None
    reference_images: list[str] | None = Field(default=None, max_length=4)


class GenerationSpec(ImageOptions):
    mode: Literal["single", "jobs", "batch"] = "single"
    prompt: str = Field(default="", max_length=100000)
    positive: str = Field(default="", max_length=100000)
    negative: str = Field(default="", max_length=100000)
    topic: str | None = Field(default=None, min_length=1, max_length=80)
    topic_root: str | None = None
    count: int = Field(default=1, ge=1, le=1000)
    concurrency: int = Field(default=2, ge=1, le=8)
    job_timeout_sec: float = Field(default=600, gt=0, le=7200)
    preset: str | None = None
    jobs: list[ImageJob] = Field(default_factory=list, max_length=1000)
    prompt_prefix: str = Field(default="", max_length=100000)
    prompt_suffix: str = Field(default="", max_length=100000)
    seed: int = 0

    @model_validator(mode="after")
    def check_mode(self):
        if self.mode == "single" and not (self.prompt.strip() or self.positive.strip()):
            raise ValueError("single requires prompt or positive")
        if self.mode == "jobs" and not self.jobs:
            raise ValueError("jobs requires a non-empty jobs array")
        if self.mode == "batch" and not self.preset:
            raise ValueError("batch requires preset")
        if self.mode != "jobs" and self.jobs:
            raise ValueError("jobs is only accepted in jobs mode")
        if self.mode != "batch" and self.preset:
            raise ValueError("preset is only accepted in batch mode")
        if self.mode == "jobs" and "count" in self.model_fields_set:
            raise ValueError("jobs mode uses the jobs array length; omit count")
        return self


class PresetCategory(StrictModel):
    name: str = Field(min_length=1, max_length=80)
    weight: int = Field(default=1, ge=1, le=100000)
    templates: list[str] = Field(min_length=1, max_length=1000)


class PresetSpec(ImageOptions):
    topic: str = Field(min_length=1, max_length=80)
    global_style: str = ""
    negative: str = ""
    categories: list[PresetCategory] = Field(min_length=1, max_length=100)


class CropRegion(StrictModel):
    name: str = Field(min_length=1, max_length=80)
    x: int = Field(ge=0)
    y: int = Field(ge=0)
    width: int = Field(gt=0)
    height: int = Field(gt=0)
