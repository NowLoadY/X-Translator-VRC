@echo off
setlocal

set "PROJECT_ROOT=%~dp0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%PROJECT_ROOT%start_rust_client.ps1" %*
exit /b %ERRORLEVEL%
