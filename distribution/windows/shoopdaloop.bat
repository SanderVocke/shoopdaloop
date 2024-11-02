@echo off

setlocal enabledelayedexpansion

for /f "delims=" %%A in (%~dp0shoop.dllpaths) do (
    set "line=%%A"
    set PATH=%~dp0!line!;%PATH%
)

endlocal

echo shoopdaloop.bat: Running shoopdaloop with path: %PATH%
"%~dp0shoopdaloop.exe" %*