# ==============================================================================
#  helpers/logging.ps1  -  Logging, Console Output, Banners
# ==============================================================================

$SCRIPT:LogPersistenceEnabled = $false

function Set-TextFileUtf8 {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$Path, [string]$Value)
    $nativePath = if (Test-HostIsWindows) { $Path -replace '/', '\' } else { $Path -replace '\\', '/' }
    $parentDir = Split-Path -Path $nativePath -Parent
    if ($parentDir) {
        [System.IO.Directory]::CreateDirectory($parentDir) | Out-Null
    }
    if ($PSCmdlet.ShouldProcess($nativePath, "Write UTF-8 text file")) {
        [System.IO.File]::WriteAllText($nativePath, $Value, [System.Text.UTF8Encoding]::new($false))
    }
}

function Add-TextFileUtf8Line {
    param([string]$Path, [string]$Value)
    $nativePath = if (Test-HostIsWindows) { $Path -replace '/', '\' } else { $Path -replace '\\', '/' }
    $parentDir = Split-Path -Path $nativePath -Parent
    if ($parentDir) {
        [System.IO.Directory]::CreateDirectory($parentDir) | Out-Null
    }
    [System.IO.File]::AppendAllText($nativePath, $Value + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
}

function Test-HostIsWindows {
    if ($env:OS -eq 'Windows_NT') { return $true }
    return ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
}

function Redact-Sensitive {
    param([AllowNull()][string]$Text)
    if ($null -eq $Text) { return $Text }

    $redacted = $Text
    if ($env:COMPUTERNAME) {
        $redacted = [regex]::Replace($redacted, [regex]::Escape($env:COMPUTERNAME), "[COMPUTER]", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }
    if ($env:USERNAME) {
        $redacted = [regex]::Replace($redacted, [regex]::Escape($env:USERNAME), "[USER]", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }
    $redacted = [regex]::Replace($redacted, '(?i)C:\\Users\\[^\\]+\\', { 'C:\Users\[USER]\' })
    return $redacted
}

function Initialize-Log {
    $SCRIPT:LogPersistenceEnabled = $false
    $dryRunActive = (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
    if ($dryRunActive) {
        Write-DebugLog "DRY-RUN: persistent log initialization skipped."
        return
    }

    Ensure-Dir $CFG_LogDir
    if (Test-Path $CFG_LogFile) {
        $stamp   = (Get-Item $CFG_LogFile).LastWriteTime.ToString("yyyyMMdd_HHmmss")
        Move-Item $CFG_LogFile (Join-Path $CFG_LogDir "frametime_$stamp.log") -Force
        Get-ChildItem $CFG_LogDir -Filter "frametime_*.log" |
            Sort-Object LastWriteTime -Descending |
            Select-Object -Skip $CFG_LogMaxFiles |
            Remove-Item -Force -ErrorAction SilentlyContinue
    }
    $header = @"
================================================================================
  frametime.cfg · Log
  Started:    $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
  Profile:    $($SCRIPT:Profile)   Mode: $($SCRIPT:Mode)   Log: $($SCRIPT:LogLevel)
  Host:       $env:COMPUTERNAME     User:  $env:USERNAME
  Windows:    $([System.Environment]::OSVersion.VersionString)
================================================================================
"@
    Set-TextFileUtf8 -Path $CFG_LogFile -Value (Redact-Sensitive $header)
    $SCRIPT:LogPersistenceEnabled = $true
}

function Write-Log($Level, $Message) {
    $Message = Redact-Sensitive $Message
    $ts      = Get-Date -Format "HH:mm:ss"
    $logLine = "[$ts][$Level] $Message"
    if ($SCRIPT:LogPersistenceEnabled -and $CFG_LogFile -and (Test-Path $CFG_LogDir -ErrorAction SilentlyContinue)) {
        try { Add-TextFileUtf8Line -Path $CFG_LogFile -Value $logLine } catch {
            # Avoid recursive logging if the log sink itself fails.
            $null = $_
        }
    }
    $show = switch ($SCRIPT:LogLevel) {
        "MINIMAL" { $Level -in @("ERROR","WARN","OK","INFO","SECTION","STEP","T1","T2","T3") }
        "NORMAL"  { $Level -notin @("DEBUG") }
        default   { $true }
    }
    if (-not $show) { return }
    $color = switch ($Level) {
        "OK"      { "Green" };    "WARN"    { "DarkYellow" }
        "ERROR"   { "Red" };      "STEP"    { "Yellow" }
        "SECTION" { "Cyan" };     "DEBUG"   { "DarkGray" }
        "INFO"    { "Cyan" };     "DRYRUN"  { "Magenta" }
        "T1"      { "Green" };    "T2"      { "Yellow" }
        "T3"      { "DarkCyan" }; default   { "DarkGray" }
    }
    $prefix = switch ($Level) {
        "OK"      { "  $([char]0x2714)" }; "WARN"    { "  $([char]0x26A0)" }; "ERROR"   { "  $([char]0x2718)" }
        "STEP"    { "  $([char]0x25BA)" }; "SECTION" { "  $([char]0x2551)" };  "DEBUG"   { "   " }
        "INFO"    { "  $([char]0x2139)" }
        "T1"      { "  $([char]0x25BA)" }; "T2"      { "  $([char]0x25B2)" };  "T3"      { "  $([char]0x25C6)" }
        default   { "   " }
    }
    Write-ConsoleLine "$prefix $Message" -ForegroundColor $color
}

function Write-OK($t)       { Write-Log "OK"      $t }
function Write-Warn($t)     { Write-Log "WARN"    $t }
function Write-Err($t)      { Write-Log "ERROR"   $t }
function Write-Step($t)     { Write-Log "STEP"    $t }
function Write-Info($t)     { Write-Log "INFO"    $t }
# Suite-specific debug logging - routes through the unified logging system
# (file + console with level filtering). Named Write-DebugLog to avoid
# shadowing the built-in Write-Debug cmdlet.
function Write-DebugLog($t)    { Write-Log "DEBUG"   $t }
function Write-ConsoleLine {
    param(
        [AllowNull()][object]$Message = "",
        [ConsoleColor]$ForegroundColor = [ConsoleColor]::Gray
    )

    $previousColor = $null
    $colorChanged = $false
    try {
        if ($Host -and $Host.UI -and $Host.UI.RawUI) {
            try {
                $previousColor = $Host.UI.RawUI.ForegroundColor
                $Host.UI.RawUI.ForegroundColor = $ForegroundColor
                $colorChanged = $true
            } catch {
                # Redirected/headless hosts can expose RawUI while rejecting
                # cursor and color operations with an invalid console handle.
                $colorChanged = $false
            }
        }
        Write-Information -MessageData ([string]$Message) -InformationAction Continue
    } finally {
        if ($colorChanged) {
            try { $Host.UI.RawUI.ForegroundColor = $previousColor } catch { $null = $_ }
        }
    }
}

function Clear-ConsoleSafe {
    try { Clear-Host -ErrorAction Stop } catch { $null = $_ }
}
function Write-Blank()      { Write-ConsoleLine "" }
function Write-Sub($t)      { Write-ConsoleLine "  · $t" -ForegroundColor White }
# Summary message after an action - suppressed in DRY-RUN because
# Set-RegistryValue/Set-BootConfig already print "[DRY-RUN] Would set:".
function Write-ActionOK($t) { if (-not $SCRIPT:DryRun) { Write-OK $t } }

function Write-TierBadge($tier, $label) {
    $color = switch ($tier) { 1 {"Green"} 2 {"Yellow"} 3 {"DarkCyan"} default {"White"} }
    $icon = switch ($tier) {
        1 { "$([char]0x2714)" }   # check mark for the baseline tier
        2 { "$([char]0x25B2)" }   # triangle - setup-dependent
        3 { "$([char]0x25C6)" }   # diamond for experimental operations
        default { "?" }
    }
    $badge = switch ($tier) {
        1 { "$icon [T1 Baseline] Project default" }
        2 { "$icon [T2 Moderate] Setup-Dependent" }
        3 { "$icon [T3 Experimental] Weak or incomplete evidence" }
        default { "? [T?] Unknown Tier" }
    }
    Write-ConsoleLine "  $badge - $label" -ForegroundColor $color
    Write-Log "T$tier" "$label"
}

function Write-Section($title) {
    $pad = "=" * ($title.Length + 4)
    Write-ConsoleLine "`n  $([char]0x2554)$pad$([char]0x2557)" -ForegroundColor DarkCyan
    Write-ConsoleLine "  $([char]0x2551)  $title  $([char]0x2551)" -ForegroundColor Cyan
    Write-ConsoleLine "  $([char]0x255A)$pad$([char]0x255D)" -ForegroundColor DarkCyan
    # Show step progress when $SCRIPT:PhaseTotal is set and title contains "Step N"
    if ((Get-Variable -Name PhaseTotal -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:PhaseTotal -and $title -match '^Step\s+(\d+)') {
        $stepNum = [int]$Matches[1]
        $pct = [math]::Round($stepNum / $SCRIPT:PhaseTotal * 100)
        $barLen = 30
        $filled = [math]::Round($pct / 100 * $barLen)
        $empty  = $barLen - $filled
        $bar = "$([char]0x2588)" * $filled + "$([char]0x2591)" * $empty
        $phaseLabel = if ((Get-Variable -Name CurrentPhase -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentPhase) { "Phase $($SCRIPT:CurrentPhase)" } else { "" }
        Write-ConsoleLine "  $bar  $phaseLabel  $([char]0x2502)  $stepNum / $($SCRIPT:PhaseTotal)  ($($pct)%)" -ForegroundColor DarkGray
    }
    Write-Log "SECTION" "=== $title ==="
}

function Write-LogoBanner($subtitle) {
    <#  Lightweight banner: ASCII logo + subtitle. For entry-point scripts that
        don't need the full phase banner (Cleanup, FpsCap, Verify, etc.).  #>
    $dryRunActive = (Get-Variable -Name DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
    if (-not $dryRunActive) { Clear-ConsoleSafe }
    Write-ConsoleLine @"

  ██████╗███████╗██████╗      ██████╗ ██████╗ ████████╗
 ██╔════╝██╔════╝╚════██╗    ██╔═══██╗██╔══██╗╚══██╔══╝
 ██║     ███████╗ █████╔╝    ██║   ██║██████╔╝   ██║
 ██║     ╚════██║██╔═══╝     ██║   ██║██╔═══╝    ██║
 ╚██████╗███████║███████╗    ╚██████╔╝██║        ██║
  ╚═════╝╚══════╝╚══════╝     ╚═════╝ ╚═╝        ╚═╝

         $subtitle
"@ -ForegroundColor Cyan
}

# ── Phase step counters ────────────────────────────────────────────────────
# Tracks applied / skipped / failed counts per phase for the summary box.
function Initialize-PhaseCounters {
    $Script:_phaseApplied = 0
    $Script:_phaseSkipped = 0
    $Script:_phaseFailed  = 0
}

function Add-PhaseApplied {
    if (-not (Get-Variable -Name _phaseApplied -Scope Script -ErrorAction SilentlyContinue)) { $Script:_phaseApplied = 0 }
    $Script:_phaseApplied++
}
function Add-PhaseSkipped {
    if (-not (Get-Variable -Name _phaseSkipped -Scope Script -ErrorAction SilentlyContinue)) { $Script:_phaseSkipped = 0 }
    $Script:_phaseSkipped++
}
function Add-PhaseFailed  {
    if (-not (Get-Variable -Name _phaseFailed -Scope Script -ErrorAction SilentlyContinue)) { $Script:_phaseFailed = 0 }
    $Script:_phaseFailed++
}

function Add-DryRunPreviewIssue {
    if (-not (Get-Variable -Name DryRunPreviewIssues -Scope Script -ErrorAction SilentlyContinue)) {
        $Script:DryRunPreviewIssues = 0
    }
    $Script:DryRunPreviewIssues++
}

function Get-DryRunPreviewIssueCount {
    if (-not (Get-Variable -Name DryRunPreviewIssues -Scope Script -ErrorAction SilentlyContinue)) {
        return 0
    }
    return [int]$Script:DryRunPreviewIssues
}

function Write-PhaseSummary {
    <#  Displays a summary box after a phase with applied/skipped/failed counts.  #>
    param(
        [string]$PhaseLabel,
        [string]$NextAction    = "",
        [switch]$DryRun,
        [switch]$ContinuePreview
    )

    if (-not (Get-Variable -Name _phaseApplied -Scope Script -ErrorAction SilentlyContinue)) { $Script:_phaseApplied = 0 }
    if (-not (Get-Variable -Name _phaseSkipped -Scope Script -ErrorAction SilentlyContinue)) { $Script:_phaseSkipped = 0 }
    if (-not (Get-Variable -Name _phaseFailed -Scope Script -ErrorAction SilentlyContinue))  { $Script:_phaseFailed  = 0 }
    $applied = [int]$Script:_phaseApplied
    $skipped = [int]$Script:_phaseSkipped
    $failed  = [int]$Script:_phaseFailed

    Write-Blank
    if ($DryRun) {
        $boxWidth = 58
        $previewIssues = Get-DryRunPreviewIssueCount
        $status = if ($previewIssues -gt 0) { "COMPLETE WITH $previewIssues ISSUE(S)" } else { "COMPLETE" }
        $writePreviewLine = {
            param([string]$Text)
            if ($Text.Length -gt $boxWidth) { $Text = $Text.Substring(0, $boxWidth) }
            Write-ConsoleLine "  $([char]0x2551)$($Text.PadRight($boxWidth))$([char]0x2551)" -ForegroundColor Magenta
        }

        Write-ConsoleLine "  $([char]0x2554)$("$([char]0x2550)" * $boxWidth)$([char]0x2557)" -ForegroundColor Magenta
        & $writePreviewLine "  DRY-RUN | $PhaseLabel PREVIEW $status"
        if ($ContinuePreview) {
            $continuation = if ($PhaseLabel -eq "PHASE 3") {
                "  No changes applied; overall lifecycle summary follows."
            } else {
                "  No changes applied; lifecycle simulation continues."
            }
            & $writePreviewLine $continuation
        } else {
            & $writePreviewLine "  No changes applied. Console output is the only result."
            & $writePreviewLine "  Portable live execution is unavailable; use an authenticated release."
        }
        Write-ConsoleLine "  $([char]0x255A)$("$([char]0x2550)" * $boxWidth)$([char]0x255D)" -ForegroundColor Magenta
    } else {
        $borderColor = if ($failed -gt 0) { "Yellow" } else { "Green" }
        Write-ConsoleLine "  $([char]0x2554)$("$([char]0x2550)" * 58)$([char]0x2557)" -ForegroundColor $borderColor
        Write-ConsoleLine "  $([char]0x2551)  $PhaseLabel COMPLETE$(' ' * [math]::Max(0, 44 - $PhaseLabel.Length))$([char]0x2551)" -ForegroundColor $borderColor
        Write-ConsoleLine "  $([char]0x2551)$(' ' * 58)$([char]0x2551)" -ForegroundColor $borderColor
        Write-ConsoleLine "  $([char]0x2551)  $([char]0x2714) Applied:  $applied$(' ' * [math]::Max(0, 46 - "$applied".Length))$([char]0x2551)" -ForegroundColor Green
        if ($skipped -gt 0) {
            Write-ConsoleLine "  $([char]0x2551)  $([char]0x25CB) Skipped:  $skipped$(' ' * [math]::Max(0, 46 - "$skipped".Length))$([char]0x2551)" -ForegroundColor DarkGray
        }
        if ($failed -gt 0) {
            Write-ConsoleLine "  $([char]0x2551)  $([char]0x2718) Failed:   $failed$(' ' * [math]::Max(0, 46 - "$failed".Length))$([char]0x2551)" -ForegroundColor Red
            Write-ConsoleLine "  $([char]0x2551)  Retry failed steps from an authenticated release$(' ' * 7)$([char]0x2551)" -ForegroundColor DarkGray
        }
        if ($NextAction) {
            Write-ConsoleLine "  $([char]0x2551)$(' ' * 58)$([char]0x2551)" -ForegroundColor $borderColor
            # Split NextAction into lines of ~54 chars max for box fitting
            foreach ($line in $NextAction -split "`n") {
                Write-ConsoleLine "  $([char]0x2551)  $line$(' ' * [math]::Max(0, 56 - $line.Length))$([char]0x2551)" -ForegroundColor $borderColor
            }
        }
        Write-ConsoleLine "  $([char]0x255A)$("$([char]0x2550)" * 58)$([char]0x255D)" -ForegroundColor $borderColor
    }
    if (-not $DryRun) { Write-Info "Log: $CFG_LogFile" }
}

function Write-Banner($phase, $total, $subtitle) {
    if (-not $SCRIPT:DryRun) { Clear-ConsoleSafe }
    $fullDryRunActive = (Get-Variable -Name FullDryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:FullDryRun
    $profileTag = if ($SCRIPT:DryRun -and $fullDryRunActive) { "[FULL PREVIEW]" } elseif ($SCRIPT:Profile) { "[$($SCRIPT:Profile)]" } else { "[$($SCRIPT:Mode)]" }
    $levelTag   = if ($SCRIPT:DryRun) { "[LOG:OFF]" } else { "[LOG:$($SCRIPT:LogLevel)]" }
    Write-ConsoleLine @"

  ██████╗███████╗██████╗      ██████╗ ██████╗ ████████╗
 ██╔════╝██╔════╝╚════██╗    ██╔═══██╗██╔══██╗╚══██╔══╝
 ██║     ███████╗ █████╔╝    ██║   ██║██████╔╝   ██║
 ██║     ╚════██║██╔═══╝     ██║   ██║██╔═══╝    ██║
 ╚██████╗███████║███████╗    ╚██████╔╝██║        ██║
  ╚═════╝╚══════╝╚══════╝     ╚═════╝ ╚═╝        ╚═╝
"@ -ForegroundColor Cyan
    Write-ConsoleLine "  Phase $phase / $total  ·  $subtitle" -ForegroundColor Cyan
    $sessionDetail = if ($SCRIPT:DryRun) { "No persistent preview log" } else { "Log: $CFG_LogFile" }
    Write-ConsoleLine "  $profileTag $levelTag  ·  $sessionDetail" -ForegroundColor DarkGray
    if ($SCRIPT:DryRun) {
        Write-ConsoleLine ""
        Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Preview mode - NO changes will be applied" -ForegroundColor Magenta
    }
    $profileDesc = if ($SCRIPT:DryRun -and $fullDryRunActive) {
        $cliFullDryRun = (Get-Variable -Name FullDryRunRequested -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:FullDryRunRequested
        if ($cliFullDryRun) {
            $gpuBranch = if (Get-Variable -Name RequestedDryRunGpu -Scope Script -ErrorAction SilentlyContinue) { [string]$SCRIPT:RequestedDryRunGpu } else { "2" }
            "All tiers and phases for selected GPU branch [$gpuBranch]. Zero prompts."
        } else {
            "All tiers and phases; GPU branch is selected in configuration."
        }
    } else {
        switch ($SCRIPT:Profile) {
            "SAFE"        { "T1 auto + T2(safe) auto. Moderate/aggressive skipped." }
            "RECOMMENDED" { "T1 auto. T2 prompted. T3 skipped." }
            "COMPETITIVE" { "T1 auto. T2+T3 prompted (up to AGGRESSIVE)." }
            "CUSTOM"      { "Everything prompted with full detail cards." }
            "YOLO"        { "ALL tiers auto-applied (up to AGGRESSIVE). Zero prompts." }
            default       { "" }
        }
    }
    if ($profileDesc) {
        Write-ConsoleLine "  Profile: $profileDesc" -ForegroundColor DarkGray
    }
    Write-ConsoleLine ""
    Write-ConsoleLine "  Tier Legend:" -ForegroundColor White
    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  $([char]0x2714) [T1 Baseline]    Project default - previewed, not applied" -ForegroundColor Green
        Write-ConsoleLine "  $([char]0x25B2) [T2 Moderate]    Setup-dependent - previewed if in scope" -ForegroundColor Yellow
        Write-ConsoleLine "  $([char]0x25C6) [T3 Experimental] Weakly evidenced - previewed if in scope" -ForegroundColor DarkCyan
    } else {
        Write-ConsoleLine "  $([char]0x2714) [T1 Baseline]    Project default - auto-applied" -ForegroundColor Green
        Write-ConsoleLine "  $([char]0x25B2) [T2 Moderate]    Setup-dependent - prompted" -ForegroundColor Yellow
        Write-ConsoleLine "  $([char]0x25C6) [T3 Experimental] Weakly evidenced - COMPETITIVE/CUSTOM only" -ForegroundColor DarkCyan
    }
    Write-ConsoleLine "  Risk: $([char]0x2714) SAFE  $([char]0x25B2) MODERATE  $([char]0x25C6) AGGRESSIVE  $([char]0x2718) CRITICAL" -ForegroundColor DarkGray
    Write-ConsoleLine "  $("$([char]0x2500)" * 60)" -ForegroundColor DarkGray
    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  STRICT PREVIEW: console output only; no restore point needed." -ForegroundColor Magenta
        Write-ConsoleLine "  A preview cannot prove that a later live hardware action succeeds." -ForegroundColor DarkGray
    } else {
        Write-ConsoleLine "  Live execution changes privileged system state." -ForegroundColor DarkRed
        Write-ConsoleLine "  Create an independent restore point or system image and review recovery limits." -ForegroundColor DarkRed
    }
    Write-ConsoleLine "  $("$([char]0x2500)" * 60)" -ForegroundColor DarkGray
    Write-Blank
}
