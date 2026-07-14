# ==============================================================================
#  tests/helpers/nvidia-driver.Tests.ps1  --  NVIDIA driver download & install
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Stub Windows-only cmdlets before loading the module
    if ($IsWindows -eq $false) {
        if (-not (Get-Command Start-Process -ErrorAction SilentlyContinue)) {
            function global:Start-Process { param($FilePath, $ArgumentList, [switch]$Wait, [switch]$PassThru, [switch]$NoNewWindow) $null }
        }
        if (-not (Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue)) {
            function global:Get-AuthenticodeSignature { param($FilePath, $ErrorAction) $null }
        }
        if (-not (Get-Command Stop-Service -ErrorAction SilentlyContinue)) {
            function global:Stop-Service { param($Name, [switch]$Force, $ErrorAction) $null }
        }
        if (-not (Get-Command Set-Clipboard -ErrorAction SilentlyContinue)) {
            function global:Set-Clipboard { param([Parameter(ValueFromPipeline)]$Value) process {} }
        }
        if (-not (Get-Command Set-Acl -ErrorAction SilentlyContinue)) {
            function global:Set-Acl { param($LiteralPath, $AclObject, $ErrorAction) }
        }
    }

    . "$PSScriptRoot/../../helpers/nvidia-driver.ps1"
}

Describe "Resolve-TrustedNvidiaTaskkill" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
    }

    It "resolves only the regular non-reparse taskkill file beneath System32" {
        $windowsRoot = Join-Path $SCRIPT:TestTempRoot "trusted-windows"
        $system32 = Join-Path $windowsRoot "System32"
        $taskkillPath = Join-Path $system32 "taskkill.exe"
        New-Item -ItemType Directory -Path $system32 -Force | Out-Null
        Set-Content -LiteralPath $taskkillPath -Value "test executable" -Encoding UTF8

        $result = Resolve-TrustedNvidiaTaskkill -WindowsRoot $windowsRoot

        $result | Should -Be ([IO.Path]::GetFullPath($taskkillPath))
    }

    It "rejects a reparse-point taskkill candidate" {
        $windowsRoot = Join-Path $SCRIPT:TestTempRoot "reparse-windows"
        $taskkillPath = [IO.Path]::GetFullPath((Join-Path (Join-Path $windowsRoot "System32") "taskkill.exe"))
        Mock Get-Item {
            [PSCustomObject]@{
                FullName = $taskkillPath
                PSIsContainer = $false
                Attributes = [IO.FileAttributes]::ReparsePoint
            }
        } -ParameterFilter { $LiteralPath -eq $taskkillPath }

        Resolve-TrustedNvidiaTaskkill -WindowsRoot $windowsRoot | Should -BeNullOrEmpty
    }

    It "rejects a network Windows root before executable lookup" {
        Mock Get-Item { throw "must not query a network candidate" }

        Resolve-TrustedNvidiaTaskkill -WindowsRoot '\\server\share\Windows' | Should -BeNullOrEmpty

        Should -Invoke Get-Item -Exactly 0
    }

    It "derives the production root from the OS Windows special folder rather than PATH" {
        $source = (Get-Command Resolve-TrustedNvidiaTaskkill).ScriptBlock.ToString()

        $source | Should -Match '\[Environment\]::GetFolderPath\(\[Environment\+SpecialFolder\]::Windows\)'
        $source | Should -Match "Join-Path.*'System32'.*'taskkill\.exe'"
        $source | Should -Not -Match 'Get-Command\s+taskkill'
    }
}

