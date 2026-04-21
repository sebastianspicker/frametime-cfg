# ==============================================================================
#  helpers/nvidia-driver.ps1  —  NVIDIA Driver Download + Clean Install
# ==============================================================================

function Get-LatestNvidiaDriver {
    <#
    .SYNOPSIS  Queries NVIDIA's driver lookup API to find the latest driver
               for the detected GPU. Returns download URL and version info.
    #>
    [Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', 'SpecificVersion',
        Justification = 'Reserved for future version-specific download support')]
    param(
        [string]$SpecificVersion,  # If set, tries to find this version instead of latest
        [string]$GpuName           # Fallback GPU name (from Phase 1 state) when driver is uninstalled
    )

    Write-Step "Detecting NVIDIA GPU for driver lookup..."

    $gpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match "NVIDIA" } | Select-Object -First 1

    if (-not $gpu -and $GpuName) {
        Write-Info "GPU not detected via CIM (driver uninstalled). Using saved GPU name: $GpuName"
    } elseif (-not $gpu) {
        Write-Warn "No NVIDIA GPU detected."
        return $null
    }

    $gpuName = if ($gpu) { $gpu.Name } else { $GpuName }
    Write-Info "GPU: $gpuName"

    # NVIDIA driver lookup API
    # psid = Product Series ID, pfid = Product Family ID
    # osid = 57 (Windows 10/11 64-bit), lid = 1 (English), whql = 1

    # Map common GPU series to NVIDIA product series/family IDs
    # [ordered] ensures deterministic match order (longest/newest series first)
    # Laptop entries MUST come before their desktop counterparts — [ordered]
    # iterates in insertion order and we break on first match. Laptop GPUs
    # contain "Laptop" in the name (e.g., "NVIDIA GeForce RTX 4060 Laptop GPU")
    # and need different psid/pfid values for NVIDIA's driver lookup API.
    $seriesMap = [ordered]@{
        "RTX 50.*Laptop" = @{ psid = 131; pfid = 1010 }  # GeForce RTX 50 Series (Laptops)
        "RTX 40.*Laptop" = @{ psid = 130; pfid = 957 }   # GeForce RTX 40 Series (Laptops)
        "RTX 30.*Laptop" = @{ psid = 118; pfid = 957 }   # GeForce RTX 30 Series (Laptops)
        "RTX 50"  = @{ psid = 129; pfid = 1010 }   # GeForce RTX 50 Series
        "RTX 40"  = @{ psid = 128; pfid = 993 }     # GeForce RTX 40 Series
        "RTX 30"  = @{ psid = 127; pfid = 945 }     # GeForce RTX 30 Series
        "RTX 20"  = @{ psid = 126; pfid = 903 }     # GeForce RTX 20 Series
        "GTX 16"  = @{ psid = 125; pfid = 904 }     # GeForce GTX 16 Series
        "GTX 10"  = @{ psid = 101; pfid = 816 }     # GeForce GTX 10 Series
    }

    $matchedSeries = $null
    foreach ($key in $seriesMap.Keys) {
        if ($gpuName -match $key) {
            $matchedSeries = $seriesMap[$key]
            break
        }
    }

    if (-not $matchedSeries) {
        # Default to latest GeForce driver page
        Write-Warn "GPU series not auto-detected. Using manual download."
        Write-Info "Download from: https://www.nvidia.com/en-us/drivers/"
        return @{
            ManualDownload = $true
            Url = "https://www.nvidia.com/en-us/drivers/"
            GpuName = $gpuName
        }
    }

    $lookupUrl = "https://www.nvidia.com/Download/processFind.aspx?" +
                 "psid=$($matchedSeries.psid)&pfid=$($matchedSeries.pfid)" +
                 "&osid=57&lid=1&whql=1&dtcid=1"

    $oldPP = $global:ProgressPreference
    try {
        Write-Step "Querying NVIDIA driver API..."
        $global:ProgressPreference = 'SilentlyContinue'
        $response = Invoke-WebRequest -Uri $lookupUrl -UseBasicParsing -TimeoutSec 30

        # Parse the response for download link and version
        $content = $response.Content
        $downloadUrl = $null
        if ($content -match "downloadURL\s*=\s*'([^']+)'") {
            $downloadUrl = $Matches[1]
        } elseif ($content -match '(https://[^"''<>\s]+\.exe)') {
            $downloadUrl = $Matches[1]
        }

        $version = $null
        if ($content -match "Version:\s*([\d.]+)") {
            $version = $Matches[1]
        }

        if ($downloadUrl) {
            # Ensure full URL — SECURITY: force HTTPS to prevent MITM during driver download.
            # NVIDIA serves all driver downloads over HTTPS; if API response contains http://,
            # upgrade it. Reject non-NVIDIA domains to prevent redirection attacks.
            if ($downloadUrl -notmatch "^https?://") {
                $downloadUrl = "https://www.nvidia.com$downloadUrl"
            }
            # Upgrade http:// to https://
            if ($downloadUrl -match "^http://") {
                $downloadUrl = $downloadUrl -replace "^http://", "https://"
                Write-DebugLog "Upgraded driver URL to HTTPS"
            }
            # Validate the download domain is NVIDIA
            if ($downloadUrl -notmatch '^https://([\w.-]+\.)?nvidia\.com/') {
                Write-Warn "Driver download URL is not from nvidia.com: $downloadUrl"
                Write-Warn "Falling back to manual download for safety."
                return @{ ManualDownload = $true; Url = "https://www.nvidia.com/en-us/drivers/"; GpuName = $gpuName }
            }
            if (-not $version) { Write-Warn "Could not parse driver version from API response." }
            Write-OK "Found driver: Version $(if ($version) { $version } else { '(unknown)' })"
            Write-Info "URL: $downloadUrl"
            return @{
                ManualDownload = $false
                Url = $downloadUrl
                Version = $version
                GpuName = $gpuName
            }
        }
    } catch {
        Write-DebugLog "NVIDIA API lookup failed: $_"
    } finally {
        $global:ProgressPreference = $oldPP
    }

    # Fallback to manual download
    Write-Warn "Auto-detection failed. Use manual download."
    Write-Info "Download from: https://www.nvidia.com/en-us/drivers/"
    return @{
        ManualDownload = $true
        Url = "https://www.nvidia.com/en-us/drivers/"
        GpuName = $gpuName
    }
}

