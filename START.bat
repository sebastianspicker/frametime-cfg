@echo off
setlocal EnableExtensions DisableDelayedExpansion
title frametime.cfg
set "FRAMETIME_CFG_POWERSHELL=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"

if /i "%~1"=="dry-run" (
    set "dryRunGpu=%~2"
    goto :fulldryrun
)

echo.
echo  Live execution from a portable source tree is unavailable.
echo  No trusted installer or signed payload currently establishes source identity.
echo  Use START.bat dry-run [1^|2^|3^|4^|all] for the no-mutation preview.
exit /b 1

:fulldryrun
if not defined dryRunGpu set "dryRunGpu=2"
if /i "%dryRunGpu%"=="all" goto :fulldryrunall
if "%dryRunGpu%"=="1" goto :fulldryrunone
if "%dryRunGpu%"=="2" goto :fulldryrunone
if "%dryRunGpu%"=="3" goto :fulldryrunone
if "%dryRunGpu%"=="4" goto :fulldryrunone
echo.
echo   Invalid DRY-RUN GPU branch: %dryRunGpu%
echo   Usage: START.bat dry-run [1^|2^|3^|4^|all]
exit /b 2

:fulldryrunone
"%FRAMETIME_CFG_POWERSHELL%" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0Run-Optimize.ps1" -FullDryRun -DryRunGpu "%dryRunGpu%"
exit /b %errorlevel%

:fulldryrunall
echo.
echo   Full DRY-RUN matrix: four isolated GPU previews will run.
set "dryRunMatrixExit=0"
for %%G in (1 2 3 4) do call :runfullpreview %%G
if not "%dryRunMatrixExit%"=="0" exit /b %dryRunMatrixExit%
echo.
echo   ALL FOUR GPU BRANCH PREVIEWS COMPLETE
exit /b 0

:runfullpreview
if not "%dryRunMatrixExit%"=="0" exit /b 0
echo.
echo   ===== GPU BRANCH %~1 OF 4 =====
"%FRAMETIME_CFG_POWERSHELL%" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0Run-Optimize.ps1" -FullDryRun -DryRunGpu "%~1"
if errorlevel 1 set "dryRunMatrixExit=%errorlevel%"
exit /b 0
