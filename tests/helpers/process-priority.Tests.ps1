# ==============================================================================
#  tests/helpers/process-priority.Tests.ps1  --  IFEO priority & X3D affinity
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Stub Windows-only cmdlets before loading the module
    if ($IsWindows -eq $false) {
        if (-not (Get-Command Get-Process -ErrorAction SilentlyContinue)) {
            function global:Get-Process { param($Name, $ErrorAction) $null }
        }
        if (-not (Get-Command Register-ScheduledTask -ErrorAction SilentlyContinue)) {
            function global:Register-ScheduledTask { param($TaskName, $Xml, [switch]$Force) $null }
        }
    }

    . "$PSScriptRoot/../../helpers/process-priority.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Get-X3DCcdInfo ──────────────────────────────────────────────────────────
Describe "Get-X3DCcdInfo" {

    BeforeEach { Reset-TestState; Reset-CachedCpuInfo }

    Context "Non-X3D CPUs" {

        It "returns null for Intel CPU" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "Intel Core i9-13900K"
                    NumberOfCores = 24
                    NumberOfLogicalProcessors = 32
                }
            }
            $result = Get-X3DCcdInfo
            $result | Should -BeNullOrEmpty
        }

        It "returns null for AMD non-X3D CPU" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 7950X"
                    NumberOfCores = 16
                    NumberOfLogicalProcessors = 32
                }
            }
            $result = Get-X3DCcdInfo
            $result | Should -BeNullOrEmpty
        }

        It "returns null when Get-CimInstance fails" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } { throw "WMI unavailable" }
            $result = Get-X3DCcdInfo
            $result | Should -BeNullOrEmpty
        }
    }

    Context "Single-CCD X3D (no pinning needed)" {

        It "detects 7800X3D as single CCD" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 7 7800X3D"
                    NumberOfCores = 8
                    NumberOfLogicalProcessors = 16
                }
            }
            $result = Get-X3DCcdInfo
            $result | Should -Not -BeNullOrEmpty
            $result.IsX3D | Should -Be $true
            $result.DualCCD | Should -Be $false
        }

        It "detects 5800X3D as single CCD" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 7 5800X3D"
                    NumberOfCores = 8
                    NumberOfLogicalProcessors = 16
                }
            }
            $result = Get-X3DCcdInfo
            $result.IsX3D | Should -Be $true
            $result.DualCCD | Should -Be $false
        }

        It "detects 9800X3D as single CCD" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 7 9800X3D"
                    NumberOfCores = 8
                    NumberOfLogicalProcessors = 16
                }
            }
            $result = Get-X3DCcdInfo
            $result.IsX3D | Should -Be $true
            $result.DualCCD | Should -Be $false
        }
    }

    Context "Dual-CCD X3D (pinning required)" {

        It "detects 7950X3D as dual CCD" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 7950X3D"
                    NumberOfCores = 16
                    NumberOfLogicalProcessors = 32
                }
            }
            $result = Get-X3DCcdInfo
            $result | Should -Not -BeNullOrEmpty
            $result.IsX3D | Should -Be $true
            $result.DualCCD | Should -Be $true
            $result.Ccd0Cores | Should -Be 8
            $result.TotalCores | Should -Be 16
            $result.SmtEnabled | Should -Be $true
        }

        It "detects 7900X3D as dual CCD" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 7900X3D"
                    NumberOfCores = 12
                    NumberOfLogicalProcessors = 24
                }
            }
            $result = Get-X3DCcdInfo
            $result.DualCCD | Should -Be $true
            $result.Ccd0Cores | Should -Be 6
        }

        It "detects 9950X3D as dual CCD" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 9950X3D"
                    NumberOfCores = 16
                    NumberOfLogicalProcessors = 32
                }
            }
            $result = Get-X3DCcdInfo
            $result.DualCCD | Should -Be $true
        }

        It "detects 9900X3D as dual CCD with asymmetric 8+4 layout" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 9900X3D"
                    NumberOfCores = 12
                    NumberOfLogicalProcessors = 24
                }
            }
            $result = Get-X3DCcdInfo
            $result.DualCCD | Should -Be $true
            $result.Ccd0Cores | Should -Be 8
            $result.TotalCores | Should -Be 12
        }

        It "calculates correct affinity mask for 9900X3D with SMT (8+4 asymmetric)" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 9900X3D"
                    NumberOfCores = 12
                    NumberOfLogicalProcessors = 24
                }
            }
            $result = Get-X3DCcdInfo
            # CCD0 cores 0-7 = bits 0-7, SMT partners = bits 12-19
            # Mask = 0xFF + (0xFF << 12) = 0xFF0FF
            $result.AffinityMask | Should -Be 0xFF0FF
        }

        It "calculates correct affinity mask for 7950X3D with SMT" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 7950X3D"
                    NumberOfCores = 16
                    NumberOfLogicalProcessors = 32
                }
            }
            $result = Get-X3DCcdInfo
            # CCD0 cores 0-7 = bits 0-7, SMT partners = bits 16-23
            # Mask = 0xFF + (0xFF << 16) = 0x00FF00FF
            $result.AffinityMask | Should -Be 0x00FF00FF
        }

        It "calculates correct affinity mask for 7900X3D with SMT" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 7900X3D"
                    NumberOfCores = 12
                    NumberOfLogicalProcessors = 24
                }
            }
            $result = Get-X3DCcdInfo
            # CCD0 cores 0-5 = bits 0-5, SMT partners = bits 12-17
            # Mask = 0x3F + (0x3F << 12) = 0x3F03F
            $result.AffinityMask | Should -Be 0x3F03F
        }

        It "includes hex representation" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen 9 7950X3D"
                    NumberOfCores = 16
                    NumberOfLogicalProcessors = 32
                }
            }
            $result = Get-X3DCcdInfo
            $result.AffinityHex | Should -Match '^0x[0-9A-F]+$'
        }
    }

    Context "Unknown X3D variant" {

        It "returns IsX3D=true with DualCCD=null for unknown model" {
            Mock Get-CimInstance -ParameterFilter { $ClassName -eq "Win32_Processor" } {
                [PSCustomObject]@{
                    Name = "AMD Ryzen X3D Future Model"
                    NumberOfCores = 32
                    NumberOfLogicalProcessors = 64
                }
            }
            $result = Get-X3DCcdInfo
            $result.IsX3D | Should -Be $true
            $result.DualCCD | Should -BeNullOrEmpty
        }
    }
}

