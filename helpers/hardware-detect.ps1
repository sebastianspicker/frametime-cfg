# ==============================================================================
#  helpers/hardware-detect.ps1  —  RAM, GPU, NIC, Chipset Detection
# ==============================================================================

# ── CPU cache (lazy-initialized) ─────────────────────────────────────────────
# Win32_Processor queries take ~50-200ms each. Cache the result for all callers.
# Call Reset-CachedCpuInfo in tests to clear the cache between mocked scenarios.
$Script:_cachedCpuInfo = $null
function Get-CachedCpuInfo {
    if ($null -eq $Script:_cachedCpuInfo) {
        $Script:_cachedCpuInfo = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue |
            Select-Object -First 1
    }
    return $Script:_cachedCpuInfo
}
function Reset-CachedCpuInfo { $Script:_cachedCpuInfo = $null }

# ── XMP / RAM ────────────────────────────────────────────────────────────────

function Get-RamInfo {
    <#  Returns actual RAM speed and capacity.  #>
    try {
        $sticks = @(Get-CimInstance Win32_PhysicalMemory)
        $totalGB = [math]::Round(($sticks | Measure-Object Capacity -Sum).Sum / 1GB, 0)
        $speedMhz = ($sticks | Select-Object -First 1).Speed
        $configMhz = ($sticks | Select-Object -First 1).ConfiguredClockSpeed
        # Detect DDR type from SMBIOSMemoryType: 34 = DDR5, 26 = DDR4, 24 = DDR3.
        $memType = ($sticks | Select-Object -First 1).SMBIOSMemoryType
        $isDDR5 = ($memType -eq 34)

        # Normalize ConfiguredClockSpeed to the same unit as Speed (MT/s for DDR5).
        # Some systems report ConfiguredClockSpeed as clock MHz (half of MT/s),
        # others report it in the same MT/s unit as Speed. Detect which by checking
        # whether configMhz is roughly half of speedMhz or roughly equal.
        $activeMTs = if ($isDDR5 -and $configMhz -gt 0 -and $speedMhz -gt 0) {
            if ($configMhz -le ($speedMhz * 0.6)) {
                $configMhz * 2   # clock MHz → MT/s
            } else {
                $configMhz       # already MT/s
            }
        } else {
            $configMhz           # DDR4/DDR3: both values in MHz
        }

        # XMP/EXPO detection: WMI cannot reliably distinguish XMP from JEDEC when
        # the module runs at its rated speed. We can only detect "below rated speed"
        # (definitely not XMP) vs "at rated speed" (ambiguous — could be either).
        # AtRatedSpeed=true means no action needed; AtRatedSpeed=false means the
        # user should enable XMP/EXPO in BIOS to reach the module's rated speed.
        $atRatedSpeed = if ($activeMTs -gt 0 -and $speedMhz -gt 0) {
            $activeMTs -ge ($speedMhz * 0.95)
        } else { $false }

        return @{
            TotalGB      = $totalGB
            SpeedMhz     = $speedMhz        # Rated module speed (MT/s for DDR5)
            ActiveMhz    = $activeMTs        # Running speed, normalized to same unit as SpeedMhz
            Sticks       = $sticks.Count
            IsDDR5       = $isDDR5
            XmpActive    = $atRatedSpeed     # Compat: true = at rated speed, false = below rated
            AtRatedSpeed = $atRatedSpeed
        }
    } catch {
        Write-DebugLog "RAM info error: $_"
        return $null
    }
}

function Test-XmpActive {
    $ram = Get-RamInfo
    if (-not $ram) { return $null }
    return $ram.XmpActive
}

# ── NVIDIA Driver ────────────────────────────────────────────────────────────

