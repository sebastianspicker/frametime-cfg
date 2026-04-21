# ==============================================================================
#  helpers/system-utils.ps1  —  Download, Registry, Boot Config, Filesystem
# ==============================================================================

function Invoke-Download {
    [CmdletBinding()]
    param([string]$url, [string]$dest, [string]$name)
    Write-Step "Download: $name"
    Write-DebugLog "URL: $url -> $dest"

    # SECURITY: Defense-in-depth URL allowlist. Currently only NVIDIA drivers are
    # downloaded; the caller (nvidia-driver.ps1) already validates the domain, but
    # this catches any future callers or data-flow changes that bypass that check.
    $allowedDomains = @('nvidia.com', 'download.nvidia.com', 'us.download.nvidia.com',
                        'international.download.nvidia.com')
    try {
        $uri = [System.Uri]::new($url)
        if ($uri.Scheme -ne 'https') {
            Write-Err "Invoke-Download: only HTTPS URLs are allowed — rejected: $url"
            return $false
        }
        $host_ = $uri.Host
        $domainMatch = $allowedDomains | Where-Object { $host_ -eq $_ -or $host_.EndsWith(".$_") }
        if (-not $domainMatch) {
            Write-Err "Invoke-Download: domain '$host_' is not in the download allowlist — rejected."
            Write-Warn "Allowed: $($allowedDomains -join ', ')"
            return $false
        }
    } catch {
        Write-Err "Invoke-Download: invalid URL — $url"
        return $false
    }

    $maxAttempts = 2
    # Set $global: scope so PS 5.1's Invoke-WebRequest sees it (function-scope has no effect in 5.1)
    $oldProgressPref = $global:ProgressPreference
    try {
        $global:ProgressPreference = 'SilentlyContinue'
        for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
            try {
                Invoke-WebRequest -Uri $url -OutFile $dest -UseBasicParsing -TimeoutSec 120
                $fileSize = (Get-Item $dest).Length
                $mb = [math]::Round($fileSize / 1MB, 2)
                # Sanity check: NVIDIA drivers are >100 MB; reject obviously truncated files
                if ($fileSize -lt 1MB) {
                    Write-Warn "Download appears incomplete ($mb MB, expected >100 MB) — removing corrupt file."
                    Remove-Item $dest -Force -ErrorAction SilentlyContinue
                    if ($attempt -lt $maxAttempts) { Write-Info "Retrying..."; continue }
                    Write-Err "Download failed after $maxAttempts attempts (file too small)."
                    Write-Host "  $([char]0x2139) What to do: Your internet connection may be unstable." -ForegroundColor Cyan
                    Write-Host "    Download manually from the URL below and provide the path when prompted." -ForegroundColor Cyan
                    Write-Warn "URL: $url"
                    return $false
                }
                Write-OK "$name ($mb MB)"
                return $true
            } catch {
                if ($attempt -lt $maxAttempts) {
                    Write-Warn "Download attempt $attempt failed: $_ — retrying..."
                    Remove-Item $dest -Force -ErrorAction SilentlyContinue
                } else {
                    Write-Err "Download failed after $maxAttempts attempts: $_"
                    Write-Host "  $([char]0x2139) What to do: Download the file manually from the URL below" -ForegroundColor Cyan
                    Write-Host "    and provide the path when prompted." -ForegroundColor Cyan
                    Write-Warn "URL: $url"
                    Remove-Item $dest -Force -ErrorAction SilentlyContinue
                    return $false
                }
            }
        }
        return $false
    } finally {
        $global:ProgressPreference = $oldProgressPref
    }
}

