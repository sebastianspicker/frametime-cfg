# ==============================================================================
#  tests/runtime-payload-bootstrap-parity.Tests.ps1
#  Adversarial parity for the self-contained Phase 2 and Phase 3 trust gates.
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"

    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:ValidatorModules = @{}
    foreach ($scriptName in @("SafeMode-DriverClean.ps1", "PostReboot-Setup.ps1")) {
        $scriptPath = Join-Path $script:ProjectRoot $scriptName
        $tokens = $null
        $parseErrors = $null
        $ast = [Management.Automation.Language.Parser]::ParseFile(
            $scriptPath,
            [ref]$tokens,
            [ref]$parseErrors
        )
        if ($parseErrors.Count -gt 0) {
            throw "$scriptName contains parse errors"
        }

        $definition = $ast.Find({
            param($node)
            $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
                $node.Name -eq "Test-PublishedRuntimePayloadBootstrap"
        }, $true)
        if (-not $definition) {
            throw "$scriptName does not define Test-PublishedRuntimePayloadBootstrap"
        }

        $script:ValidatorModules[$scriptName] = New-Module -ScriptBlock (
            [scriptblock]::Create($definition.Extent.Text)
        )
    }

    function New-ValidPublishedRuntimeFixture {
        $runtimeRoot = Join-Path $SCRIPT:TestTempRoot ([Guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Path $runtimeRoot -Force | Out-Null

        $manifestFiles = foreach ($relativePath in (Get-PhaseRuntimePayloadRelativePaths)) {
            $filePath = Join-Path $runtimeRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
            New-Item -ItemType Directory -Path (Split-Path $filePath -Parent) -Force | Out-Null
            Set-Content -LiteralPath $filePath -Value "fixture: $relativePath" -Encoding UTF8
            [PSCustomObject]@{
                path = $relativePath
                sha256 = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash
            }
        }

        [PSCustomObject]@{
            schemaVersion = 1
            payloadContract = Get-PhaseRuntimePayloadContractId
            files = @($manifestFiles)
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (
            Join-Path $runtimeRoot "runtime-manifest.json"
        ) -Encoding UTF8

        return $runtimeRoot
    }

    function Invoke-IsolatedBootstrapValidator {
        param(
            [Parameter(Mandatory)][string]$ScriptName,
            [Parameter(Mandatory)][string]$RuntimeRoot
        )

        $module = $script:ValidatorModules[$ScriptName]
        return & $module {
            param($FixtureRoot)
            Test-PublishedRuntimePayloadBootstrap -RuntimeRoot $FixtureRoot
        } $RuntimeRoot
    }
}

AfterAll {
    foreach ($module in $script:ValidatorModules.Values) {
        Remove-Module $module -Force -ErrorAction SilentlyContinue
    }
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "published runtime bootstrap parity" {
    It "returns identical fail-closed results for <Case>" -TestCases @(
        @{
            Case = "missing manifest"
            Mutate = { param($root) Remove-Item (Join-Path $root "runtime-manifest.json") -Force }
        }
        @{
            Case = "malformed manifest"
            Mutate = { param($root) Set-Content (Join-Path $root "runtime-manifest.json") "{not-json" }
        }
        @{
            Case = "unsupported schema"
            Mutate = {
                param($root)
                $path = Join-Path $root "runtime-manifest.json"
                $manifest = Get-Content $path -Raw | ConvertFrom-Json
                $manifest.schemaVersion = 2
                $manifest | ConvertTo-Json -Depth 5 | Set-Content $path
            }
        }
        @{
            Case = "empty file list"
            Mutate = {
                param($root)
                $path = Join-Path $root "runtime-manifest.json"
                $manifest = Get-Content $path -Raw | ConvertFrom-Json
                $manifest.files = @()
                $manifest | ConvertTo-Json -Depth 5 | Set-Content $path
            }
        }
        @{
            Case = "duplicate manifest path"
            Mutate = {
                param($root)
                $path = Join-Path $root "runtime-manifest.json"
                $manifest = Get-Content $path -Raw | ConvertFrom-Json
                $manifest.files = @($manifest.files) + @($manifest.files[0])
                $manifest | ConvertTo-Json -Depth 5 | Set-Content $path
            }
        }
        @{
            Case = "payload contract mismatch"
            Mutate = {
                param($root)
                $path = Join-Path $root "runtime-manifest.json"
                $manifest = Get-Content $path -Raw | ConvertFrom-Json
                $manifest.payloadContract = ("0" * 64)
                $manifest | ConvertTo-Json -Depth 5 | Set-Content $path
            }
        }
        @{
            Case = "unsafe manifest path"
            Mutate = {
                param($root)
                $path = Join-Path $root "runtime-manifest.json"
                $manifest = Get-Content $path -Raw | ConvertFrom-Json
                $manifest.files[0].path = "../escape.ps1"
                $manifest | ConvertTo-Json -Depth 5 | Set-Content $path
            }
        }
        @{
            Case = "missing runtime file"
            Mutate = { param($root) Remove-Item (Join-Path $root "helpers/logging.ps1") -Force }
        }
        @{
            Case = "extra runtime file"
            Mutate = { param($root) Set-Content (Join-Path $root "unexpected.ps1") "# extra" }
        }
        @{
            Case = "invalid manifest hash"
            Mutate = {
                param($root)
                $path = Join-Path $root "runtime-manifest.json"
                $manifest = Get-Content $path -Raw | ConvertFrom-Json
                $manifest.files[0].sha256 = "not-a-hash"
                $manifest | ConvertTo-Json -Depth 5 | Set-Content $path
            }
        }
        @{
            Case = "runtime hash mismatch"
            Mutate = { param($root) Add-Content (Join-Path $root "SafeMode-DriverClean.ps1") "# tampered" }
        }
    ) {
        param($Case, $Mutate)

        $runtimeRoot = New-ValidPublishedRuntimeFixture
        & $Mutate $runtimeRoot

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName "SafeMode-DriverClean.ps1" -RuntimeRoot $runtimeRoot
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName "PostReboot-Setup.ps1" -RuntimeRoot $runtimeRoot

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $phase2.Message | Should -BeExactly $phase3.Message
    }

    It "accepts the same complete verified payload in both phases" {
        $runtimeRoot = New-ValidPublishedRuntimeFixture

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName "SafeMode-DriverClean.ps1" -RuntimeRoot $runtimeRoot
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName "PostReboot-Setup.ps1" -RuntimeRoot $runtimeRoot

        $phase2.Valid | Should -BeTrue
        $phase3.Valid | Should -BeTrue
        $phase2.Message | Should -BeExactly $phase3.Message
    }
}
