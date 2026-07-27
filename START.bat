@echo off
setlocal EnableExtensions EnableDelayedExpansion
title frametime.cfg

if /i "%~1"=="dry-run" (
    set "dryRunGpu=%~2"
    goto :fulldryrun
)

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo  Starting as administrator...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

:menu
cls
echo.
echo  =============================================
echo   FRAMETIME.CFG
echo   Tier System: T1=Auto T2=Prompt T3=Control
echo  =============================================
echo.
echo   [1]  Start / resume optimization
echo        (Phase 1 + 2 + 3)
echo.
echo   [2]  Cleanup / Soft-Reset
echo        (Shader Cache, Temp, DNS, ...)
echo.
echo   [3]  FPS Cap Calculator
echo        (Evaluate benchmark output)
echo.
echo   [4]  Show current log
echo.
echo   [5]  Reset progress
echo.
echo   [6]  Verify settings
echo        (Check registry keys after Windows Update)
echo.
echo   [7]  Restore / Rollback
echo        (Undo changes from specific steps)
echo.
echo   [8]  Backup summary
echo        (Show what was backed up before changes)
echo.
echo   [S]  Boot to Safe Mode (Phase 2)
echo        (Re-run after Phase 1 Step 38 prepared the handoff)
echo.
echo   [P]  Post-Reboot Setup (Phase 3)
echo        (Manual start if auto-start failed)
echo.
echo   [D]  Full DRY-RUN
echo        (All phases and tiers; select GPU; no changes)
echo.
echo   [9]  Exit
echo.
set /p choice="  Choice [1-9/S/P/D]: "

if "%choice%"=="1" goto :phase1
if "%choice%"=="2" goto :cleanup
if "%choice%"=="3" goto :fpscap
if "%choice%"=="4" goto :showlog
if "%choice%"=="5" goto :resetprogress
if "%choice%"=="6" goto :verify
if "%choice%"=="7" goto :restore
if "%choice%"=="8" goto :backupsummary
if /i "%choice%"=="S" goto :safemode
if /i "%choice%"=="P" goto :phase3
if /i "%choice%"=="D" goto :dryrunselect
if "%choice%"=="9" exit /b
goto :menu

:phase1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Run-Optimize.ps1"
pause
goto :menu

:dryrunselect
echo.
echo   GPU branch to preview:
echo   [1] NVIDIA RTX 5000  [2] Other NVIDIA  [3] AMD  [4] Intel Arc
echo   [A] All four branches (long validation run)
set "dryRunGpu=2"
set /p dryRunGpu="  Choice [1/2/3/4/A, default 2]: "
if /i "%dryRunGpu%"=="A" set "dryRunGpu=all"

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
if /i "%~1"=="dry-run" exit /b 2
pause
goto :menu

:fulldryrunone
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0Run-Optimize.ps1" -FullDryRun -DryRunGpu "%dryRunGpu%"
set "dryRunExit=%errorlevel%"
if /i "%~1"=="dry-run" exit /b %dryRunExit%
pause
goto :menu

:fulldryrunall
echo.
echo   Full DRY-RUN matrix: four isolated GPU previews will run.
for %%G in (1 2 3 4) do (
    echo.
    echo   ===== GPU BRANCH %%G OF 4 =====
    powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0Run-Optimize.ps1" -FullDryRun -DryRunGpu "%%G"
    if !errorlevel! neq 0 exit /b !errorlevel!
)
echo.
echo   ALL FOUR GPU BRANCH PREVIEWS COMPLETE
if /i "%~1"=="dry-run" exit /b 0
pause
goto :menu

:cleanup
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Cleanup.ps1"
pause
goto :menu

:fpscap
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0FpsCap-Calculator.ps1"
pause
goto :menu

:showlog
powershell -NoProfile -ExecutionPolicy Bypass -Command "Set-StrictMode -Version Latest; . \"%~dp0config.env.ps1\"; if (Test-Path $CFG_LogFile) { Get-Content $CFG_LogFile | more } else { Write-Host '  No log found.'; Read-Host '  Press Enter to continue' }"
pause
goto :menu

:verify
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Verify-Settings.ps1"
pause
goto :menu

:resetprogress
echo.
echo  Progress file will be deleted.
set /p confirm="  Are you sure? [y/N]: "
if /i "%confirm%"=="y" (
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Set-StrictMode -Version Latest; . \"%~dp0config.env.ps1\"; . \"%~dp0helpers.ps1\"; Clear-Progress"
    echo  Reset complete.
)
if /i "%confirm%"=="j" (
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Set-StrictMode -Version Latest; . \"%~dp0config.env.ps1\"; . \"%~dp0helpers.ps1\"; Clear-Progress"
    echo  Reset complete.
)
pause
goto :menu

:restore
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Set-StrictMode -Version Latest; $ScriptRoot='%~dp0'.TrimEnd('\'); . \"%~dp0config.env.ps1\"; . \"%~dp0helpers.ps1\"; Initialize-ScriptDefaults; Restore-Interactive"
pause
goto :menu

:backupsummary
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Set-StrictMode -Version Latest; $ScriptRoot='%~dp0'.TrimEnd('\'); . \"%~dp0config.env.ps1\"; . \"%~dp0helpers.ps1\"; Initialize-ScriptDefaults; Show-BackupSummary"
pause
goto :menu

:safemode
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Boot-SafeMode.ps1"
pause
goto :menu

:phase3
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "& { try { Set-StrictMode -Version Latest; $ErrorActionPreference='Stop'; $ScriptRoot='%~dp0'.TrimEnd('\'); . '%~dp0config.env.ps1'; . '%~dp0helpers.ps1'; $runtimeRoot=Get-PhaseRuntimeRoot -DestinationRoot $CFG_WorkDir; $phase3Runtime=Join-Path $runtimeRoot 'PostReboot-Setup.ps1'; if (-not (Test-Path -LiteralPath $phase3Runtime -PathType Leaf)) { throw 'Phase 3 entrypoint is missing from the selected runtime generation.' }; $validation=Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot; if (-not $validation.Valid) { throw $validation.Message }; & $phase3Runtime } catch { Write-Host ''; Write-Host ('  Phase 3 runtime payload is unavailable or invalid: ' + $_) -ForegroundColor Red; Write-Host '  Re-run Phase 1 to publish and verify a fresh runtime generation.' -ForegroundColor Cyan; exit 1 } }"
pause
goto :menu