function Save-JsonAtomic {
    <#  Writes JSON to a file atomically (write-to-temp-then-rename).
        Prevents corruption if interrupted by crash or power loss.
        NOTE: This does NOT prevent lost updates from concurrent read-modify-write
        cycles. Callers modifying shared files (backup.json, progress.json) should
        acquire the advisory backup lock (Set-BackupLock) before the read step.  #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object]$Data,
        [Parameter(Mandatory)][string]$Path,
        [int]$Depth = 10
    )
    # Ensure parent directory exists — callers usually call Ensure-Dir early,
    # but defensive creation here prevents silent failures from edge-case paths.
    $parentDir = Split-Path $Path -Parent
    if ($parentDir -and -not (Test-Path $parentDir)) {
        New-Item -ItemType Directory -Path $parentDir -Force -ErrorAction Stop | Out-Null
    }
    $leafName = Split-Path -Path $Path -Leaf
    $tmpName = "{0}.{1}.{2}.tmp" -f $leafName, $PID, ([System.IO.Path]::GetRandomFileName())
    $tmp = if ($parentDir) { Join-Path $parentDir $tmpName } else { $tmpName }
    try {
        $json = $Data | ConvertTo-Json -Depth $Depth
        [System.IO.File]::WriteAllText($tmp, $json, [System.Text.UTF8Encoding]::new($false))
        # Move-Item is atomic on NTFS when source and destination are on the same volume
        # (it performs a metadata-only rename operation). This guarantees that $Path is
        # either the old complete file or the new complete file — never a partial write.
        Move-Item $tmp $Path -Force -ErrorAction Stop
    } catch {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
        throw "Save-JsonAtomic failed for '$Path': $_"
    }
}

function Set-SecureAcl {
    <#  Applies an Administrators-only ACL to a sensitive JSON file.
        NOTE: C:\CS2_OPTIMIZE should also inherit restrictive ACLs so newly created
        temp files from Save-JsonAtomic stay protected before this file ACL is re-applied.  #>
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Path)

    if (-not (Test-Path $Path)) { return }
    if (-not (Test-HostIsWindows)) { return }

    try {
        $admins = New-Object System.Security.Principal.NTAccount("BUILTIN", "Administrators")
        $acl = New-Object System.Security.AccessControl.FileSecurity
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule($admins, "FullControl", "Allow")

        $acl.SetOwner($admins)
        $acl.SetAccessRuleProtection($true, $false)
        $acl.SetAccessRule($rule)
        Set-Acl -Path $Path -AclObject $acl -ErrorAction Stop
    } catch {
        Write-Warn "Failed to secure ACL on '$Path': $_"
    }
}

function Save-State($obj, $path) {
    Save-JsonAtomic -Data $obj -Path $path
}

function Test-TrustedSuiteScriptPath {
    [CmdletBinding()]
    param([AllowEmptyString()][string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) { return $false }
    $normalizedPath = $Path -replace '/', '\'
    return (
        $normalizedPath -match '^C:\\CS2_OPTIMIZE\\' -and
        $normalizedPath -notmatch '\\\.\.(\\|$)' -and
        $normalizedPath -match '\.ps1$'
    )
}

function Copy-PhaseRuntimePayload {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$DestinationRoot
    )

    $requiredFiles = @(
        "SafeMode-DriverClean.ps1",
        "PostReboot-Setup.ps1",
        "Guide-VideoSettings.ps1",
        "helpers.ps1",
        "config.env.ps1"
    )
    $requiredDirectories = @(
        "helpers",
        "cfgs"
    )
    $requiredDocs = @(
        "docs/video.txt",
        "docs/nvidia-drs-settings.md"
    )

    Ensure-Dir $DestinationRoot

    foreach ($file in $requiredFiles) {
        $src = Join-Path $SourceRoot $file
        if (-not (Test-Path $src)) {
            throw "Required file missing: $file"
        }
        Copy-Item $src (Join-Path $DestinationRoot $file) -Force -ErrorAction Stop
        Write-OK "Copied: $file"
    }

    foreach ($dir in $requiredDirectories) {
        $src = Join-Path $SourceRoot $dir
        if (-not (Test-Path $src)) {
            throw "Required directory missing: $dir"
        }
        $dest = Join-Path $DestinationRoot $dir
        Ensure-Dir $dest
        Copy-Item (Join-Path $src '*') $dest -Force -Recurse -ErrorAction Stop
        Write-OK "Copied: $dir/"
    }

    foreach ($doc in $requiredDocs) {
        $src = Join-Path $SourceRoot $doc
        if (-not (Test-Path $src)) {
            throw "Required file missing: $doc"
        }
        $dest = Join-Path $DestinationRoot $doc
        $destDir = Split-Path -Path $dest -Parent
        if ($destDir) {
            Ensure-Dir $destDir
        }
        Copy-Item $src $dest -Force -ErrorAction Stop
        Write-OK "Copied: $doc"
    }
}