function Test-NvidiaDriverSignature {
    <#
    .SYNOPSIS  Validates the Authenticode signature on a downloaded NVIDIA driver .exe.
    .DESCRIPTION
        SECURITY (S1): Defense-in-depth check after Invoke-Download. NVIDIA drivers are
        always Authenticode-signed. An invalid or non-NVIDIA signature indicates a
        tampered binary (CDN compromise or MitM with cert injection). Failing this
        check deletes the file and returns $false.

        This is called immediately after download. Install-NvidiaDriverClean has its
        own interactive signature check as a second gate before execution.
    .PARAMETER FilePath
        Path to the downloaded driver .exe file.
    .OUTPUTS
        [bool] $true if signature is valid and from NVIDIA, $false otherwise.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$FilePath
    )

    if (-not (Test-Path $FilePath)) {
        Write-Warn "Driver file not found for signature check: $FilePath"
        return $false
    }

    $sig = Get-AuthenticodeSignature -FilePath $FilePath -ErrorAction SilentlyContinue
    if (-not $sig -or $sig.Status -ne 'Valid') {
        Write-Err "Driver signature invalid (status: $(if($sig){$sig.Status}else{'N/A'})). Removing file."
        Remove-Item $FilePath -Force -ErrorAction SilentlyContinue
        return $false
    }
    if ($sig.SignerCertificate.Subject -notmatch 'NVIDIA') {
        Write-Err "Driver not signed by NVIDIA (signer: $($sig.SignerCertificate.Subject)). Removing file."
        Remove-Item $FilePath -Force -ErrorAction SilentlyContinue
        return $false
    }

    Write-OK "Authenticode signature valid: $($sig.SignerCertificate.Subject)"
    return $true
}

