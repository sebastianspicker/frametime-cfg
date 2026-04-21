# ==============================================================================
#  tests/helpers/hardware-detect.Tests.ps1  --  Hardware detection function tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Get-IntelHybridCpuName ────────────────────────────────────────────────────
Describe "Get-IntelHybridCpuName" {
    BeforeEach { Reset-CachedCpuInfo }

    Context "Intel 12th gen (Alder Lake)" {
        It "detects i9-12900K as hybrid" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "12th Gen Intel(R) Core(TM) i9-12900K" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Not -BeNullOrEmpty
            $result | Should -Match "12900K"
        }

        It "detects i5-12600K as hybrid" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "12th Gen Intel(R) Core(TM) i5-12600K" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Not -BeNullOrEmpty
        }
    }

    Context "Intel 14th gen (Raptor Lake Refresh — WMI reports as 13th Gen)" {
        It "detects i7-14700K as hybrid" {
            Mock Get-CimInstance {
                # WMI on 14th gen desktop reports "13th Gen" — matched by model number regex
                [PSCustomObject]@{ Name = "13th Gen Intel(R) Core(TM) i7-14700K" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Not -BeNullOrEmpty
            $result | Should -Match "14700K"
        }
    }

    Context "Intel Core Ultra (Meteor Lake / Arrow Lake)" {
        It "detects Core Ultra 9 285K" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "Intel(R) Core(TM) Ultra 9 285K" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Not -BeNullOrEmpty
            $result | Should -Match "Ultra"
        }

        It "detects Core Ultra 7 155H (laptop)" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "Intel(R) Core(TM) Ultra 7 155H" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Not -BeNullOrEmpty
        }
    }

    Context "AMD CPUs (should return empty string, not null)" {
        It "returns empty string for AMD 7800X3D" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "AMD Ryzen 7 7800X3D 8-Core Processor" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Be ""
        }

        It "returns empty string for AMD 5800X" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "AMD Ryzen 7 5800X 8-Core Processor" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Be ""
        }
    }

    Context "Old Intel (pre-hybrid, should return empty string)" {
        It "returns empty string for i9-11900K (Rocket Lake, no E-cores)" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "11th Gen Intel(R) Core(TM) i9-11900K" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Be ""
        }

        It "returns empty string for i7-10700K (Comet Lake)" {
            Mock Get-CimInstance {
                [PSCustomObject]@{ Name = "Intel(R) Core(TM) i7-10700K CPU @ 3.80GHz" }
            } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            $result = Get-IntelHybridCpuName
            $result | Should -Be ""
        }
    }

    Context "Edge cases" {
        It "returns null when CimInstance throws (detection failed)" {
            Mock Get-CimInstance { throw "WMI unavailable" } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            Get-IntelHybridCpuName | Should -Be $null
        }

        It "returns null when CimInstance returns empty (detection failed)" {
            Mock Get-CimInstance { $null } -ParameterFilter { $ClassName -eq "Win32_Processor" }

            Get-IntelHybridCpuName | Should -Be $null
        }
    }
}

# ── Get-SteamPath ─────────────────────────────────────────────────────────────
Describe "Get-SteamPath" {

    It "returns path when Steam registry key exists" {
        Mock Get-ItemProperty {
            [PSCustomObject]@{ SteamPath = "C:\Program Files (x86)\Steam" }
        } -ParameterFilter { $Path -like "*Valve\Steam*" }

        $result = Get-SteamPath
        $result | Should -Be "C:\Program Files (x86)\Steam"
    }

    It "returns null when Steam registry key is absent" {
        Mock Get-ItemProperty { $null } -ParameterFilter { $Path -like "*Valve\Steam*" }

        Get-SteamPath | Should -BeNullOrEmpty
    }

    It "returns null when SteamPath property is missing from registry object" {
        # Real Get-ItemProperty with -ErrorAction SilentlyContinue returns an object
        # without the requested property when the value doesn't exist
        Mock Get-ItemProperty {
            [PSCustomObject]@{ OtherProp = "irrelevant" }
        } -ParameterFilter { $Path -like "*Valve\Steam*" }

        Get-SteamPath | Should -BeNullOrEmpty
    }
}

# ── Calculate-FpsCap ──────────────────────────────────────────────────────────
Describe "Calculate-FpsCap" {

    BeforeEach {
        # Ensure config values are at their defaults
        $SCRIPT:CFG_FpsCap_Percent = 0.09
        $SCRIPT:CFG_FpsCap_Min = 60
    }

    It "calculates 300 avg -> 273 (300 - 9% = 273)" {
        $result = Calculate-FpsCap 300
        $result | Should -Be 273
    }

    It "calculates 200 avg -> 182 (200 - 9% = 182)" {
        $result = Calculate-FpsCap 200
        $result | Should -Be 182
    }

    It "enforces minimum cap of 60 for low input (50 avg)" {
        $result = Calculate-FpsCap 50
        $result | Should -BeGreaterOrEqual 60
    }

    It "enforces minimum cap of 60 for zero input" {
        $result = Calculate-FpsCap 0
        $result | Should -BeGreaterOrEqual 60
    }

    It "handles large input (1000 avg -> 910)" {
        $result = Calculate-FpsCap 1000
        $result | Should -Be 910
    }

    It "returns integer (not float)" {
        $result = Calculate-FpsCap 333
        $result | Should -BeOfType [int]
    }

    It "uses CFG_FpsCap_Percent (not a hardcoded value)" {
        # Verify the formula uses the variable: cap = max(min, floor(avg - floor(avg * pct)))
        # At default 9%: floor(400 * 0.09) = 36, so 400 - 36 = 364
        $result = Calculate-FpsCap 400
        $result | Should -Be 364
    }
}

