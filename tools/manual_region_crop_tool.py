#!/usr/bin/env python3
"""Browser tool for drawing multiple named crop regions on one or more images."""

from __future__ import annotations

import argparse
import json
import mimetypes
import posixpath
import sys
import threading
import urllib.parse
import webbrowser
from dataclasses import dataclass
from datetime import datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw, ImageFont


IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".tif", ".tiff"}


HTML = r"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Manual Region Crop Tool</title>
  <style>
    :root { color-scheme: dark; --bg:#151515; --panel:#202020; --line:#3a3a3a; --text:#eee; --muted:#aaa; --accent:#ff4f4f; --blue:#4aa3ff; }
    * { box-sizing: border-box; }
    body { margin:0; font-family: ui-sans-serif, system-ui, "Segoe UI", sans-serif; background:var(--bg); color:var(--text); overflow:hidden; }
    .app { display:grid; grid-template-columns:260px 1fr 280px; height:100vh; min-width:980px; }
    aside,.right { background:var(--panel); overflow:auto; border-right:1px solid var(--line); }
    .right { border-right:0; border-left:1px solid var(--line); }
    .header { padding:12px; border-bottom:1px solid var(--line); background:#181818; position:sticky; top:0; z-index:5; }
    h1 { margin:0 0 6px; font-size:15px; line-height:1.2; }
    h2 { margin:0 0 10px; font-size:13px; }
    .path,.muted { color:var(--muted); font-size:11px; word-break:break-all; }
    .list { padding:8px; }
    button,input { font:inherit; color:var(--text); background:#2a2a2a; border:1px solid #464646; border-radius:6px; }
    button { padding:7px 9px; cursor:pointer; }
    button:hover { background:#353535; }
    button.active { border-color:var(--blue); background:#26313d; }
    button.primary { border-color:#c13c3c; background:#873232; }
    .item { width:100%; display:grid; grid-template-columns:1fr auto; gap:8px; margin:0 0 6px; text-align:left; }
    .name { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
    .badge { color:#111; background:#9bd38f; border-radius:99px; padding:1px 7px; font-size:11px; align-self:center; }
    main { display:grid; grid-template-rows:auto 1fr auto; min-width:0; height:100vh; background:#111; }
    .toolbar { display:flex; gap:8px; padding:8px; border-bottom:1px solid var(--line); background:#181818; align-items:center; }
    .viewer { min-height:0; overflow:auto; display:grid; place-items:center; padding:24px; }
    .stage { position:relative; background:white; box-shadow:0 12px 35px rgba(0,0,0,.45); user-select:none; }
    .stage img { display:block; max-width:calc(100vw - 610px); max-height:calc(100vh - 140px); width:auto; height:auto; }
    .box { position:absolute; border:2px solid var(--accent); background:rgba(255,79,79,.08); outline:9999px solid rgba(0,0,0,.2); display:none; cursor:move; }
    .box.visible { display:block; }
    .box-label { position:absolute; left:0; top:-24px; background:#b53737; color:white; padding:2px 6px; border-radius:4px; font-weight:700; font-size:13px; }
    .handle { position:absolute; width:12px; height:12px; background:white; border:2px solid var(--accent); border-radius:50%; }
    .nw { left:-7px; top:-7px; cursor:nwse-resize; } .ne { right:-7px; top:-7px; cursor:nesw-resize; }
    .sw { left:-7px; bottom:-7px; cursor:nesw-resize; } .se { right:-7px; bottom:-7px; cursor:nwse-resize; }
    .status { padding:8px 10px; border-top:1px solid var(--line); color:var(--muted); font-size:12px; background:#181818; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
    .panel { padding:12px; border-bottom:1px solid var(--line); }
    .regions { display:grid; grid-template-columns:repeat(3, 1fr); gap:6px; }
    .regions button { position:relative; font-weight:650; }
    .regions button.done::after { content:""; position:absolute; right:6px; top:6px; width:7px; height:7px; border-radius:50%; background:#9bd38f; }
    .grid { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
    label { display:block; color:var(--muted); font-size:11px; margin-bottom:4px; }
    input { width:100%; padding:7px 8px; }
    .row { display:flex; gap:8px; margin-top:8px; } .row button { flex:1; }
    .help { color:var(--muted); font-size:12px; line-height:1.45; margin:0; white-space:pre-line; }
    code { background:#181818; padding:1px 4px; border-radius:4px; color:#d9e8ff; }
  </style>
</head>
<body>
<div class="app">
  <aside>
    <div class="header"><h1>Region Crop Tool</h1><div class="path" id="sourcePath"></div></div>
    <div class="list" id="imageList"></div>
  </aside>
  <main>
    <div class="toolbar">
      <button id="prevBtn">Prev</button><button id="nextBtn">Next</button>
      <button id="clearBtn">Clear Region</button>
      <button class="primary" id="saveBtn">Save JSON + Crops</button>
    </div>
    <div class="viewer"><div class="stage" id="stage">
      <img id="mainImage" draggable="false" alt="" />
      <div class="box" id="box"><div class="box-label" id="boxLabel"></div><div class="handle nw" data-handle="nw"></div><div class="handle ne" data-handle="ne"></div><div class="handle sw" data-handle="sw"></div><div class="handle se" data-handle="se"></div></div>
    </div></div>
    <div class="status" id="status">Loading...</div>
  </main>
  <section class="right">
    <div class="header"><h1 id="currentName">No image</h1><div class="path" id="currentMeta"></div></div>
    <div class="panel"><h2>Regions</h2><div class="regions" id="regionList"></div></div>
    <div class="panel"><h2>Active Box</h2>
      <div class="grid">
        <div><label>x</label><input id="xInput" type="number" min="0"></div><div><label>y</label><input id="yInput" type="number" min="0"></div>
        <div><label>width</label><input id="wInput" type="number" min="1"></div><div><label>height</label><input id="hInput" type="number" min="1"></div>
      </div>
      <div class="row"><button id="applyBtn">Apply</button><button id="copyBtn">Copy</button></div>
    </div>
    <div class="panel"><h2>Workflow</h2><p class="help">1. Select a region name, e.g. A or B.
2. Drag a box on the image.
3. Select the next region and repeat.
4. Click Save JSON + Crops.

Arrow keys move the active box by 1px.
Shift + arrows move by 10px.
Ctrl+S saves.</p></div>
    <div class="panel"><h2>Output</h2><p class="help" id="outputInfo"></p></div>
  </section>
</div>
<script>
const state = { images:[], regions:[], boxes:{}, index:0, region:null, currentBox:null, drag:null, scaleX:1, scaleY:1 };
const $ = id => document.getElementById(id);
const els = { imageList:$('imageList'), regionList:$('regionList'), sourcePath:$('sourcePath'), currentName:$('currentName'), currentMeta:$('currentMeta'), outputInfo:$('outputInfo'), stage:$('stage'), img:$('mainImage'), box:$('box'), boxLabel:$('boxLabel'), status:$('status'), x:$('xInput'), y:$('yInput'), w:$('wInput'), h:$('hInput') };
function clamp(n,min,max){ return Math.max(min, Math.min(max,n)); }
function img(){ return state.images[state.index] || null; }
function boxesForImage(){ const im=img(); if(!im) return {}; if(!state.boxes[im.id]) state.boxes[im.id]={}; return state.boxes[im.id]; }
function setStatus(t){ els.status.textContent=t; }
async function api(path, options={}){ const r=await fetch(path,{headers:{'Content-Type':'application/json'},...options}); const text=await r.text(); let d=text?JSON.parse(text):{}; if(!r.ok || d.ok===false) throw new Error(d.error||r.statusText); return d; }
function normalize(b, im){ if(!b||!im) return null; let x=Math.round(+b.x), y=Math.round(+b.y), w=Math.round(+b.w), h=Math.round(+b.h); if(!Number.isFinite(x+y+w+h)||w<=0||h<=0) return null; x=clamp(x,0,im.width-1); y=clamp(y,0,im.height-1); w=clamp(w,1,im.width-x); h=clamp(h,1,im.height-y); return {x,y,w,h}; }
function renderImages(){ els.imageList.innerHTML=''; state.images.forEach((im,i)=>{ const count=Object.keys(state.boxes[im.id]||{}).length; const b=document.createElement('button'); b.className='item'+(i===state.index?' active':''); b.innerHTML=`<span class="name">${im.name}</span>${count?`<span class="badge">${count}</span>`:''}`; b.onclick=()=>loadImage(i); els.imageList.appendChild(b); }); }
function renderRegions(){ const boxes=boxesForImage(); els.regionList.innerHTML=''; state.regions.forEach(r=>{ const b=document.createElement('button'); b.textContent=r; b.className=(r===state.region?'active ':'')+(boxes[r]?'done':''); b.onclick=()=>selectRegion(r); els.regionList.appendChild(b); }); }
function updateScale(){ const im=img(); if(!im||!els.img.clientWidth||!els.img.clientHeight) return; state.scaleX=els.img.clientWidth/im.width; state.scaleY=els.img.clientHeight/im.height; }
function syncInputs(){ const b=state.currentBox; els.x.value=b?b.x:''; els.y.value=b?b.y:''; els.w.value=b?b.w:''; els.h.value=b?b.h:''; }
function renderBox(){ updateScale(); const b=state.currentBox; if(!b){ els.box.classList.remove('visible'); syncInputs(); return; } els.box.classList.add('visible'); els.box.style.left=`${b.x*state.scaleX}px`; els.box.style.top=`${b.y*state.scaleY}px`; els.box.style.width=`${b.w*state.scaleX}px`; els.box.style.height=`${b.h*state.scaleY}px`; els.boxLabel.textContent=state.region; syncInputs(); }
function store(){ if(!img()||!state.region) return; const boxes=boxesForImage(); if(state.currentBox) boxes[state.region]={...state.currentBox}; else delete boxes[state.region]; renderImages(); renderRegions(); }
function selectRegion(r){ store(); state.region=r; const im=img(); state.currentBox=normalize(boxesForImage()[r], im); renderRegions(); renderBox(); setStatus(`Active region ${r}`); }
function loadImage(i){ store(); state.index=clamp(i,0,state.images.length-1); const im=img(); els.currentName.textContent=im.name; els.currentMeta.textContent=`${im.width} x ${im.height}`; els.img.src=`/api/image?index=${state.index}&v=${Date.now()}`; if(!state.region) state.region=state.regions[0]; state.currentBox=normalize(boxesForImage()[state.region], im); renderImages(); renderRegions(); renderBox(); setStatus(`Loaded ${im.name}`); }
function stagePoint(e){ const r=els.img.getBoundingClientRect(); return {x:Math.round(clamp(e.clientX-r.left,0,r.width)/state.scaleX), y:Math.round(clamp(e.clientY-r.top,0,r.height)/state.scaleY)}; }
els.stage.addEventListener('pointerdown', e=>{ const im=img(); if(!im||e.button!==0||!state.region) return; updateScale(); const p=stagePoint(e); const handle=e.target.dataset&&e.target.dataset.handle; if(handle&&state.currentBox) state.drag={mode:'resize',handle,start:p,startBox:{...state.currentBox}}; else if(e.target===els.box&&state.currentBox) state.drag={mode:'move',start:p,startBox:{...state.currentBox}}; else { state.currentBox={x:p.x,y:p.y,w:1,h:1}; state.drag={mode:'draw',start:p,startBox:{...state.currentBox}}; } els.stage.setPointerCapture(e.pointerId); renderBox(); e.preventDefault(); });
els.stage.addEventListener('pointermove', e=>{ const im=img(), d=state.drag; if(!im||!d) return; const p=stagePoint(e); if(d.mode==='draw'){ const x1=clamp(Math.min(d.start.x,p.x),0,im.width-1), y1=clamp(Math.min(d.start.y,p.y),0,im.height-1), x2=clamp(Math.max(d.start.x,p.x),1,im.width), y2=clamp(Math.max(d.start.y,p.y),1,im.height); state.currentBox=normalize({x:x1,y:y1,w:x2-x1,h:y2-y1},im); } else if(d.mode==='move'){ state.currentBox=normalize({x:d.startBox.x+p.x-d.start.x,y:d.startBox.y+p.y-d.start.y,w:d.startBox.w,h:d.startBox.h},im); } else { let l=d.startBox.x,t=d.startBox.y,r=d.startBox.x+d.startBox.w,b=d.startBox.y+d.startBox.h; if(d.handle.includes('w')) l=p.x; if(d.handle.includes('e')) r=p.x; if(d.handle.includes('n')) t=p.y; if(d.handle.includes('s')) b=p.y; const x1=clamp(Math.min(l,r),0,im.width-1), y1=clamp(Math.min(t,b),0,im.height-1), x2=clamp(Math.max(l,r),1,im.width), y2=clamp(Math.max(t,b),1,im.height); state.currentBox=normalize({x:x1,y:y1,w:x2-x1,h:y2-y1},im); } store(); renderBox(); });
window.addEventListener('pointerup',()=>{ if(state.drag){ state.drag=null; store(); }});
window.addEventListener('resize',renderBox); els.img.addEventListener('load',renderBox);
$('prevBtn').onclick=()=>loadImage(state.index-1); $('nextBtn').onclick=()=>loadImage(state.index+1); $('clearBtn').onclick=()=>{ state.currentBox=null; store(); renderBox(); };
$('applyBtn').onclick=()=>{ state.currentBox=normalize({x:els.x.value,y:els.y.value,w:els.w.value,h:els.h.value},img()); store(); renderBox(); };
$('copyBtn').onclick=async()=>{ if(state.currentBox) await navigator.clipboard.writeText(JSON.stringify({image:img().id, region:state.region, box:state.currentBox},null,2)); setStatus('Copied active region box.'); };
async function save(){ store(); setStatus('Saving region JSON and crop PNGs...'); const r=await api('/api/save',{method:'POST',body:JSON.stringify({boxes:state.boxes})}); setStatus(`Saved ${r.saved_count} crop(s). ${r.boxes_path}`); renderImages(); renderRegions(); }
$('saveBtn').onclick=()=>save().catch(e=>setStatus(`Error: ${e.message}`));
window.addEventListener('keydown',e=>{ if(e.target&&e.target.tagName==='INPUT') return; const step=e.shiftKey?10:1; const b=state.currentBox, im=img(); if(!b||!im) { if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'){ e.preventDefault(); save().catch(err=>setStatus(`Error: ${err.message}`)); } return; } if(e.key==='ArrowLeft'){ b.x=clamp(b.x-step,0,im.width-b.w); store(); renderBox(); e.preventDefault(); } if(e.key==='ArrowRight'){ b.x=clamp(b.x+step,0,im.width-b.w); store(); renderBox(); e.preventDefault(); } if(e.key==='ArrowUp'){ b.y=clamp(b.y-step,0,im.height-b.h); store(); renderBox(); e.preventDefault(); } if(e.key==='ArrowDown'){ b.y=clamp(b.y+step,0,im.height-b.h); store(); renderBox(); e.preventDefault(); } if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'){ e.preventDefault(); save().catch(err=>setStatus(`Error: ${err.message}`)); } });
async function init(){ const s=await api('/api/state'); state.images=s.images; state.regions=s.regions; state.boxes=s.boxes_by_image||{}; els.sourcePath.textContent=s.config.images; els.outputInfo.textContent=`JSON: ${s.config.boxes_path}\nCrops: ${s.config.output_dir}`; if(!state.images.length){ setStatus('No images found.'); return; } loadImage(0); }
init().catch(e=>setStatus(`Error: ${e.message}`));
</script>
</body>
</html>
"""


@dataclass(frozen=True)
class ImageEntry:
    id: str
    name: str
    path: Path
    width: int
    height: int


def now_iso() -> str:
    return datetime.now().isoformat(timespec="seconds")


def safe_slug(text: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "-_." else "-" for ch in text).strip("-") or "image"


def normalize_box(raw: Any, width: int, height: int) -> dict[str, int] | None:
    if not isinstance(raw, dict):
        return None
    try:
        x, y, w, h = round(float(raw["x"])), round(float(raw["y"])), round(float(raw["w"])), round(float(raw["h"]))
    except (KeyError, TypeError, ValueError):
        return None
    if w <= 0 or h <= 0:
        return None
    x = max(0, min(x, width - 1))
    y = max(0, min(y, height - 1))
    w = max(1, min(w, width - x))
    h = max(1, min(h, height - y))
    return {"x": x, "y": y, "w": w, "h": h}


def load_images(path: Path, patterns: list[str], recursive: bool) -> list[ImageEntry]:
    path = path.resolve()
    if path.is_file():
        candidates = [path]
        root = path.parent
    else:
        glob = "**/*" if recursive else "*"
        candidates = [p for p in path.glob(glob) if p.is_file()]
        root = path
    result: list[ImageEntry] = []
    for item in sorted(candidates, key=lambda p: str(p).lower()):
        if item.suffix.lower() not in IMAGE_EXTS:
            continue
        if patterns and not any(item.match(pattern) for pattern in patterns):
            continue
        try:
            with Image.open(item) as im:
                width, height = im.size
        except Exception:
            continue
        name = item.relative_to(root).as_posix() if item.is_relative_to(root) else item.name
        result.append(ImageEntry(name, name, item, width, height))
    return result


class RegionCropTool:
    def __init__(
        self,
        images: Path,
        output_dir: Path,
        boxes_path: Path,
        regions: list[str],
        patterns: list[str],
        recursive: bool,
        suffix: str,
        upscale: int,
    ) -> None:
        self.images_arg = images.resolve()
        self.output_dir = output_dir.resolve()
        self.boxes_path = boxes_path.resolve()
        self.regions = regions
        self.patterns = patterns
        self.recursive = recursive
        self.suffix = suffix
        self.upscale = max(1, upscale)
        self.output_dir.mkdir(parents=True, exist_ok=True)
        self.boxes_path.parent.mkdir(parents=True, exist_ok=True)
        self.images = load_images(self.images_arg, self.patterns, self.recursive)
        self.boxes_by_image = self.load_boxes()

    def load_boxes(self) -> dict[str, dict[str, dict[str, int]]]:
        if not self.boxes_path.exists():
            return {}
        try:
            data = json.loads(self.boxes_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return {}
        raw = data.get("boxes_by_image", data.get("boxes", {})) if isinstance(data, dict) else {}
        if not isinstance(raw, dict):
            return {}
        image_by_id = {entry.id: entry for entry in self.images}
        cleaned: dict[str, dict[str, dict[str, int]]] = {}
        for image_id, regions in raw.items():
            entry = image_by_id.get(str(image_id))
            if not entry or not isinstance(regions, dict):
                continue
            for region, box in regions.items():
                if region not in self.regions:
                    continue
                clean = normalize_box(box, entry.width, entry.height)
                if clean:
                    cleaned.setdefault(entry.id, {})[region] = clean
        return cleaned

    def state(self) -> dict[str, Any]:
        return {
            "ok": True,
            "config": {
                "images": str(self.images_arg),
                "output_dir": str(self.output_dir),
                "boxes_path": str(self.boxes_path),
                "regions": self.regions,
                "patterns": self.patterns,
            },
            "regions": self.regions,
            "images": [
                {"id": i.id, "name": i.name, "path": str(i.path), "width": i.width, "height": i.height}
                for i in self.images
            ],
            "boxes_by_image": self.boxes_by_image,
        }

    def save(self, boxes_by_image: dict[str, Any]) -> dict[str, Any]:
        image_by_id = {entry.id: entry for entry in self.images}
        cleaned: dict[str, dict[str, dict[str, int]]] = {}
        for image_id, region_map in boxes_by_image.items():
            entry = image_by_id.get(str(image_id))
            if not entry or not isinstance(region_map, dict):
                continue
            for region, box in region_map.items():
                if region not in self.regions:
                    continue
                clean = normalize_box(box, entry.width, entry.height)
                if clean:
                    cleaned.setdefault(entry.id, {})[region] = clean
        self.boxes_by_image = cleaned
        outputs = self.crop_all()
        payload = {
            "updated_at": now_iso(),
            "images": str(self.images_arg),
            "output_dir": str(self.output_dir),
            "regions": self.regions,
            "suffix": self.suffix,
            "upscale": self.upscale,
            "boxes_by_image": self.boxes_by_image,
            "outputs": outputs,
        }
        self.boxes_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        return {"ok": True, "saved_count": len(outputs), "boxes_path": str(self.boxes_path), "output_dir": str(self.output_dir), "outputs": outputs}

    def crop_all(self) -> list[dict[str, Any]]:
        outputs: list[dict[str, Any]] = []
        image_by_id = {entry.id: entry for entry in self.images}
        for image_id, region_map in self.boxes_by_image.items():
            entry = image_by_id.get(image_id)
            if not entry:
                continue
            with Image.open(entry.path) as im:
                for region in self.regions:
                    box = region_map.get(region)
                    if not box:
                        continue
                    crop = im.crop((box["x"], box["y"], box["x"] + box["w"], box["y"] + box["h"]))
                    stem = safe_slug(Path(entry.name).stem)
                    out_path = self.output_dir / f"{stem}-{safe_slug(region.lower())}{self.suffix}.png"
                    crop.save(out_path)
                    up_path = None
                    if self.upscale > 1:
                        up_dir = self.output_dir / f"upscaled-{self.upscale}x"
                        up_dir.mkdir(parents=True, exist_ok=True)
                        up = crop.resize((crop.width * self.upscale, crop.height * self.upscale), Image.Resampling.LANCZOS)
                        up_path = up_dir / f"{stem}-{safe_slug(region.lower())}{self.suffix}-{self.upscale}x.png"
                        up.save(up_path)
                    outputs.append({
                        "image": entry.id,
                        "region": region,
                        "source": str(entry.path),
                        "box": box,
                        "crop": str(out_path),
                        "crop_size": [crop.width, crop.height],
                        "upscaled": str(up_path) if up_path else None,
                    })
        if outputs:
            sheet = self.write_contact_sheet(outputs)
            for item in outputs:
                item["contact_sheet"] = sheet
        return outputs

    def write_contact_sheet(self, outputs: list[dict[str, Any]]) -> str:
        tile_w, tile_h, label_h = 300, 260, 28
        cols = min(4, max(1, len(outputs)))
        rows = (len(outputs) + cols - 1) // cols
        sheet = Image.new("RGB", (cols * tile_w, rows * (tile_h + label_h)), (24, 24, 24))
        draw = ImageDraw.Draw(sheet)
        try:
            font = ImageFont.truetype("arial.ttf", 14)
        except Exception:
            font = ImageFont.load_default()
        for idx, item in enumerate(outputs):
            c, r = idx % cols, idx // cols
            x, y = c * tile_w, r * (tile_h + label_h)
            draw.rectangle([x, y, x + tile_w, y + label_h], fill=(44, 44, 44))
            draw.text((x + 8, y + 6), f"{item['region']} - {item['image']}", fill=(235, 235, 235), font=font)
            with Image.open(item["crop"]) as im:
                thumb = im.convert("RGB").copy()
                thumb.thumbnail((tile_w, tile_h), Image.Resampling.LANCZOS)
                tile = Image.new("RGB", (tile_w, tile_h), (246, 246, 242))
                tile.paste(thumb, ((tile_w - thumb.width) // 2, (tile_h - thumb.height) // 2))
                sheet.paste(tile, (x, y + label_h))
        path = self.output_dir / "manual-region-crop-contact-sheet.png"
        sheet.save(path)
        return str(path)


class Handler(BaseHTTPRequestHandler):
    tool: RegionCropTool

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))

    def send_json(self, data: dict[str, Any], status: HTTPStatus = HTTPStatus.OK) -> None:
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        route = posixpath.normpath(parsed.path)
        query = urllib.parse.parse_qs(parsed.query)
        if route in ("", "/"):
            body = HTML.encode("utf-8")
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if route == "/api/state":
            self.send_json(self.tool.state())
            return
        if route == "/api/image":
            self.serve_image(query)
            return
        self.send_json({"ok": False, "error": "not found"}, HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:  # noqa: N802
        if posixpath.normpath(urllib.parse.urlparse(self.path).path) != "/api/save":
            self.send_json({"ok": False, "error": "not found"}, HTTPStatus.NOT_FOUND)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            data = json.loads(self.rfile.read(length).decode("utf-8")) if length else {}
            result = self.tool.save(data.get("boxes", {}))
        except Exception as exc:
            self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
            return
        self.send_json(result)

    def serve_image(self, query: dict[str, list[str]]) -> None:
        try:
            entry = self.tool.images[int(query.get("index", ["0"])[0])]
        except (ValueError, IndexError):
            self.send_json({"ok": False, "error": "invalid image index"}, HTTPStatus.BAD_REQUEST)
            return
        data = entry.path.read_bytes()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", mimetypes.guess_type(entry.path.name)[0] or "application/octet-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def parse_regions(raw: str) -> list[str]:
    regions = [item.strip().upper() for item in raw.replace(";", ",").split(",") if item.strip()]
    return regions or ["A", "B", "C", "D", "E", "F", "G", "H", "J", "K", "L", "M", "N"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Draw multiple named crop boxes on image files.")
    parser.add_argument("--images", required=True, help="Image file or image directory.")
    parser.add_argument("--output", required=True, help="Output directory for region crops.")
    parser.add_argument("--boxes", required=True, help="Path to region crop JSON.")
    parser.add_argument("--regions", default="A,B,C,D,E,F,G,H,J,K,L,M,N", help="Comma-separated region names.")
    parser.add_argument("--pattern", action="append", default=[], help="Optional image filename glob. May be repeated.")
    parser.add_argument("--recursive", action="store_true", help="Scan image directory recursively.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8766)
    parser.add_argument("--open", action="store_true")
    parser.add_argument("--suffix", default="-manual-region-crop")
    parser.add_argument("--upscale", type=int, default=2)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    tool = RegionCropTool(
        images=Path(args.images),
        output_dir=Path(args.output),
        boxes_path=Path(args.boxes),
        regions=parse_regions(args.regions),
        patterns=args.pattern,
        recursive=args.recursive,
        suffix=args.suffix,
        upscale=args.upscale,
    )
    Handler.tool = tool
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    url = f"http://{args.host}:{args.port}/"
    print(json.dumps({"ok": True, "url": url, "images": len(tool.images), "regions": tool.regions, "output_dir": str(tool.output_dir), "boxes_path": str(tool.boxes_path)}, ensure_ascii=False))
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
