@echo off
setlocal EnableExtensions DisableDelayedExpansion

if "%~3"=="" goto :usage
if /I not "%~1"=="vendor-preserved" if /I not "%~1"=="lab-export" goto :usage
if not exist "%~f2" (
  echo Packaged Frametime executable does not exist: %~f2 1>&2
  exit /b 2
)
if /I not "%~x2"==".exe" (
  echo Packaged Frametime executable must have an .exe extension. 1>&2
  exit /b 2
)
if exist "%~3" (
  echo Qualification output must be an unused directory. 1>&2
  exit /b 2
)

set "FRAMETIME_EXE=%~f2"
set "FRAMETIME_SHA256="
for /f "skip=1 tokens=* delims=" %%H in ('certutil -hashfile "%FRAMETIME_EXE%" SHA256') do if not defined FRAMETIME_SHA256 set "FRAMETIME_SHA256=%%H"
if not defined FRAMETIME_SHA256 (
  echo Could not hash the packaged Frametime executable. 1>&2
  exit /b 1
)

mkdir "%~3" || exit /b 1
>"%~3\qualification.txt" echo lane=%~1
for %%F in ("%FRAMETIME_EXE%") do >>"%~3\qualification.txt" echo executable_name=%%~nxF
for %%F in ("%FRAMETIME_EXE%") do >>"%~3\qualification.txt" echo executable_bytes=%%~zF
>>"%~3\qualification.txt" echo executable_sha256=%FRAMETIME_SHA256%
>>"%~3\qualification.txt" echo started_local=%DATE% %TIME%
>>"%~3\qualification.txt" echo architecture=%PROCESSOR_ARCHITECTURE%
for /f "tokens=*" %%V in ('ver') do >>"%~3\qualification.txt" echo windows=%%V
fltmc >nul 2>&1
if errorlevel 1 (
  >>"%~3\qualification.txt" echo elevated=false
) else (
  >>"%~3\qualification.txt" echo elevated=true
)
>>"%~3\qualification.txt" echo secure_boot_policy=unchanged
>>"%~3\qualification.txt" echo testsigning_policy=unchanged
"%FRAMETIME_EXE%" driver inspect >"%~3\driver-inspect.json" || exit /b 1
>>"%~3\qualification.txt" echo finished_local=%DATE% %TIME%
echo Qualification scaffold captured. Add reviewed stage and recovery evidence per docs\native\driver-qualification.md.
exit /b 0

:usage
echo Usage: %~nx0 ^<vendor-preserved^|lab-export^> ^<packaged-frametime.exe^> ^<unused-output-directory^> 1>&2
exit /b 2
