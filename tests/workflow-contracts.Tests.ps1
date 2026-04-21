# ==============================================================================
#  tests/workflow-contracts.Tests.ps1  --  CI/workflow contract coverage
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:LintWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/lint.yml") -Raw
    $script:SecurityWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/security.yml") -Raw
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

    It "smoke-tests the shipped Safe Mode and verifier entrypoints" {
        foreach ($scriptName in @(
            'Boot-SafeMode.ps1',
            'Verify-Settings.ps1'
        )) {
            $escaped = [regex]::Escape($scriptName)
            $script:LintWorkflow | Should -Match $escaped
        }
    }

    It "fails smoke jobs when entrypoints emit PowerShell error records" {
        $script:LintWorkflow | Should -Match '\$errorRecords = @\(\$records \| Where-Object \{ \$_ -is \[System\.Management\.Automation\.ErrorRecord\] \}\)'
        $script:LintWorkflow | Should -Match 'Smoke test emitted error records'
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
            $script:LintWorkflow | Should -Match $escaped
        }
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
