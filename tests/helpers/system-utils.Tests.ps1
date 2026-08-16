# ==============================================================================
#  tests/helpers/system-utils.Tests.ps1  --  JSON I/O, registry, boot config
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/../..").Path
    $script:AddedGetAclTestSeam = $false
    if (-not (Get-Command Get-Acl -ErrorAction SilentlyContinue)) {
        function script:Get-Acl {
            param($LiteralPath, $ErrorAction)
            throw 'Get-Acl requires a test double on this non-Windows host.'
        }
        $script:AddedGetAclTestSeam = $true
    }
}

AfterAll {
    if ($script:AddedGetAclTestSeam) {
        Remove-Item -Path Function:script:Get-Acl -Force -ErrorAction SilentlyContinue
    }
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Get-TrustedWindowsToolPath" {

    BeforeEach {
        Reset-TestState
        $script:TrustedSystemDirectory = Join-Path $SCRIPT:TestTempRoot "trusted-system"
        $script:HostilePathDirectory = Join-Path $SCRIPT:TestTempRoot "hostile-path"
        New-Item -ItemType Directory -Path $script:TrustedSystemDirectory -Force | Out-Null
        New-Item -ItemType Directory -Path $script:HostilePathDirectory -Force | Out-Null
        Remove-Item -LiteralPath (Join-Path $script:TrustedSystemDirectory "bcdedit.exe") -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath (Join-Path $script:HostilePathDirectory "bcdedit.exe") -Recurse -Force -ErrorAction SilentlyContinue
        Set-Content -LiteralPath (Join-Path $script:TrustedSystemDirectory "bcdedit.exe") -Value "trusted" -Encoding ASCII
        Set-Content -LiteralPath (Join-Path $script:HostilePathDirectory "bcdedit.exe") -Value "hostile" -Encoding ASCII
    }

    It "uses the supplied system directory even when PATH contains a hostile executable" {
        $originalPath = $env:PATH
        try {
            $env:PATH = "$script:HostilePathDirectory$([IO.Path]::PathSeparator)$originalPath"
            $resolved = Get-TrustedWindowsToolPath -Name bcdedit -SystemDirectory $script:TrustedSystemDirectory
        } finally {
            $env:PATH = $originalPath
        }

        $resolved | Should -Be ([IO.Path]::GetFullPath((Join-Path $script:TrustedSystemDirectory "bcdedit.exe")))
        $resolved | Should -Not -Be ([IO.Path]::GetFullPath((Join-Path $script:HostilePathDirectory "bcdedit.exe")))
    }

    It "rejects an unknown executable name before path resolution" {
        { Get-TrustedWindowsToolPath -Name "cmd" -SystemDirectory $script:TrustedSystemDirectory } |
            Should -Throw "*ValidateSet*"
    }

    It "rejects a directory at an allowlisted executable path" {
        Remove-Item -LiteralPath (Join-Path $script:TrustedSystemDirectory "bcdedit.exe") -Force
        New-Item -ItemType Directory -Path (Join-Path $script:TrustedSystemDirectory "bcdedit.exe") -Force | Out-Null

        { Get-TrustedWindowsToolPath -Name bcdedit -SystemDirectory $script:TrustedSystemDirectory } |
            Should -Throw "*regular, non-reparse file*"
    }

    It "uses an explicit test seam without requiring a Windows host" {
        Mock Test-HostIsWindows { $false }

        $resolved = Get-TrustedWindowsToolPath -Name bcdedit -SystemDirectory $script:TrustedSystemDirectory

        $resolved | Should -Be ([IO.Path]::GetFullPath((Join-Path $script:TrustedSystemDirectory "bcdedit.exe")))
    }
}

Describe "Assert-TrustedExistingControlFile" {

    BeforeEach {
        Reset-TestState
        $script:ControlFile = Join-Path $SCRIPT:TestTempRoot 'trusted-control.json'
        Set-Content -LiteralPath $script:ControlFile -Value '{}' -Encoding UTF8
    }

    It "accepts a regular file on the explicit non-Windows shape-only boundary" {
        Mock Test-HostIsWindows { $false }

        Assert-TrustedExistingControlFile -Path $script:ControlFile | Should -BeTrue
    }

    It "rejects a reparse-point leaf before ACL inspection" {
        Mock Get-Item {
            [PSCustomObject]@{
                PSProvider = [PSCustomObject]@{ Name = 'FileSystem' }
                PSIsContainer = $false
                Attributes = [IO.FileAttributes]::ReparsePoint
            }
        }

        { Assert-TrustedExistingControlFile -Path $script:ControlFile } | Should -Throw '*regular non-reparse*'
    }

    It "rejects a user-owned exact-DACL control file" {
        Mock Test-HostIsWindows { $true }
        Mock Test-SecureAcl { [PSCustomObject]@{ Valid = $false; Message = 'ACL owner is not BUILTIN\\Administrators or SYSTEM' } }

        { Assert-TrustedExistingControlFile -Path $script:ControlFile } | Should -Throw '*owner is not*'
    }

    It "rejects an untrusted write ACE on an existing control file" {
        Mock Test-HostIsWindows { $true }
        Mock Test-SecureAcl { [PSCustomObject]@{ Valid = $false; Message = 'ACL grants an untrusted principal write or ownership rights' } }

        { Assert-TrustedExistingControlFile -Path $script:ControlFile } | Should -Throw '*untrusted principal write*'
    }

    It "accepts a Windows control file only after trusted ACL proof" {
        Mock Test-HostIsWindows { $true }
        Mock Test-SecureAcl { [PSCustomObject]@{ Valid = $true; Message = 'protected' } }

        Assert-TrustedExistingControlFile -Path $script:ControlFile | Should -BeTrue
        Should -Invoke Test-SecureAcl -Exactly 1 -ParameterFilter { $Path -eq $script:ControlFile }
    }

    It "permits the Synchronize bit Windows adds to publisher ReadAndExecute" {
        Mock Test-HostIsWindows { $true }
        $publisherSid = 'S-1-5-21-1000'
        Mock Get-Acl {
            [PSCustomObject]@{
                Owner = 'BUILTIN\Administrators'
                AreAccessRulesProtected = $true
                Access = @(
                    [PSCustomObject]@{ IdentityReference = 'BUILTIN\Administrators'; AccessControlType = [Security.AccessControl.AccessControlType]::Allow; FileSystemRights = [Security.AccessControl.FileSystemRights]::FullControl },
                    [PSCustomObject]@{ IdentityReference = 'NT AUTHORITY\SYSTEM'; AccessControlType = [Security.AccessControl.AccessControlType]::Allow; FileSystemRights = [Security.AccessControl.FileSystemRights]::FullControl },
                    [PSCustomObject]@{ IdentityReference = $publisherSid; AccessControlType = [Security.AccessControl.AccessControlType]::Allow; FileSystemRights = ([Security.AccessControl.FileSystemRights]::ReadAndExecute -bor [Security.AccessControl.FileSystemRights]::Synchronize) }
                )
            }
        }

        (Test-SecureAcl -Path $script:ControlFile -PublisherSid $publisherSid).Valid | Should -BeTrue
    }
}