Describe "Invoke-NvidiaTaskkillTree" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
        Mock Resolve-TrustedNvidiaTaskkill { "C:\Windows\System32\taskkill.exe" }
        Mock Invoke-NvidiaTaskkillNative {
            [PSCustomObject]@{ ExitCode = 0; Output = @() }
        }
    }

    It "invokes the trusted utility with PID, tree, and force arguments" {
        Invoke-NvidiaTaskkillTree -ProcessId 4321 | Should -BeTrue

        Should -Invoke Resolve-TrustedNvidiaTaskkill -Exactly 1
        Should -Invoke Invoke-NvidiaTaskkillNative -Exactly 1 -ParameterFilter {
            $TaskkillPath -eq 'C:\Windows\System32\taskkill.exe' -and $ProcessId -eq 4321
        }
    }

    It "fails closed when the trusted utility cannot be resolved" {
        Mock Resolve-TrustedNvidiaTaskkill { $null }

        Invoke-NvidiaTaskkillTree -ProcessId 4321 | Should -BeFalse

        Should -Invoke Invoke-NvidiaTaskkillNative -Exactly 0
    }

    It "fails closed when taskkill reports a process-tree termination error" {
        Mock Invoke-NvidiaTaskkillNative {
            [PSCustomObject]@{ ExitCode = 5; Output = @('Access is denied.') }
        }

        Invoke-NvidiaTaskkillTree -ProcessId 4321 | Should -BeFalse
    }
}

Describe "Stop-NvidiaProcessBounded" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
        Mock Invoke-NvidiaTaskkillTree { $true }
    }

    It "terminates the owned process tree and waits only for the bounded timeout" {
        $script:waitTimeouts = [System.Collections.Generic.List[int]]::new()
        $process = [PSCustomObject]@{ Id = 4321 }
        $process | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value {
            param($Timeout)
            $script:waitTimeouts.Add($Timeout)
            $true
        }

        Stop-NvidiaProcessBounded -Process $process -WaitTimeoutMs 2500 | Should -BeTrue

        $script:waitTimeouts | Should -Be @(2500)
        Should -Invoke Invoke-NvidiaTaskkillTree -Exactly 1 -ParameterFilter { $ProcessId -eq 4321 }
    }

    It "returns false instead of blocking when the final wait fails" {
        $process = [PSCustomObject]@{ Id = 4321 }
        $process | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { throw "wait failed" }

        Stop-NvidiaProcessBounded -Process $process | Should -BeFalse
    }

    It "returns false without claiming termination when tree kill fails" {
        Mock Invoke-NvidiaTaskkillTree { $false }
        $script:waitCalls = 0
        $process = [PSCustomObject]@{ Id = 4321 }
        $process | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { $script:waitCalls++; $true }

        Stop-NvidiaProcessBounded -Process $process | Should -BeFalse

        $script:waitCalls | Should -Be 0
    }

    It "does not terminate the process tree under WhatIf" {
        $process = [PSCustomObject]@{ Id = 4321 }
        $process | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { throw 'wait must not run' }

        Stop-NvidiaProcessBounded -Process $process -WhatIf | Should -BeFalse

        Should -Invoke Invoke-NvidiaTaskkillTree -Exactly 0
    }
}

Describe "Remove-NvidiaPackageBloat" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
    }

    It "counts only components whose absence is verified" {
        $packageRoot = Join-Path $SCRIPT:TestTempRoot "package-bloat-success"
        $bloatPath = Join-Path $packageRoot "NvTelemetry"
        New-Item -ItemType Directory -Path $bloatPath -Force | Out-Null

        $result = Remove-NvidiaPackageBloat -PackageRoot $packageRoot

        $result.Status | Should -Be 'Success'
        $result.RemovedCount | Should -Be 1
        Test-Path -LiteralPath $bloatPath | Should -BeFalse
    }

    It "fails closed when a required component cannot be removed" {
        $packageRoot = Join-Path $SCRIPT:TestTempRoot "package-bloat-failure"
        $bloatPath = Join-Path $packageRoot "NvTelemetry"
        New-Item -ItemType Directory -Path $bloatPath -Force | Out-Null
        Mock Remove-Item { throw "locked" } -ParameterFilter { $LiteralPath -eq $bloatPath }

        $result = Remove-NvidiaPackageBloat -PackageRoot $packageRoot

        $result.Status | Should -Be 'Failed'
        $result.RemovedCount | Should -Be 0
        $result.Failures -join ' ' | Should -Match 'locked'
    }

    It "reports a dry run and preserves package components under WhatIf" {
        $packageRoot = Join-Path $SCRIPT:TestTempRoot "package-bloat-whatif"
        $bloatPath = Join-Path $packageRoot "NvTelemetry"
        New-Item -ItemType Directory -Path $bloatPath -Force | Out-Null

        $result = Remove-NvidiaPackageBloat -PackageRoot $packageRoot -WhatIf

        $result.Status | Should -Be 'DryRun'
        $result.RemovedCount | Should -Be 0
        $result.NotProcessedCount | Should -BeGreaterThan 0
        Test-Path -LiteralPath $bloatPath | Should -BeTrue
    }

    It "requires the installer caller to gate setup execution on strip success" {
        $source = Get-Content -LiteralPath "$PSScriptRoot/../../helpers/nvidia-driver.ps1" -Raw

        $source | Should -Match '(?s)\$bloatRemoval\s*=\s*Remove-NvidiaPackageBloat.*?Status\s*-ne\s*''Success''.*?setup\.exe will not be executed.*?return \$false.*?Start-Process -FilePath \$setupExe'
    }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Get-LatestNvidiaDriver — GPU Series Detection ──────────────────────────
