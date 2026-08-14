Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$startBat = Get-Content .\START.bat -Raw
if ($startBat -match '(?<!\r)\n') {
    throw "START.bat must use CRLF line endings for cmd.exe compatibility."
}
if ($startBat -match '(?i)EnableDelayedExpansion' -or
    $startBat -notmatch '(?i)DisableDelayedExpansion') {
    throw "START.bat must explicitly disable delayed expansion."
}
if ($startBat -notmatch [regex]::Escape('%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe') -or
    $startBat -match '(?im)^\s*(?:powershell|powershell\.exe)\b') {
    throw "START.bat must use only the absolute Windows PowerShell system path."
}
if ($startBat -match '(?i)\-Command\b|Verb\s+RunAs|net\.exe"\s+session') {
    throw "START.bat must not elevate or evaluate command text from the portable tree."
}
$dryRunRoute = $startBat.IndexOf('if /i "%~1"=="dry-run"', [StringComparison]::OrdinalIgnoreCase)
$unavailable = $startBat.IndexOf('Live execution from a portable source tree is unavailable', [StringComparison]::OrdinalIgnoreCase)
if ($dryRunRoute -lt 0 -or $unavailable -le $dryRunRoute) {
    throw "START.bat must route strict dry-run before its fail-closed live boundary."
}
if ($startBat -notmatch '(?im)^:fulldryrun\s*$' -or
    $startBat -notmatch 'Run-Optimize\.ps1" -FullDryRun -DryRunGpu' -or
    $startBat -notmatch '(?i)-NonInteractive' -or
    $startBat -notmatch '(?im)^:fulldryrunall\s*$' -or
    $startBat -notmatch '(?i)for\s+%%G\s+in\s+\(1\s+2\s+3\s+4\)') {
    throw "START.bat is missing the strict Full DRY-RUN contracts."
}

$startGuiBat = Get-Content .\START-GUI.bat -Raw
if ($startGuiBat -match '(?<!\r)\n') {
    throw "START-GUI.bat must use CRLF line endings for cmd.exe compatibility."
}
if ($startGuiBat -match '(?i)EnableDelayedExpansion' -or
    $startGuiBat -notmatch '(?i)DisableDelayedExpansion' -or
    $startGuiBat -match '(?i)powershell|Verb\s+RunAs|frametime-gui\.ps1') {
    throw "START-GUI.bat must remain a fail-closed portable launcher."
}
if ($startGuiBat -notmatch 'portable WPF launcher is unavailable') {
    throw "START-GUI.bat must explain its fail-closed source-authentication boundary."
}

$portableGuards = @{
    'Run-Optimize.ps1' = 'Use -FullDryRun'
    'Boot-SafeMode.ps1' = 'Use -DryRun'
    'Cleanup.ps1' = 'Portable live execution is unavailable'
    'FpsCap-Calculator.ps1' = 'Portable live execution is unavailable'
    'Verify-Settings.ps1' = 'Portable live execution is unavailable'
    'frametime-gui.ps1' = 'Portable live execution is unavailable'
    'Launcher-Action.ps1' = 'Portable live execution is unavailable'
}
foreach ($entry in $portableGuards.GetEnumerator()) {
    $source = Get-Content -LiteralPath $entry.Key -Raw
    if ($source -notmatch [regex]::Escape($entry.Value)) {
        throw "$($entry.Key) is missing its portable live-execution guard."
    }
}

$launcherAction = Get-Content .\Launcher-Action.ps1 -Raw
if ($launcherAction -match '(?m)^\s*\.\s+|config\.env\.ps1|helpers\.ps1|Get-Content|Test-PhaseRuntimePayload') {
    throw "Launcher-Action.ps1 must fail before importing or reading mutable portable content."
}