function Install-NvidiaDriverClean {
    <#
    .SYNOPSIS  Installs NVIDIA driver with only essential components (no NVIDIA App).
               Uses extract → strip → setup.exe approach for a minimal install.
    .DESCRIPTION
        Strategy:
          1. Extract the self-extracting .exe to a temp folder (-s -e"<path>")
          2. Delete unwanted component folders (NVIDIA App, GFE, telemetry, NodeJS, etc.)
          3. Run setup.exe from the extracted folder with -s -noreboot -clean
        This avoids installing NVIDIA App / GeForce Experience entirely, rather than
        installing everything and trying to clean up after the fact.
        NVDisplay.ContainerLocalSystem is intentionally kept — it's required for the
        NVIDIA Control Panel to function.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$DriverExe
    )

    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would install NVIDIA driver (component-selective): $DriverExe" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   1. Extract driver package to temp folder" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   2. Remove bloat components (NVIDIA App, GFE, telemetry, NodeJS)" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   3. Run setup.exe -s -noreboot -clean (driver-only)" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   4. Disable telemetry services + scheduled tasks" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   5. Apply post-install registry tweaks" -ForegroundColor Magenta
        return $true
    }

    if (-not (Test-Path $DriverExe)) {
        Write-Err "Driver file not found: $DriverExe"
        return $false
    }

    # SECURITY: Validate driver path — this file is passed to Start-Process.
    # state.json nvidiaDriverPath or user input could point to malware.
    # Verify: must be a real .exe file (not directory/symlink to non-file), no path traversal.
    # Check for path traversal BEFORE resolving — after Get-Item, '..' is normalized away
    if ($DriverExe -match '\.\.') {
        Write-Err "Driver path contains path traversal: $DriverExe"
        return $false
    }
    $driverItem = Get-Item $DriverExe -ErrorAction SilentlyContinue
    if (-not $driverItem -or $driverItem.PSIsContainer) {
        Write-Err "Driver path is not a file: $DriverExe"
        return $false
    }
    # Verify file has Authenticode signature (NVIDIA drivers are always signed)
    $sig = Get-AuthenticodeSignature $DriverExe -ErrorAction SilentlyContinue
    if ($sig -and $sig.Status -eq 'Valid') {
        $sigSubject = $sig.SignerCertificate.Subject
        if ($sigSubject -notmatch 'NVIDIA') {
            Write-Warn "Driver .exe is signed but NOT by NVIDIA (signer: $sigSubject)"
            Write-Warn "This may not be a genuine NVIDIA driver. Proceed with caution."
            $sigConfirm = Read-Host "  Continue anyway? [y/N]"
            if ($sigConfirm -notmatch '^[jJyY]$') { return $false }
        } else {
            Write-DebugLog "Driver Authenticode signature valid: $sigSubject"
        }
    } else {
        Write-Warn "Driver .exe has no valid Authenticode signature (status: $(if($sig){$sig.Status}else{'N/A'}))"
        Write-Warn "NVIDIA drivers are always code-signed. This file may be tampered."
        $sigConfirm = Read-Host "  Continue anyway? [y/N]"
        if ($sigConfirm -notmatch '^[jJyY]$') { return $false }
    }

    # ── 1. Extract driver package ───────────────────────────────────────────
    # The NVIDIA .exe is a self-extracting archive. Use NVIDIA's native silent
    # extraction flags: -s (silent) + -e"<path>" (extract only, no install).
    # This replaces the legacy -x -gm2 -InstallDir approach which still spawns
    # a GUI dialog on modern driver packages.
    # NOTE: -e has NO space before the path — it's -e"C:\path", not -e "C:\path".
    # IMPORTANT: Pass the full argument line as a single string, NOT an array.
    # PowerShell's Start-Process can double-quote array elements, mangling the
    # -e"path" flag and causing the extractor to fall back to a full silent install
    # (which installs NVIDIA App, Control Panel, and other bloat).
    $extractDir = Join-Path $env:TEMP "NVDriverExtract_$(Get-Date -Format 'yyyyMMdd_HHmmss')"
    New-Item -ItemType Directory -Path $extractDir -Force -ErrorAction SilentlyContinue | Out-Null
    Write-Step "Extracting driver package (silent)..."
    Write-Info "Extracting to: $extractDir"

    $extractProcess = Start-Process -FilePath $DriverExe `
        -ArgumentList "-s -e`"$extractDir`"" `
        -PassThru

    # Wait up to 5 minutes for extraction — prevents indefinite hangs
    $extractTimeout = 300000  # 5 minutes in ms
    $completed = $extractProcess.WaitForExit($extractTimeout)
    if (-not $completed) {
        Write-Err "Extraction timed out after 5 minutes."
        try { $extractProcess | Stop-Process -Force -ErrorAction SilentlyContinue } catch {}
        return $false
    }

    # Find setup.exe — may be at root or in a subdirectory depending on driver version
    $setupExe = $null
    $packageRoot = $extractDir
    if (Test-Path "$extractDir\setup.exe") {
        $setupExe = "$extractDir\setup.exe"
    } else {
        # Some driver packages extract into a nested folder
        $found = Get-ChildItem $extractDir -Recurse -Filter "setup.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($found) {
            $setupExe = $found.FullName
            $packageRoot = $found.DirectoryName
            Write-Info "Found setup.exe in subdirectory: $($found.DirectoryName | Split-Path -Leaf)"
        }
    }

    $fullInstallDetected = $false
    if (-not $setupExe) {
        # The self-extractor may have performed a full install instead of extract-only.
        # This happens when argument quoting is misinterpreted by the extractor.
        # Detect by checking if NVIDIA driver appeared in WMI after extraction attempt.
        $postGpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match "NVIDIA" } | Select-Object -First 1
        if ($postGpu -and $postGpu.DriverVersion) {
            Write-Warn "setup.exe not found, but NVIDIA driver is now detected."
            Write-Warn "The installer performed a full install instead of extract-only."
            Write-Info "Detected: $($postGpu.Name) — Driver $($postGpu.DriverVersion)"
            Write-Info "Applying post-install cleanup (removing bloat, disabling telemetry)..."
            $fullInstallDetected = $true
            $installSuccess = $true
            Remove-Item $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        } else {
            Write-Err "Extraction failed — setup.exe not found in $extractDir"
            Write-Info "Exit code: $($extractProcess.ExitCode)"
            if (Test-Path $extractDir) {
                $extractContents = Get-ChildItem $extractDir -ErrorAction SilentlyContinue | Select-Object -First 10
                if ($extractContents) {
                    Write-Info "Extraction folder contains: $($extractContents.Name -join ', ')"
                } else {
                    Write-Info "Extraction folder is empty."
                }
            }
            return $false
        }
    } else {
        Write-OK "Extraction complete."
    }

    if (-not $fullInstallDetected) {
        # ── 2. Strip bloat components ────────────────────────────────────────
        # Remove component folders so setup.exe never installs them.
        # NVDisplay.ContainerLocalSystem is NOT removed — it's required for NVCP.
        Write-Step "Removing unwanted components from extracted package..."
        $bloatFolders = @(
            "GFExperience*",           # GeForce Experience (legacy)
            "NvApp*",                  # NVIDIA App (GFE replacement)
            "NvBackend*",              # GFE/App backend service
            "NvTelemetry*",            # Telemetry container
            "NvContainer\plugins\LocalSystem\NvTelemetry*",  # Telemetry plugin
            "NvNodejs*",               # NodeJS runtime (used by GFE/App)
            "nodejs*",                 # NodeJS alt location
            "NvCamera*",               # ShadowPlay / Ansel
            "ShadowPlay*",             # ShadowPlay standalone
            "NvVAD*",                  # Virtual Audio Device (ShadowPlay)
            "EULA.txt",                # Not needed for silent install
            "ListDevices.txt",         # Not needed for silent install
            "license.txt"              # Not needed for silent install
        )
        $removedCount = 0
        foreach ($pattern in $bloatFolders) {
            $items = Get-ChildItem (Join-Path $packageRoot $pattern) -ErrorAction SilentlyContinue
            foreach ($item in $items) {
                Remove-Item $item.FullName -Recurse -Force -ErrorAction SilentlyContinue
                $removedCount++
                Write-DebugLog "Removed: $($item.Name)"
            }
        }
        Write-ActionOK "Removed $removedCount bloat components from package."

        # ── 3. Run setup.exe from stripped package ───────────────────────────
        Write-Step "Installing NVIDIA driver (driver-only, silent)..."
        Write-Info "This takes 3-7 minutes. Screen may flicker — do not touch the PC."

        $installProcess = Start-Process -FilePath $setupExe `
            -ArgumentList "-s -noreboot -clean" `
            -Wait -PassThru -NoNewWindow

        $installSuccess = $false
        if ($installProcess.ExitCode -eq 0) {
            Write-OK "NVIDIA driver installed successfully."
            $installSuccess = $true
        } elseif ($installProcess.ExitCode -eq 1) {
            Write-OK "NVIDIA driver installed (exit code 1 — reboot required)."
            $installSuccess = $true
        } else {
            Write-Warn "Installer exited with code $($installProcess.ExitCode)."
            Write-Info "This may still be OK — check Device Manager after restart."
            $installSuccess = $false
        }

        # Clean up extraction folder
        Remove-Item $extractDir -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        # ── Full install detected — remove bloat that was installed ──────────
        # The extractor ran a full install including NVIDIA App, GFE, etc.
        # Remove the bloat software while keeping the display driver intact.
        Write-Step "Removing NVIDIA bloat installed during full install..."

        # Remove NVIDIA AppX packages (NVIDIA App, Control Panel from Store)
        if (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue) {
            $nvAppx = Get-AppxPackage -AllUsers -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -match "NVIDIA" -and $_.Name -notmatch "ControlPanel" }
            foreach ($pkg in $nvAppx) {
                try {
                    Remove-AppxPackage -Package $pkg.PackageFullName -AllUsers -ErrorAction Stop
                    Write-OK "Removed AppX: $($pkg.Name)"
                } catch {
                    Write-DebugLog "AppX removal: $($pkg.Name) — $_"
                }
            }
        }

        # Remove bloat directories (keep NVDisplay.Container for NVCP + driver core)
        $bloatDirs = @(
            "$env:ProgramFiles\NVIDIA Corporation\NVIDIA app",
            "$env:ProgramFiles\NVIDIA Corporation\NvNode",
            "$env:ProgramFiles\NVIDIA Corporation\NvBackend",
            "$env:ProgramFiles\NVIDIA Corporation\NvCamera",
            "$env:ProgramFiles\NVIDIA Corporation\NvTelemetry",
            "$env:ProgramFiles\NVIDIA Corporation\ShadowPlay",
            "$env:ProgramFiles\NVIDIA Corporation\GeForce Experience",
            "$env:ProgramFiles\NVIDIA Corporation\NvContainer\plugins\LocalSystem\NvTelemetry",
            "${env:ProgramFiles(x86)}\NVIDIA Corporation\NvNode",
            "${env:ProgramFiles(x86)}\NVIDIA Corporation\NvBackend",
            "${env:ProgramFiles(x86)}\NVIDIA Corporation\NvTelemetry"
        )
        $removedBloat = 0
        foreach ($dir in $bloatDirs) {
            if (Test-Path $dir) {
                Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
                $removedBloat++
                Write-DebugLog "Removed: $dir"
            }
        }
        if ($removedBloat -gt 0) { Write-ActionOK "Removed $removedBloat NVIDIA bloat directories." }

        # Remove bloat scheduled tasks
        $bloatTaskPatterns = @("NvDriverUpdateCheckDaily*", "NVIDIA GeForce*", "NvNodeLauncher*", "NvBackend*", "NvTmRep*")
        foreach ($pattern in $bloatTaskPatterns) {
            $tasks = Get-ScheduledTask -TaskName $pattern -ErrorAction SilentlyContinue
            foreach ($t in $tasks) {
                try {
                    Unregister-ScheduledTask -TaskName $t.TaskName -Confirm:$false -ErrorAction Stop
                    Write-DebugLog "Removed task: $($t.TaskName)"
                } catch { Write-DebugLog "Task removal: $($t.TaskName) — $_" }
            }
        }
        Write-ActionOK "Full-install bloat cleanup complete."
    }

    # ── 4. Disable telemetry services (if any survived the strip) ────────────
    # NVDisplay.ContainerLocalSystem is intentionally NOT disabled — it's
    # required for the NVIDIA Control Panel to start and function.
    if ($installSuccess) {
        Write-Step "Disabling telemetry services..."
        $bloatServices = @(
            "NvTelemetryContainer",
            "NvContainerNetworkService"
        )
        foreach ($svc in $bloatServices) {
            try {
                $s = Get-Service -Name $svc -ErrorAction SilentlyContinue
                if ($s) {
                    Backup-ServiceState -ServiceName $svc -StepTitle "Driver Install Bloat Cleanup"
                    Stop-Service $svc -Force -ErrorAction SilentlyContinue
                    Set-Service $svc -StartupType Disabled -ErrorAction SilentlyContinue
                    Write-ActionOK "Disabled: $svc"
                }
            } catch { Write-DebugLog "Bloat service ${svc}: $_" }
        }

        # Remove GFE / NVIDIA App scheduled tasks
        $bloatTasks = @("NvDriverUpdateCheckDaily*", "NVIDIA GeForce*", "NvNodeLauncher*", "NvBackend*", "NvTmRep*")
        foreach ($pattern in $bloatTasks) {
            try {
                $tasks = Get-ScheduledTask -TaskName $pattern -ErrorAction SilentlyContinue
                foreach ($t in $tasks) {
                    Disable-ScheduledTask -TaskName $t.TaskName -ErrorAction SilentlyContinue | Out-Null
                    Write-ActionOK "Disabled task: $($t.TaskName)"
                }
            } catch { Write-DebugLog "Bloat task ${pattern}: $_" }
        }
    }

    # ── 3. Post-install tweaks (MSI, telemetry, HDCP, MPO, etc.) ─────────────
    if ($installSuccess) {
        Apply-NvidiaPostInstallTweaks
    }

    # ── 4. Cleanup NVIDIA temp extraction folders ────────────────────────────
    Write-Step "Cleaning up NVIDIA temp folders..."
    $nvTempPatterns = @("$env:TEMP\NVIDIA*", "$env:TEMP\NV*")
    foreach ($p in $nvTempPatterns) {
        Get-ChildItem $p -Directory -ErrorAction SilentlyContinue | ForEach-Object {
            Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    Write-OK "Cleanup complete."

    return $installSuccess
}

function Apply-NvidiaPostInstallTweaks {
    <#
    .SYNOPSIS  Applies post-install registry tweaks that NVCleanstall's
               "Expert Tweaks" would normally handle.
    #>

    $origTitle = if (Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) { $SCRIPT:CurrentStepTitle } else { $null }
    try {
        if (-not (Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -or -not $SCRIPT:CurrentStepTitle) { $SCRIPT:CurrentStepTitle = "NVIDIA Post-Install Tweaks" }

        Write-Step "Applying post-install NVIDIA tweaks..."

        # ── Disable NVIDIA Telemetry ─────────────────────────────────────────────
        $telemetryPaths = @(
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client"; Name = "OptInOrOutPreference"; Value = 0 },
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID44231"; Value = 0 },
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID64640"; Value = 0 },
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID66610"; Value = 0 }
        )
        foreach ($t in $telemetryPaths) {
            Set-RegistryValue $t.Path $t.Name $t.Value "DWord" "NVIDIA telemetry disable"
        }
        Write-ActionOK "NVIDIA telemetry disabled."

        # ── Disable HDCP ─────────────────────────────────────────────────────────
        $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"
        if (Test-Path $classPath) {
            $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
                Where-Object { $_.PSChildName -match "^\d{4}$" }
            foreach ($key in $subkeys) {
                $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
                if ($props.ProviderName -match "NVIDIA" -or $props.DriverDesc -match "NVIDIA") {
                    Set-RegistryValue $key.PSPath "RMHdcpKeyglobZero" 1 "DWord" "HDCP disable for $($props.DriverDesc)"
                }
            }
        }

        # ── Enable Write Combining ───────────────────────────────────────────────
        $gfxPath = "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers"
        Set-RegistryValue $gfxPath "EnableWriteCombining" 1 "DWord" "GPU write combining"
        Write-ActionOK "Write Combining enabled."

        # ── Disable MPO (Multiplane Overlay) ─────────────────────────────────────
        Set-RegistryValue "HKLM:\SOFTWARE\Microsoft\Windows\Dwm" "OverlayTestMode" 5 "DWord" "MPO disable"
        Write-ActionOK "MPO disabled."

        # ── Disable NVIDIA telemetry services ────────────────────────────────────
        $nvTelServices = @("NvTelemetryContainer", "NvContainerNetworkService")
        foreach ($svc in $nvTelServices) {
            try {
                if (-not $SCRIPT:DryRun) {
                    Backup-ServiceState -ServiceName $svc -StepTitle $SCRIPT:CurrentStepTitle
                    Stop-Service $svc -Force -ErrorAction SilentlyContinue
                    Set-Service $svc -StartupType Disabled -ErrorAction SilentlyContinue
                } else {
                    Write-Host "  [DRY-RUN] Would stop + disable: ${svc}" -ForegroundColor Magenta
                }
            } catch { Write-DebugLog "Telemetry service ${svc}: $_" }
        }
        Write-ActionOK "NVIDIA telemetry services disabled (if present)."

        if (-not $SCRIPT:DryRun) { Write-Info "Post-install tweaks applied. Restart recommended." }
    } finally {
        $SCRIPT:CurrentStepTitle = $origTitle
    }
}
