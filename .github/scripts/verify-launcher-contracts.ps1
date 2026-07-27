$startBat = Get-Content .\START.bat -Raw
if ($startBat -match '(?<!\r)\n') {
    throw "START.bat must use CRLF line endings for cmd.exe compatibility."
}
foreach ($target in @(
    'Run-Optimize.ps1',
    'Cleanup.ps1',
    'FpsCap-Calculator.ps1',
    'Verify-Settings.ps1',
    'Boot-SafeMode.ps1',
    'PostReboot-Setup.ps1'
)) {
    if ($startBat -notmatch [regex]::Escape($target)) {
        throw "START.bat is missing launcher target: $target"
    }
}

$dryRunRoute = $startBat.IndexOf('if /i "%~1"=="dry-run"', [StringComparison]::OrdinalIgnoreCase)
$adminGate = $startBat.IndexOf('net session', [StringComparison]::OrdinalIgnoreCase)
if ($dryRunRoute -lt 0 -or $adminGate -le $dryRunRoute) {
    throw "START.bat must route the strict dry-run before its administrator gate."
}
if ($startBat.Substring($dryRunRoute, [Math]::Min(180, $startBat.Length - $dryRunRoute)) -notmatch '(?i)goto\s+:fulldryrun') {
    throw "START.bat dry-run route must enter the full preview before elevation."
}
if ($startBat -notmatch '(?im)^:fulldryrun\s*$' -or
    $startBat -notmatch 'Run-Optimize\.ps1" -FullDryRun -DryRunGpu' -or
    $startBat -notmatch '(?i)-NonInteractive') {
    throw "START.bat is missing the full DRY-RUN launcher contract."
}
if ($startBat -notmatch '(?im)^:fulldryrunall\s*$' -or
    $startBat -notmatch '(?i)for\s+%%G\s+in\s+\(1\s+2\s+3\s+4\)') {
    throw "START.bat is missing the four-branch DRY-RUN matrix contract."
}

$startGuiBat = Get-Content .\START-GUI.bat -Raw
if ($startGuiBat -match '(?<!\r)\n') {
    throw "START-GUI.bat must use CRLF line endings for cmd.exe compatibility."
}
if ($startGuiBat -notmatch [regex]::Escape('frametime-gui.ps1')) {
    throw "START-GUI.bat is missing frametime-gui.ps1"
}
