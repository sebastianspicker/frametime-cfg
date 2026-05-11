# ==============================================================================
#  tests/e2e/entrypoints.Tests.ps1  --  process-level public entrypoint coverage
# ==============================================================================

BeforeAll {
    Set-StrictMode -Version Latest
    $ErrorActionPreference = "Stop"

    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/../..").Path
    $script:PowerShellExe = (Get-Command pwsh -ErrorAction Stop).Source
    $script:SmokeTimeoutMs = 15000
    $script:Entrypoints = @(
        [PSCustomObject]@{ Script = "Run-Optimize.ps1";        Marker = "SMOKE TEST OK: Run-Optimize";        Flow = "Phase 1 optimizer" }
        [PSCustomObject]@{ Script = "Cleanup.ps1";             Marker = "SMOKE TEST OK: Cleanup";             Flow = "cleanup menu" }
        [PSCustomObject]@{ Script = "Boot-SafeMode.ps1";       Marker = "SMOKE TEST OK: Boot-SafeMode";       Flow = "Safe Mode handoff" }
        [PSCustomObject]@{ Script = "SafeMode-DriverClean.ps1"; Marker = "SMOKE TEST OK: SafeMode-DriverClean"; Flow = "Phase 2 driver cleanup" }
        [PSCustomObject]@{ Script = "PostReboot-Setup.ps1";    Marker = "SMOKE TEST OK: PostReboot-Setup";    Flow = "Phase 3 post-reboot setup" }
        [PSCustomObject]@{ Script = "FpsCap-Calculator.ps1";   Marker = "SMOKE TEST OK: FpsCap-Calculator";   Flow = "FPS cap calculator" }
        [PSCustomObject]@{ Script = "Verify-Settings.ps1";     Marker = "SMOKE TEST OK: Verify-Settings";     Flow = "settings verifier" }
        [PSCustomObject]@{ Script = "CS2-Optimize-GUI.ps1";    Marker = "SMOKE TEST OK: CS2-Optimize-GUI";    Flow = "GUI dashboard" }
    )

    function Invoke-EntrypointSmokeProcess {
        param(
            [Parameter(Mandatory)]
            [string]$ScriptName
        )

        $target = Join-Path $script:ProjectRoot $ScriptName
        if (-not (Test-Path $target)) {
            throw "Missing public entrypoint: $ScriptName"
        }

        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $script:PowerShellExe
        $startInfo.WorkingDirectory = $script:ProjectRoot
        $startInfo.UseShellExecute = $false
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $arguments = @(
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            $target,
            "-SmokeTest"
        )
        $startInfo.Arguments = ($arguments | ForEach-Object {
            if ($_ -match '[\s"]') {
                '"' + ($_ -replace '"', '\"') + '"'
            } else {
                $_
            }
        }) -join ' '

        $childProcess = [System.Diagnostics.Process]::Start($startInfo)
        $stdout = $childProcess.StandardOutput.ReadToEnd()
        $stderr = $childProcess.StandardError.ReadToEnd()
        $exited = $childProcess.WaitForExit($script:SmokeTimeoutMs)
        if (-not $exited) {
            $childProcess.Kill()
            throw "$ScriptName did not exit within $script:SmokeTimeoutMs ms"
        }

        [PSCustomObject]@{
            ExitCode = $childProcess.ExitCode
            Stdout   = $stdout
            Stderr   = $stderr
        }
    }
}

Describe "public entrypoints E2E smoke" {

    It "starts every shipped entrypoint as a real PowerShell process" {
        foreach ($entrypoint in $script:Entrypoints) {
            $result = Invoke-EntrypointSmokeProcess -ScriptName $entrypoint.Script

            $result.ExitCode | Should -Be 0 -Because "$($entrypoint.Flow) should start cleanly"
            $result.Stderr.Trim() | Should -Be "" -Because "$($entrypoint.Script) should not write stderr during smoke"
            $result.Stdout | Should -Match ([regex]::Escape($entrypoint.Marker))
        }
    }

    It "does not leave repo-local runtime state behind" {
        $runtimeArtifacts = @(
            "state.json",
            "progress.json",
            "backup.json",
            "backup.lock",
            "benchmark_history.json",
            "latency_history.json",
            "cs2_optimize.log",
            "Logs"
        )

        foreach ($artifact in $runtimeArtifacts) {
            Join-Path $script:ProjectRoot $artifact | Should -Not -Exist
        }
    }
}
