# ==============================================================================
#  tests/helpers/system-analysis.Tests.ps1  --  Analyze panel health checks
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
    . "$PSScriptRoot/../../helpers/system-analysis.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "New-CheckItem" {

    It "normalizes blank current values and assigns display metadata" {
        $item = New-CheckItem -Category "Windows" -Group "Display" -Item "HAGS" `
            -Current "" -Recommended "Enabled" -Status "INFO" -StepRef "P1-7" -Impact "Setup dependent"

        $item.Current     | Should -Be "(not set)"
        $item.StatusLabel | Should -Be "ℹ  INFO"
        $item.StatusColor | Should -Be "#6b7280"
    }
}

Describe "Invoke-CheckHardware" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
    }

    It "maps healthy CIM and helper responses into OK and INFO rows" {
        Mock Get-CimInstance {
            [PSCustomObject]@{ VirtualizationBasedSecurityStatus = 0 }
        } -ParameterFilter { $ClassName -eq "Win32_DeviceGuard" }
        Mock Test-WheaErrors {
            [PSCustomObject]@{ HasErrors = $false; RecentCount = 0; Count = 0 }
        }
        Mock Get-AmdCpuInfo {
            [PSCustomObject]@{ IsX3D = $true; MaxClockSpeed = 4200 }
        }
        Mock Get-Ddr5TimingInfo {
            [PSCustomObject]@{ IsDDR5 = $true; ActiveMTs = 6000; IsOptimal1to1 = $true }
        }
        Mock Test-DualChannel {
            [PSCustomObject]@{ DualChannel = $true; Reason = "Dual-channel active" }
        }
        Mock Get-RamInfo {
            [PSCustomObject]@{ AtRatedSpeed = $true; ActiveMhz = 6000; SpeedMhz = 6000 }
        }

        $results = Invoke-CheckHardware

        ($results | Where-Object Item -eq "VBS / HVCI").Status     | Should -Be "OK"
        ($results | Where-Object Item -eq "DDR5 Speed").Status     | Should -Be "OK"
        ($results | Where-Object Item -eq "Dual-Channel RAM").Status | Should -Be "OK"
        ($results | Where-Object Item -eq "X3D Base Clock").Status | Should -Be "INFO"
    }

    It "returns INFO when VBS CIM query fails" {
        Mock Get-CimInstance { throw "WMI unavailable" } -ParameterFilter { $ClassName -eq "Win32_DeviceGuard" }
        Mock Test-WheaErrors { $null }
        Mock Get-AmdCpuInfo { $null }
        Mock Get-Ddr5TimingInfo { $null }
        Mock Test-DualChannel { $null }
        Mock Get-RamInfo { $null }

        $results = Invoke-CheckHardware

        ($results | Where-Object Item -eq "VBS / HVCI").Status | Should -Be "INFO"
    }
}

Describe "Invoke-CheckNetwork" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
    }

    It "reports OK when Nagle is disabled and URO is disabled" {
        Mock Get-ActiveNicGuid { "{TEST-GUID}" }
        Mock Get-RegVal {
            switch ("$Path|$Name") {
                "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{TEST-GUID}|TcpNoDelay" { 1; break }
                "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS|Do not use NLA" { "1"; break }
                default { $null }
            }
        }
        Mock netsh { "Receive Offload State : disabled" }

        $results = Invoke-CheckNetwork

        ($results | Where-Object Item -eq "Nagle Disable (TcpNoDelay)").Status | Should -Be "OK"
        ($results | Where-Object Item -eq "QoS NLA Bypass").Status | Should -Be "OK"
    }

    It "falls back to WARN when active NIC settings are missing" {
        Mock Get-ActiveNicGuid { $null }
        Mock Get-ChildItem {
            @([PSCustomObject]@{ PSPath = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\NIC1" })
        } -ParameterFilter { $Path -like "*Tcpip\Parameters\Interfaces" }
        Mock Get-RegVal { $null }
        Mock netsh { "" }

        $results = Invoke-CheckNetwork

        ($results | Where-Object Item -eq "Nagle Disable (TcpNoDelay)").Status | Should -Be "WARN"
    }
}

Describe "Invoke-CheckServices" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
    }

    It "marks missing services as OK and mismatched start types as WARN" {
        Mock Get-Service {
            switch ($Name) {
                "SysMain" { [PSCustomObject]@{ Name = "SysMain"; Status = "Running"; StartType = "Automatic" }; break }
                "WSearch" { $null; break }
                default   { [PSCustomObject]@{ Name = $Name; Status = "Stopped"; StartType = "Disabled" } }
            }
        }
        Mock Get-CimInstance {
            if ($Filter -match "SysMain") {
                [PSCustomObject]@{ StartMode = "Auto" }
            } else {
                [PSCustomObject]@{ StartMode = "Disabled" }
            }
        } -ParameterFilter { $ClassName -eq "Win32_Service" }

        $results = Invoke-CheckServices

        ($results | Where-Object Item -eq "SysMain (Superfetch)").Status | Should -Be "WARN"
        ($results | Where-Object Item -eq "Windows Search").Current | Should -Be "Not present"
        ($results | Where-Object Item -eq "Windows Search").Status | Should -Be "OK"
    }
}
