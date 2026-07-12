# ==============================================================================
#  CS2-Optimize-GUI.ps1  —  WPF Dashboard
#  Launch via START-GUI.bat
# ==============================================================================
param([switch]$SmokeTest)

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: CS2-Optimize-GUI" -ForegroundColor Green
    exit 0
}

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]$identity
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "CS2-Optimize-GUI.ps1 must be run as Administrator. Start PowerShell with 'Run as administrator' and try again."
    }
}

Assert-Administrator
Add-Type -AssemblyName PresentationFramework
Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase
$Script:Root = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path $MyInvocation.MyCommand.Path -Parent }

. "$Script:Root\config.env.ps1"
. "$Script:Root\helpers.ps1"
. "$Script:Root\helpers\step-catalog.ps1"
. "$Script:Root\helpers\system-analysis.ps1"

# ── Async engine ──────────────────────────────────────────────────────────────
$Script:Pool   = [System.Management.Automation.Runspaces.RunspaceFactory]::CreateRunspacePool(1, 3)
$Script:Pool.Open()
$Script:UISync    = [hashtable]::Synchronized(@{})
$Script:Closing   = $false
$Script:AsyncTimers = [System.Collections.Generic.List[System.Windows.Threading.DispatcherTimer]]::new()

function Invoke-Async {
    param(
        [scriptblock]$Work,
        [object[]]$WorkArgs = @(),
        [scriptblock]$OnDone = {},
        [scriptblock]$OnError = {},
        [scriptblock]$OnFinally = {}
    )

    $useDefaultErrorDialog = -not $PSBoundParameters.ContainsKey("OnError")
    $rs = $null
    $timer = $null
    try {
        $rs = [System.Management.Automation.PowerShell]::Create()
        $rs.RunspacePool = $Script:Pool
        [void]$rs.AddScript($Work)
        foreach ($a in $WorkArgs) { [void]$rs.AddArgument($a) }
        $handle = $rs.BeginInvoke()
        $timer  = [System.Windows.Threading.DispatcherTimer]::new()
        $timer.Interval = [TimeSpan]::FromMilliseconds(250)
    } catch {
        $message = "$($_.Exception.GetType().Name): $($_.Exception.Message)"
        try { & $OnError $message } catch { $null = $_ }
        try { & $OnFinally } catch { $null = $_ }
        if ($rs) { $rs.Dispose() }
        if ($useDefaultErrorDialog -and $Window) {
            [System.Windows.MessageBox]::Show("Background task error: $message", "Error", "OK", "Error")
        }
        return
    }

    $operationState = [hashtable]::Synchronized(@{
        Cancelled = $false
        PowerShell = $rs
    })
    $capturedHandle = $handle
    $capturedRs     = $rs
    $capturedDone   = $OnDone
    $capturedError  = $OnError
    $capturedFinally = $OnFinally
    $capturedWindow = $Window
    $capturedUseDefaultErrorDialog = $useDefaultErrorDialog
    $capturedTimers = $Script:AsyncTimers
    $capturedOperationState = $operationState
    $timer.Add_Tick({
        $errorMessage = $null
        $completed = $Script:Closing -or $capturedHandle.IsCompleted
        if (-not $completed) { return }

        try {
            $timer.Stop()
            if ($Script:Closing) {
                $capturedOperationState.Cancelled = $true
                $capturedRs.Stop()
            } else {
                $capturedRs.EndInvoke($capturedHandle)
                if (-not $capturedOperationState.Cancelled) {
                    & $capturedDone
                }
            }
        } catch {
            if (-not $capturedOperationState.Cancelled) {
                $errorMessage = "$($_.Exception.GetType().Name): $($_.Exception.Message)"
                try { & $capturedError $errorMessage } catch { $null = $_ }
                if ($capturedWindow -and $capturedUseDefaultErrorDialog) {
                    [System.Windows.MessageBox]::Show("Background task error: $errorMessage", "Error", "OK", "Error")
                }
            }
        } finally {
            try { $capturedRs.Dispose() } catch { $null = $_ }
            try { $capturedTimers.Remove($timer) } catch { $null = $_ }
            try { & $capturedFinally } catch {
                if ($capturedWindow) {
                    [System.Windows.MessageBox]::Show("Async cleanup error: $($_.Exception.Message)", "Error", "OK", "Error")
                }
            }
        }
    }.GetNewClosure())
    $Script:AsyncTimers.Add($timer)
    $timer.Start()
    return $operationState
}

function Stop-AsyncOperation {
    param([hashtable]$Operation)
    if (-not $Operation -or $Operation.Cancelled) { return }
    $Operation.Cancelled = $true
    try {
        [void]$Operation.PowerShell.BeginStop($null, $null)
    } catch {
        try { $Operation.PowerShell.Stop() } catch { $null = $_ }
    }
}

function New-Brush { [System.Windows.Media.BrushConverter]::new().ConvertFromString($args[0]) }

