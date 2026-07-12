@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0open-manual-region-crop-tool.ps1" %*
