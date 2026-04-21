# ==============================================================================
#  tests/integration/_IntegrationInit.ps1  --  Shared setup for integration tests
# ==============================================================================
#
#  Dot-source this file in BeforeAll {} blocks to get:
#    - All helper modules loaded (same as _TestInit.ps1)
#    - Full mocks for ALL Windows APIs (registry, services, bcdedit, etc.)
#    - Mock call-tracking infrastructure for verifying write/no-write behavior
#    - Temp directory creation/cleanup
#
#  Usage in a .Tests.ps1 file:
#    BeforeAll { . "$PSScriptRoot/_IntegrationInit.ps1" }

# ── Load base test setup (paths, config, helpers, temp dirs) ─────────────────
. "$PSScriptRoot/../helpers/_TestInit.ps1"

# ── Additional helpers that _TestInit.ps1 skips (heavy deps) ────────────────
# Load with appropriate stubs in place so they parse without errors on macOS/Linux.
$_projectRoot = (Resolve-Path "$PSScriptRoot/../..").Path
$_helpersDir  = "$_projectRoot/helpers"

# Stub external commands that modules reference at parse time
if (-not (Get-Command bcdedit -ErrorAction SilentlyContinue)) {
    function global:bcdedit { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) $null }
}
if (-not (Get-Command powercfg -ErrorAction SilentlyContinue)) {
    function global:powercfg { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) $null }
}
if (-not (Get-Command netsh -ErrorAction SilentlyContinue)) {
    function global:netsh { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) $null }
}
if (-not (Get-Command pnputil -ErrorAction SilentlyContinue)) {
    function global:pnputil { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) $null }
}
if (-not (Get-Command schtasks -ErrorAction SilentlyContinue)) {
    function global:schtasks { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) $null }
}

# ── Mock Call Tracker ────────────────────────────────────────────────────────
# Provides a centralized way to track all mock invocations across modules.
# Tests can inspect $SCRIPT:MockTracker to verify calls were (or were not) made.

$SCRIPT:MockTracker = @{
    SetItemProperty    = [System.Collections.Generic.List[object]]::new()
    NewItemProperty    = [System.Collections.Generic.List[object]]::new()
    RemoveItemProperty = [System.Collections.Generic.List[object]]::new()
    SetService         = [System.Collections.Generic.List[object]]::new()
    StartService       = [System.Collections.Generic.List[object]]::new()
    Bcdedit            = [System.Collections.Generic.List[object]]::new()
    Powercfg           = [System.Collections.Generic.List[object]]::new()
    Netsh              = [System.Collections.Generic.List[object]]::new()
    Pnputil            = [System.Collections.Generic.List[object]]::new()
    NewItem            = [System.Collections.Generic.List[object]]::new()
}

function Reset-MockTracker {
    <#  Clears all tracked mock calls. Call in BeforeEach for test isolation.  #>
    foreach ($key in @($SCRIPT:MockTracker.Keys)) {
        $SCRIPT:MockTracker[$key] = [System.Collections.Generic.List[object]]::new()
    }
}

function Get-MockCallCount {
    <#  Returns the total number of write-type mock calls across all categories.  #>
    $total = 0
    foreach ($key in @($SCRIPT:MockTracker.Keys)) {
        $total += $SCRIPT:MockTracker[$key].Count
    }
    return $total
}

function Get-MockCalls {
    <#  Returns all calls for a specific mock category.  #>
    param([string]$Category)
    if ($SCRIPT:MockTracker.ContainsKey($Category)) {
        return @($SCRIPT:MockTracker[$Category])
    }
    return @()
}

# ── Integration temp directory ───────────────────────────────────────────────
# Extends the base temp directory with integration-specific subdirs.
$SCRIPT:IntegrationTempRoot = Join-Path $SCRIPT:TestTempRoot "integration"
New-Item -ItemType Directory -Path $SCRIPT:IntegrationTempRoot -Force | Out-Null

function New-IntegrationTempDir {
    <#  Creates a unique temp subdirectory for a single test. Returns the path.  #>
    param([string]$Label = "test")
    $dir = Join-Path $SCRIPT:IntegrationTempRoot "$Label-$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    return $dir
}

function Remove-IntegrationTemp {
    <#  Removes the integration temp directory tree.  #>
    if ($SCRIPT:IntegrationTempRoot -and (Test-Path $SCRIPT:IntegrationTempRoot)) {
        Remove-Item $SCRIPT:IntegrationTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Convenience: Reset full integration state ────────────────────────────────
function Reset-IntegrationState {
    <#  Resets both test state and mock tracker. Use in BeforeEach blocks.  #>
    Reset-TestState
    Reset-MockTracker
}