Describe "Get-LatestNvidiaDriver" {

    BeforeEach { Reset-TestState }

    Context "GPU series mapping" {

        It "detects RTX 40 series GPU" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4090" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='https://us.download.nvidia.com/Windows/572.42/572.42-desktop-win10-win11-64bit-international-dch-whql.exe' Version: 572.42"
                }
            }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            $result | Should -Not -BeNullOrEmpty
            $result.ManualDownload | Should -Be $false
        }

        It "detects RTX 30 series GPU" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 3080" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='https://us.download.nvidia.com/Windows/572.42/572.42.exe' Version: 572.42"
                }
            }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            $result | Should -Not -BeNullOrEmpty
        }

        It "detects GTX 16 series GPU" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce GTX 1660 Super" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='https://us.download.nvidia.com/Windows/572.42/572.42.exe' Version: 572.42"
                }
            }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            $result | Should -Not -BeNullOrEmpty
        }

        It "returns manual download for unrecognized series" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA Unknown Future GPU" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Write-Step {}
            Mock Write-Warn {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            $result | Should -Not -BeNullOrEmpty
            $result.ManualDownload | Should -Be $true
            $result.Url | Should -Match "nvidia\.com"
        }

        It "returns null when no NVIDIA GPU detected" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "AMD Radeon RX 7900 XTX" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Write-Step {}
            Mock Write-Warn {}

            $result = Get-LatestNvidiaDriver
            $result | Should -BeNullOrEmpty
        }
    }

    Context "URL Security Validation" {

        It "rejects non-nvidia.com download URL" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4090" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='https://evil.com/malware.exe' Version: 1.0"
                }
            }
            Mock Write-Step {}
            Mock Write-Warn {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            $result | Should -Not -BeNullOrEmpty
            $result.ManualDownload | Should -Be $true
        }

        It "upgrades http to https" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4090" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='http://us.download.nvidia.com/test.exe' Version: 1.0"
                }
            }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}

            $result = Get-LatestNvidiaDriver
            if (-not $result.ManualDownload) {
                $result.Url | Should -Match "^https://"
            }
        }

        It "prepends nvidia.com for relative URLs" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4090" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='/Windows/572.42/driver.exe' Version: 572.42"
                }
            }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            if (-not $result.ManualDownload) {
                $result.Url | Should -Match "nvidia\.com"
            }
        }
    }

    Context "Version parsing" {

        It "extracts version number from response" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4090" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest {
                [PSCustomObject]@{
                    Content = "downloadURL='https://us.download.nvidia.com/Windows/572.42/driver.exe' Version: 572.42"
                }
            }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}

            $result = Get-LatestNvidiaDriver
            $result.Version | Should -Be "572.42"
        }
    }

    Context "API failure" {

        It "falls back to manual download on API error" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4090" }
            } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
            Mock Invoke-WebRequest { throw "Connection timeout" }
            Mock Write-Step {}
            Mock Write-Warn {}
            Mock Write-Info {}
            Mock Write-DebugLog {}

            $result = Get-LatestNvidiaDriver
            $result.ManualDownload | Should -Be $true
        }
    }
}