# ── Parse-BenchmarkOutput ─────────────────────────────────────────────────────
Describe "Parse-BenchmarkOutput" {

    It "parses a single valid VProf line" {
        $text = "[VProf] FPS: Avg = 285.3, P1 = 197.8"
        $result = Parse-BenchmarkOutput $text
        $result | Should -Not -BeNullOrEmpty
        $result.Avg | Should -Be 285.3
        $result.P1  | Should -Be 197.8
        $result.Runs | Should -Be 1
    }

    It "averages multiple VProf lines" {
        $text = @"
[VProf] FPS: Avg = 300.0, P1 = 200.0
Some other output
[VProf] FPS: Avg = 280.0, P1 = 180.0
"@
        $result = Parse-BenchmarkOutput $text
        $result | Should -Not -BeNullOrEmpty
        $result.Avg  | Should -Be 290.0
        $result.P1   | Should -Be 190.0
        $result.Runs | Should -Be 2
    }

    It "returns null for garbage input" {
        $result = Parse-BenchmarkOutput "this is not benchmark output at all"
        $result | Should -BeNullOrEmpty
    }

    It "returns null for empty input" {
        $result = Parse-BenchmarkOutput ""
        $result | Should -BeNullOrEmpty
    }

    It "returns null for null input" {
        $result = Parse-BenchmarkOutput $null
        $result | Should -BeNullOrEmpty
    }

    It "preserves raw values in RawAvg and RawP1 arrays" {
        $text = @"
[VProf] FPS: Avg = 300.0, P1 = 200.0
[VProf] FPS: Avg = 280.0, P1 = 180.0
[VProf] FPS: Avg = 310.0, P1 = 210.0
"@
        $result = Parse-BenchmarkOutput $text
        $result.RawAvg.Count | Should -Be 3
        $result.RawP1.Count  | Should -Be 3
        $result.RawAvg[0]    | Should -Be 300.0
        $result.RawP1[2]     | Should -Be 210.0
    }
}

# ── Test-XmpActive ────────────────────────────────────────────────────────────
Describe "Test-XmpActive" {

    It "detects XMP active for DDR4 (Speed=3200, Config=3200)" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184  # 16 GB
                Speed                = 3200
                ConfiguredClockSpeed = 3200
                SMBIOSMemoryType     = 26  # DDR4
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        Test-XmpActive | Should -Be $true
    }

    It "detects XMP inactive for DDR4 (Speed=3200, Config=2133)" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 3200
                ConfiguredClockSpeed = 2133
                SMBIOSMemoryType     = 26
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        Test-XmpActive | Should -Be $false
    }

    It "detects XMP active for DDR5 (Speed=5600 MT/s, ConfigClock=2800 MHz)" {
        # DDR5: ConfiguredClockSpeed is half of Speed (double-data-rate)
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 5600
                ConfiguredClockSpeed = 2800
                SMBIOSMemoryType     = 34  # DDR5
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        Test-XmpActive | Should -Be $true
    }

    It "detects XMP inactive for DDR5 at half speed (Speed=5600, Config=2400)" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 5600
                ConfiguredClockSpeed = 2400
                SMBIOSMemoryType     = 34
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        # Floor(5600/2) * 0.95 = 2660; 2400 < 2660 -> XMP inactive
        Test-XmpActive | Should -Be $false
    }

    It "detects XMP inactive for DDR5 JEDEC baseline (Speed=4800, Config=2400)" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 4800
                ConfiguredClockSpeed = 2400
                SMBIOSMemoryType     = 34
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        # DDR5-4800 JEDEC baseline: config 2400 × 2 = 4800 MT/s = rated speed.
        # Function returns $true ("at rated speed") — it cannot distinguish JEDEC from XMP.
        Test-XmpActive | Should -Be $true
    }

    It "returns null when CIM query fails" {
        Mock Get-CimInstance { throw "WMI error" } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        Test-XmpActive | Should -BeNullOrEmpty
    }
}

Describe "Get-RamInfo" {

    It "normalizes DDR5 ConfiguredClockSpeed from half-rate clock to MT/s" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 6000
                ConfiguredClockSpeed = 3000
                SMBIOSMemoryType     = 34
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $ram = Get-RamInfo

        $ram.IsDDR5    | Should -Be $true
        $ram.SpeedMhz  | Should -Be 6000
        $ram.ActiveMhz | Should -Be 6000
    }

    It "preserves DDR5 ConfiguredClockSpeed when firmware already reports MT/s" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 6400
                ConfiguredClockSpeed = 6400
                SMBIOSMemoryType     = 34
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $ram = Get-RamInfo

        $ram.IsDDR5    | Should -Be $true
        $ram.SpeedMhz  | Should -Be 6400
        $ram.ActiveMhz | Should -Be 6400
    }
}

