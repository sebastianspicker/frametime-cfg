<#
.SYNOPSIS  frametime.cfg - Post-Reboot Setup (Normal boot after GPU driver clean)

  Steps:
    1   Install driver (native extract + install)  [T1]
    2   MSI Interrupts (native registry)  [T2]
    3   NIC Interrupt Affinity (native registry)  [T3]
    4   NVIDIA CS2 Profile (native registry)  [T3]
    5   FPS Cap Info  [T1]
    6   CS2 Launch Options + In-game Settings
    7   VBS / Core Isolation disable  [T2]
    8   AMD GPU Settings  [T2, AMD only]
    9   DNS Server Configuration  [T3]
    10  Process Priority / CCD Affinity (native IFEO)  [T3]
    11  VRAM Usage Review  [Info]
    12  Final Checklist
    13  Final Benchmark + FPS Cap Calculation  [T1, LAST STEP]
#>

param([switch]$SmokeTest)

function Test-PublishedRuntimePayloadBootstrap {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$RuntimeRoot)

    function Assert-ProtectedRuntimeObject {
        param(
            [Parameter(Mandatory)][string]$Path,
            [switch]$Directory,
            [string]$PublisherSid
        )

        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        if ($item.PSProvider.Name -ne 'FileSystem' -or
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) -or
            ($Directory -and -not $item.PSIsContainer) -or
            (-not $Directory -and $item.PSIsContainer)) {
            throw "runtime object is not a regular protected filesystem $($(if ($Directory) { 'directory' } else { 'file' })): $Path"
        }

        $acl = Get-Acl -LiteralPath $Path -ErrorAction Stop
        $trustedSids = @('S-1-5-32-544', 'S-1-5-18')
        $toSid = {
            param($Identity)
            try {
                if ($Identity -is [Security.Principal.SecurityIdentifier]) { return $Identity.Value }
                return $Identity.Translate([Security.Principal.SecurityIdentifier]).Value
            } catch {
                $identityText = [string]$Identity
                if ($identityText -match '^S-1-[0-9]+(?:-[0-9]+)+$') { return $identityText }
                if ($identityText -match '(?i)^(BUILTIN\\Administrators|S-1-5-32-544)$') { return 'S-1-5-32-544' }
                if ($identityText -match '(?i)^(NT AUTHORITY\\SYSTEM|SYSTEM|S-1-5-18)$') { return 'S-1-5-18' }
                return $null
            }
        }
        if ((& $toSid $acl.Owner) -notin $trustedSids) {
            throw "runtime object owner is not BUILTIN\\Administrators or SYSTEM: $Path"
        }
        if (-not $acl.AreAccessRulesProtected) { throw "runtime object ACL inheritance is not protected: $Path" }

        $unsafeRights = [Security.AccessControl.FileSystemRights]::WriteData -bor
            [Security.AccessControl.FileSystemRights]::AppendData -bor
            [Security.AccessControl.FileSystemRights]::WriteExtendedAttributes -bor
            [Security.AccessControl.FileSystemRights]::WriteAttributes -bor
            [Security.AccessControl.FileSystemRights]::Delete -bor
            [Security.AccessControl.FileSystemRights]::DeleteSubdirectoriesAndFiles -bor
            [Security.AccessControl.FileSystemRights]::ChangePermissions -bor
            [Security.AccessControl.FileSystemRights]::TakeOwnership
        $trustedFullControl = @{}
        $publisherReadExecute = $false
        # Windows ACL APIs normally add Synchronize to an Allow ReadAndExecute
        # FileSystemAccessRule. It is safe and required for the ACE we create.
        $readExecuteRights = [Security.AccessControl.FileSystemRights]::ReadAndExecute
        $safePublisherRights = $readExecuteRights -bor [Security.AccessControl.FileSystemRights]::Synchronize
        foreach ($rule in @($acl.Access)) {
            $ruleSid = & $toSid $rule.IdentityReference
            if ($null -eq $ruleSid) { throw "runtime object ACL has an unresolvable principal: $Path" }
            if ($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow) {
                if ($ruleSid -notin $trustedSids -and (($rule.FileSystemRights -band $unsafeRights) -ne 0)) {
                    throw "runtime object ACL grants an untrusted principal write or ownership rights: $Path"
                }
                if ($ruleSid -notin $trustedSids -and $PublisherSid) {
                    if ($ruleSid -ne $PublisherSid -or
                        (([int64]$rule.FileSystemRights -band (-bnot [int64]$safePublisherRights)) -ne 0)) {
                        throw "runtime object ACL grants an untrusted principal rights beyond the bound publisher read/execute access: $Path"
                    }
                    if (($rule.FileSystemRights -band $readExecuteRights) -eq $readExecuteRights) { $publisherReadExecute = $true }
                }
                if ($ruleSid -in $trustedSids -and (($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq [Security.AccessControl.FileSystemRights]::FullControl)) {
                    $trustedFullControl[$ruleSid] = $true
                }
            }
        }
        foreach ($trustedSid in $trustedSids) {
            if (-not $trustedFullControl.ContainsKey($trustedSid)) {
                throw "runtime object ACL lacks trusted FullControl: $Path"
            }
        }
        if ($PublisherSid -and -not $publisherReadExecute) {
            throw "runtime object ACL lacks ReadAndExecute for the bound runtime publisher: $Path"
        }
    }

    function Get-BoundRuntimePublisherSid {
        param([Parameter(Mandatory)][string]$PublisherSid)

        if ($PublisherSid -notmatch '^S-1-[0-9]+(?:-[0-9]+)+$') { throw "runtime manifest publisher SID is invalid" }
        $currentSid = if ($isWindowsPlatform) {
            [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        } else {
            # Isolated Pester validator fixtures exercise the same SID binding
            # without requiring Windows ACL APIs on macOS/Linux.
            'S-1-5-21-1000-1000-1000-1001'
        }
        if ($PublisherSid -ne $currentSid) { throw "runtime manifest publisher does not match the current user" }
        return $PublisherSid
    }

    try {
        $isWindowsPlatform = [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT
        $normalizedRuntimeRoot = if ($isWindowsPlatform) {
            [IO.Path]::GetFullPath($RuntimeRoot).TrimEnd([char[]]@('\', '/'))
        } else {
            $RuntimeRoot.TrimEnd([char[]]@('\', '/'))
        }
        if ($normalizedRuntimeRoot -notmatch '(?i)^C:\\FRAMETIME_CFG\\runtime-generations\\[a-f0-9]{32}$') {
            throw "runtime root is outside the protected generation path"
        }
        $expectedPublisherSid = if ($isWindowsPlatform) {
            [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        } else { 'S-1-5-21-1000-1000-1000-1001' }
        if ($expectedPublisherSid -notmatch '^S-1-[0-9]+(?:-[0-9]+)+$') { throw "current runtime publisher SID is invalid" }
        foreach ($trustedRuntimeAncestor in @('C:\FRAMETIME_CFG', 'C:\FRAMETIME_CFG\runtime-generations')) {
            Assert-ProtectedRuntimeObject -Path $trustedRuntimeAncestor -Directory -PublisherSid $expectedPublisherSid
        }
        Assert-ProtectedRuntimeObject -Path $normalizedRuntimeRoot -Directory -PublisherSid $expectedPublisherSid
        $manifestPath = Join-Path $RuntimeRoot "runtime-manifest.json"
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "runtime-manifest.json is missing" }
        Assert-ProtectedRuntimeObject -Path $manifestPath -PublisherSid $expectedPublisherSid
        foreach ($runtimeDirectory in @(Get-ChildItem -LiteralPath $RuntimeRoot -Directory -Recurse -Force -ErrorAction Stop)) {
            Assert-ProtectedRuntimeObject -Path $runtimeDirectory.FullName -Directory -PublisherSid $expectedPublisherSid
        }
        foreach ($runtimeFile in @(Get-ChildItem -LiteralPath $RuntimeRoot -File -Recurse -Force -ErrorAction Stop)) {
            Assert-ProtectedRuntimeObject -Path $runtimeFile.FullName -PublisherSid $expectedPublisherSid
        }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        if ($manifest.schemaVersion -ne 1) { throw "unsupported runtime manifest schema" }
        $publisherSid = Get-BoundRuntimePublisherSid -PublisherSid ([string]$manifest.publisherSid)
        Assert-ProtectedRuntimeObject -Path 'C:\FRAMETIME_CFG' -Directory -PublisherSid $publisherSid
        Assert-ProtectedRuntimeObject -Path 'C:\FRAMETIME_CFG\runtime-generations' -Directory -PublisherSid $publisherSid
        Assert-ProtectedRuntimeObject -Path $normalizedRuntimeRoot -Directory -PublisherSid $publisherSid
        Assert-ProtectedRuntimeObject -Path $manifestPath -PublisherSid $publisherSid
        foreach ($runtimeDirectory in @(Get-ChildItem -LiteralPath $RuntimeRoot -Directory -Recurse -Force -ErrorAction Stop)) {
            Assert-ProtectedRuntimeObject -Path $runtimeDirectory.FullName -Directory -PublisherSid $publisherSid
        }
        $expectedContract = "de9aade388bc34ee1c7d71fa56f994c5642e0225831d8f708c8e65c4585ebcd9"
        $entries = @($manifest.files)
        if ($entries.Count -eq 0) { throw "runtime manifest has no files" }
        $manifestPaths = @($entries | ForEach-Object { [string]$_.path })
        if (@($manifestPaths | Group-Object | Where-Object Count -gt 1).Count -gt 0) { throw "runtime manifest contains duplicate paths" }
        $contractText = (@($manifestPaths | Sort-Object) -join "`n")
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            $actualContract = (([BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($contractText))) -replace '-', '').ToLowerInvariant())
        } finally {
            $sha256.Dispose()
        }
        if ($manifest.payloadContract -ne $expectedContract -or $actualContract -ne $expectedContract) { throw "runtime payload contract mismatch" }
        foreach ($relativePath in $manifestPaths) {
            if ($relativePath -notmatch '^[a-zA-Z0-9_.-]+(?:/[a-zA-Z0-9_.-]+)*$' -or $relativePath -match '(^|/)\.\.(/|$)') {
                throw "runtime manifest contains an unsafe path"
            }
        }
        $rootPath = if ($isWindowsPlatform) {
            $normalizedRuntimeRoot
        } else {
            (Convert-Path -LiteralPath $RuntimeRoot).TrimEnd([char[]]@('\', '/'))
        }
        $manifestFullPath = Convert-Path -LiteralPath $manifestPath
        $actualPaths = @(Get-ChildItem -LiteralPath $RuntimeRoot -File -Recurse -Force -ErrorAction Stop |
            Where-Object { (Convert-Path -LiteralPath $_.FullName) -ne $manifestFullPath } |
            ForEach-Object {
                (([IO.Path]::GetFullPath($_.FullName).Substring($rootPath.Length) -replace '^[\\/]+', '') -replace '\\', '/')
            })
        if (@(Compare-Object -ReferenceObject @($manifestPaths | Sort-Object) -DifferenceObject @($actualPaths | Sort-Object)).Count -gt 0) {
            throw "runtime contains missing or extra files"
        }
        foreach ($entry in $entries) {
            $relativePath = [string]$entry.path
            $expectedHash = [string]$entry.sha256
            if ($expectedHash -notmatch '^[A-Fa-f0-9]{64}$') { throw "invalid manifest hash for $relativePath" }
            $filePath = Join-Path $RuntimeRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
            Assert-ProtectedRuntimeObject -Path $filePath -PublisherSid $publisherSid
            $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256 -ErrorAction Stop).Hash
            if ($actualHash -ne $expectedHash) { throw "runtime hash mismatch: $relativePath" }
        }
        return [PSCustomObject]@{ Valid = $true; Message = "Published runtime payload verified." }
    } catch {
        return [PSCustomObject]@{ Valid = $false; Message = "Published runtime validation failed: $_" }
    }
}


function Test-PostRebootSetupAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]$identity
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-PostRebootSetupAdministrator {
    if (-not (Test-PostRebootSetupAdministrator)) {
        throw "PostReboot-Setup.ps1 must be run as Administrator. Start PowerShell with 'Run as administrator' and try again."
    }
}

function Invoke-PostRebootSetupEntryPoint {
    param([switch]$SmokeTest)

    if ($SmokeTest) {
        Write-Host "SMOKE TEST OK: PostReboot-Setup" -ForegroundColor Green
        return
    }

    $payloadValidation = Test-PublishedRuntimePayloadBootstrap -RuntimeRoot $PSScriptRoot
    if (-not $payloadValidation.Valid) {
        Write-Host "  CRITICAL: $($payloadValidation.Message)" -ForegroundColor Red
        Write-Host "  No Phase 3 system changes were attempted." -ForegroundColor Yellow
        Write-Host "  Re-run Phase 1 to publish a complete runtime payload, then launch Phase 3 again." -ForegroundColor Cyan
        return
    }
    Assert-PostRebootSetupAdministrator
    Invoke-PostRebootSetup
}

function Remove-GpuAppxPackages {
    [CmdletBinding(SupportsShouldProcess)]
    param([Parameter(Mandatory)][ValidateSet("1", "2", "3", "4")][string]$GpuInput)

    $gpuAppxVendor = switch ($GpuInput) {
        { $_ -in @("1", "2") } { "NVIDIA" }
        "3"                      { "AMD" }
        default                  { $null }
    }
    if (-not $gpuAppxVendor) {
        return [PSCustomObject]@{ Status = 'NotApplicable'; CanCompleteStep = $true; RemovedCount = 0; Message = 'No GPU AppX cleanup is required.' }
    }

    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would remove leftover $gpuAppxVendor AppX and provisioned packages." -ForegroundColor Magenta
        return [PSCustomObject]@{ Status = 'DryRun'; CanCompleteStep = $false; RemovedCount = 0; Message = 'GPU AppX cleanup was previewed.' }
    }
    if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue) -or
        -not (Get-Command Get-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
        return [PSCustomObject]@{ Status = 'Failed'; CanCompleteStep = $false; RemovedCount = 0; Message = 'Required AppX inventory cmdlets are unavailable.' }
    }

    $removedCount = 0
    $failures = [Collections.Generic.List[string]]::new()
    $notProcessed = 0
    $selectInstalled = {
        $_.Name -match $gpuAppxVendor -and
        -not ($gpuAppxVendor -eq 'NVIDIA' -and
            ($_.Name -match 'ControlPanel' -or $_.PackageFullName -match 'ControlPanel'))
    }
    $selectProvisioned = {
        ($_.DisplayName -match $gpuAppxVendor -or $_.PackageName -match $gpuAppxVendor) -and
        -not ($gpuAppxVendor -eq 'NVIDIA' -and
            ($_.DisplayName -match 'ControlPanel' -or $_.PackageName -match 'ControlPanel'))
    }
    try {
        $gpuAppx = @(Get-AppxPackage -AllUsers -ErrorAction Stop | Where-Object $selectInstalled)
        $gpuProv = @(Get-AppxProvisionedPackage -Online -ErrorAction Stop | Where-Object $selectProvisioned)
    } catch {
        return [PSCustomObject]@{ Status = 'Failed'; CanCompleteStep = $false; RemovedCount = 0; Message = "GPU AppX inventory failed: $($_.Exception.Message)" }
    }

    foreach ($pkg in $gpuAppx) {
        if (-not $PSCmdlet.ShouldProcess($pkg.PackageFullName, 'Remove leftover GPU AppX package')) {
            $notProcessed++
            continue
        }
        try {
            Remove-AppxPackage -Package $pkg.PackageFullName -AllUsers -ErrorAction Stop
            Write-OK "Removed leftover AppX: $($pkg.Name)"
            $removedCount++
        } catch {
            $failures.Add("Installed package '$($pkg.Name)' could not be removed: $($_.Exception.Message)")
        }
    }

    foreach ($pkg in $gpuProv) {
        if (-not $PSCmdlet.ShouldProcess($pkg.PackageName, 'Remove provisioned GPU AppX package')) {
            $notProcessed++
            continue
        }
        try {
            Remove-AppxProvisionedPackage -Online -PackageName $pkg.PackageName -ErrorAction Stop | Out-Null
            Write-OK "Removed provisioned: $($pkg.DisplayName)"
            $removedCount++
        } catch {
            $failures.Add("Provisioned package '$($pkg.DisplayName)' could not be removed: $($_.Exception.Message)")
        }
    }

    if ($notProcessed -gt 0) {
        return [PSCustomObject]@{ Status = 'DryRun'; CanCompleteStep = $false; RemovedCount = $removedCount; Message = "$notProcessed GPU AppX removal(s) were not processed." }
    }

    try {
        $remainingInstalled = @(Get-AppxPackage -AllUsers -ErrorAction Stop | Where-Object $selectInstalled)
        $remainingProvisioned = @(Get-AppxProvisionedPackage -Online -ErrorAction Stop | Where-Object $selectProvisioned)
    } catch {
        $failures.Add("Post-cleanup GPU AppX inventory failed: $($_.Exception.Message)")
        $remainingInstalled = @()
        $remainingProvisioned = @()
    }
    foreach ($pkg in $remainingInstalled) {
        $failures.Add("Installed GPU AppX package remains: $($pkg.PackageFullName)")
    }
    foreach ($pkg in $remainingProvisioned) {
        $failures.Add("Provisioned GPU AppX package remains: $($pkg.PackageName)")
    }

    if ($failures.Count -gt 0) {
        foreach ($failure in $failures) { Write-Debug "AppX cleanup: $failure" }
        return [PSCustomObject]@{
            Status = 'Failed'; CanCompleteStep = $false; RemovedCount = $removedCount
            Message = ($failures -join '; ')
        }
    }

    return [PSCustomObject]@{ Status = 'Success'; CanCompleteStep = $true; RemovedCount = $removedCount; Message = 'GPU AppX cleanup completed and was verified.' }
}

function Get-VbsDetectionResult {
    <#
    .SYNOPSIS
        Returns the authoritative VBS detection outcome used by Phase 3 Step 7.
    .DESCRIPTION
        A missing DeviceGuard instance or failed CIM query is a failed detection,
        not evidence that VBS is inactive.
    #>
    try {
        $dg = Get-CimInstance -ClassName Win32_DeviceGuard `
            -Namespace root/Microsoft/Windows/DeviceGuard -ErrorAction Stop
        if (-not $dg) {
            return [PSCustomObject]@{ Status = "Failed"; CanCompleteStep = $false; IsActive = $null; Message = "VBS detection returned no Win32_DeviceGuard instance." }
        }
        if ($null -eq $dg.VirtualizationBasedSecurityStatus) {
            return [PSCustomObject]@{ Status = "Failed"; CanCompleteStep = $false; IsActive = $null; Message = "VBS detection returned no VirtualizationBasedSecurityStatus." }
        }
        return [PSCustomObject]@{
            Status          = "Success"
            CanCompleteStep = $true
            IsActive        = ([int]$dg.VirtualizationBasedSecurityStatus -ge 2)
            Message         = "Win32_DeviceGuard query completed."
        }
    } catch {
        return [PSCustomObject]@{ Status = "Failed"; CanCompleteStep = $false; IsActive = $null; Message = "VBS detection query failed: $($_.Exception.Message)" }
    }
}

function Invoke-PostRebootSetup {
[CmdletBinding()]
param(
    [object]$PreviewState,
    [switch]$SimulateNormalBoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Continue"
$ScriptRoot = $PSScriptRoot
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"
. "$ScriptRoot\Guide-VideoSettings.ps1"

if ($null -ne $PreviewState) {
    $state = if ($PreviewState -is [hashtable]) { [PSCustomObject]$PreviewState } else { $PreviewState }
    if (-not $SimulateNormalBoot -or [string]$state.mode -ne "DRY-RUN") {
        throw "Injected Phase 3 state is allowed only for an explicit simulated DRY-RUN."
    }
    $SCRIPT:Mode = "DRY-RUN"
    $SCRIPT:Profile = if ($state.PSObject.Properties['profile']) { [string]$state.profile } else { "CUSTOM" }
    $SCRIPT:LogLevel = if ($state.PSObject.Properties['logLevel']) { [string]$state.logLevel } else { "VERBOSE" }
    $SCRIPT:DryRun = $true
} else {
    try {
        # Read-only discovery prevents a saved preview from creating or hardening
        # its work directory. Live state is reloaded through the secure path.
        $state = Load-State -Path $CFG_StateFile -ReadOnly
        if (-not $SCRIPT:DryRun) {
            $state = Load-State -Path $CFG_StateFile
            Restore-ProtectedRuntimePublisherTraverse -RuntimeRoot $PSScriptRoot
        }
    } catch {
    Write-Host "  $([char]0x2718) Something went wrong: settings file (state.json) is missing or corrupted." -ForegroundColor Red
    Write-Host "    Error detail: $_" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  $([char]0x2139) What to do:" -ForegroundColor Cyan
    Write-Host "    - Phase 1 may not have finished. Re-run it from an authenticated release." -ForegroundColor White
    Write-Host "    - Or press [Y] below to use hardware detection and safe defaults." -ForegroundColor White
    $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Detect the GPU and continue with defaults? [y/N]" }
    if ($r -notmatch "^[jJyY]$") { exit 1 }
    # Detect GPU vendor instead of blindly defaulting to NVIDIA
    $detectedGpu = $null
    try {
        $gpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
               Where-Object { $_.Status -eq "OK" -and $_.Name -notmatch "Basic Display" } |
               Select-Object -First 1
        if ($gpu) {
            if ($gpu.Name -match "AMD|Radeon") { $detectedGpu = "3" }
            elseif ($gpu.Name -match "Intel.*Arc|Intel.*Graphics") { $detectedGpu = "4" }
            elseif ($gpu.Name -match "RTX\s*5\d{3}") { $detectedGpu = "1" }
            elseif ($gpu.Name -match "NVIDIA|GeForce|RTX") { $detectedGpu = "2" }
        }
    } catch {
        Write-DebugLog "GPU detection failed for default Phase 3 state: $($_.Exception.Message)"
    }
    $state = [PSCustomObject]@{ gpuInput=$detectedGpu; mode="CONTROL"; logLevel="NORMAL"; profile="RECOMMENDED"; fpsCap=0; avgFps=0; rollbackDriver=$null; nvidiaDriverPath=$null; baselineAvg=$null; baselineP1=$null }
    if ($detectedGpu) { Save-SuiteState -State $state }
    $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
    }
}
$gpuInput = Get-PhaseGpuInput -State $state
if ([string]::IsNullOrWhiteSpace([string]$gpuInput)) {
    Write-Host "  CRITICAL: state.json has no valid gpuInput value." -ForegroundColor Red
    Write-Host "  No Phase 3 system changes were attempted." -ForegroundColor Yellow
    Write-Host "  Re-run Phase 1 and select GPU branch 1, 2, 3, or 4." -ForegroundColor Cyan
    throw [IO.InvalidDataException]::new("Phase 3 state validation failed: gpuInput must be a scalar value from 1 through 4.")
}
$fpsCap   = if ($state.PSObject.Properties['fpsCap']) { $state.fpsCap } else { 0 }
$SCRIPT:fpsCap = $fpsCap
$avgFps   = if ($state.PSObject.Properties['avgFps']) { $state.avgFps } else { 0 }
$PHASE    = 3
$SCRIPT:PhaseTotal = 13
$SCRIPT:CurrentPhase = 3

# A saved DRY-RUN remains immutable for the entire invocation. Switching from a
# preview into live mutations mid-phase would violate the DRY-RUN contract.
if ($SCRIPT:DryRun) {
    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
    Write-Host "  ║  DRY-RUN MODE INHERITED FROM PHASE 1                       ║" -ForegroundColor Magenta
    Write-Host "  ║  Phase 3 will preview changes only - nothing will be       ║" -ForegroundColor Magenta
    Write-Host "  ║  applied. DRY-RUN cannot switch to live mode mid-phase.    ║" -ForegroundColor Magenta
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
}

# Guard: Phase 3 requires Normal Mode. If Safe Mode is active, the driver installer
# will fail and most optimizations cannot be applied.
if ($env:SAFEBOOT_OPTION -and -not ($SCRIPT:DryRun -and $SimulateNormalBoot)) {
    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "  ║  SAFE MODE DETECTED - Phase 3 requires Normal Mode         ║" -ForegroundColor Red
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host "  $([char]0x2139) Phase 3 installs GPU drivers and applies settings that need" -ForegroundColor Cyan
    Write-Host "    Normal Mode. The Safe Mode boot flag may not have been cleared." -ForegroundColor Cyan
    Write-Host ""
    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would clear and verify the Safe Mode flag, register Phase 3, and restart into Normal Mode." -ForegroundColor Magenta
        Write-Warn "No boot change was made. Run the full preview from Normal Mode, or recover Safe Mode manually."
        return
    }
    Write-Host "  Clearing Safe Mode flag now..." -ForegroundColor White
    $safeBootResult = Clear-SafeBootVerified
    if ($safeBootResult.Verified) {
        $runOnceResult = Set-RunOnce "FRAMETIME_Phase3" "$ScriptRoot\PostReboot-Setup.ps1" -PassThru
        if ($runOnceResult.Applied) {
            Write-Host "  $([char]0x2714) Safe Mode disabled and Phase 3 handoff applied." -ForegroundColor Green
            Write-Host "    Restarting into Normal Mode; Phase 3 will run automatically." -ForegroundColor White
            Start-Sleep -Seconds 2
            shutdown /r /t 0 /f
            return
        }
        Write-Host "  $([char]0x26A0) Safe Mode was cleared, but the Phase 3 automatic handoff failed." -ForegroundColor Yellow
    } else {
        Write-Host "  $([char]0x26A0) $($safeBootResult.Message)" -ForegroundColor Yellow
    }
    Write-Host "  Automatic restart is blocked. Remain in this session and recover manually." -ForegroundColor Yellow
    Write-Host "  Run in elevated cmd.exe:" -ForegroundColor White
    Write-Host "    bcdedit /deletevalue safeboot" -ForegroundColor Cyan
    Write-Host "    bcdedit /enum {current} /v" -ForegroundColor Cyan
    Write-Host "  After Safe Mode is confirmed absent, register or launch Phase 3 manually." -ForegroundColor White
    Write-Host "    shutdown /r /t 0" -ForegroundColor Cyan
    if (-not (Test-YoloProfile)) { Read-Host "  Press Enter to exit" }
    return
}

$backupInitialized = $false
try {
# Initialize backup system for this phase (inside try so finally releases the lock on error)
if (-not $SCRIPT:DryRun) {
    Initialize-Backup
    $backupInitialized = $true
}
Initialize-PhaseCounters

if (-not $SCRIPT:DryRun) {
    Ensure-Dir $CFG_LogDir
    Initialize-Log
}
Write-Banner 3 3 "Normal Boot  ·  Driver · MSI · CS2"
if ($SCRIPT:DryRun -and $SimulateNormalBoot) {
    Write-Info "DRY-RUN: Normal Mode is simulated; no reboot or handoff occurred."
}

$startStep = Show-ResumePrompt $PHASE 13
if (-not (Test-StepCompleted $PHASE 1)) {
    Write-Warn "The required driver installation was deferred or left incomplete; resuming at Step 1."
    $startStep = 1
} elseif ($startStep -gt 13 -and -not (Test-StepCompleted $PHASE 13)) {
    Write-Warn "The final benchmark was previously skipped or left incomplete; resuming at Step 13."
    $startStep = 13
} elseif ($startStep -gt 13) {
    Write-Info "Phase 3 already completed."
    $handoffRemoval = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru
    if (-not $handoffRemoval.Applied) {
        Write-Warn "Phase 3 is complete, but its automatic handoff could not be removed: $($handoffRemoval.Message)"
    }
    Remove-BackupLock
    if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter]" }
    exit 0
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 1 - INSTALL DRIVER  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 1) {
    Write-Section "Step 1 - Install Driver"
    Write-TierBadge 1 "Clean driver installation after GPU driver removal"

    # Phase 2 runs in Safe Mode where AppXSVC cannot start, so AppX removal is
    # deferred to Normal Mode. The helper owns the DRY-RUN boundary.
    $gpuAppxCleanup = Remove-GpuAppxPackages -GpuInput $gpuInput
    if (-not $SCRIPT:DryRun -and -not $gpuAppxCleanup.CanCompleteStep) {
        throw "Normal-Mode GPU AppX cleanup did not complete: $($gpuAppxCleanup.Message)"
    }

    # Check if Phase 2 driver removal actually completed
    if ($SCRIPT:DryRun -and $SimulateNormalBoot) {
        $p2DriverDone = $true
    } else {
        $p2DriverDone = Test-StepCompleted 2 2
    }
    if (-not $p2DriverDone) {
        Write-Host ""
        Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Yellow
        Write-Host "  ║  Phase 2 driver removal was skipped or did not complete     ║" -ForegroundColor Yellow
        Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Yellow
        Write-Host "  $([char]0x2139) The old GPU driver may still be installed. The new driver will" -ForegroundColor Cyan
        Write-Host "    install over it (not a fully clean install)." -ForegroundColor Cyan
        Write-Host "  $([char]0x2139) For a clean install, run Phase 2 first from its protected generation." -ForegroundColor Cyan
        $p2Choice = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "Y" } else { Read-Host "  Continue with driver install anyway? [Y/n]" }
        if ($p2Choice -match "^[nN]$") {
            Write-Info "Skipped. Run Phase 2 from its protected generation in Safe Mode, then return here."
            Skip-Step $PHASE 1 "Driver"
            $p2DriverDone = $null  # signal to skip the driver install block below
        }
    }

    if ($null -eq $p2DriverDone) {
        # User chose to skip - Step 1 already recorded as skipped above
    } elseif ($gpuInput -in @("1","2")) {
        # Find driver .exe - check state first, then prompt
        # SECURITY: state.json is in C:\FRAMETIME_CFG\ - if tampered, nvidiaDriverPath could
        # point to malware. Validate: must be an .exe file, must exist, must not contain
        # path traversal sequences. The file is then passed to Start-Process in Install-NvidiaDriverClean.
        $driverExe = if ($state.PSObject.Properties['nvidiaDriverPath']) { $state.nvidiaDriverPath } else { $null }
        if ($driverExe) {
            # Reject path traversal, non-.exe, and suspicious paths
            if ($driverExe -match '\.\.' -or $driverExe -notmatch '\.exe$' -or $driverExe -match '[\x00]') {
                Write-Warn "state.json nvidiaDriverPath failed validation - ignoring: $driverExe"
                $driverExe = $null
            }
        }
        if ($SCRIPT:DryRun -and [string]::IsNullOrWhiteSpace([string]$driverExe)) {
            $driverExe = "$CFG_WorkDir\nvidia_driver-preview.exe"
            Write-Host "  [DRY-RUN] Would obtain and validate the matching NVIDIA driver package." -ForegroundColor Magenta
        }
        if (-not $driverExe -or (-not $SCRIPT:DryRun -and -not (Test-Path $driverExe))) {
            Write-Info "NVIDIA driver .exe not found in expected location."
            Write-Info "If you downloaded the driver manually, provide the path now."
            Write-Info "Press [B] to browse for the file, or paste the path, or [Enter] to download."
            $driverExe = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "" } elseif ($true) {
                $driverInput = Read-Host "  Path / [B]rowse / [Enter] to download"
                if ($driverInput -match '^[bB]$') {
                    Add-Type -AssemblyName System.Windows.Forms
                    $dlg = New-Object System.Windows.Forms.OpenFileDialog
                    $dlg.Title = "Select NVIDIA Driver Installer"
                    $dlg.Filter = "Executable files (*.exe)|*.exe"
                    $dlg.InitialDirectory = [Environment]::GetFolderPath('UserProfile') + '\Downloads'
                    if ($dlg.ShowDialog() -eq 'OK') { $dlg.FileName } else { "" }
                } else {
                    # Strip surrounding quotes that paste sometimes adds
                    $driverInput -replace '^["'']|["'']$', ''
                }
            } else { "" }
            if (-not [string]::IsNullOrWhiteSpace($driverExe)) {
                # Validate user-provided path: must exist and be an .exe
                if (-not (Test-Path $driverExe)) {
                    Write-Warn "File not found: $driverExe"
                    $driverExe = $null
                } elseif ($driverExe -notmatch '\.exe$') {
                    Write-Warn "Not an .exe file: $driverExe"
                    $driverExe = $null
                }
            }
            if ([string]::IsNullOrWhiteSpace($driverExe)) {
                if ($SCRIPT:DryRun) {
                    Write-Host "  [DRY-RUN] Would attempt automatic NVIDIA driver download" -ForegroundColor Magenta
                } else {
                    Write-Step "Attempting automatic driver download..."
                    $savedGpuName = if ($state.PSObject.Properties['nvidiaGpuName']) { $state.nvidiaGpuName } else { $null }
                    $driverInfo = Get-LatestNvidiaDriver -GpuName $savedGpuName
                    if ($driverInfo -and -not $driverInfo.ManualDownload) {
                        $driverExe = "$CFG_WorkDir\nvidia_driver.exe"
                        $dlResult = Invoke-Download $driverInfo.Url $driverExe "NVIDIA Driver $($driverInfo.Version)"
                        if (-not $dlResult) {
                            $driverExe = $null
                        # SECURITY (S1): Verify Authenticode signature immediately after download
                        } elseif (-not (Test-NvidiaDriverSignature $driverExe)) {
                            Write-Err "Downloaded driver failed signature verification. File removed."
                            $driverExe = $null
                        }
                    } else {
                        Write-Warn "Auto-download failed. Download manually:"
                        Write-Info "https://www.nvidia.com/en-us/drivers/"
                        Write-Info "Press [B] to browse for the file, or paste the path."
                        $rawInput = if (Test-YoloProfile) { "" } else { Read-Host "  Path / [B]rowse" }
                        if ($rawInput -match '^[bB]$') {
                            Add-Type -AssemblyName System.Windows.Forms
                            $dlg = New-Object System.Windows.Forms.OpenFileDialog
                            $dlg.Title = "Select NVIDIA Driver Installer"
                            $dlg.Filter = "Executable files (*.exe)|*.exe"
                            $dlg.InitialDirectory = [Environment]::GetFolderPath('UserProfile') + '\Downloads'
                            $driverExe = if ($dlg.ShowDialog() -eq 'OK') { $dlg.FileName } else { $null }
                        } else {
                            $driverExe = $rawInput -replace '^["'']|["'']$', ''
                        }
                        if ($driverExe -and ($driverExe -match '\.\.' -or $driverExe -notmatch '\.exe$' -or $driverExe -match '[\x00]')) {
                            Write-Warn "Invalid driver path: $driverExe"
                            $driverExe = $null
                        }
                    }
                }
            }
        }

        if ($state.PSObject.Properties['rollbackDriver'] -and $state.rollbackDriver) {
            Write-Warn "Ignoring legacy rollbackDriver metadata; fixed-version rollback selection is not supported by this alpha."
        }

        if ($driverExe -and ($SCRIPT:DryRun -or (Test-Path $driverExe))) {
            Write-Info "Driver: $driverExe"
            Write-Info "Installing after removing selected optional package components..."
            Write-Info "Post-install device and profile operations are handled by later steps."
            Write-Blank
            $r = if ($SCRIPT:DryRun) { "Y" } elseif (Test-YoloProfile) { "Y" } else { Read-Host "  Install now? [Y/n]" }
            if ($r -notmatch "^[nN]$") {
                $result = Install-NvidiaDriverClean -DriverExe $driverExe
                if ($result) {
                    Write-OK "NVIDIA driver installed successfully."
                    Complete-Step $PHASE 1 "Driver"
                } else {
                    Write-Warn "NVIDIA installation verification failed. The step was not completed."
                    Write-Host "  $([char]0x2139) Review the log and Device Manager display-adapter state before retrying." -ForegroundColor Cyan
                    Write-Host "    This step will re-run on resume after the underlying issue is resolved." -ForegroundColor DarkGray
                }
            } else {
                Write-Warn "Driver installation deferred. Step 1 remains pending and will run again on resume."
            }
        } else {
            Write-Err "No valid driver file (.exe) found."
            Write-Host "  $([char]0x2139) What to do: Download your driver from the link below," -ForegroundColor Cyan
            Write-Host "    then re-run the protected generation's elevation bootstrap." -ForegroundColor Cyan
            Write-Info "Download: https://www.nvidia.com/en-us/drivers/"
            $skipConfirm = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "y" } else { Read-Host "  Skip driver install and continue to Step 2? [y/N]" }
            if ($skipConfirm -match "^[jJyY]$") {
                Write-Warn "Driver installation deferred. Step 1 remains pending and will run again on resume."
            } else {
                Write-Info "Restart Phase 3 when ready."
            }
        }

        if ($gpuInput -eq "1") {
            Write-Warn "RTX 5000: NVIDIA CP -> Scaling -> MONITOR (not GPU)!"
        }
    } else {
        Write-Info "$(if($gpuInput -eq '3'){'AMD: https://www.amd.com/support'}else{'Intel Arc: https://www.intel.com/content/www/us/en/download-center/home.html'})"
        Write-Info "Custom Install -> driver only, no overlay / link."
        if (Test-YoloProfile) {
            Write-Warn "Manual driver installation is required for AMD/Intel. YOLO mode cannot verify it, so Step 1 remains pending."
            Write-Info "Install the driver manually, then rerun Phase 3 interactively to acknowledge completion."
        } elseif ($SCRIPT:DryRun) {
            Write-Info "DRY-RUN: manual AMD/Intel driver workflow previewed; a live run would still require user confirmation."
        } else {
            Read-Host "  After driver installation [Enter]"
            Complete-Step $PHASE 1 "Driver"
        }
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 2 - MSI INTERRUPTS  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 2) {
    Write-Section "Step 2 - MSI Interrupts  ·  GPU + NIC + Audio"
    $null = Invoke-TieredStep -Tier 2 -Title "Request MSI support (native registry)" `
        -Why "Requests message-signaled interrupts for supported GPU, NIC, and audio devices." `
        -Evidence "LatencyMon can help diagnose DPC behavior. This repository includes no cross-device benchmark for MSI mode." `
        -Caveat "Not all devices support MSI. Missing devices are skipped; registry write failures stop the step." `
        -Risk "MODERATE" -Depth "REGISTRY" `
        -Improvement "Writes and reads back the MSI support policy; negotiated mode still requires post-reboot verification" `
        -SideEffects "Unsupported or unstable devices may require the registry value to be removed" `
        -Undo "Delete MSISupported values from device Interrupt Management registry keys" `
        -Action {
            $msiResult = Enable-DeviceMSI
            if ($msiResult.Status -eq "Skipped") {
                Skip-Step $PHASE 2 "MSI"
            } elseif (-not $msiResult.CanCompleteStep) {
                throw "MSI interrupt configuration did not complete: $($msiResult.Message)"
            } else {
                Complete-Step $PHASE 2 "MSI"
            }
        } `
        -SkipAction { Skip-Step $PHASE 2 "MSI" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 3 - NIC INTERRUPT AFFINITY  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 3) {
    Write-Section "Step 3 - NIC Interrupt Affinity"
    $null = Invoke-TieredStep -Tier 3 -Title "Set NIC interrupt affinity (native registry)" `
        -Why "Requests a repository-defined last-core affinity mask for the active wired NIC." `
        -Evidence "T3: Only relevant after LatencyMon diagnosis with clear NIC DPC issue." `
        -Caveat "Use only after diagnosis. An unsuitable affinity can increase latency or concentrate interrupt load." `
        -Risk "MODERATE" -Depth "REGISTRY" `
        -Improvement "Writes and verifies the requested NIC affinity policy for later device-level validation" `
        -SideEffects "Wrong affinity can increase latency. Only useful after LatencyMon diagnosis." `
        -Undo "Delete DevicePolicy + AssignmentSetOverride from NIC Affinity Policy key" `
        -Action {
            $affinityResult = Set-NicInterruptAffinity
            if ($affinityResult.Status -eq "Skipped") {
                Skip-Step $PHASE 3 "NicAffinity"
            } elseif (-not $affinityResult.CanCompleteStep) {
                throw "NIC interrupt affinity did not complete: $($affinityResult.Message)"
            } else {
                Complete-Step $PHASE 3 "NicAffinity"
            }
        } `
        -SkipAction { Skip-Step $PHASE 3 "NicAffinity" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 4 - NVIDIA CS2 PROFILE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 4) {
    if ($gpuInput -in @("1","2")) {
        Write-Section "Step 4 - NVIDIA CS2 Profile (DRS direct write)"
        $null = Invoke-TieredStep -Tier 3 -Title "Apply NVIDIA CS2 profile settings (DRS + registry)" `
            -Why "Writes 42 repository-defined DWORD settings to the NVIDIA DRS database through nvapi64.dll. Falls back to the checked-in registry set if DRS is unavailable." `
            -Evidence "T3: No isolated 1%-low benchmark for the full profile. Individual flags may be T2." `
            -Caveat "Requires nvapi64.dll (NVIDIA driver installed). Falls back to registry if unavailable." `
            -Risk "SAFE" -Depth "DRIVER" `
            -Improvement "Applies 42 repository-defined DWORD settings to the DRS database and the PerfLevelSrc registry value" `
            -SideEffects "Changes driver-specific DRS and GPU registry state. Prior values are recorded where the interfaces expose them." `
            -Undo "Restore via backup rollback, or NVIDIA CP -> Manage 3D Settings -> Restore Defaults" `
            -Action {
                $profileResult = Apply-NvidiaCS2Profile
                if (-not $profileResult.CanCompleteStep -and -not ($SCRIPT:DryRun -and $profileResult.Status -eq "DryRun")) {
                    throw "NVIDIA CS2 profile did not complete: $($profileResult.Message)"
                }
                Complete-Step $PHASE 4 "NVProfile"
            } `
            -SkipAction { Skip-Step $PHASE 4 "NVProfile" }
    } else {
        Skip-Step $PHASE 4 "NVProfile"
        Write-Section "Step 4 - GPU Profile (skipped - non-NVIDIA)"
        Write-Blank
        Write-Host "  This suite has no AMD/Intel equivalent of the NVIDIA DRS profile step." -ForegroundColor Yellow
        if ($gpuInput -eq "3") {
            Write-Info "Step 8 presents the repository's manual AMD review checklist. It does not apply or verify Radeon settings."
        } else {
            Write-Info "Intel Arc profile settings are not automated. Review current Intel and CS2 documentation for the installed driver."
        }
        Write-Blank
        if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] to continue" }
    }

    # NVIDIA CP hints - always show for NVIDIA
    if ($gpuInput -in @("1","2")) {
        Write-Blank
        Write-Host "  NVIDIA CONTROL PANEL - remaining manual checks:" -ForegroundColor White
        if ($fpsCap -gt 0) {
            Write-Host "  [T1] Max Frame Rate        ->  $fpsCap  (avg $avgFps - 9%)" -ForegroundColor Green
            "$fpsCap" | Set-ClipboardSafe
            Write-Info "       FPS cap $fpsCap copied to clipboard."
        } else {
            Write-Host "  [T1] Max Frame Rate        ->  set after benchmark with FpsCap-Calculator" -ForegroundColor Yellow
        }
        Write-Host "  [T2] Low Latency Mode      ->  Ultra  (only if Reflex NOT active)" -ForegroundColor Yellow
        Write-Blank
        Write-Host "  NOTE: 3 niche settings excluded (string type, GPU-specific, frame interp)." -ForegroundColor DarkGray
        Write-Host "  See docs/nvidia-drs-settings.md for full details." -ForegroundColor DarkGray
        if ($gpuInput -eq "1") {
            Write-Warn "RTX 5000: Scaling -> MONITOR (not GPU) for 4:3 stretched."
        }
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 5 - FPS CAP INFO  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 5) {
    Write-Section "Step 5 - FPS Cap Info"
    Write-TierBadge 1 "FPS cap calculation"
    Write-Blank
    Write-Host "  FPS cap will be calculated in the FINAL STEP after all optimizations." -ForegroundColor Yellow
    Write-Host "  Method: average FPS minus 9% (repository default)." -ForegroundColor DarkGray
    if ($fpsCap -gt 0) {
        Write-Host "  Already calculated cap: $fpsCap  (avg $avgFps - 9%)" -ForegroundColor Green
    }
    Complete-Step $PHASE 5 "FpsCapInfo"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 6 - CS2 LAUNCH OPTIONS + VIDEO SETTINGS
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 6) {
    Write-Section "Step 6 - Launch Options + Video Settings"
    Show-CS2SettingsGuide -fpsCap $fpsCap -avgFps $avgFps -gpuInput $gpuInput
    Complete-Step $PHASE 6 "CS2Settings"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 7 - VBS / CORE ISOLATION DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 7) {
    Write-Section "Step 7 - VBS / Core Isolation (Memory Integrity)"
    $null = Invoke-TieredStep -Tier 2 -Title "Disable VBS / Core Isolation (Memory Integrity)" `
        -Why "Virtualization-Based Security and HVCI add isolation that can affect workload scheduling. The performance effect depends on the system and workload." `
        -Evidence "Microsoft documents the security model. This repository does not include local benchmark evidence for a specific performance change." `
        -Caveat "Disabling VBS or HVCI reduces Windows security protections and can conflict with software that requires those features. Verify anti-cheat and organizational requirements first." `
        -Risk "MODERATE" -Depth "REGISTRY" `
        -Improvement "Removes the isolation layer when it is active. Measure the workload locally to determine any performance effect." `
        -SideEffects "Reduces Windows security protections and may break software that requires VBS or HVCI." `
        -Undo "Windows Security -> Device Security -> Core Isolation -> Memory Integrity: ON" `
        -Action {
            # DeviceGuard is the authoritative VBS state source for this step.
            # A failed or incomplete query must not be treated as "inactive".
            if ($SCRIPT:DryRun) {
                $vbsDetection = [PSCustomObject]@{ Status = "Success"; CanCompleteStep = $true; IsActive = $true; Message = "VBS active state simulated for full preview." }
            } else {
                $vbsDetection = Get-VbsDetectionResult
            }
            $hvciPendingOff = $false
            if ($vbsDetection.Status -ne "Success") {
                throw "VBS detection did not complete: $($vbsDetection.Message)"
            }
            if (-not $SCRIPT:DryRun) {
                try {
                    $hvciVal = Get-ItemProperty `
                        "HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity" `
                        -Name "Enabled" -ErrorAction SilentlyContinue
                    if ($hvciVal -and $hvciVal.Enabled -eq 0) { $hvciPendingOff = $true }
                } catch {
                    Write-DebugLog "HVCI registry pending-off probe failed: $($_.Exception.Message)"
                }
            }

            if (-not $vbsDetection.IsActive) {
                Write-OK "VBS/Core Isolation: not active."
                Complete-Step $PHASE 7 "VBS"
                return
            }
            if ($hvciPendingOff) {
                Write-OK "Memory Integrity is already set to disable. Reboot to apply the change."
                Complete-Step $PHASE 7 "VBS"
                return
            }

            Write-Warn "VBS/HVCI is active. Disabling it reduces Windows security protections."
            Write-Blank

            # FACEIT / Vanguard warning
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
            Write-Host "  │  Some anti-cheat software requires HVCI.                    │" -ForegroundColor Red
            Write-Host "  │  Verify compatibility before applying this step.           │" -ForegroundColor Red
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
            Write-Blank

            # Disable Memory Integrity (HVCI) via registry
            $hvciWriteResult = Set-RegistryValue `
                "HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity" `
                "Enabled" 0 "DWord" "Disable Memory Integrity (HVCI)" -PassThru

            if (-not $hvciWriteResult) {
                throw "VBS/HVCI disable did not return a registry-write result."
            }
            if ($hvciWriteResult.Status -eq "DryRun") {
                Write-Info "DRY-RUN: VBS/HVCI registry plan previewed; no state was changed."
                return
            }
            if ($hvciWriteResult.Status -ne "Success") {
                throw "VBS/HVCI disable did not complete: $($hvciWriteResult.Message)"
            }

            Write-OK "Memory Integrity (HVCI) disabled. Reboot required for full effect."
            Write-Info "Verify after reboot: msinfo32 -> Virtualization-based security -> 'Not Enabled'"
            Complete-Step $PHASE 7 "VBS"
        } `
        -SkipAction { Skip-Step $PHASE 7 "VBS" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 8 - AMD GPU SETTINGS  [T2, AMD only]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 8) {
    if ($gpuInput -eq "3") {
        Write-Section "Step 8 - AMD GPU Settings"
        $null = Invoke-TieredStep -Tier 2 -Title "Review AMD Radeon Software settings for CS2" `
            -Why "Presents a manual checklist for Boost, Chill, Fluid Motion Frames, Anti-Lag, filtering, synchronization, and tuning controls." `
            -Evidence "Feature availability and behavior depend on the GPU, driver, game build, and current AMD software. This repository includes no validated AMD compatibility or latency matrix." `
            -Caveat "Verify current AMD and game documentation, including anti-cheat compatibility, before enabling driver features. This workflow does not change firmware or AMD settings automatically." `
            -Risk "SAFE" -Depth "CHECK" `
            -Improvement "Provides a reviewable manual AMD settings checklist" `
            -SideEffects "Manual Adrenalin settings - no system changes made here." `
            -Undo "N/A (manual AMD Adrenalin settings)" `
            -Action {
                Write-Blank
                Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
                Write-Host "  │  AMD RADEON SOFTWARE - REPOSITORY REVIEW CHECKLIST           │" -ForegroundColor Red
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  Gaming -> Graphics (Global Settings):                      │" -ForegroundColor White
                Write-Host "  │  - Anti-Lag: compare supported states and verify compatibility│" -ForegroundColor Green
                Write-Host "  │  - Radeon Boost: repository default Off                      │" -ForegroundColor White
                Write-Host "  │  - Radeon Chill: repository default Off                      │" -ForegroundColor White
                Write-Host "  │  - Fluid Motion Frames: repository default Off               │" -ForegroundColor White
                Write-Host "  │  - Image Sharpening: repository default Off                  │" -ForegroundColor DarkGray
                Write-Host "  │  - Texture Filtering Quality: repository default Performance │" -ForegroundColor White
                Write-Host "  │  - Wait for Vertical Refresh: repository default Always Off  │" -ForegroundColor White
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  Performance -> Tuning:                                     │" -ForegroundColor White
                Write-Host "  │  - GPU Tuning: repository default Standard                   │" -ForegroundColor White
                Write-Host "  │  - VRAM Tuning: repository default Standard                  │" -ForegroundColor White
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  Driver and anti-cheat compatibility can change.            │" -ForegroundColor Yellow
                Write-Host "  │  Consult current vendor and game documentation first.       │" -ForegroundColor DarkYellow
                Write-Host "  │  This workflow does not recommend changing TPM firmware.    │" -ForegroundColor DarkYellow
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  DRIVER INSTALL:                                             │" -ForegroundColor White
                Write-Host "  │  amd.com/support -> Download driver                         │" -ForegroundColor White
                Write-Host "  │  -> Select a package for the exact GPU and Windows build    │" -ForegroundColor White
                Write-Host "  │  -> Follow the current AMD installer guidance               │" -ForegroundColor White
                Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
                if (Test-YoloProfile) {
                    Write-Warn "Manual AMD Adrenalin settings are required. YOLO mode cannot verify them, so this step is recorded as skipped."
                    Write-Info "Apply the listed settings, then rerun Phase 3 interactively to acknowledge completion."
                    Skip-Step $PHASE 8 "AMDSettings (manual action required)"
                } elseif ($SCRIPT:DryRun) {
                    Write-Info "DRY-RUN: AMD Adrenalin guidance previewed; a live run would still require manual configuration."
                } else {
                    Read-Host "  [Enter] when done"
                    Complete-Step $PHASE 8 "AMDSettings"
                }
            } `
            -SkipAction { Skip-Step $PHASE 8 "AMDSettings" }
    } else {
        Write-Debug "Step 8 - AMD GPU Settings skipped (not AMD)."
        Skip-Step $PHASE 8 "AMDSettings (not AMD)"
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 9 - DNS SERVER CONFIGURATION  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 9) {
    Write-Section "Step 9 - DNS Server Configuration"
    $null = Invoke-TieredStep -Tier 3 -Title "Switch DNS server to Cloudflare or Google" `
        -Why "Changes the active adapter from its current DNS configuration to a selected public resolver." `
        -Evidence "The resolver addresses and DNS query behavior are verifiable. This repository includes no location-independent resolver benchmark." `
        -Caveat "Corporate networks often need custom DNS. Only for private internet connections." `
        -Risk "SAFE" -Depth "NETWORK" `
        -Improvement "Applies the selected DNS resolver addresses" `
        -SideEffects "May not work on corporate/managed networks. ISP DNS features lost." `
        -Undo "Set DNS back to automatic: Set-DnsClientServerAddress -ResetServerAddresses" `
        -Action {
            Write-Blank
            Write-Host "  Choose DNS server:" -ForegroundColor White
            Write-Host "  [1]  Cloudflare  1.1.1.1 / 1.0.0.1" -ForegroundColor Cyan
            Write-Host "  [2]  Google      8.8.8.8 / 8.8.4.4" -ForegroundColor White
            Write-Host "  [3]  Skip" -ForegroundColor DarkGray
            if (Test-YoloProfile) { $dnsChoice = "1" }
            elseif (-not $SCRIPT:DryRun) {
                do { $dnsChoice = Read-Host "  [1/2/3]" } while ($dnsChoice -notin @("1","2","3"))
            } else { $dnsChoice = "1" }

            if ($dnsChoice -ne "3") {
                $dnsAddrs = if ($dnsChoice -eq "1") { $CFG_DNS_Cloudflare } else { $CFG_DNS_Google }
                $dnsName  = if ($dnsChoice -eq "1") { "Cloudflare" } else { "Google" }
                try {
                    # Set DNS on active physical adapters (wired + WiFi) - DNS is protocol-layer,
                    # unlike NIC hardware tweaks which are wired-only.
                    $nics = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object {
                        $_.Status -eq "Up" -and
                        $_.InterfaceDescription -notmatch $CFG_VirtualAdapterFilter
                    })
                    if ($SCRIPT:DryRun -and $nics.Count -eq 0) {
                        $nics = @([PSCustomObject]@{
                            Name = "Preview Ethernet"; InterfaceDescription = "Simulated physical adapter"
                            ifIndex = 1; InterfaceIndex = 1; Status = "Up"
                        })
                        Write-Info "DRY-RUN: using a synthetic adapter so the DNS operation plan is fully exercised."
                    }
                    if ($nics.Count -gt 0) {
                        # Show numbered list so user can identify each adapter
                        Write-Blank
                        Write-Host "  Detected active adapters:" -ForegroundColor White
                        for ($i = 0; $i -lt $nics.Count; $i++) {
                            Write-Host "    [$($i+1)]  $($nics[$i].Name)  ($($nics[$i].InterfaceDescription))" -ForegroundColor Cyan
                        }
                        Write-Blank

                        if ($nics.Count -eq 1) {
                            $confirmDns = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "Y" } else { Read-Host "  Apply $dnsName DNS to this adapter? [Y/n]" }
                            if ($confirmDns -match "^[nN]$") {
                                Write-Info "DNS not changed. Configure manually in Network Settings."
                                Complete-Step $PHASE 9 "DNS"
                                return
                            }
                            $selectedNics = $nics
                        } else {
                            Write-Host "  [A]  Apply to ALL listed adapters" -ForegroundColor White
                            Write-Host "  [S]  Select individual adapters" -ForegroundColor White
                            Write-Host "  [N]  Skip - configure DNS manually" -ForegroundColor DarkGray
                            if (Test-YoloProfile) { $multiChoice = "a" }
                            elseif (-not $SCRIPT:DryRun) {
                                do { $multiChoice = Read-Host "  [A/S/N]" } while ($multiChoice -notmatch "^[aAsSnN]$")
                            } else { $multiChoice = "a" }
                            if ($multiChoice -match "^[nN]$") {
                                Write-Info "DNS not changed. Configure manually in Network Settings."
                                Complete-Step $PHASE 9 "DNS"
                                return
                            }
                            if ($multiChoice -match "^[sS]$") {
                                $selectedNics = @()
                                for ($i = 0; $i -lt $nics.Count; $i++) {
                                    $pick = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "y" } else { Read-Host "  Apply DNS to [$($i+1)] $($nics[$i].Name)? [y/N]" }
                                    if ($pick -match "^[jJyY]$") { $selectedNics += $nics[$i] }
                                }
                                if ($selectedNics.Count -eq 0) {
                                    Write-Info "No adapters selected. DNS not changed."
                                    Complete-Step $PHASE 9 "DNS"
                                    return
                                }
                            } else {
                                $selectedNics = $nics
                            }
                        }

                        foreach ($nic in $selectedNics) {
                            $nicIndex = if ($nic.PSObject.Properties['ifIndex']) { [int]$nic.ifIndex } else { [int]$nic.InterfaceIndex }
                            if ($SCRIPT:DryRun) {
                                Write-Host "  [DRY-RUN] Would set DNS to ${dnsName}: $($dnsAddrs -join ', ') (Adapter: $($nic.Name))" -ForegroundColor Magenta
                            } else {
                                # Backup current DNS servers before modification
                                $currentDns = @()
                                try {
                                    $dnsInfo = Get-DnsClientServerAddress -InterfaceIndex $nicIndex `
                                        -AddressFamily IPv4 -ErrorAction Stop
                                    if ($dnsInfo -and $dnsInfo.ServerAddresses) {
                                        $currentDns = @($dnsInfo.ServerAddresses)
                                    }
                                } catch {
                                    throw "Could not read current DNS for $($nic.Name): $_"
                                }
                                Set-VerifiedDnsProfileForAdapter `
                                    -AdapterName $nic.Name `
                                    -InterfaceIndex $nicIndex `
                                    -Provider $dnsName `
                                    -CurrentServers $currentDns `
                                    -BackupStep $SCRIPT:CurrentStepTitle | Out-Null
                                Write-OK "DNS set to ${dnsName}: $($dnsAddrs -join ', ') (Adapter: $($nic.Name))"
                            }
                        }
                    } else {
                        Write-Warn "No active network adapter found after filtering virtual/VPN adapters."
                        Write-Info "Check your network cable or WiFi. DNS can be set manually in Network Settings."
                        throw "No active network adapter found for DNS changes."
                    }
                } catch {
                    Write-Warn "DNS change failed: $_"
                    throw
                }
            } else {
                Write-Info "DNS not changed."
            }
            Complete-Step $PHASE 9 "DNS"
        } `
        -SkipAction { Skip-Step $PHASE 9 "DNS" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 10 - PROCESS PRIORITY / X3D TOPOLOGY  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 10) {
    Write-Section "Step 10 - Process Priority / X3D Topology (native IFEO)"
    $null = Invoke-TieredStep -Tier 3 -Title "Set persistent CS2 process priority (native IFEO)" `
        -Why "High CPU priority gives CS2 scheduler preference. Dual-CCD X3D systems receive manual topology guidance; aggregate WMI counts are never treated as an affinity map." `
        -Evidence "T3: No isolated benchmark for priority class. Automatic CCD pinning is disabled until Windows exposes an authoritative logical-processor-to-CCD map." `
        -Caveat "High priority can reduce resources available to background work. The suite does not use Realtime priority." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "CPU priority for CS2 - persistent via IFEO (no background service needed)" `
        -SideEffects "Background tasks can receive less CPU time while CS2 runs. The registry value is reversible." `
        -Undo "Remove the cs2.exe IFEO PerfOptions key" `
        -Action {
            $priorityResult = Set-CS2ProcessPriority
            if (-not $priorityResult) {
                throw "Process-priority configuration did not return a result."
            }
            if ($priorityResult.Status -eq "DryRun") {
                Write-Info "DRY-RUN: process-priority registry plan previewed; no state was changed."
            } elseif ($priorityResult.Status -eq "Skipped") {
                Skip-Step $PHASE 10 "ProcessPriority"
            } elseif (-not $priorityResult.CanCompleteStep) {
                throw "Process-priority configuration did not complete: $($priorityResult.Message)"
            } else {
                Complete-Step $PHASE 10 "ProcessPriority"
            }
        } `
        -SkipAction { Skip-Step $PHASE 10 "ProcessPriority" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 11 - VRAM USAGE REVIEW  [Info]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 11) {
    Write-Section "Step 11 - VRAM Usage Review"
    Write-TierBadge 2 "CS2 VRAM usage observation"
    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
    Write-Host "  │  VRAM USAGE OBSERVATION:                                    │" -ForegroundColor Yellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  If frame-time behavior changes during a long session,     │" -ForegroundColor White
    Write-Host "  │  record GPU memory use with the same map, settings, and    │" -ForegroundColor White
    Write-Host "  │  workload before drawing a conclusion.                     │" -ForegroundColor White
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  Task Manager or a GPU telemetry tool can report memory    │" -ForegroundColor White
    Write-Host "  │  allocation. High allocation alone does not establish a   │" -ForegroundColor DarkGray
    Write-Host "  │  leak or identify the cause of a frame-time change.        │" -ForegroundColor DarkGray
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  Restart CS2 if observed behavior degrades, then compare   │" -ForegroundColor White
    Write-Host "  │  another session under the same conditions.                │" -ForegroundColor White
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
    Write-Blank
    Complete-Step $PHASE 11 "VRAMLeak"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 12 - FINAL CHECKLIST
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 12) {
    Write-Section "Step 12 - Final Checklist"

    if ($SCRIPT:DryRun) {
        Write-Host @"

  PLAN PREVIEWED - common workflow:
  - Shader-cache and fullscreen-optimization operations
  - frametime.cfg power-plan operations
  - MSI, timer, scheduler, input, Game DVR, and overlay plans
  - Video, audio, autoexec, chipset, and benchmark guidance
  - Driver removal and replacement workflow for the selected vendor
"@ -ForegroundColor Magenta

        switch ($gpuInput) {
            { $_ -in @("1", "2") } {
            Write-Host "  - NVIDIA install, Control Panel, and 42-setting DRS plans" -ForegroundColor Magenta
            }
            "3" {
                Write-Host "  - AMD cleanup plus manual driver and Adrenalin guidance" -ForegroundColor Magenta
            }
            "4" {
                Write-Host "  - Intel cleanup plus manual driver and Arc guidance" -ForegroundColor Magenta
            }
        }
        Write-Host "`n  Nothing in this checklist was applied or marked complete." -ForegroundColor Magenta
    } else {
    Write-Host @"

  WORKFLOW AREAS PROCESSED:
  - Shader cache and fullscreen-optimization operations
  - frametime.cfg power-plan operations
  - Driver removal and replacement workflow for the selected vendor
  - GPU MSI, FPS-cap, video-setting, and memory checks
  - Review the log and verification output for per-step results
"@ -ForegroundColor Green

    Write-Host @"
  SETUP-DEPENDENT AREAS PROCESSED:
  - XMP/EXPO and Resizable BAR checks
  - Network, scheduler, timer, input, recording, and overlay settings
  - Audio, autoexec.cfg, and chipset guidance
  - Results vary by hardware, software, and selected profile
"@ -ForegroundColor Yellow

    Write-Host @"
  STILL MANUAL:
  - Run a repeatable benchmark at least three times
  - Calculate FPS cap manually from a repeatable benchmark result
  - Verify the Reflex decision with CapFrameX:
     Test A (-noreflex + NVCP Ultra) vs. B (Reflex ON in-game)
     Choose what gives better lows AND better feel.
  - Check settings after Windows Update with an authenticated verifier release
"@ -ForegroundColor Cyan
        if ($gpuInput -in @("1", "2")) {
            Write-Host "  - The NVIDIA profile step uses 42 repository-defined DRS settings" -ForegroundColor Cyan
        }
    }

    # ── X3D / Hardware Validation Checks ─────────────────────────────────
    # Box width: 66 chars total. Inner: "  │  " (6) + content (up to 58) + " " + "│" (1) = 66
    # Helper: pad content to 58 chars inside the box
    $amdCpu = Get-AmdCpuInfo
    if ($amdCpu -and $amdCpu.IsX3D) {
        Write-Blank
        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
        $hdr = "$($amdCpu.CpuName) - POST-TUNING VALIDATION"
        Write-Host "  │  $($hdr.PadRight(58))│" -ForegroundColor Cyan
        Write-Host "  │                                                              │" -ForegroundColor DarkGray

        # CPU clock info (informational - MaxClockSpeed is base clock, not boost)
        if ($amdCpu.MaxClockSpeed -gt 0) {
            $msg = "$([char]0x2139)  CPU: $($amdCpu.CpuName) - base clock: $($amdCpu.MaxClockSpeed) MHz"
            Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Cyan
            $msg2 = "   (boost clock requires HWiNFO to verify)"
            Write-Host "  │  $($msg2.PadRight(58))│" -ForegroundColor DarkGray
        }

        # WHEA error check
        $whea = Test-WheaErrors
        if ($whea) {
            if ($whea.HasErrors) {
                $msg = "$([char]0x2718)  WHEA errors: $($whea.RecentCount) in last 24h"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Red
                Write-Host "  │     Review event details and recent hardware changes.      │" -ForegroundColor Red
                Write-Host "  │     Restore known defaults before isolating one setting.   │" -ForegroundColor Red
            } else {
                $totalLabel = if ($whea.Count -gt 0) { "$($whea.Count) total (none recent)" } else { "0 recent" }
                $msg = "$([char]0x2714)  WHEA errors: $totalLabel"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Green
            }
        }

        # Reported DDR5 configured/rated data rate. This inventory does not
        # measure FCLK or UCLK and cannot verify a clock ratio.
        $ddr5 = Get-Ddr5TimingInfo
        if ($ddr5 -and $ddr5.IsDDR5) {
            $mts  = $ddr5.ActiveMTs
            $ratedMts = if ($ddr5.PSObject.Properties['RatedMTs']) { "$($ddr5.RatedMTs) MT/s" } else { "not reported" }
            $isDownclocked = if ($ddr5.PSObject.Properties['IsDownclocked']) { [bool]$ddr5.IsDownclocked } else { $false }
            $msg = "$([char]0x25CB)  DDR5 active: $mts MT/s; rated: $ratedMts"
            Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor DarkGray
            if ($isDownclocked) {
                $msg = "$([char]0x2139)  Reported active speed is below rated speed"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor DarkGray
                Write-Host "  │     Review the validated memory profile and stability.     │" -ForegroundColor DarkGray
            }
        }

        Write-Host "  │                                                              │" -ForegroundColor DarkGray
        $msg = "Verify board docs; run repeatable stress and thermal tests."
        Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor DarkGray
        $msg = "Record memory timings and test conditions before changes."
        Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor DarkGray
        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
    }

    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor DarkYellow
    Write-Host "  │  WHAT THIS SCRIPT CANNOT FIX                                │" -ForegroundColor DarkYellow
    Write-Host "  │                                                              │" -ForegroundColor DarkYellow
    Write-Host "  │  The workflow cannot change the game engine, firmware,      │" -ForegroundColor White
    Write-Host "  │  hardware limits, driver behavior, or network path.         │" -ForegroundColor White
    Write-Host "  │  Results must be measured on the target system.             │" -ForegroundColor White
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor DarkYellow

    Write-Blank
    Write-Host "  NEXT STEPS - IF PROBLEMS PERSIST:" -ForegroundColor White
    Write-Host @"
  1.  Run LatencyMon  (resplendence.com/latencymon)
      -> Shows which drivers cause DPC spikes
      -> Then decide whether NIC and interrupt-policy changes are useful

  2.  Log HWiNFO64 during a match
      -> Log CPU/GPU package temp + clock + power
      -> Compare clock behavior with the recorded thermal and power data

  3.  Rule out thermal problems
      -> Compare recorded temperatures with vendor limits for the exact model
      -> Check whether clocks fall under sustained load

  4.  RAM sub-timings (only if XMP or EXPO is active and stable)
      -> Change one value at a time and run a memory stability test
      -> Firmware, memory IC, and memory-controller limits vary

  5.  Hardware changes as a last resort
      -> Identify the limiting component from captured frame-time data
      -> Do not infer an upgrade requirement from tier labels alone

  Use comparable before/after captures with the same capture tool.
  Without comparable captures, a performance change is not established.
"@ -ForegroundColor DarkGray

    if ($SCRIPT:DryRun) {
        Write-Info "Preview output: console only; no work directory or log was created."
    } else {
        Write-Info "Log: $CFG_LogFile  |  Tools: $CFG_WorkDir"
    }
    Complete-Step $PHASE 12 "Checklist"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 13 - FINAL BENCHMARK + FPS CAP  [T1, LAST STEP]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 13) {
    if ($SCRIPT:DryRun) {
        Write-Section "Step 13 - Benchmark Workflow Preview  [LAST STEP]"
        Write-TierBadge 1 "Benchmark capture and FPS-cap workflow preview"
    } else {
        Write-Section "Step 13 - Final Benchmark + FPS Cap Calculation  [LAST STEP]"
        Write-TierBadge 1 "Post-optimization benchmark comparison"
    }
    Write-Blank

    Write-Host @"
  FPSHEAVEN BENCHMARK MAPS  (by @fREQUENCYcs)
  ─────────────────────────────────────────────
  Outputs at end in console window:
    [VProf] FPS: Avg=XXX.X, P1=XXX.X

  Map 1 - DUST2:
  $CFG_Benchmark_Dust2

  Map 2 - INFERNO:
  $CFG_Benchmark_Inferno

  Map 3 - ANCIENT  (water interaction):
  $CFG_Benchmark_Ancient

  WORKFLOW:
  1.  Steam -> Workshop -> Subscribe to map
  2.  CS2: Play -> Workshop Maps -> start
  3.  Runs 2-3 minutes automatically - don't touch PC
  4.  Console opens automatically -> copy [VProf] line
  5.  Paste output below for automatic tracking + FPS cap

  FOR COMPARISON: Run at least 3 times and record the average.

  WHY AN FPS CAP?
  ────────────────
  Compare capped and uncapped captures with the same workload.
  Repository formula: average FPS minus 9%.
"@ -ForegroundColor DarkGray

    if ($fpsCap -gt 0) {
        Write-Host "  Already calculated cap: $fpsCap  (avg $avgFps - 9%)" -ForegroundColor Green
    }

    # Use the iterative benchmark tracking system
    $bmResult = Invoke-BenchmarkCapture -Label "After all optimizations"

    if ($bmResult) {
        Write-Blank
        Write-Info "Set FPS cap in: NVIDIA CP -> CS2 -> Max Frame Rate -> $($bmResult.Cap)"
        Write-Info "Repeat this benchmark from an authenticated management release."
        Write-Info "All results are tracked in: $CFG_BenchmarkFile"
        Complete-Step $PHASE 13 "FinalBenchmark"
    } elseif (-not $SCRIPT:DryRun) {
        Write-Warn "Final benchmark step remains incomplete until a result is captured and saved."
    }

    # Show full history if multiple results exist
    $history = @(Get-BenchmarkHistory)
    if ($history.Count -ge 2) {
        Write-Blank
        Show-BenchmarkComparison
    }

}

Write-Blank
if ($SCRIPT:DryRun) {
    Write-PhaseSummary -PhaseLabel "PHASE 3" -DryRun -ContinuePreview
} else {
    if (-not (Test-StepCompleted $PHASE 1)) {
        Write-Warn "Phase 3 remains incomplete; the automatic handoff is retained until the required driver installation is completed."
        Write-PhaseSummary -PhaseLabel "PHASE 3 INCOMPLETE" -NextAction "Run Phase 3 again and complete Step 1 (Driver)."
    } elseif (-not (Test-StepCompleted $PHASE 13)) {
        Write-Warn "Phase 3 remains incomplete; the automatic handoff is retained until the final benchmark is saved."
        Write-PhaseSummary -PhaseLabel "PHASE 3 INCOMPLETE" -NextAction "Run Phase 3 again to capture the final benchmark."
    } else {
        $handoffRemoval = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru
        if (-not $handoffRemoval.Applied) {
            Write-Err "Phase 3 completed, but its automatic handoff could not be removed: $($handoffRemoval.Message)"
            Write-PhaseSummary -PhaseLabel "PHASE 3 HANDOFF PENDING" -NextAction "Remove the Phase 3 handoff before restarting."
        } else {
            Write-PhaseSummary -PhaseLabel "ALL 3 PHASES" -NextAction "Good luck, have fun! GG"
            $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Final restart recommended (MSI changes). Now? [y/N]" }
            if ($r -match "^[jJyY]$") {
                Restart-Computer -Force
            }
        }
    }
}
} catch {
    if ($null -ne $PreviewState) {
        throw
    }
    Write-Host "" -ForegroundColor Red
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "  ║  FATAL ERROR - Phase 3 crashed unexpectedly                ║" -ForegroundColor Red
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host "  Error:      $_" -ForegroundColor Red
    Write-Host "  Location:   $($_.InvocationInfo.ScriptName):$($_.InvocationInfo.ScriptLineNumber)" -ForegroundColor Yellow
    Write-Host "  Line:       $($_.InvocationInfo.Line.Trim())" -ForegroundColor DarkGray
    if ($_.ScriptStackTrace) {
        Write-Host "  Stack trace:" -ForegroundColor Yellow
        $_.ScriptStackTrace -split "`n" | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
    }
    Write-Host ""
    if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  Press [Enter] to exit (error details above)" }
} finally {
    # Release only the lock acquired by this invocation. Initialize-Backup can
    # reject an active lock owned by another process before acquiring one.
    if ($backupInitialized) { Remove-BackupLock }
}
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-PostRebootSetupEntryPoint -SmokeTest:$SmokeTest
}