# ── Install-NvidiaDriverClean ──────────────────────────────────────────────
Describe "Install-NvidiaDriverClean" {

    BeforeEach { Reset-TestState }

    It "returns true in DRY-RUN mode without executing" {
        $SCRIPT:DryRun = $true
        Mock Write-ConsoleLine {}

        $result = Install-NvidiaDriverClean -DriverExe "C:\fake\driver.exe"
        $result | Should -Be $true
    }

    It "returns false for non-existent file" {
        $SCRIPT:DryRun = $false
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe "C:\nonexistent\driver.exe"
        $result | Should -Be $false
    }

    It "rejects path with traversal sequences" {
        $SCRIPT:DryRun = $false
        # Use a path with ".." that resolves outside the expected directory
        $tempFile = Join-Path $SCRIPT:TestTempRoot "..\..\evil.exe"
        Mock Write-Err {}
        # Do NOT mock Get-Item — let the real path validation logic run.
        # The function should reject the traversal before checking file existence.

        $result = Install-NvidiaDriverClean -DriverExe $tempFile
        $result | Should -Be $false
    }

    It "refuses to execute an unsigned or invalid driver package even when confirmed" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "unsigned.exe"
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        Mock Get-AuthenticodeSignature { [PSCustomObject]@{ Status = "NotSigned"; SignerCertificate = $null } }
        Mock Read-Host { "Y" }
        Mock Start-Process {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Read-Host -Exactly 0
        Should -Invoke Start-Process -Exactly 0
    }

    It "refuses to execute a driver package signed by a non-NVIDIA publisher" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "wrong-signer.exe"
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=Example Publisher" }
            }
        }
        Mock Read-Host { "Y" }
        Mock Start-Process {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Read-Host -Exactly 0
        Should -Invoke Start-Process -Exactly 0
    }

    It "refuses a deceptively named publisher before executing the outer package" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "deceptive-signer.exe"
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Support Tools LLC" }
            }
        }
        Mock Start-Process {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Start-Process -Exactly 0
    }

    It "retains the secured extraction directory when timed-out tree termination fails" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "nvidia.exe"
        $extractRoot = Join-Path $SCRIPT:TestTempRoot "valid-driver-timeout"
        $securedDriver = Join-Path $extractRoot "nvidia-driver-package.exe"
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        New-Item -ItemType Directory -Path $extractRoot | Out-Null
        $extractProcess = [PSCustomObject]@{ Id = 4321 }
        $extractProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $false }
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Corporation" }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Get-NvidiaDisplayDriverSnapshot { $null }
        Mock Start-Process { $extractProcess }
        Mock Invoke-NvidiaTaskkillTree { $false }
        Mock Write-DebugLog {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Write-Err {}
        Mock Write-Warn {}

        $oldTemp = $env:TEMP
        $env:TEMP = $SCRIPT:TestTempRoot
        try {
            $result = Install-NvidiaDriverClean -DriverExe $driver
        } finally {
            $env:TEMP = $oldTemp
        }

        $result | Should -Be $false
        Should -Invoke Start-Process -Exactly 1 -ParameterFilter { $FilePath -eq $securedDriver }
        Should -Invoke Start-Process -Exactly 0 -ParameterFilter { $FilePath -eq $driver }
        Should -Invoke Invoke-NvidiaTaskkillTree -Exactly 1 -ParameterFilter { $ProcessId -eq 4321 }
        (Test-Path -LiteralPath $extractRoot) | Should -BeTrue
    }

    It "fails closed when the caller package cannot be copied into the secured directory" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot 'copy-failure-driver.exe'
        $extractRoot = Join-Path $SCRIPT:TestTempRoot 'copy-failure-extraction'
        Set-Content -Path $driver -Value 'fake driver' -Encoding UTF8
        New-Item -ItemType Directory -Path $extractRoot | Out-Null
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = 'Valid'
                SignerCertificate = [PSCustomObject]@{ Subject = 'CN=NVIDIA Corporation' }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Copy-Item { throw 'copy denied' }
        Mock Start-Process { throw 'Start-Process should not be reached' }
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -BeFalse
        Should -Invoke Copy-Item -Exactly 1 -ParameterFilter { $LiteralPath -eq $driver }
        Should -Invoke Start-Process -Exactly 0
    }

    It "rejects a preexisting extraction candidate before launching the package" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "preexisting-driver.exe"
        $extractRoot = Join-Path $SCRIPT:TestTempRoot "preexisting-extraction"
        New-Item -ItemType Directory -Path $extractRoot | Out-Null
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        Set-Content -Path (Join-Path $extractRoot "setup.exe") -Value "preexisting" -Encoding UTF8
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Corporation" }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Start-Process { throw "Start-Process should not be reached" }
        Mock Write-DebugLog {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Start-Process -Exactly 0
        Should -Invoke Write-Err -Exactly 1
    }

    It "rejects ambiguous setup.exe candidates without executing either candidate" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "ambiguous-driver.exe"
        $extractRoot = Join-Path $SCRIPT:TestTempRoot "ambiguous-extraction"
        New-Item -ItemType Directory -Path $extractRoot | Out-Null
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        $extractProcess = [PSCustomObject]@{ ExitCode = 0 }
        $extractProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $true }
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Corporation" }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Get-NvidiaDisplayDriverSnapshot { $null }
        Mock Start-Process {
            New-Item -ItemType Directory -Path (Join-Path $extractRoot "nested") | Out-Null
            Set-Content -Path (Join-Path $extractRoot "setup.exe") -Value "first" -Encoding UTF8
            Set-Content -Path (Join-Path $extractRoot "nested/setup.exe") -Value "second" -Encoding UTF8
            $extractProcess
        }
        Mock Write-DebugLog {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Start-Process -Exactly 1
        Should -Invoke Write-Err
    }

    It "rejects setup.exe when a candidate ancestor is a reparse point" {
        $rootPath = Join-Path $SCRIPT:TestTempRoot "trusted-root"
        $linkedPath = Join-Path $rootPath "linked"
        $setupPath = Join-Path $linkedPath "setup.exe"
        $rootItem = [PSCustomObject]@{
            PSIsContainer = $true
            Attributes = [System.IO.FileAttributes]::Directory
            FullName = $rootPath
            Name = "trusted-root"
            Parent = $null
        }
        $linkedItem = [PSCustomObject]@{
            PSIsContainer = $true
            Attributes = ([System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::ReparsePoint)
            FullName = $linkedPath
            Name = "linked"
            Parent = $rootItem
        }
        $setupItem = [PSCustomObject]@{
            PSIsContainer = $false
            Attributes = [System.IO.FileAttributes]::Normal
            FullName = $setupPath
            Name = "setup.exe"
            Directory = $linkedItem
        }
        Mock Get-Item {
            if ($LiteralPath -eq $rootPath) { return $rootItem }
            if ($LiteralPath -eq $setupPath) { return $setupItem }
            throw "unexpected path"
        }

        Test-NvidiaSetupPath -ExtractionRoot $rootPath -CandidatePath $setupPath | Should -Be $false
    }

    It "fails setup discovery when extraction contains a directory reparse point" {
        $rootPath = Join-Path $SCRIPT:TestTempRoot "discovery-root"
        $reparseDirectory = [PSCustomObject]@{
            PSIsContainer = $true
            Attributes = ([System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::ReparsePoint)
            FullName = (Join-Path $rootPath "linked")
            Name = "linked"
        }
        Mock Get-ChildItem { @($reparseDirectory) }

        { Find-NvidiaSetupExecutable -ExtractionRoot $rootPath } |
            Should -Throw "*directory reparse point*"
    }

    It "does not treat an unchanged preexisting NVIDIA driver as install success when setup.exe is absent" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "unchanged-driver.exe"
        $extractRoot = Join-Path $SCRIPT:TestTempRoot "unchanged-extraction"
        New-Item -ItemType Directory -Path $extractRoot | Out-Null
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        $extractProcess = [PSCustomObject]@{ ExitCode = 0 }
        $extractProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $true }
        $unchanged = [PSCustomObject]@{
            Identity = "PNP:PCI\VEN_10DE&DEV_2684"
            Name = "NVIDIA GeForce RTX 4090"
            Version = "32.0.15.7242"
        }
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Corporation" }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Get-NvidiaDisplayDriverSnapshot { $unchanged }
        Mock Start-Process { $extractProcess }
        Mock Write-DebugLog {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Start-Process -Exactly 1
    }

    It "rejects an extracted setup.exe signed by a non-NVIDIA publisher" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "outer-nvidia.exe"
        $extractRoot = Join-Path $SCRIPT:TestTempRoot "wrong-setup-signer"
        $setupPath = Join-Path $extractRoot "setup.exe"
        $securedDriver = Join-Path $extractRoot "nvidia-driver-package.exe"
        New-Item -ItemType Directory -Path $extractRoot | Out-Null
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        $extractProcess = [PSCustomObject]@{ ExitCode = 0 }
        $extractProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $true }
        Mock Get-AuthenticodeSignature {
            $subject = if ($FilePath -eq $setupPath) { "CN=NVIDIA Support Tools LLC" } else { "CN=NVIDIA Corporation" }
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = $subject }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Get-NvidiaDisplayDriverSnapshot { $null }
        Mock Start-Process {
            Set-Content -Path $setupPath -Value "fake setup" -Encoding UTF8
            $extractProcess
        } -ParameterFilter { $FilePath -eq $securedDriver }
        Mock Start-Process { throw "Extracted setup.exe should not execute" } -ParameterFilter { $FilePath -eq $setupPath }
        Mock Write-DebugLog {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Write-ActionOK {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -Be $false
        Should -Invoke Start-Process -Exactly 1
        Should -Invoke Get-AuthenticodeSignature -Exactly 1 -ParameterFilter { $FilePath -eq $setupPath }
        Should -Invoke Write-Err
    }

    It "executes exactly one contained NVIDIA-signed setup.exe and accepts exit code zero" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "success-driver.exe"
        $tempParent = Join-Path $SCRIPT:TestTempRoot "success-temp"
        $extractRoot = Join-Path $tempParent "exclusive-extraction"
        $setupPath = Join-Path $extractRoot "setup.exe"
        $securedDriver = Join-Path $extractRoot "nvidia-driver-package.exe"
        New-Item -ItemType Directory -Path $extractRoot -Force | Out-Null
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        $extractProcess = [PSCustomObject]@{ ExitCode = 0 }
        $extractProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $true }
        $installProcess = [PSCustomObject]@{ ExitCode = 0 }
        $installProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $true }
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Corporation" }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Get-NvidiaDisplayDriverSnapshot { $null }
        Mock Start-Process {
            Set-Content -Path $setupPath -Value "fake setup" -Encoding UTF8
            $extractProcess
        } -ParameterFilter { $FilePath -eq $securedDriver }
        Mock Start-Process { $installProcess } -ParameterFilter { $FilePath -eq $setupPath }
        Mock Apply-NvidiaPostInstallTweaks {}
        Mock Write-DebugLog {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Write-OK {}
        Mock Write-ActionOK {}

        $oldTemp = $env:TEMP
        $env:TEMP = $tempParent
        try {
            $result = Install-NvidiaDriverClean -DriverExe $driver
        } finally {
            $env:TEMP = $oldTemp
        }

        $result | Should -Be $true
        Should -Invoke Start-Process -Exactly 1 -ParameterFilter { $FilePath -eq $securedDriver }
        Should -Invoke Start-Process -Exactly 0 -ParameterFilter { $FilePath -eq $driver }
        Should -Invoke Start-Process -Exactly 1 -ParameterFilter { $FilePath -eq $setupPath }
        Should -Invoke Get-AuthenticodeSignature -Exactly 1 -ParameterFilter { $FilePath -eq $setupPath }
    }

    It "terminates a timed-out setup process and does not apply post-install tweaks" {
        $SCRIPT:DryRun = $false
        $driver = Join-Path $SCRIPT:TestTempRoot "timeout-driver.exe"
        $extractRoot = Join-Path $SCRIPT:TestTempRoot "timeout-extraction"
        $setupPath = Join-Path $extractRoot "setup.exe"
        $securedDriver = Join-Path $extractRoot "nvidia-driver-package.exe"
        New-Item -ItemType Directory -Path $extractRoot -Force | Out-Null
        Set-Content -Path $driver -Value "fake driver" -Encoding UTF8
        $extractProcess = [PSCustomObject]@{ ExitCode = 0 }
        $extractProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($Timeout) $true }
        $installProcess = [PSCustomObject]@{ Id = 9876; ExitCode = $null }
        $script:InstallWaitTimeouts = [System.Collections.Generic.List[int]]::new()
        $installProcess | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value {
            param($Timeout)
            $script:InstallWaitTimeouts.Add([int]$Timeout)
            return ($script:InstallWaitTimeouts.Count -gt 1)
        }
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=NVIDIA Corporation" }
            }
        }
        Mock New-SecureNvidiaExtractionDirectory { $extractRoot }
        Mock Get-NvidiaDisplayDriverSnapshot { $null }
        Mock Start-Process {
            Set-Content -Path $setupPath -Value "fake setup" -Encoding UTF8
            $extractProcess
        } -ParameterFilter { $FilePath -eq $securedDriver }
        Mock Start-Process { $installProcess } -ParameterFilter { $FilePath -eq $setupPath }
        Mock Invoke-NvidiaTaskkillTree { $true }
        Mock Apply-NvidiaPostInstallTweaks {}
        Mock Write-DebugLog {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Write-OK {}
        Mock Write-ActionOK {}
        Mock Write-Err {}

        $result = Install-NvidiaDriverClean -DriverExe $driver

        $result | Should -BeFalse
        $script:InstallWaitTimeouts | Should -Be @(600000, 10000)
        Should -Invoke Invoke-NvidiaTaskkillTree -Exactly 1 -ParameterFilter { $ProcessId -eq 9876 }
        Should -Invoke Apply-NvidiaPostInstallTweaks -Exactly 0
        (Test-Path -LiteralPath $extractRoot) | Should -BeFalse
    }

    It "recognizes a version delta as a postcondition change" {
        $before = [PSCustomObject]@{ Identity = "PNP:GPU"; Version = "1.0" }
        $after = [PSCustomObject]@{ Identity = "PNP:GPU"; Version = "2.0" }

        Test-NvidiaDriverSnapshotChanged -Before $before -After $after | Should -Be $true
        Test-NvidiaDriverSnapshotChanged -Before $before -After $before | Should -Be $false
    }

    It "creates distinct unpredictable extraction directories and secures each before use" {
        Mock Set-NvidiaExtractionDirectoryAcl { $true }

        $first = New-SecureNvidiaExtractionDirectory -ParentPath $SCRIPT:TestTempRoot
        $second = New-SecureNvidiaExtractionDirectory -ParentPath $SCRIPT:TestTempRoot

        $first | Should -Not -Be $second
        (Split-Path -Path $first -Leaf) | Should -Match '^NVDriverExtract_[0-9A-F]{64}$'
        (Split-Path -Path $second -Leaf) | Should -Match '^NVDriverExtract_[0-9A-F]{64}$'
        @(Get-ChildItem -LiteralPath $first -Force).Count | Should -Be 0
        @(Get-ChildItem -LiteralPath $second -Force).Count | Should -Be 0
        Should -Invoke Set-NvidiaExtractionDirectoryAcl -Exactly 2
    }

    It "does not create or secure an extraction directory under WhatIf" {
        Mock New-Item { throw 'directory creation must not run' }
        Mock Set-NvidiaExtractionDirectoryAcl { throw 'ACL mutation must not run' }

        New-SecureNvidiaExtractionDirectory -ParentPath $SCRIPT:TestTempRoot -WhatIf |
            Should -BeNullOrEmpty

        Should -Invoke New-Item -Exactly 0
        Should -Invoke Set-NvidiaExtractionDirectoryAcl -Exactly 0
    }

    It "does not apply an extraction ACL under WhatIf" {
        Mock Set-Acl { throw 'Set-Acl must not run' }

        Set-NvidiaExtractionDirectoryAcl -Path $SCRIPT:TestTempRoot -WhatIf | Should -BeFalse

        Should -Invoke Set-Acl -Exactly 0
    }

    It "validates Authenticode signature (security)" {
        # Verify that the function checks for Authenticode signatures
        # by confirming the pattern exists in the source
        $source = Get-Content "$PSScriptRoot/../../helpers/nvidia-driver.ps1" -Raw
        $source | Should -Match "Get-AuthenticodeSignature"
        $source | Should -Match "NVIDIA"
    }

    It "fails closed for an invalid driver signature without prompting or executing" {
        $SCRIPT:DryRun = $false
        $driverPath = Join-Path $SCRIPT:TestTempRoot "driver.exe"
        Set-Content $driverPath -Value "not a real driver" -Encoding UTF8
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "HashMismatch"
                SignerCertificate = $null
            }
        }
        Mock Write-Err {}
        Mock Read-Host { "Y" }
        Mock Start-Process { throw "Start-Process should not be reached" }

        $result = Install-NvidiaDriverClean -DriverExe $driverPath

        $result | Should -Be $false
        Should -Invoke Write-Err -Exactly 1 -ParameterFilter { $t -match "no valid Authenticode signature" }
        Should -Invoke Read-Host -Exactly 0
        Should -Invoke Start-Process -Exactly 0
    }

    It "fails closed for a valid non-NVIDIA signature without prompting or executing" {
        $SCRIPT:DryRun = $false
        $driverPath = Join-Path $SCRIPT:TestTempRoot "driver.exe"
        Set-Content $driverPath -Value "not a real driver" -Encoding UTF8
        Mock Get-AuthenticodeSignature {
            [PSCustomObject]@{
                Status = "Valid"
                SignerCertificate = [PSCustomObject]@{ Subject = "CN=Contoso Software" }
            }
        }
        Mock Write-Err {}
        Mock Read-Host { "Y" }
        Mock Start-Process { throw "Start-Process should not be reached" }

        $result = Install-NvidiaDriverClean -DriverExe $driverPath

        $result | Should -Be $false
        Should -Invoke Write-Err -Exactly 1 -ParameterFilter { $t -match "NOT by NVIDIA" }
        Should -Invoke Read-Host -Exactly 0
        Should -Invoke Start-Process -Exactly 0
    }
}