Describe 'Set-SecureAcl WhatIf boundary' {

    It 'does not write or validate when required ACL hardening is previewed' -Skip:(-not $IsWindows) {
        $fixturePath = Join-Path $SCRIPT:TestTempRoot 'whatif-acl-fixture.json'
        Set-Content -LiteralPath $fixturePath -Value '{}' -Encoding UTF8
        Mock Test-Path { $true }
        Mock Test-HostIsWindows { $true }
        Mock Set-Acl { throw 'Set-Acl must not run under WhatIf.' }
        Mock Test-SecureAcl { throw 'Test-SecureAcl must not run under WhatIf.' }

        { Set-SecureAcl -Path $fixturePath -Required -WhatIf } | Should -Not -Throw
        Should -Invoke Set-Acl -Exactly 0
        Should -Invoke Test-SecureAcl -Exactly 0
    }

    It 'keeps the WhatIf no-write and no-validation guard in the non-Windows source contract' -Skip:$IsWindows {
        $source = Get-Content -LiteralPath (Join-Path $script:ProjectRoot 'helpers/system-utils.ps1') -Raw

        $source | Should -Match '(?s)if \(-not \$PSCmdlet\.ShouldProcess\(\$TargetPath, "Apply restricted Administrators/SYSTEM ACL"\)\) \{ return \$false \}.*?Set-Acl'
        $source | Should -Match '(?s)\$applied = Set-SuiteDacl.*?if \(\$applied -eq \$false\) \{ return \}.*?\$proof = Test-SecureAcl'
    }
}

Describe "trusted inbox command compatibility wrappers" {

    It "routes existing bcdedit-shaped calls through the trusted invoker" {
        Mock Invoke-TrustedWindowsTool {
            param($Name, $Arguments)
            $global:LASTEXITCODE = 0
            "$Name|$($Arguments -join '|')"
        }

        $result = bcdedit /enum "{current}"

        $result | Should -Be "bcdedit|/enum|{current}"
    }
}

# ── Save-JsonAtomic ──────────────────────────────────────────────────────────
Describe "Save-JsonAtomic" {

    BeforeEach { Reset-TestState }

    It "writes valid JSON to a new file" {
        $path = "$SCRIPT:TestTempRoot\test-atomic.json"
        $data = @{ foo = "bar"; count = 42 }

        Save-JsonAtomic -Data $data -Path $path

        Test-Path $path | Should -Be $true
        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.foo   | Should -Be "bar"
        $loaded.count | Should -Be 42
    }

    It "overwrites an existing file" {
        $path = "$SCRIPT:TestTempRoot\test-overwrite.json"
        @{ old = "data" } | ConvertTo-Json | Set-Content $path -Encoding UTF8

        Save-JsonAtomic -Data @{ new = "data" } -Path $path

        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.new | Should -Be "data"
        # Old key should not be present
        $loaded.PSObject.Properties.Name | Should -Not -Contain "old"
    }

    It "creates parent directory if it does not exist" {
        $path = "$SCRIPT:TestTempRoot\deep\nested\dir\test.json"

        Save-JsonAtomic -Data @{ ok = $true } -Path $path

        Test-Path $path | Should -Be $true
    }

    It "cleans up .tmp file on failure and preserves existing target" {
        $path = "$SCRIPT:TestTempRoot\test-fail.json"
        $tmpPath = "$path.tmp"

        # Pre-create target file to verify it survives a failed write
        @{ original = "preserved" } | ConvertTo-Json | Set-Content $path -Encoding UTF8

        # Inject a commit failure after the temp file is durably written.
        Mock Invoke-AtomicJsonFileCommit { throw "Simulated disk full" }

        { Save-JsonAtomic -Data @{ x = 1 } -Path $path } | Should -Throw "*Save-JsonAtomic*"

        # .tmp file should be cleaned up
        Test-Path $tmpPath | Should -Be $false
        # Original file should be preserved (atomic guarantee - no partial write)
        Test-Path $path | Should -Be $true
        $preserved = Get-Content $path -Raw | ConvertFrom-Json
        $preserved.original | Should -Be "preserved"
    }

    It "uses atomic filesystem replacement rather than provider force-move" {
        $source = Get-Content -LiteralPath (Join-Path $PSScriptRoot "../../helpers/system-utils.ps1") -Raw

        $source | Should -Match '\[IO\.File\]::Replace\(\$TemporaryPath, \$DestinationPath, \$replacementBackup\)'
        $source | Should -Match '\[IO\.File\]::Move\(\$TemporaryPath, \$DestinationPath, \$true\)'
        $source | Should -Match '\[IO\.File\]::Move\(\$TemporaryPath, \$DestinationPath\)'
        $source | Should -Not -Match 'Move-Item \$tmp \$Path -Force'
    }

    It "preserves nested objects with default depth" {
        $path = "$SCRIPT:TestTempRoot\test-nested.json"
        $data = @{
            level1 = @{
                level2 = @{
                    level3 = @{
                        value = "deep"
                    }
                }
            }
        }

        Save-JsonAtomic -Data $data -Path $path -Depth 10

        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.level1.level2.level3.value | Should -Be "deep"
    }

    It "writes valid UTF-8 encoded content" {
        $path = Join-Path $SCRIPT:TestTempRoot "test-encoding.json"
        $data = @{ name = "test" }

        Save-JsonAtomic -Data $data -Path $path

        # Verify content is readable as UTF-8
        $content = [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
        $content | Should -Match '"name"'
    }
}

Describe "v3 legacy phase handoff guard" {

    BeforeEach { Reset-TestState }

    It "does nothing when no v2.3 handoff is armed" {
        Mock Test-HostIsWindows { $true }
        Mock Test-Path { $false }

        { Assert-NoLegacyPhaseHandoff } | Should -Not -Throw
    }

    It "fails closed when the v2.3 Safe Mode handoff is armed" {
        Mock Test-HostIsWindows { $true }
        Mock Test-Path { $true }
        Mock Get-ItemProperty {
            [PSCustomObject]@{ '*CS2_Phase2' = 'powershell.exe -File C:\CS2_OPTIMIZE\SafeMode-DriverClean.ps1' }
        }

        { Assert-NoLegacyPhaseHandoff } | Should -Throw '*v2.3 phase handoff is still armed*'
    }

    It "fails closed when legacy registry state cannot be verified" {
        Mock Test-HostIsWindows { $true }
        Mock Test-Path { throw "registry unavailable" }

        { Assert-NoLegacyPhaseHandoff } | Should -Throw '*Could not verify whether legacy phase handoff*'
    }
}

# ── Set-RegistryValue DRY-RUN ────────────────────────────────────────────────
Describe "Set-RegistryValue DRY-RUN" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true
        $SCRIPT:CurrentStepTitle = "Test Step"
        # Mock all functions that Set-RegistryValue might call
        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Backup-RegistryValue {
            [PSCustomObject]@{ Captured = $true; Entry = [PSCustomObject]@{}; Message = "captured" }
        }
        Mock Flush-BackupBuffer {}
        Mock Get-BackupDataRaw {
            [PSCustomObject]@{
                entries = @([PSCustomObject]@{
                    type = "registry"; step = $SCRIPT:CurrentStepTitle
                    path = "HKLM:\SOFTWARE\Test"; name = "TestValue"
                })
            }
        }
    }

    It "does not write to registry in DRY-RUN mode" {
        Mock Set-ItemProperty {}
        Mock New-Item {}

        Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason"

        # Set-ItemProperty should NOT be called
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke New-Item -Exactly 0
    }

    It "outputs DRY-RUN message with value details" {
        Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 42 "DWord" "Test reason"

        Should -Invoke Write-ConsoleLine -ParameterFilter {
            $Message -match "DRY-RUN" -and $Message -match "TestValue" -and $Message -match "42"
        }
    }

    It "does not enqueue a registry backup in DRY-RUN mode" {
        Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason"

        Should -Invoke Backup-RegistryValue -Exactly 0
    }
}