function Get-NvidiaDriverVersion {
    try {
        $gpu = Get-CimInstance Win32_VideoController | Where-Object { $_.Name -match "NVIDIA" } | Select-Object -First 1
        if (-not $gpu -or -not $gpu.DriverVersion) { return $null }
        # DriverVersion "31.0.15.5762" → NVIDIA 557.62
        # Decode: concatenate last two segments, drop leading char, split major/minor
        $parts = $gpu.DriverVersion.Split('.')
        if ($parts.Count -lt 4) {
            Write-DebugLog "NVIDIA DriverVersion has unexpected format (expected >= 4 dot-separated parts): $($gpu.DriverVersion)"
            return $null
        }
        $combined = "$($parts[-2])$($parts[-1])"
        if ($combined.Length -lt 5) {
            Write-DebugLog "NVIDIA version combined segment too short for reliable decode: $combined"
            return $null
        }
        $nvStr = $combined.Substring(1)  # Remove Windows prefix digit
        if ($nvStr.Length -ge 3) {
            $major = [int]$nvStr.Substring(0, $nvStr.Length - 2)
            $minor = [int]$nvStr.Substring($nvStr.Length - 2)
            $ver = "$major.$minor"
        } else {
            $major = [int]$nvStr
            $ver = "$major"
        }
        return @{ Version = $ver; Major = $major; Name = $gpu.Name }
    } catch { Write-DebugLog "NVIDIA driver detection error: $_"; return $null }
}

# Known problematic driver ranges (as of 2025/2026)
# R570+ (576.x and above) causes stutter on some CS2 systems
$NVIDIA_PROBLEMATIC_MAJOR = 576
$NVIDIA_STABLE_VERSION    = "566.36"

# ── Benchmark Parser ─────────────────────────────────────────────────────────

function Parse-BenchmarkOutput($text) {
    $pattern = '\[VProf\]\s*FPS:\s*Avg\s*=\s*([\d.]+)\s*,\s*P1\s*=\s*([\d.]+)'
    $m = [regex]::Matches($text, $pattern)
    if ($m.Count -eq 0) { return $null }
    $avgs = @($m | ForEach-Object { [float]::Parse($_.Groups[1].Value, [System.Globalization.CultureInfo]::InvariantCulture) })
    $p1s  = @($m | ForEach-Object { [float]::Parse($_.Groups[2].Value, [System.Globalization.CultureInfo]::InvariantCulture) })
    return @{
        Avg    = [math]::Round(($avgs | Measure-Object -Average).Average, 1)
        P1     = [math]::Round(($p1s  | Measure-Object -Average).Average, 1)
        Runs   = $m.Count
        RawAvg = $avgs; RawP1 = $p1s
    }
}

function Calculate-FpsCap($avgFps) {
    # Use [math]::Floor instead of [int] cast to avoid banker's rounding
    # (PowerShell [int] cast uses MidpointRounding.ToEven, e.g. [int]2.5 = 2, [int]3.5 = 4)
    return [math]::Max($CFG_FpsCap_Min, [math]::Floor($avgFps - [math]::Floor($avgFps * $CFG_FpsCap_Percent)))
}

# ── Steam + CS2 Install Path ─────────────────────────────────────────────────

function Get-SteamPath {
    <#  Returns the Steam installation root directory from the registry, or $null.
        Single source of truth for all callers that need the Steam base path.  #>
    $reg = Get-ItemProperty "HKCU:\SOFTWARE\Valve\Steam" -Name "SteamPath" -ErrorAction SilentlyContinue
    if ($reg -and $reg.PSObject.Properties['SteamPath'] -and $reg.SteamPath) { return $reg.SteamPath }
    return $null
}

