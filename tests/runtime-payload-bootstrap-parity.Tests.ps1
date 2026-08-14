# ==============================================================================
#  tests/runtime-payload-bootstrap-parity.Tests.ps1
#  Adversarial parity for the self-contained Phase 2 and Phase 3 trust gates.
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"

    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:ValidatorModules = @{}
    foreach ($scriptName in @("SafeMode-DriverClean.ps1", "PostReboot-Setup.ps1", "PhaseRuntime-ElevationBootstrap.ps1")) {
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
        $runtimeRoot = "C:\FRAMETIME_CFG\runtime-generations\$([Guid]::NewGuid().ToString("N"))"
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
            publisherSid = 'S-1-5-21-1000-1000-1000-1001'
            files = @($manifestFiles)
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (
            Join-Path $runtimeRoot "runtime-manifest.json"
        ) -Encoding UTF8

        return $runtimeRoot
    }

    function Invoke-IsolatedBootstrapValidator {
        param(
            [Parameter(Mandatory)][string]$ScriptName,
            [Parameter(Mandatory)][string]$RuntimeRoot,
            [string]$ReparseDirectory,
            [string]$UntrustedOwnerPath,
            [string]$UntrustedWritePath
        )

        function Get-FixturePathAliases {
            param([string]$Path)

            if ([string]::IsNullOrWhiteSpace($Path)) { return @() }
            $aliases = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
            [void]$aliases.Add($Path)
            [void]$aliases.Add(($Path -replace '/', '\\').TrimEnd('\\'))
            try {
                $resolved = Resolve-Path -LiteralPath $Path -ErrorAction Stop
                [void]$aliases.Add($resolved.Path)
                [void]$aliases.Add($resolved.ProviderPath)
            } catch {
                # The validator will exercise its own missing-path failure.
                $null = $_
            }
            return @($aliases)
        }

        $unsafeDirectoryPaths = Get-FixturePathAliases -Path $ReparseDirectory
        $unsafeOwnerPaths = Get-FixturePathAliases -Path $UntrustedOwnerPath
        $unsafeWritePaths = Get-FixturePathAliases -Path $UntrustedWritePath
        $module = $script:ValidatorModules[$ScriptName]
        return & $module {
            param($FixtureRoot, $UnsafeDirectories, $UnsafeOwners, $UnsafeWrites)
            function Get-Item {
                param([string]$LiteralPath, [switch]$Force, $ErrorAction)
                [PSCustomObject]@{
                    PSProvider = [PSCustomObject]@{ Name = 'FileSystem' }
                    PSIsContainer = [string]::IsNullOrEmpty([IO.Path]::GetExtension($LiteralPath))
                    Attributes = if ($LiteralPath -in $UnsafeDirectories) {
                        [IO.FileAttributes]::Directory -bor [IO.FileAttributes]::ReparsePoint
                    } else {
                        [IO.FileAttributes]::Directory
                    }
                }
            }
            function Get-Acl {
                param([string]$LiteralPath, $ErrorAction)
                $rules = @(
                    [PSCustomObject]@{
                        IdentityReference = 'BUILTIN\Administrators'
                        AccessControlType = [Security.AccessControl.AccessControlType]::Allow
                        FileSystemRights = [Security.AccessControl.FileSystemRights]::FullControl
                    },
                    [PSCustomObject]@{
                        IdentityReference = 'NT AUTHORITY\SYSTEM'
                        AccessControlType = [Security.AccessControl.AccessControlType]::Allow
                        FileSystemRights = [Security.AccessControl.FileSystemRights]::FullControl
                    },
                    [PSCustomObject]@{
                        IdentityReference = 'S-1-5-21-1000-1000-1000-1001'
                        AccessControlType = [Security.AccessControl.AccessControlType]::Allow
                        FileSystemRights = ([Security.AccessControl.FileSystemRights]::ReadAndExecute -bor [Security.AccessControl.FileSystemRights]::Synchronize)
                    }
                )
                if ($LiteralPath -in $UnsafeWrites) {
                    $rules += [PSCustomObject]@{
                        IdentityReference = 'S-1-5-21-1000'
                        AccessControlType = [Security.AccessControl.AccessControlType]::Allow
                        FileSystemRights = [Security.AccessControl.FileSystemRights]::WriteData
                    }
                }
                [PSCustomObject]@{
                    Owner = if ($LiteralPath -in $UnsafeOwners) { 'S-1-5-21-1000' } else { 'BUILTIN\Administrators' }
                    AreAccessRulesProtected = $true
                    Access = $rules
                }
            }
            Test-PublishedRuntimePayloadBootstrap -RuntimeRoot $FixtureRoot
        } $RuntimeRoot $unsafeDirectoryPaths $unsafeOwnerPaths $unsafeWritePaths
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
    BeforeEach {
        New-PSDrive -Name C -PSProvider FileSystem -Root $SCRIPT:TestTempRoot -Scope Global | Out-Null
    }

    AfterEach {
        Remove-PSDrive -Name C -Scope Global -Force -ErrorAction SilentlyContinue
    }

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
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName "PhaseRuntime-ElevationBootstrap.ps1" -RuntimeRoot $runtimeRoot

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $elevationBootstrap.Valid | Should -BeFalse
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }

    It "accepts the same complete verified payload in both phases" {
        $runtimeRoot = New-ValidPublishedRuntimeFixture

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName "SafeMode-DriverClean.ps1" -RuntimeRoot $runtimeRoot
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName "PostReboot-Setup.ps1" -RuntimeRoot $runtimeRoot
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName "PhaseRuntime-ElevationBootstrap.ps1" -RuntimeRoot $runtimeRoot

        $phase2.Valid | Should -BeTrue -Because $phase2.Message
        $phase3.Valid | Should -BeTrue -Because $phase3.Message
        $elevationBootstrap.Valid | Should -BeTrue -Because $elevationBootstrap.Message
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }

    It "rejects the same reparse directory in every protected path position" -TestCases @(
        @{ Directory = 'C:\FRAMETIME_CFG' }
        @{ Directory = 'C:\FRAMETIME_CFG\runtime-generations' }
        @{ Directory = 'generation' }
        @{ Directory = 'nested generation directory' }
    ) {
        param($Directory)
        $runtimeRoot = New-ValidPublishedRuntimeFixture
        $unsafeDirectory = if ($Directory -eq 'generation') { $runtimeRoot } elseif ($Directory -eq 'nested generation directory') { Join-Path $runtimeRoot 'helpers' } else { $Directory }

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName "SafeMode-DriverClean.ps1" -RuntimeRoot $runtimeRoot -ReparseDirectory $unsafeDirectory
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName "PostReboot-Setup.ps1" -RuntimeRoot $runtimeRoot -ReparseDirectory $unsafeDirectory
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName "PhaseRuntime-ElevationBootstrap.ps1" -RuntimeRoot $runtimeRoot -ReparseDirectory $unsafeDirectory

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $elevationBootstrap.Valid | Should -BeFalse
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }

    It "rejects an otherwise exact trusted DACL when <Case> remains user-owned" -TestCases @(
        @{ Case = 'the fixed runtime root'; Target = { param($root) 'C:\FRAMETIME_CFG' } }
        @{ Case = 'the generations ancestor'; Target = { param($root) 'C:\FRAMETIME_CFG\runtime-generations' } }
        @{ Case = 'the selected generation'; Target = { param($root) $root } }
        @{ Case = 'the manifest'; Target = { param($root) Join-Path $root 'runtime-manifest.json' } }
        @{ Case = 'a payload file'; Target = { param($root) Join-Path $root 'SafeMode-DriverClean.ps1' } }
    ) {
        param($Case, $Target)
        $runtimeRoot = New-ValidPublishedRuntimeFixture
        $userOwnedPath = & $Target $runtimeRoot

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName 'SafeMode-DriverClean.ps1' -RuntimeRoot $runtimeRoot -UntrustedOwnerPath $userOwnedPath
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName 'PostReboot-Setup.ps1' -RuntimeRoot $runtimeRoot -UntrustedOwnerPath $userOwnedPath
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName 'PhaseRuntime-ElevationBootstrap.ps1' -RuntimeRoot $runtimeRoot -UntrustedOwnerPath $userOwnedPath

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $elevationBootstrap.Valid | Should -BeFalse
        $phase2.Message | Should -Match 'owner is not'
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }

    It "rejects an untrusted write grant on every protected runtime object" -TestCases @(
        @{ Target = { param($root) 'C:\FRAMETIME_CFG' } }
        @{ Target = { param($root) 'C:\FRAMETIME_CFG\runtime-generations' } }
        @{ Target = { param($root) $root } }
        @{ Target = { param($root) Join-Path $root 'helpers' } }
        @{ Target = { param($root) Join-Path $root 'runtime-manifest.json' } }
        @{ Target = { param($root) Join-Path $root 'SafeMode-DriverClean.ps1' } }
    ) {
        param($Target)
        $runtimeRoot = New-ValidPublishedRuntimeFixture
        $unsafePath = & $Target $runtimeRoot

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName 'SafeMode-DriverClean.ps1' -RuntimeRoot $runtimeRoot -UntrustedWritePath $unsafePath
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName 'PostReboot-Setup.ps1' -RuntimeRoot $runtimeRoot -UntrustedWritePath $unsafePath
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName 'PhaseRuntime-ElevationBootstrap.ps1' -RuntimeRoot $runtimeRoot -UntrustedWritePath $unsafePath

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $elevationBootstrap.Valid | Should -BeFalse
        $phase2.Message | Should -Match 'grants an untrusted principal write or ownership rights'
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }

    It "rejects an otherwise exact trusted DACL when a nested payload directory remains user-owned" {
        $runtimeRoot = New-ValidPublishedRuntimeFixture
        $userOwnedDirectory = Join-Path $runtimeRoot 'helpers'

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName 'SafeMode-DriverClean.ps1' -RuntimeRoot $runtimeRoot -UntrustedOwnerPath $userOwnedDirectory
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName 'PostReboot-Setup.ps1' -RuntimeRoot $runtimeRoot -UntrustedOwnerPath $userOwnedDirectory
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName 'PhaseRuntime-ElevationBootstrap.ps1' -RuntimeRoot $runtimeRoot -UntrustedOwnerPath $userOwnedDirectory

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $elevationBootstrap.Valid | Should -BeFalse
        $phase2.Message | Should -Match 'owner is not'
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }

    It 'rejects a payload whose manifest publisher SID is not the current user' {
        $runtimeRoot = New-ValidPublishedRuntimeFixture
        $manifestPath = Join-Path $runtimeRoot 'runtime-manifest.json'
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        $manifest.publisherSid = 'S-1-5-21-1000-1000-1000-1002'
        $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

        $phase2 = Invoke-IsolatedBootstrapValidator -ScriptName 'SafeMode-DriverClean.ps1' -RuntimeRoot $runtimeRoot
        $phase3 = Invoke-IsolatedBootstrapValidator -ScriptName 'PostReboot-Setup.ps1' -RuntimeRoot $runtimeRoot
        $elevationBootstrap = Invoke-IsolatedBootstrapValidator -ScriptName 'PhaseRuntime-ElevationBootstrap.ps1' -RuntimeRoot $runtimeRoot

        $phase2.Valid | Should -BeFalse
        $phase3.Valid | Should -BeFalse
        $elevationBootstrap.Valid | Should -BeFalse
        $phase2.Message | Should -Match 'publisher does not match the current user'
        $phase2.Message | Should -BeExactly $phase3.Message
        $phase2.Message | Should -BeExactly $elevationBootstrap.Message
    }
}

Describe "protected runtime publisher traverse lifecycle" {

    It "restores publisher traverse only after live state loading in <ScriptName>" -TestCases @(
        @{ ScriptName = 'SafeMode-DriverClean.ps1' }
        @{ ScriptName = 'PostReboot-Setup.ps1' }
    ) {
        param($ScriptName)
        $source = Get-Content -LiteralPath (Join-Path $script:ProjectRoot $ScriptName) -Raw
        $liveLoadBlock = [regex]::Match(
            $source,
            'if \(-not \$SCRIPT:DryRun\) \{\s*\$state = Load-State -Path \$CFG_StateFile\s*Restore-ProtectedRuntimePublisherTraverse -RuntimeRoot \$PSScriptRoot\s*\}',
            [Text.RegularExpressions.RegexOptions]::Singleline
        )

        $liveLoadBlock.Success | Should -BeTrue
        $source | Should -Not -Match 'Load-State -Path \$CFG_StateFile -ReadOnly\s*Restore-ProtectedRuntimePublisherTraverse'
    }
}
