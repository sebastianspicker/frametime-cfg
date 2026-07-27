# ==============================================================================
#  tests/phase1-preview-startup.Tests.ps1 -- Phase 1 preview persistence boundary
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path

    function Assert-Administrator {
        throw "Administrator validation must not run during a preview."
    }

    function Test-Administrator { return $false }

    function Get-PreviewTreeManifest {
        param([Parameter(Mandatory)][string]$Root)

        if (-not (Test-Path -LiteralPath $Root)) { return @() }
        $prefix = $Root.TrimEnd([char[]]@("\", "/")) + [System.IO.Path]::DirectorySeparatorChar
        return @(
            Get-ChildItem -LiteralPath $Root -Recurse -Force | ForEach-Object {
                $relative = $_.FullName.Substring($prefix.Length).Replace("\", "/")
                if ($_.PSIsContainer) {
                    "D|$relative"
                } else {
                    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
                    "F|$relative|$($_.Length)|$hash"
                }
            } | Sort-Object
        )
    }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Phase 1 preview startup persistence boundary" {
    BeforeEach {
        Reset-TestState
        $script:PreviewRoot = Join-Path $SCRIPT:TestTempRoot "phase1-preview"
        if (Test-Path -LiteralPath $script:PreviewRoot) {
            Remove-Item -LiteralPath $script:PreviewRoot -Recurse -Force
        }

        $CFG_WorkDir = $script:PreviewRoot
        $CFG_LogDir = Join-Path $CFG_WorkDir "Logs"
        $CFG_LogFile = Join-Path $CFG_LogDir "frametime_current.log"
        $CFG_StateFile = Join-Path $CFG_WorkDir "state.json"
        $CFG_ProgressFile = Join-Path $CFG_WorkDir "progress.json"
        $CFG_BackupFile = Join-Path $CFG_WorkDir "backup.json"
        $CFG_BackupLockFile = Join-Path $CFG_WorkDir "backup.lock"
        $CFG_LatencyHistoryFile = Join-Path $CFG_WorkDir "latency_history.json"

        $PHASE = 1
        $TOTAL_STEPS = 38
        $ScriptRoot = $script:ProjectRoot
        $SCRIPT:DryRun = $false
        $SCRIPT:LogPersistenceEnabled = $false
        $script:ReadAnswers = [System.Collections.Generic.Queue[string]]::new()
        @("D", "1", "4", "0") | ForEach-Object { $script:ReadAnswers.Enqueue($_) }

        Mock Read-Host {
            if ($script:ReadAnswers.Count -eq 0) { throw "Unexpected preview prompt: $Prompt" }
            return $script:ReadAnswers.Dequeue()
        }
        Mock Write-Host {}
        Mock Write-LogoBanner {}
        Mock Write-Banner {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Blank {}
        Mock Write-Section {}
        Mock Write-DebugLog {}
        Mock Test-SystemCompatibility {}
        Mock Complete-Step {}

        Mock Assert-Administrator {}
        Mock Test-Administrator { $false }
        Mock Assert-NoLegacyPhaseHandoff {}
        Mock Ensure-SecureWorkDir {}
        Mock Ensure-Dir {}
        Mock Initialize-Log {}
        Mock Initialize-Backup {}
        Mock Save-SuiteState {}
        Mock Set-BackupLock {}
        Mock Confirm-Risk { throw "Restore-point confirmation must not run during a preview." }
    }
    It "keeps an absent work directory absent and skips every persistence initializer" {
        . "$script:ProjectRoot/Setup-Profile.ps1"

        $SCRIPT:DryRun | Should -BeTrue
        $SCRIPT:Profile | Should -Be "SAFE"
        $startStep | Should -Be 1
        Test-Path -LiteralPath $script:PreviewRoot | Should -BeFalse
        $script:ReadAnswers.Count | Should -Be 0

        Should -Invoke Assert-Administrator -Exactly 0
        Should -Invoke Assert-NoLegacyPhaseHandoff -Exactly 0
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Ensure-Dir -Exactly 0
        Should -Invoke Initialize-Log -Exactly 0
        Should -Invoke Initialize-Backup -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
        Should -Invoke Set-BackupLock -Exactly 0
        Should -Invoke Confirm-Risk -Exactly 0
    }

    It "preserves a saved YOLO preview and every pre-existing suite artifact byte-for-byte" {
        New-Item -ItemType Directory -Path $CFG_LogDir -Force | Out-Null
        '{"profile":"YOLO","mode":"DRY-RUN"}' | Set-Content -LiteralPath $CFG_StateFile -Encoding UTF8
        '{"phase":1,"lastCompletedStep":12,"completedSteps":["P1:12"],"skippedSteps":[]}' | Set-Content -LiteralPath $CFG_ProgressFile -Encoding UTF8
        '{"entries":[{"type":"sentinel"}]}' | Set-Content -LiteralPath $CFG_BackupFile -Encoding UTF8
        '{"token":"sentinel","state":"owned"}' | Set-Content -LiteralPath $CFG_BackupLockFile -Encoding UTF8
        'preview-log-sentinel' | Set-Content -LiteralPath $CFG_LogFile -Encoding UTF8
        $before = Get-PreviewTreeManifest -Root $script:PreviewRoot

        . "$script:ProjectRoot/Setup-Profile.ps1"

        $SCRIPT:DryRun | Should -BeTrue
        $SCRIPT:Profile | Should -Be "SAFE"
        (Get-PreviewTreeManifest -Root $script:PreviewRoot) | Should -BeExactly $before
        Should -Invoke Assert-Administrator -Exactly 0
        Should -Invoke Initialize-Log -Exactly 0
        Should -Invoke Initialize-Backup -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
        Should -Invoke Confirm-Risk -Exactly 0
    }

    It "lets a non-elevated user choose preview despite a saved live YOLO profile" {
        New-Item -ItemType Directory -Path $CFG_WorkDir -Force | Out-Null
        '{"profile":"YOLO","mode":"YOLO"}' | Set-Content -LiteralPath $CFG_StateFile -Encoding UTF8
        $before = Get-PreviewTreeManifest -Root $script:PreviewRoot

        . "$script:ProjectRoot/Setup-Profile.ps1"

        $SCRIPT:DryRun | Should -BeTrue
        $SCRIPT:Profile | Should -Be "SAFE"
        (Get-PreviewTreeManifest -Root $script:PreviewRoot) | Should -BeExactly $before
        Should -Invoke Test-Administrator -Exactly 1
        Should -Invoke Assert-Administrator -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
    }

    It "continues into preview when the saved-state path is inaccessible" {
        Mock Test-Path {
            throw [System.UnauthorizedAccessException]::new("Access denied")
        } -ParameterFilter { $LiteralPath -eq $CFG_StateFile }

        . "$script:ProjectRoot/Setup-Profile.ps1"

        $SCRIPT:DryRun | Should -BeTrue
        $SCRIPT:Profile | Should -Be "SAFE"
        Test-Path -LiteralPath $script:PreviewRoot | Should -BeFalse
        Should -Invoke Test-Path -Exactly 2 -ParameterFilter { $LiteralPath -eq $CFG_StateFile }
        Should -Invoke Assert-Administrator -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
    }

    It "does not append to existing logs while inspecting corrupt saved state" {
        New-Item -ItemType Directory -Path $CFG_LogDir -Force | Out-Null
        'not-json' | Set-Content -LiteralPath $CFG_StateFile -Encoding UTF8
        'preview-log-sentinel' | Set-Content -LiteralPath $CFG_LogFile -Encoding UTF8
        $before = Get-PreviewTreeManifest -Root $script:PreviewRoot

        . "$script:ProjectRoot/Setup-Profile.ps1"

        (Get-PreviewTreeManifest -Root $script:PreviewRoot) | Should -BeExactly $before
        $SCRIPT:LogPersistenceEnabled | Should -BeFalse
        Should -Invoke Initialize-Log -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
    }
}
