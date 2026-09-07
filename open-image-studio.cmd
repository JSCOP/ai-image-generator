@echo off
rem AI Image Studio one-click launcher (this clone only).
rem Forwards all args to open-image-studio.ps1; pauses only on error.
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0open-image-studio.ps1" %*
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" pause
exit /b %EXIT_CODE%
