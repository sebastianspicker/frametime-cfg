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
        [PSCustomObject]@{ Script = "frametime-gui.ps1";    Marker = "SMOKE TEST OK: frametime-gui";    Flow = "GUI dashboard" }
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

    function Get-DryRunTreeManifest {
        param([Parameter(Mandatory)][string]$Root)

        if (-not (Test-Path -LiteralPath $Root)) { return @("__ABSENT__") }
        $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
        return @(
            "__PRESENT__"
            Get-ChildItem -LiteralPath $rootPath -Recurse -Force -ErrorAction Stop | ForEach-Object {
                $relative = $_.FullName.Substring($rootPath.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
                if ($_.PSIsContainer) {
                    "D|$relative"
                } else {
                    "F|$relative|$($_.Length)|$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash)"
                }
            } | Sort-Object
        )
    }

    function Invoke-DryRunProcess {
        param(
            [Parameter(Mandatory)][string]$ScriptName,
            [string[]]$ExtraArguments = @(),
            [hashtable]$ExtraEnvironment = @{},
            [int]$TimeoutMs = 120000
        )

        $startInfo = [Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $script:PowerShellExe
        $startInfo.WorkingDirectory = $script:ProjectRoot
        $startInfo.UseShellExecute = $false
        $startInfo.RedirectStandardInput = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        foreach ($entry in $ExtraEnvironment.GetEnumerator()) {
            $startInfo.EnvironmentVariables[[string]$entry.Key] = [string]$entry.Value
        }
        $arguments = @(
            "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
            "-File", (Join-Path $script:ProjectRoot $ScriptName)
        ) + $ExtraArguments
        $startInfo.Arguments = ($arguments | ForEach-Object {
            if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
        }) -join ' '

        $childProcess = [Diagnostics.Process]::Start($startInfo)
        $childProcess.StandardInput.Close()
        $stdoutTask = $childProcess.StandardOutput.ReadToEndAsync()
        $stderrTask = $childProcess.StandardError.ReadToEndAsync()
        if (-not $childProcess.WaitForExit($TimeoutMs)) {
            $childProcess.Kill()
            throw "$ScriptName DRY-RUN did not exit within $TimeoutMs ms"
        }

        [PSCustomObject]@{
            ExitCode = $childProcess.ExitCode
            Stdout = $stdoutTask.GetAwaiter().GetResult()
            Stderr = $stderrTask.GetAwaiter().GetResult()
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
            "frametime.log",
            "Logs"
        )

        foreach ($artifact in $runtimeArtifacts) {
            Join-Path $script:ProjectRoot $artifact | Should -Not -Exist
        }
    }
}

Describe "public full DRY-RUN E2E" {

    It "routes START.bat dry-run before its administrator gate" {
        $launcher = Get-Content -LiteralPath (Join-Path $script:ProjectRoot "START.bat") -Raw
        $dryRoute = $launcher.IndexOf('if /i "%~1"=="dry-run"', [StringComparison]::OrdinalIgnoreCase)
        $adminGate = $launcher.IndexOf('net session', [StringComparison]::OrdinalIgnoreCase)

        $dryRoute | Should -BeGreaterOrEqual 0
        $adminGate | Should -BeGreaterThan $dryRoute
        $launcher.Substring($dryRoute, [Math]::Min(180, $launcher.Length - $dryRoute)) | Should -Match '(?i)goto\s+:fulldryrun'
        $launcher | Should -Match '(?im)^:fulldryrun\s*$'
        $launcher | Should -Match '(?im)Run-Optimize\.ps1" -FullDryRun -DryRunGpu'
        $launcher | Should -Match '(?i)-NonInteractive'
        $launcher | Should -Match '(?im)^:fulldryrunall\s*$'
        $launcher | Should -Match '(?i)for\s+%%G\s+in\s+\(1\s+2\s+3\s+4\)'
        $launcher | Should -Not -Match '(?<!\r)\n' -Because "cmd.exe launchers require CRLF line endings"
    }

    It "rejects a GPU preview selector without FullDryRun" {
        $workDir = "C:\FRAMETIME_CFG"
        $before = Get-DryRunTreeManifest -Root $workDir

        $result = Invoke-DryRunProcess -ScriptName "Run-Optimize.ps1" `
            -ExtraArguments @("-DryRunGpu", "3") -TimeoutMs 30000

        $result.ExitCode | Should -Not -Be 0
        ($result.Stdout + $result.Stderr) | Should -Match "DryRunGpu is only valid with -FullDryRun"
        (Get-DryRunTreeManifest -Root $workDir) | Should -BeExactly $before
    }

    It "previews every phase and representative feature without persistent changes or prompts" {
        $workDir = "C:\FRAMETIME_CFG"
        $before = Get-DryRunTreeManifest -Root $workDir

        $result = Invoke-DryRunProcess -ScriptName "Run-Optimize.ps1" -ExtraArguments @("-FullDryRun", "-DryRunGpu", "2")

        $result.ExitCode | Should -Be 0
        $result.Stderr.Trim() | Should -Be ""
        $result.Stdout | Should -Match "PHASE 1 PREVIEW COMPLETE"
        $result.Stdout | Should -Match "Simulated reboot into Safe Mode"
        $result.Stdout | Should -Match "PHASE 2 PREVIEW COMPLETE"
        $result.Stdout | Should -Match "Simulated reboot into Normal Mode"
        $result.Stdout | Should -Match "PHASE 3 PREVIEW COMPLETE"
        $result.Stdout | Should -Match "ALL 3 PHASES PREVIEW COMPLETE"
        $result.Stdout | Should -Match "Would publish and verify the immutable Phase 2/3 runtime payload"
        $result.Stdout | Should -Match "Would remove matching NVIDIA display-driver packages and selected rebuildable residue"
        $result.Stdout | Should -Match "Would install NVIDIA driver \(component-selective\)"
        $result.Stdout | Should -Match "Would remove leftover NVIDIA AppX"
        $result.Stdout | Should -Match "Would set DRS:"
        $result.Stdout | Should -Match "Would evaluate NVIDIA Reflex"
        $result.Stdout | Should -Match "Would set DNS to Cloudflare"
        $result.Stdout | Should -Match "Would capture and parse FPSHeaven"
        $result.Stdout | Should -Not -Match '(?m)^[ \t]*(?:True|False)[ \t]*\r?$' -Because "internal step results should not leak into public output"
        foreach ($phase1Step in 2..38) {
            $result.Stdout | Should -Match "Step $phase1Step\s+[^A-Za-z0-9]" -Because "Phase 1 Step $phase1Step should be exercised"
        }
        $result.Stdout | Should -Not -Match "FATAL ERROR|preview issue \(DRY-RUN\)"
        $result.Stdout | Should -Not -Match "Profile \[1/2/3/4/5/D\]|Continue in DRY-RUN mode|Proceed with GPU driver removal|Install now\?|Restart now\?|Press Enter to exit"
        (Get-DryRunTreeManifest -Root $workDir) | Should -BeExactly $before
    }

    It "fails closed when the host is genuinely in Safe Mode" {
        $workDir = "C:\FRAMETIME_CFG"
        $before = Get-DryRunTreeManifest -Root $workDir

        $result = Invoke-DryRunProcess -ScriptName "Run-Optimize.ps1" `
            -ExtraArguments @("-FullDryRun", "-DryRunGpu", "2") `
            -ExtraEnvironment @{ SAFEBOOT_OPTION = "MINIMAL" } `
            -TimeoutMs 30000

        $result.ExitCode | Should -Not -Be 0
        ($result.Stdout + $result.Stderr) | Should -Match "DRY-RUN must be launched from Normal Mode"
        ($result.Stdout + $result.Stderr) | Should -Not -Match "Would remove matching NVIDIA display-driver packages|PHASE 2 PREVIEW COMPLETE"
        (Get-DryRunTreeManifest -Root $workDir) | Should -BeExactly $before
    }

    It "previews the Safe Mode shortcut without elevation or boot mutations" {
        $workDir = "C:\FRAMETIME_CFG"
        $before = Get-DryRunTreeManifest -Root $workDir

        $result = Invoke-DryRunProcess -ScriptName "Boot-SafeMode.ps1" -ExtraArguments @("-DryRun") -TimeoutMs 30000

        $result.ExitCode | Should -Be 0
        $result.Stderr.Trim() | Should -Be ""
        $result.Stdout | Should -Match "SAFE MODE SHORTCUT PREVIEW COMPLETE"
        $result.Stdout | Should -Match "Would set and verify: bcdedit /set safeboot minimal"
        $result.Stdout | Should -Match "Would restart into Safe Mode"
        (Get-DryRunTreeManifest -Root $workDir) | Should -BeExactly $before
    }
}
