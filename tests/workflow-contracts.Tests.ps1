# ==============================================================================
#  tests/workflow-contracts.Tests.ps1  --  CI/workflow contract coverage
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:LintWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/lint.yml") -Raw
    $script:SecurityWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/security.yml") -Raw
    $script:RustWorkflow = Get-Content (Join-Path $script:ProjectRoot ".github/workflows/rust.yml") -Raw
    $script:RepositorySettings = Get-Content (Join-Path $script:ProjectRoot ".github/REPO_SETTINGS.md") -Raw
    $script:Dependabot = Get-Content (Join-Path $script:ProjectRoot ".github/dependabot.yml") -Raw
    $script:SmokeEntrypointsScript = Get-Content (Join-Path $script:ProjectRoot ".github/scripts/smoke-entrypoints.ps1") -Raw
    $script:SyntaxVerifier = Get-Content (Join-Path $script:ProjectRoot ".github/scripts/verify-syntax.ps1") -Raw
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

    It "keeps the syntax verifier compatible with Windows PowerShell 5.1 and fail-fast" {
        $script:SyntaxVerifier | Should -Not -Match 'Path\]::GetRelativePath'
        $script:SyntaxVerifier | Should -Match '\$ErrorActionPreference\s*=\s*"Stop"'
        $script:SyntaxVerifier | Should -Match 'StringComparison\]::OrdinalIgnoreCase'
    }

    It "targets the protected main branch" {
        $script:LintWorkflow | Should -Match 'branches:\s+\[main\]'
        $script:SecurityWorkflow | Should -Match 'branches:\s+\[main\]'
    }

    It "runs dedicated dependency-free checks for demo changes" {
        [regex]::Matches($script:LintWorkflow, "'demo/\*\*'").Count | Should -Be 2
        $script:LintWorkflow | Should -Match 'demo-checks:'
        $script:LintWorkflow | Should -Match 'name: Browser demonstration checks'
        $script:LintWorkflow | Should -Match 'node --check demo/app\.js'
        $script:LintWorkflow | Should -Match 'node --test demo/demo\.test\.mjs'
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
            'frametime-gui.ps1'
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

    It "smoke-checks public scripts independently of the fail-closed launchers" {
        $script:LintWorkflow | Should -Match 'Verify launcher contracts'
        foreach ($target in @(
            'Run-Optimize.ps1',
            'Cleanup.ps1',
            'FpsCap-Calculator.ps1',
            'Verify-Settings.ps1',
            'Boot-SafeMode.ps1',
            'PostReboot-Setup.ps1',
            'frametime-gui.ps1'
        )) {
            $escaped = [regex]::Escape($target)
            $script:SmokeEntrypointsScript | Should -Match $escaped
        }
    }

    It "enforces fail-closed portable launchers and absolute preview PowerShell" {
        $launcherVerifier = Get-Content (Join-Path $script:ProjectRoot ".github/scripts/verify-launcher-contracts.ps1") -Raw

        $launcherVerifier | Should -Match 'DisableDelayedExpansion'
        $launcherVerifier | Should -Match 'must not elevate'
        $launcherVerifier | Should -Match 'portable live-execution guard'
        $launcherVerifier | Should -Match 'Launcher-Action\.ps1'
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
        $script:LintWorkflow | Should -Match 'pester:\s*\r?\n\s+name: Pester tests\s*\r?\n\s+runs-on: windows-latest\s*\r?\n\s+timeout-minutes: 75'
        $script:LintWorkflow | Should -Match 'pester-macos:\s*\r?\n\s+name: Pester tests \(macOS\)\s*\r?\n\s+runs-on: macos-latest\s*\r?\n\s+timeout-minutes: 75'
        $script:LintWorkflow | Should -Match 'name: pester-test-results-windows'
        $script:LintWorkflow | Should -Match 'name: pester-test-results-macos'
        $script:LintWorkflow | Should -Match 'pester-5\.7\.1-\$\{\{ runner\.os \}\}'
        $script:LintWorkflow | Should -Match 'psscriptanalyzer-1\.24\.0-\$\{\{ runner\.os \}\}'
    }

    It "covers documentation changes and tracked EstimateKey references" {
        $script:LintWorkflow | Should -Not -Match '!docs/archive/\*\*'
        $script:LintWorkflow | Should -Not -Match '\(docs/archive\|vendor'
        $script:LintWorkflow | Should -Match 'git grep -n -E'
        $script:LintWorkflow | Should -Match "grep -v 'tests/workflow-contracts.Tests.ps1'"
        $script:LintWorkflow | Should -Match "grep -v '.github/REPO_SETTINGS.md'"
    }

    It "documents exactly the required branch-protection checks" {
        $expectedChecks = @(
            'Browser demonstration checks',
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
            'Workflow file integrity',
            'Frametime Rust host gates',
            'Frametime Rust Windows gates',
            'Northclock source gates',
            'Driver Foundry source gates',
            'Driver Foundry Windows tests'
        )
        $requiredChecksLine = ($script:RepositorySettings -split '\r?\n' | Where-Object { $_ -match 'Required checks:' })
        $documentedChecks = [regex]::Matches($requiredChecksLine, '`([^`]+)`') | ForEach-Object Value | ForEach-Object { $_.Trim('`') }
        $documentedChecks | Should -BeExactly $expectedChecks
    }
}