# ── XAML ──────────────────────────────────────────────────────────────────────
$xamlPath = Join-Path $Script:Root "ui\CS2-Optimize-GUI.xaml"
if (-not (Test-Path -LiteralPath $xamlPath -PathType Leaf)) {
    throw "GUI layout not found: $xamlPath"
}
try {
    [xml]$XAML = Get-Content -LiteralPath $xamlPath -Raw -ErrorAction Stop
} catch {
    throw "Unable to load GUI layout '$xamlPath': $($_.Exception.Message)"
}

# ── Load window ───────────────────────────────────────────────────────────────
$reader = [System.Xml.XmlNodeReader]::new($XAML)
$Window = [Windows.Markup.XamlReader]::Load($reader)
$reader.Dispose()

function Set-GuiThemeResources {
    if ([System.Windows.SystemParameters]::HighContrast) {
        $systemBrushes = @{
            BgMain        = [System.Windows.SystemColors]::WindowBrush
            BgSide        = [System.Windows.SystemColors]::WindowBrush
            BgCard        = [System.Windows.SystemColors]::ControlBrush
            BgHeader      = [System.Windows.SystemColors]::ControlBrush
            BgRaised      = [System.Windows.SystemColors]::ControlBrush
            BgSelected    = [System.Windows.SystemColors]::HighlightBrush
            TextPri       = [System.Windows.SystemColors]::WindowTextBrush
            TextSecondary = [System.Windows.SystemColors]::WindowTextBrush
            TextMuted     = [System.Windows.SystemColors]::GrayTextBrush
            Border        = [System.Windows.SystemColors]::WindowTextBrush
            ControlBorder = [System.Windows.SystemColors]::WindowTextBrush
            Accent        = [System.Windows.SystemColors]::HighlightBrush
            AccentHover   = [System.Windows.SystemColors]::HighlightBrush
            AccentPressed = [System.Windows.SystemColors]::HighlightBrush
            AccentText    = [System.Windows.SystemColors]::HighlightTextBrush
            Success       = [System.Windows.SystemColors]::WindowTextBrush
            SuccessFill   = [System.Windows.SystemColors]::WindowBrush
            Warning       = [System.Windows.SystemColors]::WindowTextBrush
            WarningFill   = [System.Windows.SystemColors]::WindowBrush
            Danger        = [System.Windows.SystemColors]::WindowTextBrush
            DangerFill    = [System.Windows.SystemColors]::HighlightBrush
            DangerText    = [System.Windows.SystemColors]::HighlightTextBrush
            Info          = [System.Windows.SystemColors]::WindowTextBrush
        }
        foreach ($entry in $systemBrushes.GetEnumerator()) {
            $Window.Resources[$entry.Key] = $entry.Value
        }
        return
    }

    $themeColors = @{
        BgMain = "#0B0D10"; BgSide = "#11151A"; BgCard = "#11151A"; BgHeader = "#11151A"
        BgRaised = "#181D23"; BgSelected = "#242B33"; TextPri = "#F4F6F8"
        TextSecondary = "#B8C0CC"; TextMuted = "#9AA5B4"; Border = "#313943"
        ControlBorder = "#667085"; Accent = "#E8520A"; AccentHover = "#F05A16"
        AccentPressed = "#D94B08"; AccentText = "#0B0D10"; Success = "#22C55E"
        SuccessFill = "#153A27"; Warning = "#FBBF24"; WarningFill = "#3F3312"
        Danger = "#F87171"; DangerFill = "#B42318"; DangerText = "#FFFFFF"; Info = "#38BDF8"
    }
    foreach ($entry in $themeColors.GetEnumerator()) {
        $Window.Resources[$entry.Key] = New-Brush $entry.Value
    }
}

Set-GuiThemeResources
$Script:HighContrastHandler = [System.ComponentModel.PropertyChangedEventHandler]{
    param($eventSource, $propertyEvent)
    if ($propertyEvent.PropertyName -eq "HighContrast" -and $Window) {
        $Window.Dispatcher.Invoke([Action]{ Set-GuiThemeResources })
    }
}
[System.Windows.SystemParameters]::add_StaticPropertyChanged($Script:HighContrastHandler)

# ── Named element shortcuts ───────────────────────────────────────────────────
function El {
    $e = $Window.FindName($args[0])
    if ($null -eq $e) { Write-Warning "El: XAML element '$($args[0])' not found" }
    $e
}

# ── Version labels (from config.env.ps1) ─────────────────────────────────────
(El "SettingsVersion").Text = "  $CFG_Version"
$Window.Title = "CS2 Optimize $CFG_Version"

# ── Navigation ────────────────────────────────────────────────────────────────
$Script:AllPanels = "PanelDashboard","PanelAnalyze","PanelOptimize","PanelBackup","PanelBenchmark","PanelNetwork","PanelVideo"
$Script:NavMap    = @{
    "PanelDashboard"  = "NavDashboard"
    "PanelAnalyze"    = "NavAnalyze"
    "PanelOptimize"   = "NavOptimize"
    "PanelBackup"     = "NavBackup"
    "PanelBenchmark"  = "NavBenchmark"
    "PanelNetwork"    = "NavNetwork"
    "PanelVideo"      = "NavVideo"
}
$Script:ActivePanel = "PanelDashboard"

