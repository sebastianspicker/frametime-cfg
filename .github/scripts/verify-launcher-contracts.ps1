$startBat = Get-Content .\START.bat -Raw
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

$startGuiBat = Get-Content .\START-GUI.bat -Raw
if ($startGuiBat -notmatch [regex]::Escape('CS2-Optimize-GUI.ps1')) {
    throw "START-GUI.bat is missing CS2-Optimize-GUI.ps1"
}