function Test-Phase1SafeModeReady {
    [CmdletBinding()]
    param($State)

    return (
        $null -ne $State -and
        $State.PSObject.Properties['phase1SafeModeReady'] -and
        $State.phase1SafeModeReady -eq $true
    )
}

function Set-Phase1SafeModeReadyFlag {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Path,
        [bool]$Ready = $true
    )

    if (-not (Test-Path $Path)) {
        throw "Settings file not found at '$Path' — run Phase 1 first (START.bat -> [1])."
    }

    $state = Get-Content $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    $state | Add-Member -NotePropertyName "phase1SafeModeReady" -NotePropertyValue $Ready -Force
    Save-JsonAtomic -Data $state -Path $Path
    Set-SecureAcl -Path $Path
    return $state
}

function Load-State($path) {
    if (-not (Test-Path $path)) { throw "Settings file not found at '$path' — run Phase 1 first (START.bat -> [1])." }
    # -Raw ensures the entire file is read as a single string (consistent with backup-restore.ps1).
    # Without -Raw, multi-line JSON could be split into a string array, causing ConvertFrom-Json to
    # receive individual lines instead of a complete JSON document.
    $raw = Get-Content $path -Raw -ErrorAction Stop
    try {
        $s = $raw | ConvertFrom-Json
    } catch {
        $corruptPath = "$path.corrupt.$(Get-Date -Format 'yyyyMMdd_HHmmss')"
        try { Copy-Item $path $corruptPath -Force -ErrorAction Stop } catch { Write-DebugLog "Could not preserve corrupted state file." }
        Write-Warn "State file corrupt — preserved as $corruptPath"
        throw "State file corrupt — preserved as $corruptPath"
    }
    $SCRIPT:Mode     = if ($s.PSObject.Properties['mode']) { $s.mode } else { $null }
    $SCRIPT:LogLevel = if ($s.PSObject.Properties['logLevel'] -and $s.logLevel) { $s.logLevel } else { "NORMAL" }
    $SCRIPT:Profile  = if ($s.PSObject.Properties['profile'] -and $s.profile) { $s.profile } else { "RECOMMENDED" }
    if (-not $SCRIPT:Mode) {
        Write-DebugLog "Load-State: mode was null/empty — defaulting to CONTROL"
        $SCRIPT:Mode = "CONTROL"
    }
    $SCRIPT:DryRun   = ($SCRIPT:Mode -eq "DRY-RUN")
    return $s
}

function Initialize-ScriptDefaults {
    <#  Soft state loader for entry-point scripts (Cleanup, FpsCap, Verify).
        Loads state.json if present, otherwise sets safe defaults. Never exits.  #>
    if (Test-Path $CFG_StateFile) {
        try {
            # -ErrorAction Stop ensures Get-Content failures throw into the catch block.
            $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
            $SCRIPT:Mode     = $st.mode
            $SCRIPT:LogLevel = if ($st.logLevel) { $st.logLevel } else { "NORMAL" }
            $SCRIPT:Profile  = if ($st.profile) { $st.profile } else { "RECOMMENDED" }
            $SCRIPT:DryRun   = ($SCRIPT:Mode -eq "DRY-RUN")
        } catch {
            $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
        }
    } else {
        $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
    }
}