function Get-CS2InstallPath {
    <#  Finds CS2 install directory via Steam registry + libraryfolders.vdf  #>
    $steamPath = Get-SteamPath
    if (-not $steamPath) { return $null }

    $defaultCS2 = "$steamPath\steamapps\common\Counter-Strike Global Offensive"
    if (Test-Path "$defaultCS2\game\bin\win64\cs2.exe") { return $defaultCS2 }

    $vdf = "$steamPath\steamapps\libraryfolders.vdf"
    if (Test-Path $vdf) {
        # Read as UTF-8 explicitly — PS 5.1 defaults to ANSI codepage, which
        # mangles Unicode library paths (e.g., non-ASCII usernames or drive labels).
        $content = Get-Content $vdf -Raw -Encoding UTF8
        $paths = [regex]::Matches($content, '"path"\s+"([^"]+)"') | ForEach-Object { $_.Groups[1].Value -replace '\\\\', '\' -replace '/', '\' }
        foreach ($lp in $paths) {
            # SECURITY: VDF is a Valve text file in userspace — a tampered libraryfolders.vdf
            # could contain paths with traversal sequences. Reject paths with .. components.
            # The path is only used in Test-Path and Get-Content on autoexec.cfg / optimization.cfg.
            # Symlink/junction attacks: if CS2 install path is a junction pointing to e.g. C:\Windows,
            # Set-Content would write to that location. The cs2.exe existence check below mitigates
            # this — a junction to C:\Windows would fail the cs2.exe check. Accepted residual risk:
            # if an attacker creates a junction containing a fake cs2.exe, we'd write autoexec there.
            if ($lp -match '\.\.') {
                Write-DebugLog "VDF path rejected (path traversal): $lp"
                continue
            }
            $cs2Path = "$lp\steamapps\common\Counter-Strike Global Offensive"
            if (Test-Path "$cs2Path\game\bin\win64\cs2.exe") { return $cs2Path }
        }
    }

    foreach ($d in @("C","D","E","F")) {
        foreach ($base in @("$($d):\Steam","$($d):\Program Files (x86)\Steam","$($d):\Program Files\Steam","$($d):\SteamLibrary")) {
            $cs2Path = "$base\steamapps\common\Counter-Strike Global Offensive"
            if (Test-Path "$cs2Path\game\bin\win64\cs2.exe") { return $cs2Path }
        }
    }
    return $null
}

# ── Active NIC Adapter ───────────────────────────────────────────────────────

function Get-ActiveNicAdapter {
    <#  Returns the active (Up) wired network adapter object, or $null.
        Centralized NIC selection logic — used by NIC tweaks, RSS config, etc.
        Best-effort heuristic: may exclude USB Ethernet or include unintended adapters.  #>
    try {
        # Combine hardcoded essentials with configurable $CFG_VirtualAdapterFilter
        $filterPattern = "Wi-Fi|Wireless"
        # NOTE: $CFG_VirtualAdapterFilter is a regex alternation pattern (e.g., "Hyper-V|VPN|Virtual")
        # — metacharacters in adapter names would need escaping before inclusion here.
        if ((Get-Variable -Name CFG_VirtualAdapterFilter -ErrorAction SilentlyContinue) -and $CFG_VirtualAdapterFilter) {
            try {
                $testPattern = "$filterPattern|$CFG_VirtualAdapterFilter"
                $null = [regex]$testPattern
                $filterPattern = $testPattern
            } catch {
                Write-DebugLog "CFG_VirtualAdapterFilter contains invalid regex — ignored: $_"
            }
        }
        return Get-NetAdapter -ErrorAction Stop | Where-Object {
            $_.Status -eq "Up" -and
            $_.InterfaceDescription -notmatch $filterPattern
        } | Sort-Object { $_.Speed } -Descending | Select-Object -First 1
    } catch {
        Write-DebugLog "Get-ActiveNicAdapter error: $_"
        return $null
    }
}

function Get-ActiveNicGuid {
    <#  Returns the GUID of the active (Up) wired network adapter for registry writes.
        SECURITY: The GUID is used in registry paths (e.g., Tcpip\Parameters\Interfaces\{GUID}).
        The value comes from Get-NetAdapter (system WMI), not user input. A malicious NIC driver
        could theoretically report a GUID containing path injection characters, but Windows
        enforces GUID format in the network stack. We validate the format as defense-in-depth.  #>
    $nic = Get-ActiveNicAdapter
    if ($nic) {
        $guid = $nic.InterfaceGuid
        # Defense-in-depth: validate GUID format {xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}
        if ($guid -and $guid -match '^\{[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}\}$') {
            return $guid
        }
        Write-Warn "NIC GUID failed format validation: $guid"
        return $null
    }
    return $null
}

# ── Intel 12th-gen+ / Core Ultra CPU Detection ──────────────────────────────

function Get-IntelHybridCpuName {
    <#  Returns the CPU name string if this is an Intel 12th-gen-or-newer Core
        (12xxx–19xxx series with suffix) or Intel Core Ultra CPU, empty string ""
        if the CPU was detected but is NOT an Intel hybrid, or $null if CPU
        detection itself failed. Used to gate Intel 12th-gen+ / Ultra-specific
        tweaks (e.g., PowerThrottlingOff, thread_pool_option). Note: this is a
        coarse heuristic and may include some non-hybrid (no E-core) SKUs.

        Return value semantics:
          "Intel Core i9-12900K"  -> Intel hybrid detected (truthy)
          ""                      -> CPU detected, not Intel hybrid (falsy, not $null)
          $null                   -> detection failed (CIM error)
    #>
    try {
        $cpuObj = Get-CachedCpuInfo
        $cpuName = if ($cpuObj) { $cpuObj.Name } else { $null }
        if (-not $cpuName) { return $null }
        if ($cpuName -match "Intel" -and (
            $cpuName -match "\b1[2-9]\d{3}[A-Z]" -or  # 12th–19th gen Core-series (suffix required)
            $cpuName -match "\bUltra\b"                 # Core Ultra (Meteor Lake / Arrow Lake)
        )) {
            return $cpuName
        }
        # CPU detected successfully but is not an Intel hybrid — return empty string
        # (falsy but distinguishable from $null which means detection failed)
        return ""
    } catch {
        Write-DebugLog "Intel 12th-gen+ CPU detection failed: $_"
        return $null
    }
}

# ── Chipset Vendor ───────────────────────────────────────────────────────────

function Get-ChipsetVendor {
    <#  Returns "AMD" or "Intel" based on CPU manufacturer  #>
    try {
        $cpu = (Get-CachedCpuInfo).Manufacturer
        if ($cpu -match "AMD")   { return "AMD" }
        if ($cpu -match "Intel") { return "Intel" }
    } catch { Write-DebugLog "CPU vendor detection failed: $($_.Exception.Message)" }
    return "Unknown"
}

# ── Dual-Channel RAM ────────────────────────────────────────────────────────

function Test-DualChannel {
    <#  Checks if RAM is likely running in dual-channel (2+ sticks in different banks/slots)  #>
    try {
        $sticks = @(Get-CimInstance Win32_PhysicalMemory)
        if ($sticks.Count -lt 2) {
            return @{ DualChannel = $false; Sticks = $sticks.Count; Reason = "Only $($sticks.Count) RAM stick detected — single-channel." }
        }
        # Check BankLabel first, but some BIOSes report the same BankLabel for all DIMMs
        # (e.g., "BANK 0" for everything). Fall back to DeviceLocator which is more reliable
        # on modern systems (e.g., "DIMM_A1", "DIMM_B1" — different letters = different channels).
        $banks = @($sticks | ForEach-Object {
            if ($_.PSObject.Properties['BankLabel']) { $_.BankLabel }
        } | Where-Object { $_ } | Select-Object -Unique)
        if ($banks.Count -ge 2) {
            return @{ DualChannel = $true;  Sticks = $sticks.Count; Reason = "$($sticks.Count) sticks in $($banks.Count) banks — dual-channel likely." }
        }
        # BankLabel was same or empty — check DeviceLocator for channel letters (A/B, 0/1)
        $locators = @($sticks | ForEach-Object {
            if ($_.PSObject.Properties['DeviceLocator']) { $_.DeviceLocator }
        } | Where-Object { $_ } | Select-Object -Unique)
        if ($locators.Count -ge 2) {
            # Extract channel identifiers (e.g., "A" from "DIMM_A1", "B" from "DIMM_B2")
            $channels = $locators | ForEach-Object {
                if ($_ -match '[_]([A-Z])\d') { $Matches[1] }
                elseif ($_ -match 'Channel\s*([A-Z0-9])') { $Matches[1] }
                elseif ($_ -match 'DIMM\s+([A-Z])\d') { $Matches[1] }
            } | Where-Object { $_ } | Select-Object -Unique
            if ($channels.Count -ge 2) {
                return @{ DualChannel = $true; Sticks = $sticks.Count; Reason = "$($sticks.Count) sticks across $($channels.Count) channels (DeviceLocator) — dual-channel likely." }
            }
        }
        return @{ DualChannel = $false; Sticks = $sticks.Count; Reason = "$($sticks.Count) sticks but same bank/channel — possibly wrong slots." }
    } catch {
        return @{ DualChannel = $null; Sticks = 0; Reason = "Could not read RAM info." }
    }
}

# ── AMD Ryzen X3D CPU Info ─────────────────────────────────────────────────

function Get-AmdCpuInfo {
    <#
    .SYNOPSIS  Detects AMD Ryzen CPU details for X3D-specific BIOS guidance.
    .DESCRIPTION
        Returns structured info about AMD CPUs including X3D status, generation,
        and per-model recommended PBO/CO settings from the X3D tuning guide.

        Return value semantics:
          @{ IsAMD=$true; IsX3D=$true; ... }  -> AMD X3D detected
          @{ IsAMD=$true; IsX3D=$false; ... } -> AMD but not X3D
          @{ IsAMD=$false; ... }              -> Not AMD (Intel/other)
          $null                                -> Detection failed
    #>
    try {
        $cpu = Get-CachedCpuInfo
        if (-not $cpu -or -not $cpu.Name) { return $null }
        $name = $cpu.Name.Trim()

        $isAMD = $cpu.Manufacturer -match "AMD"
        if (-not $isAMD) {
            return @{ IsAMD = $false; IsX3D = $false; CpuName = $name }
        }

        $isX3D = $name -match "X3D"
        $isDualCCD = $name -match "(7900X3D|7950X3D|9900X3D|9950X3D)"
        $isSingleCCD = $isX3D -and -not $isDualCCD

        # Zen 5 (9000-series) vs Zen 4 (7000-series) vs Zen 3 (5000-series)
        $isZen5 = $name -match "\b9[0-9]{2,3}X3D\b"
        $isZen4 = $name -match "\b7[0-9]{2,3}X3D\b"

        # Per-model recommended settings from X3D tuning guide
        $recommendedCO = if ($isZen5) { -20 } elseif ($isZen4) { -15 } else { -10 }
        $recommendedBoostOverride = if ($isZen5) { 200 } else { 0 }
        $expectedBoostMhz = if ($name -match "9800X3D") { 5200 }
            elseif ($name -match "9900X3D") { 5500 }
            elseif ($name -match "9950X3D") { 5700 }
            elseif ($name -match "9700X3D") { 5100 }
            elseif ($name -match "7800X3D") { 5050 }
            elseif ($name -match "7900X3D") { 5600 }
            elseif ($name -match "7950X3D") { 5700 }
            elseif ($name -match "5800X3D") { 4500 }
            elseif ($name -match "5700X3D") { 4100 }
            else { 0 }

        $maxTempC = if ($isZen5) { 90 } else { 85 }

        return @{
            IsAMD                    = $true
            IsX3D                    = $isX3D
            IsSingleCCD              = $isSingleCCD
            IsDualCCD                = $isDualCCD
            IsZen5                   = $isZen5
            IsZen4                   = $isZen4
            CpuName                  = $name
            RecommendedCO            = $recommendedCO
            RecommendedBoostOverride = $recommendedBoostOverride
            ExpectedBoostMhz         = $expectedBoostMhz
            MaxTempC                 = $maxTempC
            CoreCount                = $cpu.NumberOfCores
            LogicalProcessors        = $cpu.NumberOfLogicalProcessors
            MaxClockSpeed            = $cpu.MaxClockSpeed
        }
    } catch {
        Write-DebugLog "AMD CPU detection error: $_"
        return $null
    }
}

# ── DDR5 Timing Info ───────────────────────────────────────────────────────

function Get-Ddr5TimingInfo {
    <#
    .SYNOPSIS  Extended DDR5 timing analysis for FCLK/MCLK 1:1 verification.
    .DESCRIPTION
        Builds on Get-RamInfo with DDR5-specific checks: FCLK:MCLK ratio,
        rated vs active speed gap (detects downclocked EXPO kits), and
        DIMM count for dual-channel verification.
    #>
    $ram = Get-RamInfo
    if (-not $ram -or -not $ram.IsDDR5) { return $null }

    # Get-RamInfo already normalizes DDR5 active speed to MT/s.
    $activeMTs = $ram.ActiveMhz
    # FCLK on AM5: max stable 1:1 is 2000 MHz (DDR5-6000, MCLK 3000).
    # This is an approximation — actual FCLK ceiling varies by silicon quality.
    # For display purposes only; not used in any gating logic.
    $expectedFclk = [math]::Min($ram.ActiveMhz, 2000)
    # Detect if kit is downclocked (rated > active, e.g., 8200 → 6000)
    $isDownclocked = ($ram.SpeedMhz -gt ($activeMTs * 1.05))
    # Optimal AM5/X3D window is centered around DDR5-6000 MT/s.
    $isOptimal1to1 = ($activeMTs -ge 5600 -and $activeMTs -le 6400)

    return @{
        TotalGB        = $ram.TotalGB
        SpeedMhz       = $ram.SpeedMhz
        ActiveMhz      = $ram.ActiveMhz
        ActiveMTs      = $activeMTs
        Sticks         = $ram.Sticks
        IsDDR5         = $true
        XmpActive      = $ram.XmpActive
        IsDownclocked  = $isDownclocked
        IsOptimal1to1  = $isOptimal1to1
        ExpectedFclk   = $expectedFclk
        RatedMTs       = $ram.SpeedMhz
    }
}

# ── WHEA Hardware Error Check ──────────────────────────────────────────────

function Test-WheaErrors {
    <#
    .SYNOPSIS  Checks Windows Event Log for WHEA (hardware) errors.
    .DESCRIPTION
        Queries System log for WHEA-Logger events which indicate hardware
        instability (CPU, RAM, PCIe). After PBO/CO tuning, WHEA errors
        mean the undervolt is too aggressive.

        Returns:
          @{ Count=0; Recent=@(); HasErrors=$false }  -> clean
          @{ Count=5; Recent=@(...); HasErrors=$true } -> instability detected
          $null -> query failed (e.g., insufficient permissions)
    #>
    try {
        $events = @(Get-WinEvent -FilterHashtable @{
            LogName      = 'System'
            ProviderName = 'Microsoft-Windows-WHEA-Logger'
        } -MaxEvents 50 -ErrorAction SilentlyContinue)

        # Filter to last 24 hours for actionable count
        $cutoff = (Get-Date).AddHours(-24)
        $recent = @($events | Where-Object { $_.TimeCreated -ge $cutoff })

        return @{
            Count     = $events.Count
            Recent    = $recent
            RecentCount = $recent.Count
            HasErrors = ($recent.Count -gt 0)
            LastError = if ($events.Count -gt 0) { $events[0].TimeCreated } else { $null }
        }
    } catch {
        Write-DebugLog "WHEA event query failed: $_"
        return $null
    }
}

# ── Motherboard Detection ──────────────────────────────────────────────────

function Get-MotherboardInfo {
    <#
    .SYNOPSIS  Returns motherboard manufacturer and product name.
    .DESCRIPTION  Used for board-specific BIOS guidance (e.g., ASUS Strix M.2 slots).
    #>
    try {
        $board = Get-CimInstance Win32_BaseBoard -ErrorAction Stop | Select-Object -First 1
        return @{
            Manufacturer = if ($board.Manufacturer) { $board.Manufacturer.Trim() } else { "" }
            Product      = if ($board.Product) { $board.Product.Trim() } else { "" }
            IsASUS       = ($board.Manufacturer -match "ASUSTeK|ASUS")
        }
    } catch {
        Write-DebugLog "Motherboard detection error: $_"
        return $null
    }
}