# ── Set-CS2ProcessPriority ──────────────────────────────────────────────────
Describe "Set-CS2ProcessPriority" {

    BeforeEach { Reset-TestState }

    It "calls Set-RegistryValue for IFEO PerfOptions" {
        $SCRIPT:DryRun = $false
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Path = $path; Name = $name; Value = $value })
        }
        Mock Get-Process { $null }
        Mock Get-X3DCcdInfo { $null }
        Mock Write-Blank {}
        Mock Write-OK {}
        Mock Write-Host {}

        Set-CS2ProcessPriority

        $ifeoCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "CpuPriorityClass" -and $_.Value -eq 3 }
        $ifeoCall | Should -Not -BeNullOrEmpty
    }

    It "uses correct IFEO registry path" {
        $SCRIPT:DryRun = $false
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Path = $path; Name = $name; Value = $value })
        }
        Mock Get-Process { $null }
        Mock Get-X3DCcdInfo { $null }
        Mock Write-Blank {}
        Mock Write-OK {}
        Mock Write-Host {}

        Set-CS2ProcessPriority

        $ifeoCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "CpuPriorityClass" }
        $ifeoCall.Path | Should -Match "Image File Execution Options\\cs2\.exe\\PerfOptions"
    }

    Context "DRY-RUN mode" {

        It "does not modify running process in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Set-RegistryValue {}
            Mock Get-Process {
                [PSCustomObject]@{ PriorityClass = 'Normal' }
            }
            Mock Get-X3DCcdInfo { $null }
            Mock Write-Blank {}
            Mock Write-OK {}
            Mock Write-Host {}

            Set-CS2ProcessPriority
            # Should not throw; DRY-RUN prints message instead of modifying
        }
    }
}

# ── Install-CS2AffinityTask ────────────────────────────────────────────────
Describe "Install-CS2AffinityTask" {

    BeforeEach { Reset-TestState }

    It "skips task creation in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        Mock Register-ScheduledTask {}
        Mock Write-Host {}

        Install-CS2AffinityTask -AffinityMask 0xFF -AffinityHex "0xFF"

        Should -Invoke Register-ScheduledTask -Times 0
    }

    It "uses correct task name constant" {
        $CS2_AffinityTaskName | Should -Be "CS2_Optimize_CCD_Affinity"
    }
}
