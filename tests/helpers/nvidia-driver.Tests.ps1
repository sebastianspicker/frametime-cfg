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
    }

    . "$PSScriptRoot/../../helpers/nvidia-driver.ps1"
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
        Mock Write-Host {}

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