# ── Set-BootConfig DRY-RUN ───────────────────────────────────────────────────
Describe "Set-BootConfig DRY-RUN" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true
        $SCRIPT:CurrentStepTitle = "Test Step"
        Mock Write-ConsoleLine {}
        Mock Backup-BootConfig {}
    }

    It "does not execute bcdedit in DRY-RUN mode" {
        # We cannot easily mock bcdedit (external exe), but in DRY-RUN mode
        # the function returns before reaching the bcdedit call
        Mock Write-Step {}

        Set-BootConfig "disabledynamictick" "yes" "Test boot config"

        # Write-Step is called only in the non-DRY-RUN path
        Should -Invoke Write-Step -Exactly 0
    }

    It "outputs DRY-RUN message with key and value" {
        Set-BootConfig "disabledynamictick" "yes" "Disable dynamic tick"

        Should -Invoke Write-ConsoleLine -ParameterFilter {
            $Message -match "DRY-RUN" -and $Message -match "disabledynamictick" -and $Message -match "yes"
        }
    }

    It "does not enqueue a boot backup in DRY-RUN mode" {
        Set-BootConfig "disabledynamictick" "yes" "Test"

        Should -Invoke Backup-BootConfig -Exactly 0
    }
}

# ── Write helper result contracts ────────────────────────────────────────────
Describe "Write helper result contracts" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Result Contract Test"
        Mock Write-ConsoleLine {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-Step {}
        Mock Write-DebugLog {}
        Mock Backup-RegistryValue {
            [PSCustomObject]@{ Captured = $true; Entry = [PSCustomObject]@{}; Message = "captured" }
        }
        Mock Backup-BootConfig {
            [PSCustomObject]@{ Captured = $true; Entry = [PSCustomObject]@{}; Message = "captured" }
        }
        Mock Flush-BackupBuffer {}
        Mock Get-BackupDataRaw {
            [PSCustomObject]@{
                entries = @(
                    [PSCustomObject]@{
                        type = "registry"; step = $SCRIPT:CurrentStepTitle
                        path = "HKLM:\SOFTWARE\Test"; name = "TestValue"
                    }
                    [PSCustomObject]@{
                        type = "bootconfig"; step = $SCRIPT:CurrentStepTitle; key = "disabledynamictick"
                    }
                )
            }
        }
        Mock Ensure-SecureWorkDir {}
        Mock Set-SecureAcl {}
        Mock New-Item {}
        Mock Get-TrustedWindowsToolPath { "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" }
    }

    It "Set-RegistryValue returns success status with -PassThru after a write" {
        Mock Test-Path { $true }
        Mock Set-ItemProperty {}

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason" -PassThru

        $result.Status | Should -Be "Success"
        $result.Applied | Should -Be $true
        Should -Invoke Set-ItemProperty -Exactly 1
    }

    It "Set-RegistryValue returns failed status with -PassThru when the write throws" {
        Mock Test-Path { $true }
        Mock Set-ItemProperty { throw "denied" }

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "Registry write failed"
    }

    It "Set-RegistryValue blocks the write when backup capture fails" {
        Mock Backup-RegistryValue {
            [PSCustomObject]@{ Captured = $false; Entry = $null; Message = "registry read denied" }
        }
        Mock Set-ItemProperty {}

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Message | Should -Match "original value was not captured"
        Should -Invoke Flush-BackupBuffer -Exactly 0
        Should -Invoke Set-ItemProperty -Exactly 0
    }

    It "Set-RegistryValue blocks the write when backup persistence fails" {
        Mock Flush-BackupBuffer { throw "disk full" }
        Mock Set-ItemProperty {}

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Message | Should -Match "restore record was not persisted"
        Should -Invoke Set-ItemProperty -Exactly 0
    }

    It "Set-RegistryValue returns dry-run status without applying writes" {
        $SCRIPT:DryRun = $true
        Mock Test-Path { $true }
        Mock Set-ItemProperty {}

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason" -PassThru

        $result.Status | Should -Be "DryRun"
        $result.Applied | Should -Be $false
        Should -Invoke Set-ItemProperty -Exactly 0
    }

    It "Set-RegistryValue returns skipped status under WhatIf without applying writes" {
        Mock Test-Path { $true }
        Mock Set-ItemProperty {}
        Mock New-Item {}

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason" -PassThru -WhatIf

        $result.Status | Should -Be "Skipped"
        $result.Applied | Should -Be $false
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke New-Item -Exactly 0
        Should -Invoke Backup-RegistryValue -Exactly 0
    }

    It "Set-RegistryValue keeps default no-output behavior for existing callers" {
        Mock Test-Path { $true }
        Mock Set-ItemProperty {}

        $result = Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason"

        $result | Should -BeNullOrEmpty
    }

    It "Set-RunOnce returns success status with -PassThru after registration" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\SafeMode-DriverClean.ps1"
        Mock Test-Path { $true } -ParameterFilter { $Path -eq $generationPath }
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = 'verified' } }
        Mock Set-ItemProperty {}

        $result = Set-RunOnce "FRAMETIME_Phase2" $generationPath -SafeMode -PassThru

        $result.Status | Should -Be "Success"
        $result.Applied | Should -Be $true
        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Path -eq "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" -and
            $Name -eq "*!FRAMETIME_Phase2" -and
            $Value -match "-File" -and $Value -notmatch "-Command"
        }
    }

    It "Set-RunOnce accepts an immutable runtime generation target" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\SafeMode-DriverClean.ps1"
        Mock Test-Path { $true } -ParameterFilter { $Path -eq $generationPath }
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = 'verified' } }
        Mock Set-ItemProperty {}

        $result = Set-RunOnce "FRAMETIME_Phase2" $generationPath -SafeMode -PassThru

        $result.Status | Should -Be "Success"
        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Name -eq "*!FRAMETIME_Phase2" -and $Value -match [regex]::Escape($generationPath)
        }
    }

    It "restores publisher traverse immediately before the Safe Mode registry write" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\SafeMode-DriverClean.ps1"
        $script:TrustOrder = [System.Collections.Generic.List[string]]::new()
        Mock Test-HostIsWindows { $true }
        Mock Get-PhaseRuntimePublisherSid { 'S-1-5-21-1000-1000-1000-1001' }
        Mock Test-Path { $true }
        Mock Get-TrustedWindowsToolPath { 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' }
        Mock Set-PhaseRuntimePayloadAcl { $script:TrustOrder.Add('restore') | Out-Null }
        Mock Test-PhaseRuntimePayload { $script:TrustOrder.Add('validate') | Out-Null; [PSCustomObject]@{ Valid = $true; Message = 'verified' } }
        Mock Ensure-SecureWorkDir { $script:TrustOrder.Add('harden') | Out-Null }
        Mock Set-ItemProperty {}

        $result = Set-RunOnce 'FRAMETIME_Phase2' $generationPath -SafeMode -PassThru

        $result.Status | Should -Be 'Success'
        $script:TrustOrder | Should -Be @('harden', 'restore', 'validate')
        Should -Invoke Set-PhaseRuntimePayloadAcl -Exactly 1 -ParameterFilter {
            $Path -eq $CFG_WorkDir -and $PublisherSid -eq 'S-1-5-21-1000-1000-1000-1001' -and $NoInheritance
        }
    }

    It "Set-RunOnce returns failed status with -PassThru when registration throws" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\SafeMode-DriverClean.ps1"
        Mock Test-Path { $true } -ParameterFilter { $Path -eq $generationPath }
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = 'verified' } }
        Mock Set-ItemProperty { throw "access denied" }

        $result = Set-RunOnce "FRAMETIME_Phase2" $generationPath -SafeMode -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "Failed to register phase handoff"
    }

    It "Set-RunOnce returns dry-run status without applying writes" {
        $SCRIPT:DryRun = $true
        Mock Set-ItemProperty {}

        $result = Set-RunOnce "FRAMETIME_Phase3" "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\PostReboot-Setup.ps1" -PassThru

        $result.Status | Should -Be "DryRun"
        $result.Applied | Should -Be $false
        Should -Invoke Set-ItemProperty -Exactly 0
    }

    It "Set-RunOnce returns skipped status under WhatIf without registration" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\SafeMode-DriverClean.ps1"
        Mock Test-Path { $true } -ParameterFilter { $Path -eq $generationPath }
        Mock Set-ItemProperty {}
        Mock Ensure-SecureWorkDir {}
        Mock Set-SecureAcl {}
        Mock Set-PhaseRuntimePayloadAcl {}
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = 'verified' } }

        $result = Set-RunOnce "FRAMETIME_Phase2" $generationPath -SafeMode -PassThru -WhatIf

        $result.Status | Should -Be "Skipped"
        $result.Applied | Should -Be $false
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Set-SecureAcl -Exactly 0
        Should -Invoke Set-PhaseRuntimePayloadAcl -Exactly 0
        Should -Invoke Test-PhaseRuntimePayload -Exactly 0
    }

    It "Set-RunOnce keeps default no-output behavior for existing callers" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\PostReboot-Setup.ps1"
        Mock Test-Path { $true }
        Mock Get-Item {
            [PSCustomObject]@{
                PSProvider = [PSCustomObject]@{ Name = 'FileSystem' }
                PSIsContainer = $false
                Attributes = [IO.FileAttributes]::Normal
            }
        }
        Mock Get-TrustedWindowsToolPath { "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" }
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = "verified" } }
        Mock Set-SecureAcl {}
        Mock New-Item {}
        Mock Set-ItemProperty {}

        $result = Set-RunOnce "FRAMETIME_Phase3" $generationPath

        $result | Should -BeNullOrEmpty
    }

    It "stores the fixed normal-mode elevation bootstrap without an interpolated command wrapper" {
        $generationPath = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\PostReboot-Setup.ps1"
        Mock Test-Path { $true }
        Mock Get-Item {
            [PSCustomObject]@{
                PSProvider = [PSCustomObject]@{ Name = 'FileSystem' }
                PSIsContainer = $false
                Attributes = [IO.FileAttributes]::Normal
            }
        }
        Mock Get-TrustedWindowsToolPath { "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" }
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = "verified" } }
        Mock Set-SecureAcl {}
        Mock New-Item {}
        Mock Set-ItemProperty {}
        Mock Remove-ItemProperty {}

        $result = Set-RunOnce "FRAMETIME_Phase3" $generationPath -PassThru

        $result.Status | Should -Be "Success"
        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Value -match 'PhaseRuntime-ElevationBootstrap\.ps1' -and
            $Value -match '-File' -and
            $Value -notmatch '-Command' -and
            $Value -notmatch '-Target(?:ExecutionPolicy)?\b' -and
            $Value -match 'PostReboot-Setup\.ps1 Bypass$' -and
            $Value.Length -le 260
        }
        Should -Invoke Remove-ItemProperty -Exactly 0
    }

    It "removes the durable handoff only when completion requests cleanup" {
        $script:HandoffPresent = $true
        Mock Test-Path { $true } -ParameterFilter {
            $LiteralPath -eq "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
        }
        Mock Get-ItemProperty {
            $properties = [PSCustomObject]@{}
            if ($script:HandoffPresent) {
                $properties | Add-Member -NotePropertyName "FRAMETIME_CFG_FRAMETIME_Phase3" -NotePropertyValue "command"
            }
            $properties
        }
        Mock Remove-ItemProperty { $script:HandoffPresent = $false }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase3"

        $result | Should -BeNullOrEmpty
        Should -Invoke Remove-ItemProperty -Exactly 1 -ParameterFilter {
            $LiteralPath -eq "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" -and
            $Name -eq "FRAMETIME_CFG_FRAMETIME_Phase3"
        }
    }

    It "returns success without deletion when the handoff registry key is already absent" {
        Mock Test-Path { $false } -ParameterFilter {
            $LiteralPath -eq "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
        }
        Mock Get-ItemProperty { throw "must not query an absent key" }
        Mock Remove-ItemProperty { throw "must not delete an absent handoff" }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru

        $result.Status | Should -Be "Success"
        $result.Applied | Should -Be $true
        $result.Message | Should -Match "already absent"
        Should -Invoke Get-ItemProperty -Exactly 0
        Should -Invoke Remove-ItemProperty -Exactly 0
    }

    It "returns success without deletion when the handoff value is already absent" {
        Mock Test-Path { $true } -ParameterFilter {
            $LiteralPath -eq "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
        }
        Mock Get-ItemProperty { [PSCustomObject]@{ UnrelatedValue = "command" } }
        Mock Remove-ItemProperty { throw "must not delete an absent handoff" }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru

        $result.Status | Should -Be "Success"
        $result.Applied | Should -Be $true
        $result.Message | Should -Match "already absent"
        Should -Invoke Get-ItemProperty -Exactly 1
        Should -Invoke Remove-ItemProperty -Exactly 0
    }

    It "removes a Safe Mode handoff and verifies the value is absent afterward" {
        $script:HandoffPresent = $true
        Mock Test-Path { $true } -ParameterFilter {
            $LiteralPath -eq "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce"
        }
        Mock Get-ItemProperty {
            $properties = [PSCustomObject]@{}
            if ($script:HandoffPresent) {
                $properties | Add-Member -NotePropertyName "*!FRAMETIME_Phase2" -NotePropertyValue "command"
            }
            $properties
        }
        Mock Remove-ItemProperty { $script:HandoffPresent = $false }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase2" -SafeMode -PassThru

        $result.Status | Should -Be "Success"
        $result.Applied | Should -Be $true
        $result.Message | Should -Match "verified absent"
        Should -Invoke Get-ItemProperty -Exactly 2
        Should -Invoke Remove-ItemProperty -Exactly 1 -ParameterFilter {
            $LiteralPath -eq "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" -and
            $Name -eq "*!FRAMETIME_Phase2" -and $ErrorAction -eq "Stop"
        }
    }

    It "reports a registry query failure instead of treating it as absence" {
        Mock Test-Path { throw "registry provider unavailable" }
        Mock Get-ItemProperty { throw "must not query after key query failure" }
        Mock Remove-ItemProperty { throw "must not delete after query failure" }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "registry provider unavailable"
        Should -Invoke Get-ItemProperty -Exactly 0
        Should -Invoke Remove-ItemProperty -Exactly 0
    }

    It "reports a handoff value query failure instead of treating it as absence" {
        Mock Test-Path { $true }
        Mock Get-ItemProperty { throw "registry value query denied" }
        Mock Remove-ItemProperty { throw "must not delete after value query failure" }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "registry value query denied"
        Should -Invoke Get-ItemProperty -Exactly 1
        Should -Invoke Remove-ItemProperty -Exactly 0
    }

    It "reports a Safe Mode handoff removal failure instead of suppressing it" {
        Mock Test-Path { $true }
        Mock Get-ItemProperty {
            $properties = [PSCustomObject]@{}
            $properties | Add-Member -NotePropertyName "*!FRAMETIME_Phase2" -NotePropertyValue "command"
            $properties
        }
        Mock Remove-ItemProperty { throw "access denied" }

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase2" -SafeMode -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "Failed to remove phase handoff"
    }

    It "reports failure when the handoff remains present after deletion" {
        Mock Test-Path { $true }
        Mock Get-ItemProperty {
            [PSCustomObject]@{ FRAMETIME_CFG_FRAMETIME_Phase3 = "command" }
        }
        Mock Remove-ItemProperty {}

        $result = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "remains present after deletion"
        Should -Invoke Get-ItemProperty -Exactly 2
        Should -Invoke Remove-ItemProperty -Exactly 1
    }

    It "does not set safeboot when payload preparation fails" {
        Mock Copy-PhaseRuntimePayload { throw "copy failed" }
        Mock Set-RunOnce {}
        Mock Set-BootConfig {}

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile

        $result.Applied | Should -Be $false
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke Set-BootConfig -Exactly 0
    }

    It "does not publish, prepare, register, change BCD, or persist state under WhatIf" {
        Mock Ensure-SecureWorkDir {}
        Mock Copy-PhaseRuntimePayload { throw "must not publish" }
        Mock Set-RunOnce { throw "must not register" }
        Mock Set-BootConfig { throw "must not change BCD" }
        Mock Set-Phase1SafeModeReadyFlag { throw "must not persist state" }

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile -WhatIf

        $result.Status | Should -Be "Skipped"
        $result.Applied | Should -Be $false
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Copy-PhaseRuntimePayload -Exactly 0
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke Set-BootConfig -Exactly 0
        Should -Invoke Set-Phase1SafeModeReadyFlag -Exactly 0
    }

    It "does not set safeboot when Phase 2 RunOnce registration is not applied" {
        Mock Ensure-SecureWorkDir {}
        Mock Copy-PhaseRuntimePayload { $SCRIPT:TestTempRoot }
        Mock Set-RunOnce { [PSCustomObject]@{ Applied = $false; Message = "RunOnce failed" } }
        Mock Set-BootConfig {}

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile

        $result.Applied | Should -Be $false
        Should -Invoke Set-BootConfig -Exactly 0
    }

    It "disarms Phase 2 when the safeboot write fails" {
        $script:RollbackOrder = [System.Collections.Generic.List[string]]::new()
        Mock Ensure-SecureWorkDir {}
        Mock Copy-PhaseRuntimePayload { $SCRIPT:TestTempRoot }
        Mock Get-PhaseRuntimeRoot { $SCRIPT:TestTempRoot }
        Mock Set-RunOnce { $script:RollbackOrder.Add("arm"); [PSCustomObject]@{ Applied = $true; Message = "RunOnce set" } }
        Mock Set-BootConfig { $script:RollbackOrder.Add("bcd"); [PSCustomObject]@{ Applied = $false; Message = "bcdedit failed" } }
        Mock Clear-SafeBootVerified { $script:RollbackOrder.Add("clear"); [PSCustomObject]@{ Verified = $true; Message = "cleared" } }
        Mock Remove-PhaseHandoff { $script:RollbackOrder.Add("disarm"); [PSCustomObject]@{ Applied = $true; Message = "RunOnce removed" } }
        Mock Test-BootConfigSet { throw "must not verify a failed boot write" }

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile

        $result.Applied | Should -Be $false
        $result.SafeBootCleared | Should -Be $true
        $script:RollbackOrder | Should -Be @("arm", "bcd", "clear", "disarm")
        Should -Invoke Remove-PhaseHandoff -Exactly 1 -ParameterFilter { $Name -eq "FRAMETIME_Phase2" -and $SafeMode -and $PassThru }
        Should -Invoke Test-BootConfigSet -Exactly 0
    }

    It "retains and re-arms Phase 2 when safeboot rollback cannot be verified" {
        $script:RollbackOrder = [System.Collections.Generic.List[string]]::new()
        Mock Ensure-SecureWorkDir {}
        Mock Copy-PhaseRuntimePayload { $SCRIPT:TestTempRoot }
        Mock Get-PhaseRuntimeRoot { $SCRIPT:TestTempRoot }
        Mock Set-RunOnce { $script:RollbackOrder.Add("arm"); [PSCustomObject]@{ Applied = $true; Message = "RunOnce set" } }
        Mock Set-BootConfig { $script:RollbackOrder.Add("bcd"); [PSCustomObject]@{ Applied = $true; Message = "bcdedit set" } }
        Mock Test-BootConfigSet { $script:RollbackOrder.Add("verify"); $false }
        Mock Clear-SafeBootVerified { $script:RollbackOrder.Add("clear"); [PSCustomObject]@{ Verified = $false; Message = "still armed" } }
        Mock Remove-PhaseHandoff { throw "must not disarm while SafeBoot might remain" }

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile

        $result.Applied | Should -Be $false
        $result.SafeBootCleared | Should -Be $false
        $result.RecoveryHandoffApplied | Should -Be $true
        $result.Message | Should -Match "CRITICAL"
        $script:RollbackOrder | Should -Be @("arm", "bcd", "verify", "clear", "arm")
        Should -Invoke Remove-PhaseHandoff -Exactly 0
        Should -Invoke Set-RunOnce -Exactly 2
    }

    It "clears safeboot before disarming when readiness persistence fails" {
        $script:RollbackOrder = [System.Collections.Generic.List[string]]::new()
        Mock Ensure-SecureWorkDir {}
        Mock Copy-PhaseRuntimePayload { $SCRIPT:TestTempRoot }
        Mock Get-PhaseRuntimeRoot { $SCRIPT:TestTempRoot }
        Mock Set-RunOnce { [PSCustomObject]@{ Applied = $true; Message = "RunOnce set" } }
        Mock Set-BootConfig { [PSCustomObject]@{ Applied = $true; Message = "bcdedit set" } }
        Mock Test-BootConfigSet { $true }
        Mock Set-Phase1SafeModeReadyFlag { throw "state write failed" }
        Mock Clear-SafeBootVerified { $script:RollbackOrder.Add("clear"); [PSCustomObject]@{ Verified = $true; Message = "cleared" } }
        Mock Remove-PhaseHandoff { $script:RollbackOrder.Add("disarm"); [PSCustomObject]@{ Applied = $true; Message = "removed" } }

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile

        $result.Applied | Should -Be $false
        $result.SafeBootCleared | Should -Be $true
        $script:RollbackOrder | Should -Be @("clear", "disarm")
    }

    It "persists readiness only after payload, RunOnce, safeboot write, and live verification succeed" {
        $script:TransactionOrder = [System.Collections.Generic.List[string]]::new()
        Mock Ensure-SecureWorkDir { $script:TransactionOrder.Add("secure") }
        Mock Copy-PhaseRuntimePayload { $script:TransactionOrder.Add("copy"); $SCRIPT:TestTempRoot }
        Mock Set-RunOnce { $script:TransactionOrder.Add("runonce"); [PSCustomObject]@{ Applied = $true; Message = "RunOnce set" } }
        Mock Set-BootConfig { $script:TransactionOrder.Add("bcd"); [PSCustomObject]@{ Applied = $true; Message = "bcdedit set" } }
        Mock Test-BootConfigSet { $script:TransactionOrder.Add("verify"); $true }
        Mock Set-Phase1SafeModeReadyFlag { $script:TransactionOrder.Add("ready") }

        $result = Enable-Phase2SafeModeTransaction -SourceRoot "C:\source" -DestinationRoot $SCRIPT:TestTempRoot -StatePath $CFG_StateFile

        $result.Applied | Should -Be $true
        $script:TransactionOrder | Should -Be @("copy", "runonce", "bcd", "verify", "ready")
    }

    It "Set-BootConfig returns dry-run status with -PassThru without applying a boot write" {
        $SCRIPT:DryRun = $true
        Mock bcdedit { throw "should not be called" }

        $result = Set-BootConfig "disabledynamictick" "yes" "Test boot config" -PassThru

        $result.Status | Should -Be "DryRun"
        $result.Applied | Should -Be $false
        Should -Invoke bcdedit -Exactly 0
    }

    It "Set-BootConfig returns skipped status under WhatIf without applying a boot write" {
        Mock bcdedit { throw "should not be called" }

        $result = Set-BootConfig "disabledynamictick" "yes" "Test boot config" -PassThru -WhatIf

        $result.Status | Should -Be "Skipped"
        $result.Applied | Should -Be $false
        Should -Invoke bcdedit -Exactly 0
        Should -Invoke Backup-BootConfig -Exactly 0
    }

    It "Set-BootConfig returns failed status with -PassThru when bcdedit fails" {
        Mock bcdedit {
            if ($args -contains "/enum") {
                $global:LASTEXITCODE = 0
                return @("Windows Boot Loader", "identifier {current}")
            }
            $global:LASTEXITCODE = 1
            "failed"
        }

        $result = Set-BootConfig "disabledynamictick" "yes" "Test boot config" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Applied | Should -Be $false
        $result.Message | Should -Match "Boot config change failed"
    }

    It "Set-BootConfig returns success status with -PassThru when bcdedit succeeds" {
        Mock bcdedit {
            $global:LASTEXITCODE = 0
            "ok"
        }

        $result = Set-BootConfig "disabledynamictick" "yes" "Test boot config" -PassThru

        $result.Status | Should -Be "Success"
        $result.Applied | Should -Be $true
    }

    It "Set-BootConfig blocks the write when backup capture fails" {
        Mock Backup-BootConfig {
            [PSCustomObject]@{ Captured = $false; Entry = $null; Message = "BCD inventory failed" }
        }
        Mock bcdedit { throw "must not write" }

        $result = Set-BootConfig "disabledynamictick" "yes" "Test boot config" -PassThru

        $result.Status | Should -Be "Failed"
        $result.Message | Should -Match "original value was not captured"
        Should -Invoke bcdedit -Exactly 0
    }

    It "Set-BootConfig keeps the existing boolean contract without -PassThru" {
        $SCRIPT:DryRun = $true

        Set-BootConfig "disabledynamictick" "yes" "Test boot config" | Should -Be $true
    }
}