Describe "native Rust workflow contract" {

    It "runs read-only native validation for main and pull requests" {
        $script:RustWorkflow | Should -Match 'name:\s+Native Rust validation'
        $script:RustWorkflow | Should -Match 'permissions:\s*\r?\n\s+contents:\s+read'
        [regex]::Matches($script:RustWorkflow, 'branches:\s+\[main\]').Count | Should -Be 2
        $script:RustWorkflow | Should -Not -Match 'upload-artifact'
        $script:RustWorkflow | Should -Not -Match 'unsupported action preview'
    }

    It "pins every third-party action to an immutable revision" {
        $actionLines = @($script:RustWorkflow -split '\r?\n' | Where-Object { $_ -match '^\s+- uses:' })
        $actionLines.Count | Should -BeGreaterThan 0
        foreach ($line in $actionLines) {
            $line | Should -Match '@[0-9a-f]{40}(?:\s|$)'
        }
    }

    It "runs the strict Frametime host and fail-closed Windows package gates" {
        $script:RustWorkflow | Should -Match 'name:\s+Frametime Rust host gates'
        $script:RustWorkflow | Should -Match 'name:\s+Frametime Rust Windows unsigned package gates'
        $script:RustWorkflow | Should -Match 'toolchain:\s+1\.96\.0'
        $script:RustWorkflow | Should -Match 'cargo fmt --all -- --check'
        $script:RustWorkflow | Should -Match 'cargo clippy --workspace --all-targets --all-features --locked'
        $script:RustWorkflow | Should -Match 'clippy::too_many_lines'
        $script:RustWorkflow | Should -Match 'scripts\\verify\.cmd'
        $script:RustWorkflow | Should -Match 'cargo build --release -p frametime-cli -p frametime-gui --locked --target x86_64-pc-windows-msvc'
        $script:RustWorkflow | Should -Match 'call scripts\\package\.cmd /unsigned'
        $script:RustWorkflow | Should -Match 'call scripts\\package\.cmd /verify /unsigned'
        $script:RustWorkflow | Should -Not -Match 'cargo build --release --workspace --all-features --locked --target x86_64-pc-windows-msvc'
        $script:RustWorkflow | Should -Match 'dist\\frametime-cfg-rust\\frametime\.exe --help'
    }

    It "validates both nested native source workspaces" {
        $script:RustWorkflow | Should -Match 'name:\s+Northclock source gates'
        $script:RustWorkflow | Should -Match 'working-directory:\s+rust/northclock'
        $script:RustWorkflow | Should -Match 'CARGO_TARGET_DIR:\s+\$\{\{ runner\.temp \}\}/northclock-target'
        $script:RustWorkflow | Should -Match 'cargo run -p xtask --locked -- hygiene'
        $script:RustWorkflow | Should -Match 'cargo deny check all'
        $script:RustWorkflow | Should -Match 'cargo audit --file Cargo.lock --deny warnings'
        $script:RustWorkflow | Should -Match 'manifest-path driver/Cargo\.toml'
        $script:RustWorkflow | Should -Match 'cargo audit --file fuzz/Cargo\.lock --deny warnings'
        $script:RustWorkflow | Should -Match 'cargo audit --file driver/Cargo\.lock --deny warnings'
        $script:RustWorkflow | Should -Match 'name:\s+Driver Foundry source gates'
        $script:RustWorkflow | Should -Match 'working-directory:\s+rust/driver-foundry'
        $script:RustWorkflow | Should -Match 'cargo audit --deny warnings'
        $script:RustWorkflow | Should -Match 'name:\s+Driver Foundry Windows tests'
    }

    It "keeps the canonical Windows verifier locked to committed dependencies" {
        $verifier = Get-Content (Join-Path $script:ProjectRoot "rust/scripts/verify.cmd") -Raw
        foreach ($command in @('cargo clippy', 'cargo test', 'cargo check')) {
            $matchingLines = @($verifier -split '\r?\n' | Where-Object { $_ -match [regex]::Escape($command) })
            $matchingLines.Count | Should -BeGreaterThan 0
            foreach ($line in $matchingLines) {
                $line | Should -Match '--locked'
            }
        }
    }

    It "configures weekly dependency updates for all five Cargo locks" {
        foreach ($directory in @(
            '/rust',
            '/rust/northclock',
            '/rust/northclock/driver',
            '/rust/northclock/fuzz',
            '/rust/driver-foundry'
        )) {
            $script:Dependabot | Should -Match ([regex]::Escape("directory: `"$directory`""))
        }
    }
}

Describe "security workflow contract" {

    It "extends secret scanning to public batch launchers" {
        $script:SecurityWorkflow.Contains("--include='*.bat'") | Should -Be $true
        $script:SecurityWorkflow.Contains("--include='*.cmd'") | Should -Be $true
    }

    It "scans native Rust source and dependency manifests for credentials" {
        $script:SecurityWorkflow.Contains("--include='*.rs'") | Should -Be $true
        $script:SecurityWorkflow.Contains("--include='*.toml'") | Should -Be $true
        $script:SecurityWorkflow.Contains("--include='Cargo.lock'") | Should -Be $true
    }

    It "contains a dedicated launcher safety check for the public batch entrypoints" {
        $script:SecurityWorkflow | Should -Match 'Check public launcher scripts'
        $script:SecurityWorkflow | Should -Match 'START\.bat START-GUI\.bat'
    }

    It "pins START-GUI.bat to the fail-closed portable boundary" {
        $script:SecurityWorkflow | Should -Match 'portable WPF launcher is unavailable'
        $script:SecurityWorkflow | Should -Match 'must not start PowerShell'
    }
}
