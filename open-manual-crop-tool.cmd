@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0open-manual-crop-tool.ps1" %*