# ── Fail-closed SafeBoot removal ─────────────────────────────────────────────
Describe "Clear-SafeBootVerified" {

    BeforeEach {
        Reset-TestState
        $script:BcdDeleteExit = 0
        $script:BcdEnumExit = 0
        $script:BcdEnumOutput = "identifier {current}"
        Mock Invoke-BcdEditCaptured {
            if ($Arguments[0] -eq '/deletevalue') {
                return [PSCustomObject]@{
                    Output = "delete output"
                    ExitCode = $script:BcdDeleteExit
                }
            }
            return [PSCustomObject]@{
                Output = $script:BcdEnumOutput
                ExitCode = $script:BcdEnumExit
            }
        }
    }

    It "reports an applied verified success when deletion succeeds" {
        $result = Clear-SafeBootVerified

        $result.Status | Should -Be "Success"
        $result.Verified | Should -BeTrue
        $result.Applied | Should -BeTrue
        $result.DeleteExitCode | Should -Be 0
        $result.EnumExitCode | Should -Be 0
        Should -Invoke Invoke-BcdEditCaptured -Exactly 2
    }

    It "treats an already absent element as verified but not applied" {
        $script:BcdDeleteExit = 1

        $result = Clear-SafeBootVerified

        $result.Status | Should -Be "Success"
        $result.Verified | Should -BeTrue
        $result.Applied | Should -BeFalse
        $result.DeleteExitCode | Should -Be 1
        $result.EnumExitCode | Should -Be 0
    }

    It "fails closed when the raw SafeBoot element remains" {
        $script:BcdEnumOutput = "  0x26000081    0x1"

        $result = Clear-SafeBootVerified

        $result.Status | Should -Be "Failed"
        $result.Verified | Should -BeFalse
        $result.Applied | Should -BeTrue
        $result.DeleteExitCode | Should -Be 0
        $result.EnumExitCode | Should -Be 0
        $result.Message | Should -Match "0x26000081"
    }

    It "fails closed on enum failure and preserves both native exit codes" {
        $script:BcdDeleteExit = 7
        $script:BcdEnumExit = 31

        $result = Clear-SafeBootVerified

        $result.Status | Should -Be "Failed"
        $result.Verified | Should -BeFalse
        $result.Applied | Should -BeFalse
        $result.DeleteExitCode | Should -Be 7
        $result.EnumExitCode | Should -Be 31
        Should -Invoke Invoke-BcdEditCaptured -Exactly 1 -ParameterFilter { $Arguments -join ' ' -eq '/enum {current} /v' }
    }
}

