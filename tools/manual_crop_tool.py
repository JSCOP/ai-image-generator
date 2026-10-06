#!/usr/bin/env python3
"""Local browser tool for drawing exact manual crop boxes on images."""

from __future__ import annotations

import argparse
import base64
import io
import json
import mimetypes
import os
import posixpath
import sys
import threading
import time
import urllib.parse
import webbrowser
from dataclasses import dataclass
from datetime import datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from gallery import topic_dirs


IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".tif", ".tiff"}


INDEX_HTML = r"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Manual Crop Tool</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #151515;
      --panel: #202020;
      --panel-2: #2b2b2b;
      --text: #eeeeee;
      --muted: #a8a8a8;
      --accent: #ff4f4f;
      --accent-2: #4aa3ff;
      --line: #3a3a3a;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
      overflow: hidden;
    }
    .app {
      display: grid;
      grid-template-columns: 280px 1fr 310px;
      height: 100vh;
      min-width: 980px;
    }
    aside, .right {
      background: var(--panel);
      border-right: 1px solid var(--line);
      overflow: auto;
    }
    .right { border-right: 0; border-left: 1px solid var(--line); }
    .header {
      padding: 12px;
      border-bottom: 1px solid var(--line);
      background: #181818;
      position: sticky;
      top: 0;
      z-index: 3;
    }
    h1 {
      font-size: 15px;
      line-height: 1.2;
      margin: 0 0 6px;
      font-weight: 650;
    }
    .path {
      font-size: 11px;
      color: var(--muted);
      word-break: break-all;
    }
    .image-list {
      padding: 8px;
    }
    .item {
      display: grid;
      grid-template-columns: 1fr auto;
      gap: 8px;
      width: 100%;
      padding: 9px 10px;
      margin: 0 0 6px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #242424;
      color: var(--text);
      text-align: left;
      cursor: pointer;
    }
    .item:hover { background: #2d2d2d; }
    .item.active { border-color: var(--accent-2); background: #26313d; }
    .item .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .badge {
      font-size: 11px;
      color: #101010;
      background: #9bd38f;
      border-radius: 99px;
      padding: 1px 7px;
      align-self: center;
    }
    main {
      min-width: 0;
      display: grid;
      grid-template-rows: auto 1fr auto;
      height: 100vh;
      background: #111;
    }
    .toolbar {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px;
      border-bottom: 1px solid var(--line);
      background: #181818;
    }
    button, input {
      font: inherit;
      color: var(--text);
      background: #2a2a2a;
      border: 1px solid #464646;
      border-radius: 6px;
    }
    button {
      padding: 7px 10px;
      cursor: pointer;
    }
    button:hover { background: #353535; }
    button.primary {
      border-color: #c13c3c;
      background: #873232;
    }
    button.primary:hover { background: #9b3a3a; }
    button:disabled {
      opacity: .45;
      cursor: default;
    }
    .viewer-wrap {
      min-height: 0;
      overflow: auto;
      display: grid;
      place-items: center;
      padding: 24px;
    }
    .stage {
      position: relative;
      user-select: none;
      box-shadow: 0 12px 35px rgba(0,0,0,.4);
      background: #fff;
    }
    .stage img {
      display: block;
      max-width: calc(100vw - 640px);
      max-height: calc(100vh - 130px);
      width: auto;
      height: auto;
    }
    .crop-box {
      position: absolute;
      border: 2px solid var(--accent);
      background: rgba(255, 79, 79, .08);
      outline: 9999px solid rgba(0, 0, 0, .22);
      cursor: move;
      display: none;
    }
    .crop-box.visible { display: block; }
    .handle {
      position: absolute;
      width: 12px;
      height: 12px;
      background: #fff;
      border: 2px solid var(--accent);
      border-radius: 50%;
    }
    .handle.nw { left: -7px; top: -7px; cursor: nwse-resize; }
    .handle.ne { right: -7px; top: -7px; cursor: nesw-resize; }
    .handle.sw { left: -7px; bottom: -7px; cursor: nesw-resize; }
    .handle.se { right: -7px; bottom: -7px; cursor: nwse-resize; }
    .status {
      padding: 8px 10px;
      border-top: 1px solid var(--line);
      color: var(--muted);
      font-size: 12px;
      background: #181818;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .panel {
      padding: 12px;
      border-bottom: 1px solid var(--line);
    }
    .panel h2 {
      font-size: 13px;
      margin: 0 0 10px;
      color: #dedede;
      font-weight: 650;
    }
    .grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 8px;
    }
    label {
      display: block;
      color: var(--muted);
      font-size: 11px;
      margin-bottom: 4px;
    }
    input {
      width: 100%;
      padding: 7px 8px;
      font-size: 13px;
    }
    .row {
      display: flex;
      gap: 8px;
      margin-top: 8px;
    }
    .row button { flex: 1; }
    .help {
      color: var(--muted);
      font-size: 12px;
      line-height: 1.45;
      margin: 0;
    }
    code {
      color: #d9e8ff;
      background: #181818;
      padding: 1px 4px;
      border-radius: 4px;
    }
    .saved-list {
      display: grid;
      gap: 6px;
      margin-top: 10px;
    }
    .saved-item {
      padding: 8px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #252525;
      min-width: 0;
    }
    .saved-item a {
      display: block;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      color: #9dccff;
      text-decoration: none;
      font-size: 12px;
    }
    .saved-item a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="app">
    <aside>
      <div class="header">
        <h1>Manual Crop Tool</h1>
        <div class="path" id="sourcePath"></div>
      </div>
      <div class="image-list" id="imageList"></div>
    </aside>

    <main>
      <div class="toolbar">
        <button id="openImagesBtn">Open Images</button>
        <input id="fileInput" type="file" accept="image/*" multiple hidden />
        <button id="prevBtn">Prev</button>
        <button id="nextBtn">Next</button>
        <button id="fitBtn">Fit</button>
        <button id="clearBtn">Clear Box</button>
        <button class="primary" id="saveCurrentBtn">Save Crop Image</button>
        <button id="saveBtn">Save All Crops</button>
      </div>
      <div class="viewer-wrap" id="viewerWrap">
        <div class="stage" id="stage">
          <img id="mainImage" alt="" draggable="false" />
          <div class="crop-box" id="cropBox">
            <div class="handle nw" data-handle="nw"></div>
            <div class="handle ne" data-handle="ne"></div>
            <div class="handle sw" data-handle="sw"></div>
            <div class="handle se" data-handle="se"></div>
          </div>
        </div>
      </div>
      <div class="status" id="status">Loading...</div>
    </main>

    <section class="right">
      <div class="header">
        <h1 id="currentName">No image</h1>
        <div class="path" id="currentMeta"></div>
      </div>
      <div class="panel">
        <h2>Crop Box</h2>
        <div class="grid">
          <div><label for="xInput">x</label><input id="xInput" type="number" min="0" step="1"></div>
          <div><label for="yInput">y</label><input id="yInput" type="number" min="0" step="1"></div>
          <div><label for="wInput">width</label><input id="wInput" type="number" min="1" step="1"></div>
          <div><label for="hInput">height</label><input id="hInput" type="number" min="1" step="1"></div>
        </div>
        <div class="row">
          <button id="applyBtn">Apply Numbers</button>
          <button id="copyBtn">Copy Box</button>
        </div>
      </div>
      <div class="panel">
        <h2>How To Use</h2>
        <p class="help">
          Drag on the image to draw a crop box. Drag inside the box to move it.
          Drag corner handles to resize. Arrow keys move by 1px, <code>Shift</code> + arrows move by 10px.
          Click <code>Save Crop Image</code> to write the current PNG crop.
        </p>
      </div>
      <div class="panel">
        <h2>Output</h2>
        <p class="help" id="outputInfo"></p>
        <div class="saved-list" id="savedList"></div>
      </div>
    </section>
  </div>

  <script>
    const state = {
      images: [],
      boxes: {},
      index: 0,
      currentBox: null,
      drag: null,
      scaleX: 1,
      scaleY: 1,
      config: null
    };

    const els = {
      imageList: document.getElementById('imageList'),
      sourcePath: document.getElementById('sourcePath'),
      currentName: document.getElementById('currentName'),
      currentMeta: document.getElementById('currentMeta'),
      outputInfo: document.getElementById('outputInfo'),
      savedList: document.getElementById('savedList'),
      fileInput: document.getElementById('fileInput'),
      openImagesBtn: document.getElementById('openImagesBtn'),
      stage: document.getElementById('stage'),
      mainImage: document.getElementById('mainImage'),
      cropBox: document.getElementById('cropBox'),
      status: document.getElementById('status'),
      xInput: document.getElementById('xInput'),
      yInput: document.getElementById('yInput'),
      wInput: document.getElementById('wInput'),
      hInput: document.getElementById('hInput')
    };

    function clamp(n, min, max) {
      return Math.max(min, Math.min(max, n));
    }

    function currentImage() {
      return state.images[state.index] || null;
    }

    function setStatus(text) {
      els.status.textContent = text;
    }

    async function api(path, options = {}) {
      const headers = { ...(options.headers || {}) };
      if (!(options.body instanceof FormData) && !headers['Content-Type']) {
        headers['Content-Type'] = 'application/json';
      }
      const res = await fetch(path, {
        ...options,
        headers
      });
      const text = await res.text();
      let data = null;
      try { data = text ? JSON.parse(text) : null; }
      catch (_) { throw new Error(text || res.statusText); }
      if (!res.ok || (data && data.ok === false)) {
        throw new Error((data && data.error) || res.statusText);
      }
      return data;
    }

    function renderConfig(data) {
      const importLine = data.config.import_dir ? `\nImported: ${data.config.import_dir}` : '';
      els.sourcePath.textContent = data.config.images_dir;
      els.outputInfo.textContent = `JSON: ${data.config.boxes_path}\nCrops: ${data.config.output_dir}${importLine}`;
    }

    function readImageFile(file) {
      return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve({ name: file.name, data_url: reader.result });
        reader.onerror = () => reject(reader.error || new Error(`Could not read ${file.name}`));
        reader.readAsDataURL(file);
      });
    }

    async function importSelectedFiles(fileList) {
      const files = Array.from(fileList || []).filter(file => {
        return file.type.startsWith('image/') || /\.(png|jpe?g|webp|bmp|tiff?)$/i.test(file.name);
      });
      if (!files.length) {
        setStatus('No image files selected.');
        return;
      }
      storeCurrentBox();
      setStatus(`Opening ${files.length} image(s)...`);
      const payload = { files: await Promise.all(files.map(readImageFile)) };
      const data = await api('/api/import-images', {
        method: 'POST',
        body: JSON.stringify(payload)
      });
      state.images = data.images;
      state.boxes = data.boxes || {};
      state.config = data.config;
      renderConfig(data);
      const selected = Number.isInteger(data.selected_index)
        ? data.selected_index
        : Math.max(0, state.images.length - 1);
      loadImage(selected);
      setStatus(`Opened ${data.imported_count} image(s).`);
    }

    function renderList() {
      els.imageList.innerHTML = '';
      state.images.forEach((img, idx) => {
        const btn = document.createElement('button');
        btn.className = 'item' + (idx === state.index ? ' active' : '');
        btn.innerHTML = `<span class="name">${img.name}</span>${state.boxes[img.id] ? '<span class="badge">box</span>' : ''}`;
        btn.addEventListener('click', () => loadImage(idx));
        els.imageList.appendChild(btn);
      });
    }

    function updateScale() {
      const img = currentImage();
      if (!img || !els.mainImage.clientWidth || !els.mainImage.clientHeight) return;
      state.scaleX = els.mainImage.clientWidth / img.width;
      state.scaleY = els.mainImage.clientHeight / img.height;
    }

    function normalizeBox(box, img) {
      if (!box || !img) return null;
      let x = Math.round(Number(box.x));
      let y = Math.round(Number(box.y));
      let w = Math.round(Number(box.w));
      let h = Math.round(Number(box.h));
      if (!Number.isFinite(x + y + w + h) || w <= 0 || h <= 0) return null;
      x = clamp(x, 0, img.width - 1);
      y = clamp(y, 0, img.height - 1);
      w = clamp(w, 1, img.width - x);
      h = clamp(h, 1, img.height - y);
      return { x, y, w, h };
    }

    function syncInputs() {
      const b = state.currentBox;
      els.xInput.value = b ? b.x : '';
      els.yInput.value = b ? b.y : '';
      els.wInput.value = b ? b.w : '';
      els.hInput.value = b ? b.h : '';
    }

    function renderBox() {
      updateScale();
      const b = state.currentBox;
      if (!b) {
        els.cropBox.classList.remove('visible');
        syncInputs();
        return;
      }
      els.cropBox.classList.add('visible');
      els.cropBox.style.left = `${b.x * state.scaleX}px`;
      els.cropBox.style.top = `${b.y * state.scaleY}px`;
      els.cropBox.style.width = `${b.w * state.scaleX}px`;
      els.cropBox.style.height = `${b.h * state.scaleY}px`;
      syncInputs();
    }

    function storeCurrentBox() {
      const img = currentImage();
      if (!img) return;
      if (state.currentBox) state.boxes[img.id] = { ...state.currentBox };
      else delete state.boxes[img.id];
      renderList();
    }

    function stagePoint(evt) {
      const rect = els.mainImage.getBoundingClientRect();
      const sx = clamp(evt.clientX - rect.left, 0, rect.width);
      const sy = clamp(evt.clientY - rect.top, 0, rect.height);
      return {
        x: Math.round(sx / state.scaleX),
        y: Math.round(sy / state.scaleY)
      };
    }

    function loadImage(index) {
      storeCurrentBox();
      state.index = clamp(index, 0, state.images.length - 1);
      const img = currentImage();
      if (!img) return;
      els.currentName.textContent = img.name;
      els.currentMeta.textContent = `${img.width} x ${img.height}`;
      els.mainImage.src = `/api/image?index=${state.index}&v=${Date.now()}`;
      state.currentBox = normalizeBox(state.boxes[img.id], img);
      renderList();
      setStatus(`Loaded ${img.name}`);
    }

    function applyInputs() {
      const img = currentImage();
      if (!img) return;
      state.currentBox = normalizeBox({
        x: els.xInput.value,
        y: els.yInput.value,
        w: els.wInput.value,
        h: els.hInput.value
      }, img);
      storeCurrentBox();
      renderBox();
    }

    function fileUrl(path) {
      return `/api/output-file?path=${encodeURIComponent(path)}`;
    }

    function renderSavedOutputs(outputs) {
      const items = Array.isArray(outputs) ? outputs : [];
      els.savedList.innerHTML = '';
      items.slice(-8).reverse().forEach(item => {
        if (!item || !item.crop) return;
        const div = document.createElement('div');
        div.className = 'saved-item';
        const cropLink = document.createElement('a');
        cropLink.href = fileUrl(item.crop);
        cropLink.target = '_blank';
        cropLink.title = item.crop;
        cropLink.textContent = `Open crop: ${item.crop.split(/[\\/]/).pop()}`;
        div.appendChild(cropLink);
        if (item.upscaled) {
          const upscaledLink = document.createElement('a');
          upscaledLink.href = fileUrl(item.upscaled);
          upscaledLink.target = '_blank';
          upscaledLink.title = item.upscaled;
          upscaledLink.textContent = `Open 2x: ${item.upscaled.split(/[\\/]/).pop()}`;
          div.appendChild(upscaledLink);
        }
        els.savedList.appendChild(div);
      });
    }

    async function saveCurrent() {
      const img = currentImage();
      if (!img || !state.currentBox) {
        setStatus('Draw a crop box first.');
        return;
      }
      storeCurrentBox();
      setStatus('Saving current crop image...');
      const result = await api('/api/save-current', {
        method: 'POST',
        body: JSON.stringify({ id: img.id, box: state.currentBox })
      });
      setStatus(`Saved crop image: ${result.output.crop}`);
      renderSavedOutputs([result.output]);
      renderList();
    }

    async function saveAll() {
      storeCurrentBox();
      const payload = { boxes: state.boxes };
      setStatus('Saving crop JSON and PNG outputs...');
      const result = await api('/api/save', {
        method: 'POST',
        body: JSON.stringify(payload)
      });
      setStatus(`Saved ${result.saved_count} crop(s). ${result.boxes_path}`);
      renderSavedOutputs(result.outputs || []);
      renderList();
    }

    function moveBox(dx, dy) {
      const img = currentImage();
      const b = state.currentBox;
      if (!img || !b) return;
      b.x = clamp(b.x + dx, 0, img.width - b.w);
      b.y = clamp(b.y + dy, 0, img.height - b.h);
      storeCurrentBox();
      renderBox();
    }

    els.stage.addEventListener('pointerdown', (evt) => {
      const img = currentImage();
      if (!img || evt.button !== 0) return;
      updateScale();
      const p = stagePoint(evt);
      const handle = evt.target.dataset && evt.target.dataset.handle;
      if (handle && state.currentBox) {
        state.drag = { mode: 'resize', handle, start: p, startBox: { ...state.currentBox } };
      } else if (evt.target === els.cropBox && state.currentBox) {
        state.drag = { mode: 'move', start: p, startBox: { ...state.currentBox } };
      } else {
        state.currentBox = { x: p.x, y: p.y, w: 1, h: 1 };
        state.drag = { mode: 'draw', start: p, startBox: { ...state.currentBox } };
      }
      els.stage.setPointerCapture(evt.pointerId);
      renderBox();
      evt.preventDefault();
    });

    els.stage.addEventListener('pointermove', (evt) => {
      const img = currentImage();
      const drag = state.drag;
      if (!img || !drag) return;
      const p = stagePoint(evt);
      if (drag.mode === 'draw') {
        const x1 = clamp(Math.min(drag.start.x, p.x), 0, img.width - 1);
        const y1 = clamp(Math.min(drag.start.y, p.y), 0, img.height - 1);
        const x2 = clamp(Math.max(drag.start.x, p.x), 1, img.width);
        const y2 = clamp(Math.max(drag.start.y, p.y), 1, img.height);
        state.currentBox = normalizeBox({ x: x1, y: y1, w: x2 - x1, h: y2 - y1 }, img);
      } else if (drag.mode === 'move') {
        const dx = p.x - drag.start.x;
        const dy = p.y - drag.start.y;
        state.currentBox = normalizeBox({
          x: drag.startBox.x + dx,
          y: drag.startBox.y + dy,
          w: drag.startBox.w,
          h: drag.startBox.h
        }, img);
      } else if (drag.mode === 'resize') {
        let left = drag.startBox.x;
        let top = drag.startBox.y;
        let right = drag.startBox.x + drag.startBox.w;
        let bottom = drag.startBox.y + drag.startBox.h;
        if (drag.handle.includes('w')) left = p.x;
        if (drag.handle.includes('e')) right = p.x;
        if (drag.handle.includes('n')) top = p.y;
        if (drag.handle.includes('s')) bottom = p.y;
        const x1 = clamp(Math.min(left, right), 0, img.width - 1);
        const y1 = clamp(Math.min(top, bottom), 0, img.height - 1);
        const x2 = clamp(Math.max(left, right), 1, img.width);
        const y2 = clamp(Math.max(top, bottom), 1, img.height);
        state.currentBox = normalizeBox({ x: x1, y: y1, w: x2 - x1, h: y2 - y1 }, img);
      }
      storeCurrentBox();
      renderBox();
    });

    window.addEventListener('pointerup', () => {
      if (state.drag) {
        state.drag = null;
        storeCurrentBox();
      }
    });

    window.addEventListener('resize', renderBox);
    els.mainImage.addEventListener('load', renderBox);
    document.getElementById('prevBtn').addEventListener('click', () => loadImage(state.index - 1));
    document.getElementById('nextBtn').addEventListener('click', () => loadImage(state.index + 1));
    els.openImagesBtn.addEventListener('click', () => els.fileInput.click());
    els.fileInput.addEventListener('change', () => {
      importSelectedFiles(els.fileInput.files)
        .catch(err => setStatus(`Error: ${err.message}`))
        .finally(() => { els.fileInput.value = ''; });
    });
    document.getElementById('fitBtn').addEventListener('click', () => {
      document.getElementById('viewerWrap').scrollTo({ top: 0, left: 0, behavior: 'smooth' });
      renderBox();
    });
    document.getElementById('clearBtn').addEventListener('click', () => {
      state.currentBox = null;
      storeCurrentBox();
      renderBox();
    });
    document.getElementById('saveCurrentBtn').addEventListener('click', () => {
      saveCurrent().catch(err => setStatus(`Error: ${err.message}`));
    });
    document.getElementById('saveBtn').addEventListener('click', () => {
      saveAll().catch(err => setStatus(`Error: ${err.message}`));
    });
    document.getElementById('applyBtn').addEventListener('click', applyInputs);
    document.getElementById('copyBtn').addEventListener('click', async () => {
      const img = currentImage();
      if (!img || !state.currentBox) return;
      await navigator.clipboard.writeText(JSON.stringify({ id: img.id, box: state.currentBox }, null, 2));
      setStatus('Copied current box JSON to clipboard.');
    });
    window.addEventListener('keydown', (evt) => {
      if (evt.target && evt.target.tagName === 'INPUT') return;
      if (evt.key === 'ArrowLeft') { moveBox(evt.shiftKey ? -10 : -1, 0); evt.preventDefault(); }
      if (evt.key === 'ArrowRight') { moveBox(evt.shiftKey ? 10 : 1, 0); evt.preventDefault(); }
      if (evt.key === 'ArrowUp') { moveBox(0, evt.shiftKey ? -10 : -1); evt.preventDefault(); }
      if (evt.key === 'ArrowDown') { moveBox(0, evt.shiftKey ? 10 : 1); evt.preventDefault(); }
      if (evt.key === '[') loadImage(state.index - 1);
      if (evt.key === ']') loadImage(state.index + 1);
      if ((evt.ctrlKey || evt.metaKey) && evt.key.toLowerCase() === 's') {
        evt.preventDefault();
        saveAll().catch(err => setStatus(`Error: ${err.message}`));
      }
    });

    async function init() {
      const data = await api('/api/state');
      state.images = data.images;
      state.boxes = data.boxes || {};
      state.config = data.config;
      renderConfig(data);
      if (!state.images.length) {
        setStatus('No images found. Click Open Images.');
        return;
      }
      loadImage(0);
    }

    init().catch(err => setStatus(`Error: ${err.message}`));
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
    keep = []
    for ch in text:
        if ch.isalnum() or ch in ("-", "_", "."):
            keep.append(ch)
        else:
            keep.append("-")
    return "".join(keep).strip("-") or "image"


def find_images(images_dir: Path, recursive: bool, patterns: list[str]) -> list[Path]:
    pattern = "**/*" if recursive else "*"
    files = [
        p
        for p in images_dir.glob(pattern)
        if p.is_file() and p.suffix.lower() in IMAGE_EXTS
    ]
    if patterns:
        files = [p for p in files if any(p.match(item) for item in patterns)]
    return sorted(files, key=lambda p: str(p).lower())


def read_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    if isinstance(data, dict):
        return data
    return {}


def normalize_box(raw: Any, width: int, height: int) -> dict[str, int] | None:
    if not isinstance(raw, dict):
        return None
    try:
        x = round(float(raw["x"]))
        y = round(float(raw["y"]))
        w = round(float(raw["w"]))
        h = round(float(raw["h"]))
    except (KeyError, TypeError, ValueError):
        return None
    if w <= 0 or h <= 0 or width <= 0 or height <= 0:
        return None
    x = max(0, min(x, width - 1))
    y = max(0, min(y, height - 1))
    w = max(1, min(w, width - x))
    h = max(1, min(h, height - y))
    return {"x": x, "y": y, "w": w, "h": h}


def fit_tile(im: Image.Image, size: tuple[int, int], bg: tuple[int, int, int]) -> Image.Image:
    tile = Image.new("RGB", size, bg)
    thumb = im.convert("RGB").copy()
    thumb.thumbnail(size, Image.Resampling.LANCZOS)
    tile.paste(thumb, ((size[0] - thumb.width) // 2, (size[1] - thumb.height) // 2))
    return tile


class CropTool:
    def __init__(
        self,
        images_dir: Path,
        output_dir: Path,
        boxes_path: Path,
        recursive: bool,
        patterns: list[str],
        suffix: str,
        upscale: int,
    ) -> None:
        self.images_dir = images_dir.resolve()
        self.output_dir = output_dir.resolve()
        self.boxes_path = boxes_path.resolve()
        self.recursive = recursive
        self.patterns = patterns
        self.suffix = suffix
        self.upscale = max(1, upscale)
        self.import_dir = self.boxes_path.parent / "refs"
        self.output_dir.mkdir(parents=True, exist_ok=True)
        self.boxes_path.parent.mkdir(parents=True, exist_ok=True)
        self.images = self._load_images()
        self.boxes = self._load_boxes()

    def _load_images(self) -> list[ImageEntry]:
        paths = find_images(self.images_dir, self.recursive, self.patterns)
        entries: list[ImageEntry] = []
        for path in paths:
            try:
                with Image.open(path) as im:
                    width, height = im.size
            except Exception:
                continue
            rel = path.relative_to(self.images_dir).as_posix()
            entries.append(ImageEntry(rel, rel, path, width, height))
        return entries

    def _load_boxes(self) -> dict[str, dict[str, int]]:
        data = read_json(self.boxes_path)
        raw_boxes = data.get("boxes", data)
        if not isinstance(raw_boxes, dict):
            return {}
        image_by_id = {entry.id: entry for entry in self.images}
        boxes: dict[str, dict[str, int]] = {}
        for image_id, box in raw_boxes.items():
            entry = image_by_id.get(str(image_id))
            if not entry:
                continue
            clean = normalize_box(box, entry.width, entry.height)
            if clean:
                boxes[entry.id] = clean
        return boxes

    def state(self) -> dict[str, Any]:
        return {
            "ok": True,
            "config": {
                "images_dir": str(self.images_dir),
                "output_dir": str(self.output_dir),
                "boxes_path": str(self.boxes_path),
                "recursive": self.recursive,
                "patterns": self.patterns,
                "suffix": self.suffix,
                "upscale": self.upscale,
                "import_dir": str(self.import_dir),
            },
            "images": [
                {
                    "id": entry.id,
                    "name": entry.name,
                    "path": str(entry.path),
                    "width": entry.width,
                    "height": entry.height,
                }
                for entry in self.images
            ],
            "boxes": self.boxes,
        }

    def import_images(self, files: list[Any]) -> dict[str, Any]:
        imported: list[ImageEntry] = []
        self.import_dir.mkdir(parents=True, exist_ok=True)
        for index, item in enumerate(files, start=1):
            if not isinstance(item, dict):
                raise ValueError(f"files[{index - 1}] must be an object")
            raw_name = Path(str(item.get("name") or f"selected-image-{index}.png")).name
            data_url = str(item.get("data_url") or "")
            if "," in data_url:
                header, encoded = data_url.split(",", 1)
            else:
                header, encoded = "", data_url
            suffix = Path(raw_name).suffix.lower()
            if suffix not in IMAGE_EXTS:
                suffix = ".jpg" if "image/jpeg" in header else ".png"
            stem = safe_slug(Path(raw_name).stem or f"selected-image-{index}")
            try:
                raw = base64.b64decode(encoded, validate=True)
            except Exception as exc:
                raise ValueError(f"could not decode {raw_name}") from exc
            try:
                with Image.open(io.BytesIO(raw)) as im:
                    width, height = im.size
                    im.verify()
            except Exception as exc:
                raise ValueError(f"not a supported image: {raw_name}") from exc
            dest = self.unique_import_path(stem, suffix)
            dest.write_bytes(raw)
            entry_id = f"ui-selected/{dest.name}"
            imported.append(ImageEntry(entry_id, dest.name, dest, width, height))
        selected_index = len(self.images)
        self.images.extend(imported)
        state = self.state()
        state["imported_count"] = len(imported)
        state["selected_index"] = selected_index if imported else max(0, len(self.images) - 1)
        return state

    def unique_import_path(self, stem: str, suffix: str) -> Path:
        dest = self.import_dir / f"{stem}{suffix}"
        counter = 2
        while dest.exists():
            dest = self.import_dir / f"{stem}-{counter}{suffix}"
            counter += 1
        return dest

    def save(self, boxes: dict[str, Any]) -> dict[str, Any]:
        image_by_id = {entry.id: entry for entry in self.images}
        clean_boxes: dict[str, dict[str, int]] = {}
        for image_id, box in boxes.items():
            entry = image_by_id.get(str(image_id))
            if not entry:
                continue
            clean = normalize_box(box, entry.width, entry.height)
            if clean:
                clean_boxes[entry.id] = clean
        self.boxes = clean_boxes
        outputs = self.crop_all()
        payload = {
            "updated_at": now_iso(),
            "images_dir": str(self.images_dir),
            "output_dir": str(self.output_dir),
            "suffix": self.suffix,
            "upscale": self.upscale,
            "boxes": self.boxes,
            "outputs": outputs,
        }
        self.boxes_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        return {
            "ok": True,
            "saved_count": len(outputs),
            "boxes_path": str(self.boxes_path),
            "output_dir": str(self.output_dir),
            "outputs": outputs,
        }

    def save_current(self, image_id: str, box: Any) -> dict[str, Any]:
        image_by_id = {entry.id: entry for entry in self.images}
        entry = image_by_id.get(str(image_id))
        if not entry:
            raise ValueError("unknown image id")
        clean = normalize_box(box, entry.width, entry.height)
        if not clean:
            raise ValueError("invalid crop box")
        self.boxes[entry.id] = clean
        output = self.crop_one(entry, clean)
        payload = {
            "updated_at": now_iso(),
            "images_dir": str(self.images_dir),
            "output_dir": str(self.output_dir),
            "suffix": self.suffix,
            "upscale": self.upscale,
            "boxes": self.boxes,
            "outputs": [output] if output else [],
        }
        self.boxes_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        if not output:
            raise ValueError("could not save crop")
        return {
            "ok": True,
            "saved_count": 1,
            "boxes_path": str(self.boxes_path),
            "output_dir": str(self.output_dir),
            "output": output,
        }

    def resolve_output_file(self, raw_path: str) -> Path:
        path = Path(raw_path).resolve()
        allowed_roots = [self.output_dir.resolve(), self.import_dir.resolve()]
        if not any(path == root or root in path.parents for root in allowed_roots):
            raise ValueError("file is outside the crop output directory")
        if not path.is_file():
            raise ValueError("file not found")
        return path

    def crop_all(self) -> list[dict[str, Any]]:
        image_by_id = {entry.id: entry for entry in self.images}
        outputs: list[dict[str, Any]] = []
        for image_id, box in self.boxes.items():
            entry = image_by_id.get(image_id)
            if not entry:
                continue
            out = self.crop_one(entry, box)
            if out:
                outputs.append(out)
        return outputs

    def crop_one(self, entry: ImageEntry, box: dict[str, int]) -> dict[str, Any] | None:
        with Image.open(entry.path) as im:
            clean = normalize_box(box, im.width, im.height)
            if not clean:
                return None
            crop = im.crop(
                (
                    clean["x"],
                    clean["y"],
                    clean["x"] + clean["w"],
                    clean["y"] + clean["h"],
                )
            )
            stem = safe_slug(Path(entry.name).stem)
            out_path = self.output_dir / f"{stem}{self.suffix}.png"
            crop.save(out_path)
            upscale_path = None
            if self.upscale > 1:
                up_dir = self.output_dir / f"upscaled-{self.upscale}x"
                up_dir.mkdir(parents=True, exist_ok=True)
                up = crop.resize(
                    (crop.width * self.upscale, crop.height * self.upscale),
                    Image.Resampling.LANCZOS,
                )
                upscale_path = up_dir / f"{stem}{self.suffix}-{self.upscale}x.png"
                up.save(upscale_path)
            return {
                "id": entry.id,
                "source": str(entry.path),
                "box": clean,
                "crop": str(out_path),
                "crop_size": [crop.width, crop.height],
                "upscaled": str(upscale_path) if upscale_path else None,
                "upscaled_size": [crop.width * self.upscale, crop.height * self.upscale]
                if upscale_path
                else None,
            }

    def write_contact_sheet(self, outputs: list[dict[str, Any]]) -> str:
        tile_w, tile_h, label_h = 320, 260, 28
        cols = min(4, max(1, len(outputs)))
        rows = (len(outputs) + cols - 1) // cols
        sheet = Image.new("RGB", (cols * tile_w, rows * (tile_h + label_h)), (24, 24, 24))
        draw = ImageDraw.Draw(sheet)
        try:
            font = ImageFont.truetype("arial.ttf", 14)
        except Exception:
            font = ImageFont.load_default()
        for idx, item in enumerate(outputs):
            c = idx % cols
            r = idx // cols
            x = c * tile_w
            y = r * (tile_h + label_h)
            draw.rectangle([x, y, x + tile_w, y + label_h], fill=(44, 44, 44))
            draw.text((x + 8, y + 6), item["id"], fill=(235, 235, 235), font=font)
            with Image.open(item["crop"]) as im:
                sheet.paste(fit_tile(im, (tile_w, tile_h), (246, 246, 242)), (x, y + label_h))
        path = self.output_dir / "manual-crop-contact-sheet.png"
        sheet.save(path)
        return str(path)


class Handler(BaseHTTPRequestHandler):
    tool: CropTool

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))

    def send_json(self, data: dict[str, Any], status: HTTPStatus = HTTPStatus.OK) -> None:
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_text(self, text: str, content_type: str = "text/html; charset=utf-8") -> None:
        body = text.encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        route = posixpath.normpath(parsed.path)
        query = urllib.parse.parse_qs(parsed.query)
        if route in ("", "/"):
            self.send_text(INDEX_HTML)
            return
        if route == "/api/state":
            self.send_json(self.tool.state())
            return
        if route == "/api/image":
            self.serve_image(query)
            return
        if route == "/api/output-file":
            self.serve_output_file(query)
            return
        self.send_json({"ok": False, "error": "not found"}, HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:  # noqa: N802
        parsed = urllib.parse.urlparse(self.path)
        route = posixpath.normpath(parsed.path)
        if route == "/api/import-images":
            try:
                data = self.read_body_json()
                files = data.get("files", [])
                if not isinstance(files, list):
                    raise ValueError("files must be an array")
                result = self.tool.import_images(files)
            except Exception as exc:
                self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
                return
            self.send_json(result)
            return
        if route == "/api/save":
            try:
                data = self.read_body_json()
                boxes = data.get("boxes", {})
                if not isinstance(boxes, dict):
                    raise ValueError("boxes must be an object")
                result = self.tool.save(boxes)
            except Exception as exc:
                self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
                return
            self.send_json(result)
            return
        if route == "/api/save-current":
            try:
                data = self.read_body_json()
                result = self.tool.save_current(str(data.get("id") or ""), data.get("box"))
            except Exception as exc:
                self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
                return
            self.send_json(result)
            return
        self.send_json({"ok": False, "error": "not found"}, HTTPStatus.NOT_FOUND)

    def read_body_json(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        if not raw:
            return {}
        data = json.loads(raw.decode("utf-8"))
        if not isinstance(data, dict):
            raise ValueError("request body must be a JSON object")
        return data

    def serve_image(self, query: dict[str, list[str]]) -> None:
        try:
            index = int(query.get("index", ["0"])[0])
            entry = self.tool.images[index]
        except (ValueError, IndexError):
            self.send_json({"ok": False, "error": "invalid image index"}, HTTPStatus.BAD_REQUEST)
            return
        content_type = mimetypes.guess_type(entry.path.name)[0] or "application/octet-stream"
        data = entry.path.read_bytes()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def serve_output_file(self, query: dict[str, list[str]]) -> None:
        try:
            raw_path = query.get("path", [""])[0]
            path = self.tool.resolve_output_file(raw_path)
        except Exception as exc:
            self.send_json({"ok": False, "error": str(exc)}, HTTPStatus.BAD_REQUEST)
            return
        content_type = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        data = path.read_bytes()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Open a browser UI for drawing exact crop boxes.")
    parser.add_argument("--images", default=".", help="Directory containing source images.")
    parser.add_argument("--output", default=None, help="Directory where cropped PNGs are written.")
    parser.add_argument("--boxes", default=None, help="Path to manual-crop-boxes.json.")
    parser.add_argument("--recursive", action="store_true", help="Scan image directory recursively.")
    parser.add_argument(
        "--pattern",
        action="append",
        default=[],
        help="Only include images matching this glob, e.g. ck-*-wide-crop.png. May be repeated.",
    )
    parser.add_argument("--host", default="127.0.0.1", help="HTTP host. Default: 127.0.0.1")
    parser.add_argument("--port", type=int, default=8765, help="HTTP port. Default: 8765")
    parser.add_argument("--open", action="store_true", help="Open the browser automatically.")
    parser.add_argument("--suffix", default="-manual-control-crop", help="Output filename suffix.")
    parser.add_argument("--upscale", type=int, default=1, help="Also write deterministic Nx upscaled crops when >1.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    images_dir = Path(args.images).resolve()
    if not images_dir.exists() or not images_dir.is_dir():
        print(f"Image directory does not exist: {images_dir}", file=sys.stderr)
        return 2
    default_output, records = topic_dirs(Path.cwd(), "manual-crops")
    output_dir = Path(args.output).resolve() if args.output else default_output
    boxes_path = Path(args.boxes).resolve() if args.boxes else records / "manual-crop-boxes.json"
    tool = CropTool(
        images_dir=images_dir,
        output_dir=output_dir,
        boxes_path=boxes_path,
        recursive=args.recursive,
        patterns=args.pattern,
        suffix=args.suffix,
        upscale=args.upscale,
    )
    Handler.tool = tool
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    url = f"http://{args.host}:{args.port}/"
    print(json.dumps({
        "ok": True,
        "url": url,
        "images": len(tool.images),
        "images_dir": str(tool.images_dir),
        "output_dir": str(tool.output_dir),
        "boxes_path": str(tool.boxes_path),
    }, ensure_ascii=False))
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