$ActiveStyle   = $Window.Resources["NavBtnActive"]
$InactiveStyle = $Window.Resources["NavBtn"]

(El "NavDashboard").Add_Click({ Switch-Panel "PanelDashboard"; Load-Dashboard })
(El "NavAnalyze"  ).Add_Click({ Switch-Panel "PanelAnalyze" ; Start-Analysis })
(El "NavOptimize" ).Add_Click({ Switch-Panel "PanelOptimize" ; Load-Settings; Load-Optimize })
(El "NavBackup"   ).Add_Click({ Switch-Panel "PanelBackup"   ; Load-Backup    })
(El "NavBenchmark").Add_Click({ Switch-Panel "PanelBenchmark"; Load-Benchmark })
(El "NavNetwork"  ).Add_Click({ Switch-Panel "PanelNetwork"  ; Load-NetworkDiagnostics })
(El "NavVideo"    ).Add_Click({ Switch-Panel "PanelVideo"    ; Load-Video     })

# ── Sidebar status helpers ────────────────────────────────────────────────────
function Update-SidebarStatus {
    $state = Get-StateDataSafe
    $prof = if ($state) { $state.profile } else { "—" }
    $isDry = ($state -and $state.mode -eq "DRY-RUN")
    $phaseText = "—"
    Ensure-SecureWorkDir -Path (Split-Path $CFG_ProgressFile -Parent)
    if (Test-Path $CFG_ProgressFile) {
        try {
            $prog = Get-Content $CFG_ProgressFile -Raw | ConvertFrom-Json
            if ($prog.phase) { $phaseText = "$($prog.phase)" }
        } catch {
            Write-DebugLog "Status bar progress load failed: $($_.Exception.Message)"
        }
    }
    $Window.Dispatcher.Invoke({
        (El "SbProfile").Text = "Profile: $prof"
        (El "SbDryRun" ).Text = if ($isDry) { "DRY-RUN" } else { "" }
        (El "SbDryRunBadge").Visibility = if ($isDry) { "Visible" } else { "Collapsed" }
        (El "SbPhase").Text = "Phase: $phaseText"
    })
}

# ── Load panel functions and event handlers ─────────────────────────────────
. "$Script:Root\helpers\gui-panels.ps1"

$Script:NavigationShortcuts = @{
    D1 = { Switch-Panel "PanelDashboard"; Load-Dashboard }
    D2 = { Switch-Panel "PanelAnalyze"; Start-Analysis }
    D3 = { Switch-Panel "PanelOptimize"; Load-Settings; Load-Optimize }
    D4 = { Switch-Panel "PanelBenchmark"; Load-Benchmark }
    D5 = { Switch-Panel "PanelNetwork"; Load-NetworkDiagnostics }
    D6 = { Switch-Panel "PanelVideo"; Load-Video }
    D7 = { Switch-Panel "PanelBackup"; Load-Backup }
}
$Window.Add_KeyDown({
    param($eventSource, $keyEvent)
    $hasControl = ([System.Windows.Input.Keyboard]::Modifiers -band [System.Windows.Input.ModifierKeys]::Control) -ne 0
    if (-not $hasControl) { return }
    $keyName = $keyEvent.Key.ToString()
    if ($Script:NavigationShortcuts.ContainsKey($keyName)) {
        & $Script:NavigationShortcuts[$keyName]
        $keyEvent.Handled = $true
    }
})

# ══════════════════════════════════════════════════════════════════════════════
# STARTUP
# ══════════════════════════════════════════════════════════════════════════════
$Window.Add_Loaded({
    Update-SidebarStatus
    Update-StartupDriftBanner
    Switch-Panel "PanelDashboard"
    Load-Dashboard
})

$Window.Add_Closing({
    param($eventSource, $closingEvent)
    if ($Script:CriticalOperation) {
        $closingEvent.Cancel = $true
        [System.Windows.MessageBox]::Show(
            "$($Script:CriticalOperation) is still running. Wait for it to finish before closing the application.",
            "Operation in progress",
            "OK",
            "Warning"
        )
    }
})

$Window.Add_Closed({
    $Script:Closing = $true
    if ($Script:HighContrastHandler) {
        [System.Windows.SystemParameters]::remove_StaticPropertyChanged($Script:HighContrastHandler)
    }
    # Snapshot the list before iterating — Tick handlers call Remove($timer) on this
    # same list, which would throw InvalidOperationException during enumeration.
    $timersSnapshot = @($Script:AsyncTimers)
    foreach ($t in $timersSnapshot) { try { $t.Stop() } catch { $null = $_ } }
    try { $Script:Pool.Close(); $Script:Pool.Dispose() } catch { $null = $_ }
})

$Window.ShowDialog() | Out-Null