# ── Initialize-VerifyCounters / Get-VerifyCounters ───────────────────────────
Describe "Initialize-VerifyCounters / Get-VerifyCounters" {

    BeforeEach { Reset-TestState }

    It "initializes all counters to zero" {
        Initialize-VerifyCounters

        $c = Get-VerifyCounters
        $c.okCount      | Should -Be 0
        $c.changedCount | Should -Be 0
        $c.missingCount | Should -Be 0
    }

    It "returns hashtable with correct keys" {
        Initialize-VerifyCounters

        $c = Get-VerifyCounters
        $c.Keys | Should -Contain "okCount"
        $c.Keys | Should -Contain "changedCount"
        $c.Keys | Should -Contain "missingCount"
    }

    It "resets counters when called again" {
        Initialize-VerifyCounters
        $Script:_verifyOkCount = 5
        $Script:_verifyChangedCount = 3

        Initialize-VerifyCounters
        $c = Get-VerifyCounters
        $c.okCount      | Should -Be 0
        $c.changedCount | Should -Be 0
    }
}

# ── Test-RegistryCheck ────────────────────────────────────────────────────────
Describe "Test-RegistryCheck" {

    BeforeEach {
        Reset-TestState
        Initialize-VerifyCounters
        Mock Write-ConsoleLine {}
    }

    Context "with -Quiet switch (returns structured result)" {

        It "returns OK when value matches expected" {
            # Use a real temp registry-like path via mocking
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 1 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "OK"
            $result.Value  | Should -Be 1
        }

        It "returns CHANGED when value differs from expected" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 0 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "CHANGED"
            $result.Value  | Should -Be 0
        }

        It "returns MISSING when key does not exist" {
            Mock Test-Path { $false } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "MISSING"
        }

        It "returns MISSING when value read throws" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty { throw "Access denied" } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "MISSING"
        }
    }

    Context "without -Quiet (updates global counters)" {

        It "increments okCount on match" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 1 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test"

            $c = Get-VerifyCounters
            $c.okCount | Should -Be 1
        }

        It "increments changedCount on mismatch" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 99 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test"

            $c = Get-VerifyCounters
            $c.changedCount | Should -Be 1
        }

        It "increments missingCount when key absent" {
            Mock Test-Path { $false } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }

            Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test"

            $c = Get-VerifyCounters
            $c.missingCount | Should -Be 1
        }
    }
}

