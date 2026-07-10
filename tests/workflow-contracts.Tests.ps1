# ==============================================================================
#  tests/workflow-contracts.Tests.ps1  --  CI/workflow contract coverage
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:LintWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/lint.yml") -Raw
    $script:SecurityWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/security.yml") -Raw
    $script:RepositorySettings = Get-Content (Join-Path $script:ProjectRoot ".github/REPO_SETTINGS.md") -Raw
    $script:SmokeEntrypointsScript = Get-Content (Join-Path $script:ProjectRoot ".github/scripts/smoke-entrypoints.ps1") -Raw
    $script:PesterInstaller = Get-Content (Join-Path $script:ProjectRoot ".github/scripts/install-pester.ps1") -Raw
    $script:PSScriptAnalyzerRunner = Get-Content (Join-Path $script:ProjectRoot ".github/scripts/run-psscriptanalyzer.ps1") -Raw
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "lint workflow contract" {

    It "defines a Windows PowerShell 5.1 compatibility lane" {
        $script:LintWorkflow | Should -Match 'windows-powershell-compat:'
        $script:LintWorkflow | Should -Match 'shell:\s+powershell'
    }

    It "targets the protected main branch" {
        $script:LintWorkflow | Should -Match 'branches:\s+\[main\]'
        $script:SecurityWorkflow | Should -Match 'branches:\s+\[main\]'
    }

    It "smoke-tests the shipped entrypoints" {
        foreach ($scriptName in @(
            'Run-Optimize.ps1',
            'Cleanup.ps1',
            'Boot-SafeMode.ps1',
            'SafeMode-DriverClean.ps1',
            'PostReboot-Setup.ps1',
            'FpsCap-Calculator.ps1',
            'Verify-Settings.ps1',
            'CS2-Optimize-GUI.ps1'
        )) {
            $escaped = [regex]::Escape($scriptName)
            $script:SmokeEntrypointsScript | Should -Match $escaped
        }
    }

    It "fails smoke jobs when entrypoints emit PowerShell error records" {
        $script:LintWorkflow | Should -Match 'smoke-entrypoints\.ps1'
        $script:SmokeEntrypointsScript | Should -Match '\$errorRecords = @\(\$records \| Where-Object \{ \$_ -is \[System\.Management\.Automation\.ErrorRecord\] \}\)'
        $script:SmokeEntrypointsScript | Should -Match 'Smoke test emitted error records'
    }

    It "asserts launcher targets exposed by START.bat and START-GUI.bat" {
        $script:LintWorkflow | Should -Match 'Verify launcher contracts'
        foreach ($target in @(
            'Run-Optimize.ps1',
            'Cleanup.ps1',
            'FpsCap-Calculator.ps1',
            'Verify-Settings.ps1',
            'Boot-SafeMode.ps1',
            'PostReboot-Setup.ps1',
            'CS2-Optimize-GUI.ps1'
        )) {
            $escaped = [regex]::Escape($target)
            $script:SmokeEntrypointsScript | Should -Match $escaped
        }
    }

    It "runs the process-level E2E suite in CI" {
        $script:LintWorkflow | Should -Match 'e2e:'
        $script:LintWorkflow | Should -Match 'Invoke-Pester -Path \./tests/e2e -CI'
    }

    It "pins and verifies the exact Pester and PSScriptAnalyzer versions" {
        $script:PesterInstaller | Should -Match "\[version\]'5\.7\.1'"
        $script:PesterInstaller | Should -Match 'Install-Module -Name Pester -RequiredVersion'
        $script:PesterInstaller | Should -Match 'Import-Module -Name Pester -RequiredVersion'
        $script:PSScriptAnalyzerRunner | Should -Match "\[version\]'1\.24\.0'"
        $script:PSScriptAnalyzerRunner | Should -Match 'Install-Module -Name PSScriptAnalyzer -RequiredVersion'
        $script:PSScriptAnalyzerRunner | Should -Match 'Import-Module -Name PSScriptAnalyzer -RequiredVersion'
    }

    It "runs the full Pester suite on Windows and macOS with unique artifacts" {
        $script:LintWorkflow | Should -Match 'pester:\s*\r?\n\s+name: Pester tests\s*\r?\n\s+runs-on: windows-latest\s*\r?\n\s+timeout-minutes: 10'
        $script:LintWorkflow | Should -Match 'pester-macos:\s*\r?\n\s+name: Pester tests \(macOS\)\s*\r?\n\s+runs-on: macos-latest\s*\r?\n\s+timeout-minutes: 10'
        $script:LintWorkflow | Should -Match 'name: pester-test-results-windows'
        $script:LintWorkflow | Should -Match 'name: pester-test-results-macos'
        $script:LintWorkflow | Should -Match 'pester-5\.7\.1-\$\{\{ runner\.os \}\}'
        $script:LintWorkflow | Should -Match 'psscriptanalyzer-1\.24\.0-\$\{\{ runner\.os \}\}'
    }

    It "covers documentation changes and tracked EstimateKey references" {
        $script:LintWorkflow | Should -Not -Match '!docs/archive/\*\*'
        $script:LintWorkflow | Should -Not -Match '!docs/agent/\*\*'
        $script:LintWorkflow | Should -Not -Match '\(docs/archive\|docs/agent\|vendor'
    }

    It "documents exactly the required branch-protection checks" {
        $expectedChecks = @(
            'PSScriptAnalyzer',
            'Verify syntax (parse check)',
            'Windows PowerShell 5.1 compatibility',
            'Pester tests',
            'Pester tests (macOS)',
            'EstimateKey cross-reference',
            'E2E process smoke',
            'Entry point smoke tests',
            'Secret & credential detection',
            'PowerShell safety patterns',
            'Workflow file integrity'
        )
        $requiredChecksLine = ($script:RepositorySettings -split '\r?\n' | Where-Object { $_ -match 'Required checks:' })
        $documentedChecks = [regex]::Matches($requiredChecksLine, '`([^`]+)`') | ForEach-Object Value | ForEach-Object { $_.Trim('`') }
        $documentedChecks | Should -BeExactly $expectedChecks
    }
}

Describe "security workflow contract" {

    It "extends secret scanning to public batch launchers" {
        $script:SecurityWorkflow.Contains("--include='*.bat'") | Should -Be $true
        $script:SecurityWorkflow.Contains("--include='*.cmd'") | Should -Be $true
    }

    It "contains a dedicated launcher safety check for the public batch entrypoints" {
        $script:SecurityWorkflow | Should -Match 'Check public launcher scripts'
        $script:SecurityWorkflow | Should -Match 'START\.bat START-GUI\.bat'
    }

    It "pins START-GUI.bat to the trusted GUI script target" {
        $script:SecurityWorkflow | Should -Match 'CS2-Optimize-GUI\.ps1'
        $script:SecurityWorkflow | Should -Match 'START-GUI\.bat no longer launches CS2-Optimize-GUI\.ps1'
    }
}
