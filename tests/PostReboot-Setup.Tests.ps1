# ==============================================================================
#  tests/PostReboot-Setup.Tests.ps1  --  direct shipped-entrypoint contract tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:TargetScript = Join-Path $script:ProjectRoot "PostReboot-Setup.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "PostReboot-Setup.ps1 shipped smoke contract" {

    It "supports -SmokeTest as a clean short-circuit" -Skip:(-not $IsWindows) {
        $records = & powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File $script:TargetScript -SmokeTest 2>&1
        $exitCode = $LASTEXITCODE
        $output = ($records | ForEach-Object { $_.ToString() }) -join "`n"
        $errorRecords = @($records | Where-Object { $_ -is [System.Management.Automation.ErrorRecord] })

        $exitCode | Should -Be 0
        $errorRecords | Should -BeNullOrEmpty
        $output | Should -Match 'SMOKE TEST OK'
    }
}