# ── Load-State ────────────────────────────────────────────────────────────────
Describe "Get-ModeForProfile" {

    It "maps each profile to the runtime mode used by setup and GUI settings" {
        Get-ModeForProfile -Profile "SAFE"        | Should -Be "AUTO"
        Get-ModeForProfile -Profile "RECOMMENDED" | Should -Be "AUTO"
        Get-ModeForProfile -Profile "COMPETITIVE" | Should -Be "CONTROL"
        Get-ModeForProfile -Profile "CUSTOM"      | Should -Be "INFORMED"
        Get-ModeForProfile -Profile "YOLO"        | Should -Be "YOLO"
    }

    It "uses DRY-RUN mode as an explicit modifier independent of profile" {
        Get-ModeForProfile -Profile "SAFE" -DryRun | Should -Be "DRY-RUN"
    }
}

Describe "Load-State" {

    BeforeEach { Reset-TestState }

    It "round-trips state through file" {
        $state = [PSCustomObject]@{
            mode     = "DRY-RUN"
            logLevel = "VERBOSE"
            profile  = "COMPETITIVE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $loaded = Load-State $CFG_StateFile
        $SCRIPT:Mode     | Should -Be "DRY-RUN"
        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
        $SCRIPT:DryRun   | Should -Be $true
    }

    It "throws when state file is missing" {
        { Load-State "$SCRIPT:TestTempRoot\nonexistent.json" } | Should -Throw "*Settings file not found*"
    }

    It "sets DryRun to false for non-DRY-RUN mode" {
        $state = [PSCustomObject]@{ mode = "CONTROL"; profile = "SAFE" }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile | Out-Null
        $SCRIPT:DryRun | Should -Be $false
    }

    It "loads preview state read-only without hardening the parent directory" {
        $state = [PSCustomObject]@{ mode = "DRY-RUN"; profile = "CUSTOM"; logLevel = "VERBOSE" }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile
        Mock Ensure-SecureWorkDir { throw "Read-only load must not harden storage" }

        { Load-State -Path $CFG_StateFile -ReadOnly | Out-Null } | Should -Not -Throw

        $SCRIPT:DryRun | Should -BeTrue
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
    }

    It "does not preserve a corrupt state copy during read-only loading" {
        Set-Content -LiteralPath $CFG_StateFile -Value '{not-json' -Encoding UTF8
        Mock Ensure-SecureWorkDir { throw "Read-only load must not harden storage" }
        Mock Copy-Item { throw "Read-only load must not copy corrupt state" }

        { Load-State -Path $CFG_StateFile -ReadOnly } | Should -Throw "*read-only mode left it unchanged*"

        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Copy-Item -Exactly 0
    }

    It "does not mutate script runtime state under WhatIf" {
        $SCRIPT:Mode = "CONTROL"
        $SCRIPT:Profile = "RECOMMENDED"
        $SCRIPT:LogLevel = "NORMAL"
        $SCRIPT:DryRun = $false

        Set-ScriptStateFromStateObject -State ([PSCustomObject]@{
            mode = "DRY-RUN"
            profile = "COMPETITIVE"
            logLevel = "VERBOSE"
        }) -WhatIf

        $SCRIPT:Mode | Should -Be "CONTROL"
        $SCRIPT:Profile | Should -Be "RECOMMENDED"
        $SCRIPT:LogLevel | Should -Be "NORMAL"
        $SCRIPT:DryRun | Should -Be $false
    }

    It "defaults logLevel to NORMAL when absent" {
        $state = [PSCustomObject]@{ mode = "CONTROL"; profile = "SAFE" }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile | Out-Null
        $SCRIPT:LogLevel | Should -Be "NORMAL"
    }

    It "derives missing mode from the saved profile without discarding log level" {
        $state = [PSCustomObject]@{ profile = "SAFE"; logLevel = "VERBOSE" }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile | Out-Null

        $SCRIPT:Mode | Should -Be "AUTO"
        $SCRIPT:Profile | Should -Be "SAFE"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
        $SCRIPT:DryRun | Should -Be $false
    }

    It "defaults malformed fields independently" {
        $state = [PSCustomObject]@{
            mode = @{ bad = "value" }
            profile = "COMPETITIVE"
            logLevel = "VERBOSE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile | Out-Null

        $SCRIPT:Mode | Should -Be "CONTROL"
        $SCRIPT:Profile | Should -Be "COMPETITIVE"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
    }
}

# ── Initialize-ScriptDefaults ────────────────────────────────────────────────
Describe "Initialize-ScriptDefaults" {

    BeforeEach { Reset-TestState }

    It "loads from state file when present" {
        $state = [PSCustomObject]@{
            mode     = "DRY-RUN"
            logLevel = "VERBOSE"
            profile  = "COMPETITIVE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "DRY-RUN"
        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
        $SCRIPT:DryRun   | Should -Be $true
    }

    It "sets safe defaults when state file is absent" {
        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:Profile  | Should -Be "RECOMMENDED"
        $SCRIPT:DryRun   | Should -Be $false
    }

    It "sets safe defaults when state file is corrupted" {
        "this is not json" | Set-Content $CFG_StateFile -Encoding UTF8

        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:Profile  | Should -Be "RECOMMENDED"
    }

    It "derives a missing mode without downgrading the saved profile" {
        $state = [PSCustomObject]@{
            profile = "SAFE"
            logLevel = "VERBOSE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:Mode | Should -Be "AUTO"
        $SCRIPT:Profile | Should -Be "SAFE"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
        $SCRIPT:DryRun | Should -Be $false
    }

    It "preserves DRY-RUN mode when other fields are malformed or missing" {
        $state = [PSCustomObject]@{
            mode = "DRY-RUN"
            profile = @{ bad = "value" }
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:Mode | Should -Be "DRY-RUN"
        $SCRIPT:Profile | Should -Be "RECOMMENDED"
        $SCRIPT:LogLevel | Should -Be "NORMAL"
        $SCRIPT:DryRun | Should -Be $true
    }
}

Describe "Copy-PhaseRuntimePayload" {

    BeforeEach {
        Reset-TestState
        $script:PayloadSource = Join-Path $SCRIPT:TestTempRoot "payload-src"
        $script:PayloadDest = Join-Path $SCRIPT:TestTempRoot "payload-dest"
        Remove-Item -LiteralPath $script:PayloadSource -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $script:PayloadDest -Recurse -Force -ErrorAction SilentlyContinue
        foreach ($file in (Get-PhaseRuntimePayloadRelativePaths)) {
            $sourceFile = Join-Path $script:PayloadSource $file
            New-Item -ItemType Directory -Path (Split-Path $sourceFile -Parent) -Force | Out-Null
            Set-Content $sourceFile -Value "# $file" -Encoding UTF8
        }
        Mock Write-OK {}
    }

    It "publishes the exact hashed runtime payload without touching persistent data" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        Set-Content (Join-Path $script:PayloadDest "state.json") -Value '{"profile":"RECOMMENDED"}' -Encoding UTF8
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest

        $runtimeRoot | Should -Match ([regex]::Escape((Join-Path $script:PayloadDest "runtime-generations")) + '[\\/][a-f0-9]{32}$')
        (Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot).Valid | Should -Be $true
        (Get-PhaseRuntimeRoot -DestinationRoot $script:PayloadDest) | Should -Be $runtimeRoot
        Test-Path (Join-Path $runtimeRoot "runtime-manifest.json") | Should -Be $true
        Test-Path (Join-Path $script:PayloadDest "runtime-current.json") | Should -Be $true
        Test-Path (Join-Path $script:PayloadDest "state.json") | Should -Be $true
    }

    It "rejects a missing published runtime file" {
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        Remove-Item (Join-Path $runtimeRoot "helpers/logging.ps1") -Force

        $result = Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot

        $result.Valid | Should -Be $false
        $result.Message | Should -Match "missing or extra"
    }

    It "rejects an extra stale runtime file" {
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        Set-Content (Join-Path $runtimeRoot "helpers/stale.ps1") -Value "# stale" -Encoding UTF8

        $result = Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot

        $result.Valid | Should -Be $false
        $result.Message | Should -Match "missing or extra"
    }

    It "rejects a runtime hash mismatch" {
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        Add-Content (Join-Path $runtimeRoot "SafeMode-DriverClean.ps1") -Value "# tampered"

        $result = Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot

        $result.Valid | Should -Be $false
        $result.Message | Should -Match "hash mismatch"
    }

    It "preserves the previous verified publish when staging is interrupted" {
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        $originalHash = (Get-FileHash (Join-Path $runtimeRoot "SafeMode-DriverClean.ps1") -Algorithm SHA256).Hash
        Remove-Item (Join-Path $script:PayloadSource "helpers/logging.ps1") -Force

        { Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest } | Should -Throw "*Required runtime file missing*"

        (Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot).Valid | Should -Be $true
        (Get-FileHash (Join-Path $runtimeRoot "SafeMode-DriverClean.ps1") -Algorithm SHA256).Hash | Should -Be $originalHash
        @(Get-ChildItem $script:PayloadDest -Directory -Force | Where-Object Name -like '.runtime-staging-*').Count | Should -Be 0
    }

    It "keeps every previously armed generation present when a later publish commits" {
        $firstRuntime = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        Set-Content (Join-Path $script:PayloadSource "SafeMode-DriverClean.ps1") -Value "# generation two" -Encoding UTF8

        $secondRuntime = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest

        $secondRuntime | Should -Not -Be $firstRuntime
        Test-Path -LiteralPath $firstRuntime -PathType Container | Should -Be $true
        (Test-PhaseRuntimePayload -RuntimeRoot $firstRuntime).Valid | Should -Be $true
        (Test-PhaseRuntimePayload -RuntimeRoot $secondRuntime).Valid | Should -Be $true
        (Get-PhaseRuntimeRoot -DestinationRoot $script:PayloadDest) | Should -Be $secondRuntime
    }

    It "preserves the old pointer and armed target when pointer commit fails" {
        $firstRuntime = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        $script:AtomicJsonWriter = ${function:Save-JsonAtomic}
        Mock Save-JsonAtomic {
            param($Data, $Path, $Depth)
            if ((Split-Path -Path $Path -Leaf) -eq "runtime-current.json") {
                throw "injected pointer commit failure"
            }
            & $script:AtomicJsonWriter -Data $Data -Path $Path -Depth 10
        }

        { Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest } |
            Should -Throw "*injected pointer commit failure*"

        (Get-PhaseRuntimeRoot -DestinationRoot $script:PayloadDest) | Should -Be $firstRuntime
        Test-Path -LiteralPath $firstRuntime -PathType Container | Should -Be $true
        (Test-PhaseRuntimePayload -RuntimeRoot $firstRuntime).Valid | Should -Be $true
        @(Get-ChildItem (Join-Path $script:PayloadDest "runtime-generations") -Directory).Count | Should -Be 1
    }

    It "leaves the previous pointer untouched when final-generation validation fails" {
        $firstRuntime = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        $script:RuntimeValidator = ${function:Test-PhaseRuntimePayload}
        $script:RuntimeValidationCalls = 0
        Mock Test-PhaseRuntimePayload {
            param($RuntimeRoot)
            $script:RuntimeValidationCalls++
            if ($script:RuntimeValidationCalls -eq 2) {
                return [PSCustomObject]@{ Valid = $false; Status = "Failed"; Message = "injected final-generation validation failure" }
            }
            & $script:RuntimeValidator -RuntimeRoot $RuntimeRoot
        }

        { Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest } |
            Should -Throw "*injected final-generation validation failure*"

        (Get-PhaseRuntimeRoot -DestinationRoot $script:PayloadDest) | Should -Be $firstRuntime
        Test-Path -LiteralPath $firstRuntime -PathType Container | Should -Be $true
        @(Get-ChildItem (Join-Path $script:PayloadDest "runtime-generations") -Directory).Count | Should -Be 1
    }

    It "performs no filesystem or lock mutation under WhatIf" {
        $result = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest -WhatIf

        $result | Should -BeNullOrEmpty
        Test-Path -LiteralPath $script:PayloadDest | Should -Be $false
    }

    It "rejects an unsafe runtime pointer instead of traversing outside the work directory" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        [PSCustomObject]@{ schemaVersion = 1; relativePath = "../outside" } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:PayloadDest "runtime-current.json") -Encoding UTF8

        { Get-PhaseRuntimeRoot -DestinationRoot $script:PayloadDest } |
            Should -Throw "*Phase runtime pointer is invalid*"
    }

    It "keeps the newly verified generation when legacy cleanup fails" {
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest
        Set-Content (Join-Path $script:PayloadSource "SafeMode-DriverClean.ps1") -Value "# generation two" -Encoding UTF8
        Mock Remove-LegacyPhaseRuntimePayload { throw "cleanup blocked" }

        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest

        (Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot).Valid | Should -Be $true
        Get-Content (Join-Path $runtimeRoot "SafeMode-DriverClean.ps1") -Raw | Should -Match "generation two"
    }

    It "rejects a competing publisher without touching its staging paths" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        $lock = Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest
        try {
            { Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest } | Should -Throw "*publication is already in progress*"

            Test-Path (Join-Path $script:PayloadDest "runtime") | Should -Be $false
            @(Get-ChildItem $script:PayloadDest -Directory -Force | Where-Object Name -like '.runtime-staging-*').Count | Should -Be 0
        } finally {
            Exit-PhaseRuntimePublishLock -Lock $lock
        }
    }

    It "removes the exact newly created lock when owner-record write or flush fails" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        $lockPath = Join-Path $script:PayloadDest ".runtime-publish.lock"
        Mock Initialize-PhaseRuntimePublishLockOwner { throw "simulated owner-record write failure" }

        { Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest } |
            Should -Throw "*Could not initialize the Phase runtime publication lock*exact failed lock was removed*"

        Should -Invoke Initialize-PhaseRuntimePublishLockOwner -Exactly 1
        Test-Path -LiteralPath $lockPath | Should -Be $false
    }

    It "recovers a corrupt unlocked publication record" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        $lockPath = Join-Path $script:PayloadDest ".runtime-publish.lock"
        Set-Content -LiteralPath $lockPath -Value '{not-json' -Encoding UTF8

        $lock = Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest
        try {
            $lock.Stream.CanWrite | Should -Be $true
            $lock.Token | Should -Not -BeNullOrEmpty
        } finally {
            Exit-PhaseRuntimePublishLock -Lock $lock
        }

        Test-Path -LiteralPath $lockPath | Should -Be $false
    }

    It "never steals an active owner lock and remains available after that owner exits" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        $firstLock = Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest
        try {
            { Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest } |
                Should -Throw "*publication is already in progress*"
            $firstLock.Stream.CanWrite | Should -Be $true
        } finally {
            Exit-PhaseRuntimePublishLock -Lock $firstLock
        }

        $nextLock = Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest
        try {
            $nextLock.Token | Should -Not -Be $firstLock.Token
        } finally {
            Exit-PhaseRuntimePublishLock -Lock $nextLock
        }
    }

    It "reclaims an unlocked stale record when its live PID belongs to an older process instance" {
        New-Item -ItemType Directory -Path $script:PayloadDest -Force | Out-Null
        $lockPath = Join-Path $script:PayloadDest ".runtime-publish.lock"
        $currentProcess = Get-Process -Id $PID -ErrorAction Stop
        [PSCustomObject]@{
            pid = $PID
            processStartUtc = $currentProcess.StartTime.ToUniversalTime().AddHours(-1).ToString("o")
            processName = $currentProcess.ProcessName
            token = "stale"
            state = "owned"
        } | ConvertTo-Json -Compress | Set-Content -LiteralPath $lockPath -Encoding UTF8

        $lock = Enter-PhaseRuntimePublishLock -DestinationRoot $script:PayloadDest
        try {
            $lock.Token | Should -Not -Be "stale"
            Test-Path -LiteralPath $lockPath -PathType Leaf | Should -Be $true
        } finally {
            Exit-PhaseRuntimePublishLock -Lock $lock
        }

        Test-Path -LiteralPath $lockPath | Should -Be $false
    }
}

Describe "Phase 1 Safe Mode readiness marker" {

    BeforeEach {
        Reset-TestState
        Mock Set-SecureAcl {}
    }

    It "persists the Safe Mode readiness flag into state.json" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "AUTO"
        }) -Path $CFG_StateFile

        Set-Phase1SafeModeReadyFlag -Path $CFG_StateFile | Out-Null

        $saved = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json
        $saved.phase1SafeModeReady | Should -Be $true
    }

    It "does not persist the Safe Mode readiness flag under WhatIf" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "AUTO"
        }) -Path $CFG_StateFile

        $result = Set-Phase1SafeModeReadyFlag -Path $CFG_StateFile -WhatIf
        $saved = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json

        $result.phase1SafeModeReady | Should -Be $true
        $saved.PSObject.Properties.Name | Should -Not -Contain "phase1SafeModeReady"
    }

    It "detects the readiness marker only when explicitly set" {
        Test-Phase1SafeModeReady -State ([PSCustomObject]@{ profile = "RECOMMENDED" }) | Should -Be $false
        Test-Phase1SafeModeReady -State ([PSCustomObject]@{ phase1SafeModeReady = $true }) | Should -Be $true
    }
}