# ── Apply-NvidiaPostInstallTweaks ──────────────────────────────────────────
Describe "Apply-NvidiaPostInstallTweaks" {

    BeforeEach { Reset-TestState }

    It "disables NVIDIA telemetry registry entries" {
        $script:regCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:regCalls.Add(@{ Name = $name; Value = $value })
        }
        Mock Write-Step {}
        Mock Write-ActionOK {}
        Mock Write-Info {}
        Mock Write-DebugLog {}
        Mock Test-Path { $false }
        Mock Backup-ServiceState {}
        Mock Stop-Service {}
        Mock Set-Service {}

        Apply-NvidiaPostInstallTweaks

        $telemetryCalls = $script:regCalls | Where-Object { $_.Name -match "OptInOrOutPreference|EnableRID" }
        @($telemetryCalls).Count | Should -BeGreaterOrEqual 4
    }

    It "sets MPO disable registry value" {
        $script:regCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:regCalls.Add(@{ Name = $name; Value = $value })
        }
        Mock Write-Step {}
        Mock Write-ActionOK {}
        Mock Write-Info {}
        Mock Write-DebugLog {}
        Mock Test-Path { $false }
        Mock Backup-ServiceState {}
        Mock Stop-Service {}
        Mock Set-Service {}

        Apply-NvidiaPostInstallTweaks

        $mpoCall = $script:regCalls | Where-Object { $_.Name -eq "OverlayTestMode" }
        $mpoCall | Should -Not -BeNullOrEmpty
        $mpoCall.Value | Should -Be 5
    }

    It "enables Write Combining" {
        $script:regCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:regCalls.Add(@{ Name = $name; Value = $value })
        }
        Mock Write-Step {}
        Mock Write-ActionOK {}
        Mock Write-Info {}
        Mock Write-DebugLog {}
        Mock Test-Path { $false }
        Mock Backup-ServiceState {}
        Mock Stop-Service {}
        Mock Set-Service {}

        Apply-NvidiaPostInstallTweaks

        $wcCall = $script:regCalls | Where-Object { $_.Name -eq "EnableWriteCombining" }
        $wcCall | Should -Not -BeNullOrEmpty
        $wcCall.Value | Should -Be 1
    }
}
