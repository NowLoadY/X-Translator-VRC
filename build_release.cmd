@echo off
setlocal

set "PROJECT_ROOT=%~dp0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%PROJECT_ROOT%build_release.ps1" %*
exit /b %ERRORLEVEL%