function Set-RunOnce {
    [CmdletBinding()]
    param([string]$name, [string]$scriptPath, [switch]$SafeMode)
    # SECURITY: Validate RunOnce name — alphanumeric + underscore only.
    # Prevents injection into the HKLM RunOnce key namespace.
    if ($name -notmatch '^[a-zA-Z0-9_]+$') {
        Write-Warn "Set-RunOnce: invalid name '$name' — rejected (security: registry injection prevention)"
        return
    }
    # Windows RunOnce entries prefixed with '*' execute even in Safe Mode.
    # Without the prefix, Safe Mode deletes them without running.
    if ($SafeMode) { $name = "*$name" }
    # SECURITY: Validate script path — must be under C:\CS2_OPTIMIZE\ and end in .ps1.
    # RunOnce executes at boot as the logged-on user with admin elevation (HKLM).
    # If an attacker could set $scriptPath to an arbitrary location, they get code execution.
    $normalizedPath = $scriptPath -replace '/', '\'
    if (-not (Test-TrustedSuiteScriptPath -Path $normalizedPath)) {
        Write-Warn "Set-RunOnce: script path must be under C:\CS2_OPTIMIZE\ and end in .ps1 — rejected: $scriptPath"
        return
    }

    if ($SCRIPT:DryRun) {
        Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would set RunOnce: $name -> $scriptPath" -ForegroundColor Magenta
        return
    }
    # Validate target script exists before registering — a RunOnce pointing to a missing
    # file would silently fail on next boot, leaving Phase 3 unexecuted with no error.
    if (-not (Test-Path $scriptPath)) {
        Write-Warn "RunOnce target does not exist: $scriptPath"
        Write-Host "  $([char]0x2139) What to do: Phase 3 will NOT auto-start on next boot." -ForegroundColor Cyan
        Write-Host "    After rebooting, launch Phase 3 manually: START.bat -> [P]" -ForegroundColor Cyan
        return
    }
    $allowedPolicies = @("Bypass", "RemoteSigned", "AllSigned")
    $executionPolicy = [string]$CFG_RunOnceExecutionPolicy
    if ($executionPolicy -eq "Undefined") {
        Write-Warn "Set-RunOnce: CFG_RunOnceExecutionPolicy 'Undefined' is unsupported on client systems due to policy precedence and GPOs; use one of: $($allowedPolicies -join ', ')"
        return
    }
    if ($executionPolicy -notin $allowedPolicies) {
        Write-Warn "Set-RunOnce: invalid CFG_RunOnceExecutionPolicy '$executionPolicy' — expected one of: $($allowedPolicies -join ', ')"
        return
    }
    # Bypass stays the default because the suite runs locally and is already admin-elevated.
    $cmd = "powershell.exe -NoProfile -ExecutionPolicy $executionPolicy -WindowStyle Normal -File `"$normalizedPath`""
    try {
        Set-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" -Name $name -Value $cmd -ErrorAction Stop
        Write-OK "RunOnce: $name -> $normalizedPath"
    } catch {
        Write-Err "Failed to set RunOnce '$name': $_"
        Write-Host "  $([char]0x2139) What to do: Phase 3 will NOT auto-start after reboot." -ForegroundColor Cyan
        Write-Host "    After rebooting, run Phase 3 manually: START.bat -> [P]" -ForegroundColor Cyan
    }
}

function Set-BootConfig {
    [CmdletBinding()]
    param([string]$key, [string]$val, [string]$why)
    # SECURITY: Validate bcdedit key/value — these are passed as command-line arguments.
    # An attacker who controls state.json or backup.json could inject arbitrary bcdedit args.
    # bcdedit keys are alphanumeric identifiers; values are alphanumeric, hex, or simple tokens.
    if ($key -notmatch '^[a-zA-Z][a-zA-Z0-9_]*$') {
        Write-Warn "Set-BootConfig: invalid key format '$key' — rejected (security: command injection prevention)"
        return $false
    }
    if ($val -notmatch '^[a-zA-Z0-9_.{}\-]+$') {
        Write-Warn "Set-BootConfig: invalid value format '$val' — rejected (security: command injection prevention)"
        return $false
    }

    # Auto-backup before modification
    if ((Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentStepTitle) {
        Backup-BootConfig -Key $key -StepTitle $SCRIPT:CurrentStepTitle
    }
    if ($SCRIPT:DryRun) {
        Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would set: bcdedit /set $key $val  ($why)" -ForegroundColor Magenta
        return $true
    }
    Write-Step "bcdedit /set $key $val  ($why)"
    $output = bcdedit /set $key $val 2>&1
    $bcdeditExit = $LASTEXITCODE
    $outputStr = $output | Out-String
    if ($bcdeditExit -ne 0) {
        Write-Warn "Boot config change failed: bcdedit /set $key $val"
        Write-Host "  $([char]0x2139) This is usually fine — Windows may not support this setting on your PC." -ForegroundColor Cyan
        Write-DebugLog "bcdedit exit $bcdeditExit — $outputStr"
        return $false
    }
    Write-OK "Set: $key = $val"
    return $true
}

function Test-BootConfigSet($key) {
    <#  Verifies a bcdedit value is present in the current BCD entry.
        Uses hex element IDs via /v for locale-independent matching.
        Returns $true if the key exists, $false otherwise.  #>
    $bcdElementMap = @{
        "safeboot"           = "0x26000081"
        "disabledynamictick" = "0x26000060"
        "useplatformtick"    = "0x26000092"
        "useplatformclock"   = "0x26000091"
    }
    $hexId = if ($bcdElementMap.ContainsKey($key)) { $bcdElementMap[$key] } else { $null }
    try {
        # Run bcdedit and capture output — stringify each line to avoid ErrorRecord
        # objects from 2>&1 that would fail regex matching.
        $bcdOutput = bcdedit /enum "{current}" /v 2>&1
        $bcdExit = $LASTEXITCODE
        if ($bcdExit -ne 0) {
            # Fallback: try without /v (some builds return non-zero with /v)
            $bcdOutput = bcdedit /enum "{current}" 2>&1
            $bcdExit = $LASTEXITCODE
            if ($bcdExit -ne 0) { return $false }
            # Without /v, match the friendly key name instead of hex ID
            foreach ($line in $bcdOutput) {
                $s = "$line"
                if ($s -match "^\s*$([regex]::Escape($key))\s+") { return $true }
            }
            return $false
        }
        foreach ($line in $bcdOutput) {
            $s = "$line"   # force to string (ErrorRecords from 2>&1 won't match otherwise)
            if ($hexId -and $s -match "^\s*$hexId\s+") { return $true }
            elseif (-not $hexId -and $s -match "^\s*$([regex]::Escape($key))\s+") { return $true }
        }
    } catch { Write-DebugLog "Test-BootConfigSet: bcdedit enum failed for '$key': $_" }
    return $false
}

function Set-RegistryValue {
    [CmdletBinding()]
    param([string]$path, [string]$name, $value, [string]$type, [string]$why)
    # SECURITY: Validate registry path — must start with a known hive prefix.
    # An attacker who controls backup.json or state.json could inject arbitrary paths
    # to write to sensitive registry locations outside the expected scope.
    if ($path -notmatch '^(HKLM:|HKCU:|HKCR:|HKU:|HKCC:|Microsoft\.PowerShell\.Core\\Registry::HK)') {
        Write-Warn "Set-RegistryValue: path does not start with a valid registry hive — rejected: $path"
        return
    }
    # SECURITY: Registry value name must not contain path separators or null bytes.
    if ($name -match '[\\/\x00]') {
        Write-Warn "Set-RegistryValue: name contains invalid characters — rejected: $name"
        return
    }

    # Auto-backup before modification
    if ((Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentStepTitle) {
        Backup-RegistryValue -Path $path -Name $name -StepTitle $SCRIPT:CurrentStepTitle
    }
    if ($SCRIPT:DryRun) {
        Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would set: $name = $value [$type]  ($why)" -ForegroundColor Magenta
        Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)    Path: $path" -ForegroundColor DarkMagenta
        return
    }
    Write-DebugLog "Registry: $path | $name = $value [$type] — $why"
    try {
        if (-not (Test-Path $path)) { New-Item -Path $path -Force -ErrorAction Stop | Out-Null }
        Set-ItemProperty -Path $path -Name $name -Value $value -Type $type -ErrorAction Stop
        Write-OK "Registry: $name = $value"
    } catch {
        Write-Warn "Registry write failed ($name): $_"
        Write-Host "  $([char]0x2139) This is not critical — the optimization will be skipped for this setting." -ForegroundColor Cyan
    }
}

function Ensure-Dir($path) {
    if (-not (Test-Path $path)) { New-Item -ItemType Directory -Path $path -Force -ErrorAction SilentlyContinue | Out-Null }
}

function Set-ClipboardSafe {
    <#  Wraps Set-Clipboard in try/catch. Set-Clipboard can fail on headless/remote
        sessions, minimal Windows Server editions, or when the clipboard service is
        unavailable. Non-critical — failure is logged, not thrown.  #>
    param([Parameter(ValueFromPipeline)][string]$Text)
    process {
        try { $Text | Set-Clipboard -ErrorAction Stop }
        catch { Write-DebugLog "Set-Clipboard failed (headless/remote session?): $_" }
    }
}

function Clear-Dir($path, $label) {
    if ($SCRIPT:DryRun) { Write-DebugLog "DRY-RUN: Clear-Dir skipped for $path"; return 0 }
    if (-not (Test-Path $path)) { Write-DebugLog "${label}: not found ($path)"; return 0 }
    $items = Get-ChildItem $path -Recurse -Force -ErrorAction SilentlyContinue
    $files = @($items | Where-Object { -not $_.PSIsContainer })
    $n = $files.Count
    $mb = [math]::Round(([int64]($files | Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue).Sum) / 1MB, 1)
    Write-Step "$label  ($n files · $mb MB)"
    $items | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    $remaining = @(Get-ChildItem $path -Recurse -Force -ErrorAction SilentlyContinue | Where-Object { -not $_.PSIsContainer }).Count
    $del = [math]::Max(0, $n - $remaining)
    Write-OK "${label}: $del deleted$(if($remaining){" ($remaining locked — normal)"})"
    Write-DebugLog "${label}: del=$del locked=$remaining path=$path"
    return $del
}

# ── System Compatibility Checks ──────────────────────────────────────────────
# Runs once at startup to detect and warn about edge-case environments.
# All issues are non-fatal — the suite degrades gracefully.

function Test-SystemCompatibility {
    <#
    .SYNOPSIS  Detects environment limitations and logs warnings.
    .DESCRIPTION
        Checks for: ARM64, Constrained Language Mode, Windows Server/LTSC,
        PowerShell 7 (missing Get-WmiObject), missing AppX cmdlets.
        Does not block execution — all limitations have graceful fallbacks.
    #>
    $warnings = 0

    # ARM64 Windows — nvapi64.dll and some x64 P/Invoke won't work
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
        Write-Warn "ARM64 Windows detected. NVIDIA DRS writes will fall back to registry-only method."
        $warnings++
    }

    # Constrained Language Mode — Add-Type blocked (AppLocker, WDAC, DeviceGuard)
    if ($ExecutionContext.SessionState.LanguageMode -eq 'ConstrainedLanguage') {
        Write-Warn "Constrained Language Mode active. NVIDIA DRS and RAM trim will be skipped."
        Write-Warn "Registry-only paths will be used where available."
        $warnings++
    }

    # Windows Server / LTSC — missing AppX, Xbox services, some consumer features
    $productType = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).ProductType
    # ProductType: 1=Workstation, 2=DomainController, 3=Server
    if ($productType -and $productType -ne 1) {
        Write-Warn "Windows Server/DC edition detected (ProductType=$productType)."
        Write-Warn "AppX debloat, Xbox services, and some consumer features may not exist."
        $warnings++
    }

    # PowerShell 7+ — Get-WmiObject removed (pagefile step affected)
    if ($PSVersionTable.PSVersion.Major -ge 7) {
        Write-Warn "PowerShell $($PSVersionTable.PSVersion) detected. Pagefile configuration"
        Write-Warn "requires Get-WmiObject (PS 5.1 only). Run with Windows PowerShell for full support."
        $warnings++
    }

    # Missing AppX cmdlets (Server Core, minimal installs)
    if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
        Write-DebugLog "AppX cmdlets not available — debloat package removal will be skipped."
        $warnings++
    }

    if ($warnings -gt 0) {
        Write-Info "Detected $warnings compatibility note(s) — nothing to worry about, the suite adapts automatically."
    }
}

# ── Verification counter infrastructure ─────────────────────────────────────
# Uses $Script: scope (caller's scope via dot-sourcing). Entry-point scripts
# must call Initialize-VerifyCounters before use to reset stale values.

function Initialize-VerifyCounters {
    $Script:_verifyOkCount      = 0
    $Script:_verifyChangedCount = 0
    $Script:_verifyMissingCount = 0
    $Script:_verifyInfoCount    = 0
}

function Get-VerifyCounters {
    return @{
        okCount      = [int]$Script:_verifyOkCount
        changedCount = [int]$Script:_verifyChangedCount
        missingCount = [int]$Script:_verifyMissingCount
        infoCount    = [int]$Script:_verifyInfoCount
    }
}

function Test-RegistryCheck {
    param(
        [string] $Path,
        [string] $Name,
        $Expected,
        [string] $Label,
        [switch] $Quiet   # Returns structured @{Status; Value} without console output or counter updates
    )
    $result = $null
    try {
        if (Test-Path $Path) {
            $val = Get-ItemProperty -Path $Path -Name $Name -ErrorAction Stop
            $result = $val.$Name
        }
    } catch { Write-DebugLog "Test-RegistryCheck: could not read '$Name' from '$Path'" }

    $status = if ($null -eq $result) { "MISSING" } elseif ($result -eq $Expected) { "OK" } else { "CHANGED" }

    if ($Quiet) {
        return @{ Status = $status; Value = $result }
    }

    switch ($status) {
        "MISSING" {
            Write-Host "  ?  MISSING   $Label" -ForegroundColor Red
            Write-Host "               $Path\$Name" -ForegroundColor DarkGray
            $Script:_verifyMissingCount++
        }
        "OK" {
            Write-Host "  ✓  OK        $Label  ($result)" -ForegroundColor Green
            $Script:_verifyOkCount++
        }
        "CHANGED" {
            Write-Host "  ✗  CHANGED   $Label  (is: $result, expected: $Expected)" -ForegroundColor Yellow
            Write-Host "               $Path\$Name" -ForegroundColor DarkGray
            # Warn if the key lives under a Policies path — Group Policy may override user writes
            if ($Path -match '\\Policies\\') {
                Write-Host "               NOTE: This key is under a Policies path — may be managed by Group Policy" -ForegroundColor DarkYellow
            }
            $Script:_verifyChangedCount++
        }
    }
    # No return value when not -Quiet — prevents stdout clutter in Verify-Settings.ps1
}

function Test-ServiceCheck {
    param(
        [string] $ServiceName,
        [string] $ExpectedStartType,
        [string] $Label
    )
    try {
        $svc = Get-Service -Name $ServiceName -ErrorAction Stop
        # Escape single quotes in the service name to prevent WQL injection
        $escapedName = $ServiceName -replace "'", "''"
        $cimSvc = Get-CimInstance Win32_Service -Filter "Name='$escapedName'" -ErrorAction SilentlyContinue
        $rawStartType = if ($cimSvc) { $cimSvc.StartMode } else { $svc.StartType.ToString() }
        # WMI returns "Auto" but Set-Service uses "Automatic" — normalize for comparison
        $startType = switch ($rawStartType) {
            "Auto"         { "Automatic" }
            "Auto Delayed" { "AutomaticDelayedStart" }
            default        { $rawStartType }
        }
        if ($startType -eq $ExpectedStartType) {
            Write-Host "  ✓  OK        $Label  (StartType: $startType, Status: $($svc.Status))" -ForegroundColor Green
            $Script:_verifyOkCount++
        } else {
            Write-Host "  ✗  CHANGED   $Label  (StartType: $startType, expected: $ExpectedStartType)" -ForegroundColor Yellow
            $Script:_verifyChangedCount++
        }
    } catch {
        Write-Host "  ?  MISSING   $Label  (Service not found)" -ForegroundColor Red
        $Script:_verifyMissingCount++
    }
}
