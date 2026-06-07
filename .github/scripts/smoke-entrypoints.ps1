param(
    [ValidateSet("pwsh", "powershell")]
    [string]$Engine = "pwsh"
)

$scripts = @(
    'Run-Optimize.ps1',
    'Cleanup.ps1',
    'Boot-SafeMode.ps1',
    'SafeMode-DriverClean.ps1',
    'PostReboot-Setup.ps1',
    'FpsCap-Calculator.ps1',
    'Verify-Settings.ps1',
    'CS2-Optimize-GUI.ps1'
)

foreach ($script in $scripts) {
    Write-Output "=== $Engine smoke test: $script ==="
    $records = & $Engine -NoLogo -NoProfile -ExecutionPolicy Bypass -File $script -SmokeTest 2>&1
    $output = $records | Out-String
    $errorRecords = @($records | Where-Object { $_ -is [System.Management.Automation.ErrorRecord] })
    if ($LASTEXITCODE -ne 0) {
        throw "Smoke test failed for $script`n$output"
    }
    if ($errorRecords.Count -gt 0) {
        throw "Smoke test emitted error records for $script`n$output"
    }
    if ($output -notmatch 'SMOKE TEST OK') {
        throw "Smoke test marker missing for $script`n$output"
    }
    Write-Output $output
}