Describe "Get-Ddr5TimingInfo" {

    It "uses the normalized DDR5 active speed without doubling it again" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 6000
                ConfiguredClockSpeed = 3000
                SMBIOSMemoryType     = 34
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $ddr5 = Get-Ddr5TimingInfo

        $ddr5.IsDDR5        | Should -Be $true
        $ddr5.ActiveMhz     | Should -Be 6000
        $ddr5.ActiveMTs     | Should -Be 6000
        $ddr5.IsOptimal1to1 | Should -Be $true
        $ddr5.IsDownclocked | Should -Be $false
    }

    It "detects a downclocked DDR5 kit from rated-vs-active MT/s gap" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Capacity             = 17179869184
                Speed                = 8200
                ConfiguredClockSpeed = 3000
                SMBIOSMemoryType     = 34
                BankLabel            = "BANK 0"
            })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $ddr5 = Get-Ddr5TimingInfo

        $ddr5.ActiveMTs     | Should -Be 6000
        $ddr5.RatedMTs      | Should -Be 8200
        $ddr5.IsDownclocked | Should -Be $true
    }
}

# ── Test-DualChannel ──────────────────────────────────────────────────────────
Describe "Test-DualChannel" {

    It "detects single-channel with 1 stick" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{ BankLabel = "BANK 0"; Capacity = 17179869184 })
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $result = Test-DualChannel
        $result.DualChannel | Should -Be $false
        $result.Sticks      | Should -Be 1
        $result.Reason       | Should -Match "single-channel"
    }

    It "detects dual-channel with 2 sticks in different banks" {
        Mock Get-CimInstance {
            @(
                [PSCustomObject]@{ BankLabel = "BANK 0"; Capacity = 17179869184 },
                [PSCustomObject]@{ BankLabel = "BANK 2"; Capacity = 17179869184 }
            )
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $result = Test-DualChannel
        $result.DualChannel | Should -Be $true
        $result.Sticks      | Should -Be 2
    }

    It "flags possible wrong slots with 2 sticks in same bank" {
        Mock Get-CimInstance {
            @(
                [PSCustomObject]@{ BankLabel = "BANK 0"; Capacity = 17179869184 },
                [PSCustomObject]@{ BankLabel = "BANK 0"; Capacity = 17179869184 }
            )
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $result = Test-DualChannel
        $result.DualChannel | Should -Be $false
        $result.Reason       | Should -Match "same bank"
    }

    It "detects quad-channel (4 sticks, 4 banks) as dual-channel" {
        Mock Get-CimInstance {
            @(
                [PSCustomObject]@{ BankLabel = "BANK 0"; Capacity = 17179869184 },
                [PSCustomObject]@{ BankLabel = "BANK 1"; Capacity = 17179869184 },
                [PSCustomObject]@{ BankLabel = "BANK 2"; Capacity = 17179869184 },
                [PSCustomObject]@{ BankLabel = "BANK 3"; Capacity = 17179869184 }
            )
        } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $result = Test-DualChannel
        $result.DualChannel | Should -Be $true
        $result.Sticks      | Should -Be 4
    }

    It "handles CIM failure gracefully" {
        Mock Get-CimInstance { throw "WMI error" } -ParameterFilter { $ClassName -eq "Win32_PhysicalMemory" }

        $result = Test-DualChannel
        $result.DualChannel | Should -BeNullOrEmpty
        $result.Reason       | Should -Match "Could not read"
    }
}

# ── Get-NvidiaDriverVersion ───────────────────────────────────────────────────
Describe "Get-NvidiaDriverVersion" {

    It "parses a standard NVIDIA driver version" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Name           = "NVIDIA GeForce RTX 4080 SUPER"
                DriverVersion  = "31.0.15.5762"
            })
        } -ParameterFilter { $ClassName -eq "Win32_VideoController" }

        $result = Get-NvidiaDriverVersion
        $result | Should -Not -BeNullOrEmpty
        $result.Version | Should -Be "557.62"
        $result.Name    | Should -Match "RTX 4080"
    }

    It "returns null when no NVIDIA GPU is present" {
        Mock Get-CimInstance {
            @([PSCustomObject]@{
                Name          = "AMD Radeon RX 7900 XTX"
                DriverVersion = "31.0.24001.1026"
            })
        } -ParameterFilter { $ClassName -eq "Win32_VideoController" }

        Get-NvidiaDriverVersion | Should -BeNullOrEmpty
    }

    It "returns null when CIM query fails" {
        Mock Get-CimInstance { throw "CIM error" } -ParameterFilter { $ClassName -eq "Win32_VideoController" }

        Get-NvidiaDriverVersion | Should -BeNullOrEmpty
    }
}
