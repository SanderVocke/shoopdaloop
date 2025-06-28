@echo off
setlocal enabledelayedexpansion

:: Check that both arguments were passed
if "%1"=="" goto usage
if "%2"=="" goto usage

:: Get the path of this script (not the working directory)
for %%I in ("%~dp0.") do (
  set "SCRIPT_DIR=%%~fI"
)

:: Define portable folder and version as arguments
set "PORTABLE_FOLDER=%1"
set "VERSION=%2"

:: Run iscc.exe with options
iscc.exe /O+ /DMyAppVersion="%VERSION%" /DMyAppPortableFolder="%PORTABLE_FOLDER%" "%SCRIPT_DIR%\shoopdaloop.iss"
goto :eof

:: Print usage text if either argument was missing
:usage
echo Usage:
echo   script.bat PORTABLE_FOLDER VERSION
echo Example:
echo   script.bat "C:\shoopdaloop-portable" 1.2.3
