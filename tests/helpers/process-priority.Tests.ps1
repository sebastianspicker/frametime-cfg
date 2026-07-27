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

    BeforeEach {
        Reset-TestState
        $script:_cachedCpuInfo = $null
    }

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

    Context "Dual-CCD X3D (manual topology required)" {

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
            $result.HasAuthoritativeTopology | Should -Be $false
            $result.Reason | Should -Match "automatic affinity is disabled"
            $result.ContainsKey("AffinityMask") | Should -Be $false
            $result.ContainsKey("AffinityHex") | Should -Be $false
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
            [PSCustomObject]@{ Status = "Success"; Message = "written" }
        }
        Mock Get-Process { $null }
        Mock Get-X3DCcdInfo { $null }
        Mock Write-Blank {}
        Mock Write-OK {}
        Mock Write-ConsoleLine {}

        Set-CS2ProcessPriority

        $ifeoCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "CpuPriorityClass" -and $_.Value -eq 3 }
        $ifeoCall | Should -Not -BeNullOrEmpty
    }

    It "uses correct IFEO registry path" {
        $SCRIPT:DryRun = $false
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Path = $path; Name = $name; Value = $value })
            [PSCustomObject]@{ Status = "Success"; Message = "written" }
        }
        Mock Get-Process { $null }
        Mock Get-X3DCcdInfo { $null }
        Mock Write-Blank {}
        Mock Write-OK {}
        Mock Write-ConsoleLine {}

        Set-CS2ProcessPriority

        $ifeoCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "CpuPriorityClass" }
        $ifeoCall.Path | Should -Match "Image File Execution Options\\cs2\.exe\\PerfOptions"
    }

    Context "DRY-RUN mode" {

        It "does not modify running process in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Set-RegistryValue { [PSCustomObject]@{ Status = "DryRun"; Message = "previewed" } }
            Mock Get-Process {
                [PSCustomObject]@{ PriorityClass = 'Normal' }
            }
            Mock Get-X3DCcdInfo { $null }
            Mock Write-Blank {}
            Mock Write-OK {}
            Mock Write-ConsoleLine {}

            $result = Set-CS2ProcessPriority

            $result.Status | Should -Be "DryRun"
            $result.CanCompleteStep | Should -Be $false
            # Should not throw; DRY-RUN prints message instead of modifying
        }
    }

    It "does not report completion when the mandatory IFEO write fails" {
        $SCRIPT:DryRun = $false
        Mock Set-RegistryValue { [PSCustomObject]@{ Status = "Failed"; Message = "access denied" } }
        Mock Get-Process { throw "live process updates must not run after IFEO failure" }

        $result = Set-CS2ProcessPriority

        $result.Status | Should -Be "Failed"
        $result.CanCompleteStep | Should -Be $false
        $result.Message | Should -Match "IFEO"
    }

    It "does not auto-pin or register a task for a dual-CCD model without authoritative topology" {
        $SCRIPT:DryRun = $false
        Mock Set-RegistryValue { [PSCustomObject]@{ Status = "Success"; Message = "written" } }
        Mock Get-Process { [PSCustomObject]@{ PriorityClass = "Normal"; ProcessorAffinity = [IntPtr]::Zero } }
        Mock Get-X3DCcdInfo {
            @{ IsX3D = $true; DualCCD = $true; CpuName = "AMD Ryzen 9 7950X3D"; HasAuthoritativeTopology = $false; Reason = "manual topology required" }
        }
        Mock Install-CS2AffinityTask { throw "must not register a task without an authoritative topology" }
        Mock Write-Blank {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-ConsoleLine {}

        $result = Set-CS2ProcessPriority

        $result.Status | Should -Be "Success"
        $result.CanCompleteStep | Should -Be $true
        Should -Invoke Install-CS2AffinityTask -Exactly 0
        $result.AffinityTaskResult | Should -BeNullOrEmpty
    }
}

# ── Install-CS2AffinityTask ────────────────────────────────────────────────
Describe "Install-CS2AffinityTask" {

    BeforeEach { Reset-TestState }

    It "skips task creation in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        Mock Register-ScheduledTask {}
        Mock Write-ConsoleLine {}

        Install-CS2AffinityTask -AffinityMask 0xFF -AffinityHex "0xFF"

        Should -Invoke Register-ScheduledTask -Times 0
    }

    It "uses correct task name constant" {
        $CS2_AffinityTaskName | Should -Be "frametime_cfg_cs2_affinity"
        $LegacyCS2AffinityTaskName | Should -Be "CS2_Optimize_CCD_Affinity"
    }
}
