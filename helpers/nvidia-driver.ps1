# ==============================================================================
#  helpers/nvidia-driver.ps1  -  NVIDIA Driver Download + Clean Install
# ==============================================================================

function Get-LatestNvidiaDriver {
    <#
    .SYNOPSIS  Queries NVIDIA's driver lookup API to find the latest driver
               for the detected GPU. Returns download URL and version info.
    #>
    param(
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
    # Laptop entries MUST come before their desktop counterparts - [ordered]
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

        # NVIDIA's current lookup response links to a driver result record
        # instead of embedding the executable URL. Resolve that record through
        # the details service used by the NVIDIA driver page.
        if (-not $downloadUrl -and
            $content -match '(?i)href=[''"](?:(?:https?:)?//www\.nvidia\.com)?/download/driverResults\.aspx/([0-9]+)/en-us[''"]') {
            $driverId = $Matches[1]
            $detailsUrl = "https://www.nvidia.com/services/com.nvidia.services/AEMDriversContent/getDownloadDetails?%7B%22ddID%22:%22$driverId%22%7D"
            $detailsResponse = Invoke-WebRequest -Uri $detailsUrl -UseBasicParsing -TimeoutSec 30
            $details = $detailsResponse.Content | ConvertFrom-Json -ErrorAction Stop
            $detailRecords = @($details.driverDetails.IDS)
            if ($detailRecords.Count -eq 0 -or -not $detailRecords[0].downloadInfo) {
                throw "NVIDIA driver details response did not contain download metadata."
            }

            $downloadInfo = $detailRecords[0].downloadInfo
            if ([string]$downloadInfo.Success -ne '1' -or [string]$downloadInfo.ID -ne $driverId) {
                throw "NVIDIA driver details response did not match result $driverId."
            }
            $downloadUrl = [string]$downloadInfo.DownloadURL
            $version = [string]$downloadInfo.Version
            if ($version -notmatch '^\d+(?:\.\d+)+$') { $version = $null }
        }

        if ($downloadUrl) {
            # Ensure full URL - SECURITY: force HTTPS to prevent MITM during driver download.
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

function Test-NvidiaSignerSubject {
    [CmdletBinding()]
    param([AllowEmptyString()][string]$Subject)

    return ($Subject -match '(?:^|,\s*)(?:CN|O)=NVIDIA Corporation(?:,|$)')
}

function Resolve-TrustedNvidiaTaskkill {
    <# Resolves taskkill.exe from the OS Windows directory, never from PATH. #>
    [CmdletBinding()]
    param(
        [string]$WindowsRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows)
    )

    try {
        if ([string]::IsNullOrWhiteSpace($WindowsRoot) -or $WindowsRoot -match '^[\\/]{2}') {
            throw 'The Windows directory is missing or is not local.'
        }

        $expectedPath = [IO.Path]::GetFullPath(
            (Join-Path (Join-Path $WindowsRoot 'System32') 'taskkill.exe')
        )
        if (-not [IO.Path]::IsPathRooted($expectedPath) -or $expectedPath -match '^[\\/]{2}') {
            throw 'The taskkill path is not a rooted local path.'
        }

        $pathRoot = [IO.Path]::GetPathRoot($expectedPath)
        if ([string]::IsNullOrWhiteSpace($pathRoot) -or
            ([IO.DriveInfo]::new($pathRoot)).DriveType -ne [IO.DriveType]::Fixed) {
            throw 'The taskkill path is not on a fixed local drive.'
        }

        $taskkillItem = Get-Item -LiteralPath $expectedPath -Force -ErrorAction Stop
        if ($taskkillItem.PSIsContainer -or
            ($taskkillItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'taskkill.exe is not a regular non-reparse file.'
        }

        $actualPath = [IO.Path]::GetFullPath($taskkillItem.FullName)
        if (-not [string]::Equals($actualPath, $expectedPath, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'taskkill.exe resolved outside the expected System32 path.'
        }
        return $actualPath
    } catch {
        Write-DebugLog "Trusted taskkill.exe resolution failed: $($_.Exception.Message)"
        return $null
    }
}

function Invoke-NvidiaTaskkillTree {
    <# Uses the trusted Windows task-kill utility to terminate a PID and descendants. #>
    [CmdletBinding()]
    param([Parameter(Mandatory)][ValidateRange(1, 2147483647)][int]$ProcessId)

    $taskkillPath = Resolve-TrustedNvidiaTaskkill
    if (-not $taskkillPath) { return $false }

    try {
        $nativeResult = Invoke-NvidiaTaskkillNative -TaskkillPath $taskkillPath -ProcessId $ProcessId
        if ($null -eq $nativeResult.ExitCode -or $nativeResult.ExitCode -ne 0) {
            Write-DebugLog "NVIDIA process-tree termination failed for PID $ProcessId (exit $($nativeResult.ExitCode)): $($nativeResult.Output -join ' ')"
            return $false
        }
        return $true
    } catch {
        Write-DebugLog "NVIDIA process-tree termination raised an error for PID ${ProcessId}: $($_.Exception.Message)"
        return $false
    }
}

function Invoke-NvidiaTaskkillNative {
    <# Isolates native process invocation so tests never resolve or execute taskkill.exe. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$TaskkillPath,
        [Parameter(Mandatory)][ValidateRange(1, 2147483647)][int]$ProcessId
    )

    $global:LASTEXITCODE = $null
    $output = & $TaskkillPath /PID ([string]$ProcessId) /T /F 2>&1
    [PSCustomObject]@{
        ExitCode = $LASTEXITCODE
        Output = @($output)
    }
}

function Stop-NvidiaProcessBounded {
    <# Terminates a process tree started by this module and bounds the final wait. #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)]$Process,
        [int]$WaitTimeoutMs = 10000
    )

    try {
        if ($Process.PSObject.Properties.Match('HasExited').Count -gt 0 -and $Process.HasExited) {
            return $true
        }
        $processId = 0
        if ($Process.PSObject.Properties.Match('Id').Count -eq 0 -or
            -not [int]::TryParse([string]$Process.Id, [ref]$processId) -or
            $processId -le 0) {
            Write-DebugLog 'NVIDIA process-tree termination refused a process without a valid PID.'
            return $false
        }

        if (-not $PSCmdlet.ShouldProcess("PID $processId", 'Terminate NVIDIA process tree')) {
            return $false
        }
        if (-not (Invoke-NvidiaTaskkillTree -ProcessId $processId)) {
            return $false
        }
    } catch {
        Write-DebugLog "NVIDIA process-tree termination failed: $($_.Exception.Message)"
        return $false
    }

    try {
        return [bool]$Process.WaitForExit($WaitTimeoutMs)
    } catch {
        Write-DebugLog "NVIDIA process termination wait failed: $($_.Exception.Message)"
        return $false
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

        This is called immediately after download. Install-NvidiaDriverClean repeats
        the same fail-closed signature check before execution.
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
    $signerSubject = if ($sig.SignerCertificate) { [string]$sig.SignerCertificate.Subject } else { '' }
    if (-not (Test-NvidiaSignerSubject -Subject $signerSubject)) {
        Write-Err "Driver not signed by NVIDIA Corporation (signer: $signerSubject). Removing file."
        Remove-Item $FilePath -Force -ErrorAction SilentlyContinue
        return $false
    }

    Write-OK "Authenticode signature valid: $signerSubject"
    return $true
}

function Set-NvidiaExtractionDirectoryAcl {
    <#
    .SYNOPSIS  Restricts an extraction directory to SYSTEM and elevated local
               Administrators, excluding sibling processes using the caller's
               unelevated token.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    if (-not $PSCmdlet.ShouldProcess($Path, 'Restrict NVIDIA extraction directory ACL')) {
        return $false
    }

    if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
        throw "NVIDIA driver installation requires Windows ACL support."
    }

    $systemSid = New-Object System.Security.Principal.SecurityIdentifier(
        [System.Security.Principal.WellKnownSidType]::LocalSystemSid, $null
    )
    $administratorsSid = New-Object System.Security.Principal.SecurityIdentifier(
        [System.Security.Principal.WellKnownSidType]::BuiltinAdministratorsSid, $null
    )

    $acl = New-Object System.Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner($administratorsSid)
    foreach ($sid in @($systemSid, $administratorsSid)) {
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
            $sid,
            [System.Security.AccessControl.FileSystemRights]::FullControl,
            [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
            [System.Security.AccessControl.PropagationFlags]::None,
            [System.Security.AccessControl.AccessControlType]::Allow
        )
        [void]$acl.AddAccessRule($rule)
    }

    Set-Acl -LiteralPath $Path -AclObject $acl -ErrorAction Stop
    return $true
}

function New-SecureNvidiaExtractionDirectory {
    <#
    .SYNOPSIS  Creates an unpredictable extraction directory exclusively and
               applies a restrictive ACL before returning it.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)]
        [string]$ParentPath
    )

    $parent = Get-Item -LiteralPath $ParentPath -ErrorAction Stop
    if (-not $parent.PSIsContainer -or ($parent.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
        throw "The temporary directory is not a trusted physical directory: $ParentPath"
    }
    if ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT) {
        $volumeRoot = [System.IO.Path]::GetPathRoot($parent.FullName)
        if ($parent.FullName -match '^\\\\' -or $volumeRoot -notmatch '^[A-Za-z]:\\$' -or
            [System.IO.DriveInfo]::new($volumeRoot).DriveType -ne [System.IO.DriveType]::Fixed) {
            throw "The NVIDIA extraction parent must be on a local fixed-path Windows volume: $($parent.FullName)"
        }
        $ancestor = $parent
        while ($ancestor) {
            if ($ancestor.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw "The NVIDIA extraction parent has a reparse point in its ancestry: $($ancestor.FullName)"
            }
            $ancestor = $ancestor.Parent
        }
    }

    $random = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        for ($attempt = 0; $attempt -lt 16; $attempt++) {
            $bytes = New-Object byte[] 32
            $random.GetBytes($bytes)
            $token = [System.BitConverter]::ToString($bytes).Replace('-', '')
            $candidate = Join-Path $parent.FullName "NVDriverExtract_$token"

            if (-not $PSCmdlet.ShouldProcess($candidate, 'Create secure NVIDIA extraction directory')) {
                return $null
            }

            try {
                $created = New-Item -ItemType Directory -Path $candidate -ErrorAction Stop
            } catch {
                if (Test-Path -LiteralPath $candidate) {
                    continue
                }
                throw
            }

            try {
                if (-not (Set-NvidiaExtractionDirectoryAcl -Path $created.FullName)) {
                    throw 'The NVIDIA extraction directory ACL was not applied.'
                }
                $contents = @(Get-ChildItem -LiteralPath $created.FullName -Force -ErrorAction Stop)
                if ($contents.Count -ne 0) {
                    throw "The new extraction directory was not empty."
                }
                return $created.FullName
            } catch {
                Remove-Item -LiteralPath $created.FullName -Recurse -Force -ErrorAction SilentlyContinue
                throw
            }
        }
    } finally {
        $random.Dispose()
    }

    throw "Could not create an exclusive NVIDIA extraction directory after 16 attempts."
}

function Test-NvidiaSetupPath {
    <#
    .SYNOPSIS  Verifies that setup.exe is a regular descendant of the trusted
               extraction root and has no reparse point in its ancestry.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$ExtractionRoot,

        [Parameter(Mandatory)]
        [string]$CandidatePath
    )

    try {
        $rootItem = Get-Item -LiteralPath $ExtractionRoot -Force -ErrorAction Stop
        $candidateItem = Get-Item -LiteralPath $CandidatePath -Force -ErrorAction Stop
    } catch {
        return $false
    }

    if (-not $rootItem.PSIsContainer -or $candidateItem.PSIsContainer -or
        $candidateItem.Name -ine 'setup.exe') {
        return $false
    }

    $separator = [System.IO.Path]::DirectorySeparatorChar
    $rootCanonical = [System.IO.Path]::GetFullPath($rootItem.FullName).TrimEnd($separator)
    $candidateCanonical = [System.IO.Path]::GetFullPath($candidateItem.FullName)
    $rootPrefix = $rootCanonical + $separator
    if (-not $candidateCanonical.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }

    $current = $candidateItem
    while ($current) {
        if ($current.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            return $false
        }

        $currentCanonical = [System.IO.Path]::GetFullPath($current.FullName).TrimEnd($separator)
        if ($currentCanonical -eq $rootCanonical) {
            return $true
        }

        if ($current.PSIsContainer) {
            $current = $current.Parent
        } else {
            $current = $current.Directory
        }
    }

    return $false
}

function Find-NvidiaSetupExecutable {
    <#
    .SYNOPSIS  Finds exactly one structurally trusted setup.exe without
               traversing directory reparse points.
    #>
    param(
        [Parameter(Mandatory)]
        [string]$ExtractionRoot
    )

    $pending = New-Object 'System.Collections.Generic.Queue[string]'
    $pending.Enqueue($ExtractionRoot)
    $candidates = @()

    try {
        while ($pending.Count -gt 0) {
            $directory = $pending.Dequeue()
            $children = @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop |
                Sort-Object -Property FullName)
            foreach ($child in $children) {
                $isReparsePoint = [bool]($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint)
                if ($child.PSIsContainer) {
                    if ($isReparsePoint) {
                        throw "Extracted package contains a directory reparse point: $($child.FullName)"
                    }
                    $pending.Enqueue($child.FullName)
                } elseif ($child.Name -ieq 'setup.exe') {
                    $candidates += $child
                }
            }
        }
    } catch {
        throw "Could not enumerate the extracted NVIDIA package safely: $($_.Exception.Message)"
    }

    if ($candidates.Count -eq 0) {
        return $null
    }
    if ($candidates.Count -ne 1) {
        throw "Extraction produced $($candidates.Count) setup.exe candidates; refusing an ambiguous installer selection."
    }

    $candidate = $candidates[0].FullName
    if (-not (Test-NvidiaSetupPath -ExtractionRoot $ExtractionRoot -CandidatePath $candidate)) {
        throw "Extracted setup.exe failed canonical containment or reparse-point validation."
    }

    return [System.IO.Path]::GetFullPath($candidate)
}

function Get-NvidiaDisplayDriverSnapshot {
    <# .SYNOPSIS Captures a stable NVIDIA display-driver identity and version. #>
    $gpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match 'NVIDIA' } |
        Sort-Object -Property PNPDeviceID, DeviceID, Name |
        Select-Object -First 1

    if (-not $gpu) {
        return $null
    }

    $identity = if ($gpu.PNPDeviceID) {
        "PNP:$($gpu.PNPDeviceID)"
    } elseif ($gpu.DeviceID) {
        "DEVICE:$($gpu.DeviceID)"
    } else {
        "NAME:$($gpu.Name)"
    }

    return [PSCustomObject]@{
        Identity = $identity
        Name = [string]$gpu.Name
        Version = [string]$gpu.DriverVersion
    }
}

function Remove-NvidiaPackageBloat {
    <# Removes required unwanted components before setup.exe can execute. #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)]
        [string]$PackageRoot
    )

    try {
        $packageRootItem = Get-Item -LiteralPath $PackageRoot -Force -ErrorAction Stop
        if (-not $packageRootItem.PSIsContainer -or
            ($packageRootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw "Package root is not a trusted physical directory."
        }
    } catch {
        return [PSCustomObject]@{
            Status = 'Failed'; RemovedCount = 0
            Failures = @("Could not validate package root '$PackageRoot': $($_.Exception.Message)")
        }
    }

    $patterns = @(
        "GFExperience*", "NvApp*", "NvBackend*", "NvTelemetry*",
        "NvContainer\plugins\LocalSystem\NvTelemetry*", "NvNodejs*", "nodejs*",
        "NvCamera*", "ShadowPlay*", "NvVAD*", "EULA.txt", "ListDevices.txt", "license.txt"
    )
    $removedCount = 0
    $notProcessed = 0
    $failures = [System.Collections.Generic.List[string]]::new()
    foreach ($pattern in $patterns) {
        try {
            $relativeParent = Split-Path -Path $pattern -Parent
            $leafPattern = Split-Path -Path $pattern -Leaf
            $searchRoot = if ($relativeParent) { Join-Path $PackageRoot $relativeParent } else { $PackageRoot }
            if (-not (Test-Path -LiteralPath $searchRoot -PathType Container -ErrorAction Stop)) {
                continue
            }
            $items = @(Get-ChildItem -LiteralPath $searchRoot -Force -ErrorAction Stop |
                Where-Object { $_.Name -like $leafPattern })
        } catch {
            $failures.Add("Could not enumerate '$pattern': $($_.Exception.Message)")
            continue
        }
        foreach ($item in $items) {
            if (-not $PSCmdlet.ShouldProcess($item.FullName, 'Remove NVIDIA package component')) {
                $notProcessed++
                continue
            }
            try {
                Remove-Item -LiteralPath $item.FullName -Recurse -Force -ErrorAction Stop
                if (Test-Path -LiteralPath $item.FullName) {
                    throw "Path still exists after removal."
                }
                $removedCount++
                Write-DebugLog "Removed: $($item.Name)"
            } catch {
                $failures.Add("Could not remove '$($item.FullName)': $($_.Exception.Message)")
            }
        }
    }

    return [PSCustomObject]@{
        Status = if ($failures.Count -gt 0) { 'Failed' } elseif ($notProcessed -gt 0) { 'DryRun' } else { 'Success' }
        RemovedCount = $removedCount
        Failures = @($failures)
        NotProcessedCount = $notProcessed
    }
}

function Test-NvidiaDriverSnapshotChanged {
    <# .SYNOPSIS Tests whether an NVIDIA driver appeared or changed version. #>
    param(
        $Before,
        $After
    )

    if (-not $After -or -not $After.Version) {
        return $false
    }
    if (-not $Before) {
        return $true
    }

    return ($Before.Identity -ne $After.Identity -or $Before.Version -ne $After.Version)
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
        NVDisplay.ContainerLocalSystem is intentionally kept - it's required for the
        NVIDIA Control Panel to function.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$DriverExe
    )

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  [DRY-RUN] Would install NVIDIA driver (component-selective): $DriverExe" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   1. Extract driver package to temp folder" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   2. Remove bloat components (NVIDIA App, GFE, telemetry, NodeJS)" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   3. Run setup.exe -s -noreboot -clean (driver-only)" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   4. Disable telemetry services + scheduled tasks" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   5. Apply post-install registry tweaks" -ForegroundColor Magenta
        return $true
    }

    if (-not (Test-Path -LiteralPath $DriverExe -PathType Leaf)) {
        Write-Err "Driver file not found: $DriverExe"
        return $false
    }

    # SECURITY: Validate driver path - this file is passed to Start-Process.
    # state.json nvidiaDriverPath or user input could point to malware.
    # Verify: must be a real .exe file (not directory/symlink to non-file), no path traversal.
    # Check for path traversal BEFORE resolving - after Get-Item, '..' is normalized away
    if ($DriverExe -match '\.\.') {
        Write-Err "Driver path contains path traversal: $DriverExe"
        return $false
    }
    $driverItem = Get-Item -LiteralPath $DriverExe -Force -ErrorAction SilentlyContinue
    if (-not $driverItem -or $driverItem.PSIsContainer -or
        ($driverItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
        Write-Err "Driver path is not a file: $DriverExe"
        return $false
    }
    # Require a valid Authenticode signature and NVIDIA signer identity.
    # This is an execution boundary: the installer runs as admin, so invalid or
    # non-NVIDIA signatures must fail closed. Manual/persisted driver paths are
    # untrusted because they can cross an admin execution boundary.
    $sig = Get-AuthenticodeSignature -FilePath $driverItem.FullName -ErrorAction SilentlyContinue
    if (-not $sig -or $sig.Status -ne 'Valid') {
        Write-Err "Driver .exe has no valid Authenticode signature (status: $(if($sig){$sig.Status}else{'N/A'})). Refusing to execute."
        return $false
    }
    $sigSubject = if ($sig.SignerCertificate) { [string]$sig.SignerCertificate.Subject } else { '' }
    if (-not (Test-NvidiaSignerSubject -Subject $sigSubject)) {
        Write-Err "Driver .exe is signed but not by NVIDIA Corporation (signer: $sigSubject). Refusing to execute."
        return $false
    }
    Write-DebugLog "Driver Authenticode signature valid: $sigSubject"

    # ── 1. Extract driver package ───────────────────────────────────────────
    # The NVIDIA .exe is a self-extracting archive. Use NVIDIA's native silent
    # extraction flags: -s (silent) + -e"<path>" (extract only, no install).
    # This replaces the legacy -x -gm2 -InstallDir approach which still spawns
    # a GUI dialog on modern driver packages.
    # NOTE: -e has NO space before the path - it's -e"C:\path", not -e "C:\path".
    # IMPORTANT: Pass the full argument line as a single string, NOT an array.
    # PowerShell's Start-Process can double-quote array elements, mangling the
    # -e"path" flag and causing the extractor to fall back to a full silent install
    # (which installs NVIDIA App, Control Panel, and other bloat).
    $tempParent = if ($env:TEMP) { $env:TEMP } else { [System.IO.Path]::GetTempPath() }
    try {
        $extractDir = New-SecureNvidiaExtractionDirectory -ParentPath $tempParent
    } catch {
        Write-Err "Could not create a secure NVIDIA extraction directory: $($_.Exception.Message)"
        return $false
    }

    # Defense in depth: the creator is expected to return an empty directory, but verify
    # again at the call site before exposing the path to another process.
    try {
        $preexistingItems = @(Get-ChildItem -LiteralPath $extractDir -Force -ErrorAction Stop)
    } catch {
        Write-Err "Could not verify the NVIDIA extraction directory: $($_.Exception.Message)"
        Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        return $false
    }
    if ($preexistingItems.Count -ne 0) {
        Write-Err "The NVIDIA extraction directory contains preexisting items; refusing to execute the package."
        Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        return $false
    }

    # The caller-supplied package remains writable by the invoking user while
    # this elevated process is running.  Copy it into the newly ACL-restricted
    # directory, prove the copy is byte-for-byte identical, then execute only
    # that secured copy.  This closes the authenticate-then-execute race on the
    # original download path.
    $securedDriverExe = Join-Path $extractDir 'nvidia-driver-package.exe'
    try {
        $sourceHash = (Get-FileHash -LiteralPath $driverItem.FullName -Algorithm SHA256 -ErrorAction Stop).Hash
        $sourceLength = [int64]$driverItem.Length
        Copy-Item -LiteralPath $driverItem.FullName -Destination $securedDriverExe -ErrorAction Stop
        $securedItem = Get-Item -LiteralPath $securedDriverExe -Force -ErrorAction Stop
        if ($securedItem.PSIsContainer -or
            ($securedItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -or
            [int64]$securedItem.Length -ne $sourceLength) {
            throw 'The secured NVIDIA package copy is not a regular file with the expected length.'
        }
        $securedHash = (Get-FileHash -LiteralPath $securedItem.FullName -Algorithm SHA256 -ErrorAction Stop).Hash
        if (-not [string]::Equals($sourceHash, $securedHash, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'The secured NVIDIA package copy does not match the verified source hash.'
        }
        $securedSignature = Get-AuthenticodeSignature -FilePath $securedItem.FullName -ErrorAction Stop
        $securedSubject = if ($securedSignature -and $securedSignature.SignerCertificate) {
            [string]$securedSignature.SignerCertificate.Subject
        } else {
            ''
        }
        if (-not $securedSignature -or $securedSignature.Status -ne 'Valid' -or
            -not (Test-NvidiaSignerSubject -Subject $securedSubject)) {
            throw "The secured NVIDIA package copy failed Authenticode validation (status: $(if($securedSignature){$securedSignature.Status}else{'N/A'}); signer: $(if($securedSubject){$securedSubject}else{'N/A'}))."
        }
    } catch {
        Write-Err "Could not secure and verify the NVIDIA package copy: $($_.Exception.Message)"
        Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        return $false
    }

    # Capture the pre-operation state before either executable is launched. If
    # the outer package unexpectedly performs a full installation, an unchanged
    # preexisting WMI record is not evidence that this operation succeeded.
    $baselineGpu = Get-NvidiaDisplayDriverSnapshot
    Write-Step "Extracting driver package (silent)..."
    Write-Info "Extracting to: $extractDir"

    try {
        $extractProcess = Start-Process -FilePath $securedDriverExe `
            -ArgumentList "-s -e`"$extractDir`"" `
            -PassThru
    } catch {
        Write-Err "Could not start NVIDIA package extraction: $($_.Exception.Message)"
        Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        return $false
    }

    # Wait up to 5 minutes for extraction - prevents indefinite hangs
    $extractTimeout = 300000  # 5 minutes in ms
    $completed = $false
    $extractWaitFailed = $false
    try {
        $completed = $extractProcess.WaitForExit($extractTimeout)
    } catch {
        $extractWaitFailed = $true
        Write-Err "Could not wait for NVIDIA package extraction: $($_.Exception.Message)"
    }
    if (-not $completed) {
        if (-not $extractWaitFailed) { Write-Err "Extraction timed out after 5 minutes." }
        $extractTerminated = Stop-NvidiaProcessBounded -Process $extractProcess
        if ($extractTerminated) {
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        } else {
            Write-Warn "Extraction termination could not be verified; retained the secured working directory: $extractDir"
        }
        return $false
    }

    # Find setup.exe - exactly one regular, contained candidate is required.
    # Directory reparse points are never traversed by Find-NvidiaSetupExecutable.
    try {
        $setupExe = Find-NvidiaSetupExecutable -ExtractionRoot $extractDir
    } catch {
        Write-Err $_.Exception.Message
        Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        return $false
    }
    $packageRoot = $extractDir
    if ($setupExe) {
        $packageRoot = Split-Path -Path $setupExe -Parent
        if ($packageRoot -ne $extractDir) {
            Write-Info "Found setup.exe in subdirectory: $(Split-Path -Path $packageRoot -Leaf)"
        }
    }

    $fullInstallDetected = $false
    if (-not $setupExe) {
        # The self-extractor may have performed a full install instead of extract-only.
        # This happens when argument quoting is misinterpreted by the extractor.
        # Detect by checking if NVIDIA driver appeared in WMI after extraction attempt.
        $postGpu = Get-NvidiaDisplayDriverSnapshot
        if (Test-NvidiaDriverSnapshotChanged -Before $baselineGpu -After $postGpu) {
            Write-Warn "setup.exe not found, but the NVIDIA driver state changed during this operation."
            Write-Warn "The installer performed a full install instead of extract-only."
            Write-Info "Detected: $($postGpu.Name) - Driver $($postGpu.Version)"
            Write-Info "Applying post-install cleanup (removing bloat, disabling telemetry)..."
            $fullInstallDetected = $true
            $installSuccess = $true
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        } else {
            Write-Err "Extraction failed - setup.exe not found in $extractDir"
            Write-Info "Exit code: $($extractProcess.ExitCode)"
            if (Test-Path -LiteralPath $extractDir) {
                $extractContents = Get-ChildItem -LiteralPath $extractDir -ErrorAction SilentlyContinue | Select-Object -First 10
                if ($extractContents) {
                    Write-Info "Extraction folder contains: $($extractContents.Name -join ', ')"
                } else {
                    Write-Info "Extraction folder is empty."
                }
            }
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
            return $false
        }
    } else {
        Write-OK "Extraction complete."
    }

    if (-not $fullInstallDetected) {
        # ── 2. Strip bloat components ────────────────────────────────────────
        # Remove component folders so setup.exe never installs them.
        # NVDisplay.ContainerLocalSystem is NOT removed - it's required for NVCP.
        Write-Step "Removing unwanted components from extracted package..."
        $bloatRemoval = Remove-NvidiaPackageBloat -PackageRoot $packageRoot
        if ($bloatRemoval.Status -ne 'Success') {
            foreach ($failure in $bloatRemoval.Failures) { Write-Warn $failure }
            Write-Err "Required NVIDIA package components could not be stripped; setup.exe will not be executed."
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
            return $false
        }
        Write-ActionOK "Removed $($bloatRemoval.RemovedCount) bloat components from package."

        # ── 3. Run setup.exe from stripped package ───────────────────────────
        Write-Step "Installing NVIDIA driver (driver-only, silent)..."
        Write-Info "This takes 3-7 minutes. Screen may flicker - do not touch the PC."

        # Revalidate the execution boundary after package modification and
        # immediately before elevation. Fail closed if setup.exe was replaced,
        # redirected through a reparse point, or signed by another publisher.
        if (-not (Test-NvidiaSetupPath -ExtractionRoot $extractDir -CandidatePath $setupExe)) {
            Write-Err "Extracted setup.exe is no longer a trusted descendant of the extraction directory."
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
            return $false
        }
        $setupSignature = Get-AuthenticodeSignature -FilePath $setupExe -ErrorAction SilentlyContinue
        $setupSignerSubject = if ($setupSignature -and $setupSignature.SignerCertificate) {
            [string]$setupSignature.SignerCertificate.Subject
        } else {
            ''
        }
        $isNvidiaSigner = Test-NvidiaSignerSubject -Subject $setupSignerSubject
        if (-not $setupSignature -or $setupSignature.Status -ne 'Valid' -or
            -not $setupSignature.SignerCertificate -or
            -not $isNvidiaSigner) {
            $setupStatus = if ($setupSignature) { $setupSignature.Status } else { 'N/A' }
            $setupSigner = if ($setupSignerSubject) { $setupSignerSubject } else { 'N/A' }
            Write-Err "Extracted setup.exe failed NVIDIA Authenticode validation (status: $setupStatus; signer: $setupSigner). Refusing to execute."
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
            return $false
        }

        try {
            $installProcess = Start-Process -FilePath $setupExe `
                -ArgumentList "-s -noreboot -clean" `
                -PassThru -NoNewWindow
        } catch {
            Write-Err "Could not start the extracted NVIDIA installer: $($_.Exception.Message)"
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
            return $false
        }

        # A healthy driver install normally completes in 3-7 minutes.  Bound
        # the wait so a stalled vendor installer cannot block Phase 3 forever.
        $installTimeout = 600000  # 10 minutes in ms
        $installCompleted = $false
        $installWaitFailed = $false
        try {
            $installCompleted = $installProcess.WaitForExit($installTimeout)
        } catch {
            $installWaitFailed = $true
            Write-Err "Could not wait for the extracted NVIDIA installer: $($_.Exception.Message)"
        }
        if (-not $installCompleted) {
            if (-not $installWaitFailed) { Write-Err "NVIDIA installer timed out after 10 minutes; terminating it." }
            $installTerminated = Stop-NvidiaProcessBounded -Process $installProcess
            if ($installTerminated) {
                Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
            } else {
                Write-Warn "Installer termination could not be verified; retained the secured working directory: $extractDir"
            }
            return $false
        }

        $installSuccess = $false
        if ($installProcess.ExitCode -in @(0, 1)) {
            $postInstallGpu = Get-NvidiaDisplayDriverSnapshot
            if (-not $postInstallGpu) {
                Write-Err "NVIDIA installer returned exit code $($installProcess.ExitCode), but no NVIDIA display driver was found afterward."
            } else {
                $rebootNote = if ($installProcess.ExitCode -eq 1) { " Reboot required." } else { "" }
                Write-OK "NVIDIA display driver verified after installation: $($postInstallGpu.Name) $($postInstallGpu.Version).$rebootNote"
                $installSuccess = $true
            }
        } else {
            Write-Warn "Installer exited with code $($installProcess.ExitCode)."
            Write-Info "Check the NVIDIA installer log and Device Manager before retrying."
            $installSuccess = $false
        }

        # Clean up extraction folder
        Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        # ── Full install detected - remove bloat that was installed ──────────
        # The extractor ran a full install including NVIDIA App, GFE, etc.
        # Remove the bloat software while keeping the display driver intact.
        Write-Step "Removing NVIDIA bloat installed during full install..."
        $fullInstallCleanupFailures = [System.Collections.Generic.List[string]]::new()

        # Remove NVIDIA AppX packages (NVIDIA App, Control Panel from Store)
        if (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue) {
            try {
                $nvAppx = @(Get-AppxPackage -AllUsers -ErrorAction Stop |
                    Where-Object { $_.Name -match "NVIDIA" -and $_.Name -notmatch "ControlPanel" })
                foreach ($pkg in $nvAppx) {
                    Remove-AppxPackage -Package $pkg.PackageFullName -AllUsers -ErrorAction Stop
                    Write-OK "Removed AppX: $($pkg.Name)"
                }
            } catch {
                $fullInstallCleanupFailures.Add("NVIDIA AppX cleanup failed: $($_.Exception.Message)")
            }
        } else {
            $fullInstallCleanupFailures.Add("NVIDIA AppX cleanup is unavailable on this system.")
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
            if (Test-Path -LiteralPath $dir) {
                try {
                    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction Stop
                    if (Test-Path -LiteralPath $dir) { throw "Path still exists after removal." }
                    $removedBloat++
                    Write-DebugLog "Removed: $dir"
                } catch {
                    $fullInstallCleanupFailures.Add("Could not remove '$dir': $($_.Exception.Message)")
                }
            }
        }
        if ($removedBloat -gt 0) { Write-ActionOK "Removed $removedBloat NVIDIA bloat directories." }

        # Remove bloat scheduled tasks
        $bloatTaskPatterns = @("NvDriverUpdateCheckDaily*", "NVIDIA GeForce*", "NvNodeLauncher*", "NvBackend*", "NvTmRep*")
        try {
            $tasks = @(Get-ScheduledTask -ErrorAction Stop | Where-Object {
                $taskName = $_.TaskName
                @($bloatTaskPatterns | Where-Object { $taskName -like $_ }).Count -gt 0
            })
            foreach ($t in $tasks) {
                try {
                    Unregister-ScheduledTask -TaskName $t.TaskName -Confirm:$false -ErrorAction Stop
                    Write-DebugLog "Removed task: $($t.TaskName)"
                } catch {
                    $fullInstallCleanupFailures.Add("Could not remove task '$($t.TaskName)': $($_.Exception.Message)")
                }
            }
        } catch {
            $fullInstallCleanupFailures.Add("NVIDIA scheduled-task enumeration failed: $($_.Exception.Message)")
        }
        if ($fullInstallCleanupFailures.Count -gt 0) {
            foreach ($failure in $fullInstallCleanupFailures) { Write-Warn $failure }
            Write-Warn "The NVIDIA driver was installed, but full-install bloat cleanup did not complete."
            $installSuccess = $false
        } else {
            Write-ActionOK "Full-install bloat cleanup complete."
        }
    }

    # ── 4. Disable telemetry services (if any survived the strip) ────────────
    # NVDisplay.ContainerLocalSystem is intentionally NOT disabled - it's
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
                    $serviceStepTitle = "Driver Install Bloat Cleanup"
                    $capture = Backup-ServiceState -ServiceName $svc -StepTitle $serviceStepTitle -PassThru
                    if (-not $capture -or -not $capture.Captured) {
                        $detail = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
                        throw "Service change blocked because its original state was not captured: $detail"
                    }
                    Flush-BackupBuffer
                    $durableBackup = Get-BackupDataRaw
                    if (@($durableBackup.entries | Where-Object {
                        $_.type -eq 'service' -and $_.step -eq $serviceStepTitle -and $_.name -eq $svc
                    }).Count -eq 0) {
                        throw "Service change blocked because backup.json has no restore record for '$svc'."
                    }
                    Stop-Service $svc -Force -ErrorAction Stop
                    Set-Service $svc -StartupType Disabled -ErrorAction Stop
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
        $postInstallResult = Apply-NvidiaPostInstallTweaks
        if ($postInstallResult -and -not $postInstallResult.Succeeded) {
            Write-Warn "NVIDIA driver installation was verified, but $($postInstallResult.Failed) optional post-install change(s) failed."
        }
    }

    # The transaction removes only its own unpredictable extraction directory
    # on every return path. Never sweep broad %TEMP%\NV* patterns: another
    # installer or user process may own those directories concurrently.
    Write-OK "Owned extraction cleanup complete."

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
        $outcomes = [System.Collections.Generic.List[object]]::new()

        # ── Disable NVIDIA Telemetry ─────────────────────────────────────────────
        $telemetryPaths = @(
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client"; Name = "OptInOrOutPreference"; Value = 0 },
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID44231"; Value = 0 },
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID64640"; Value = 0 },
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID66610"; Value = 0 }
        )
        foreach ($t in $telemetryPaths) {
            $result = Set-RegistryValue $t.Path $t.Name $t.Value "DWord" "NVIDIA telemetry disable" -PassThru
            $status = if (-not $result) { "Failed" } elseif ($result.Status -eq "DryRun") { "Previewed" } elseif ($result.Applied) { "Applied" } else { "Failed" }
            $outcomes.Add([PSCustomObject]@{ Label = "Registry $($t.Name)"; Status = $status }) | Out-Null
            if ($status -eq "Failed") { Write-Warn "NVIDIA telemetry registry change was not applied: $($t.Name)." }
        }
        if (@($outcomes | Where-Object { $_.Label -like 'Registry *' -and $_.Status -eq 'Applied' }).Count -eq $telemetryPaths.Count) {
            Write-ActionOK "NVIDIA telemetry registry values applied."
        }

        # ── Disable HDCP ─────────────────────────────────────────────────────────
        $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"
        if (Test-Path $classPath) {
            $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
                Where-Object { $_.PSChildName -match "^\d{4}$" }
            foreach ($key in $subkeys) {
                $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
                if ($props.ProviderName -match "NVIDIA" -or $props.DriverDesc -match "NVIDIA") {
                    $result = Set-RegistryValue $key.PSPath "RMHdcpKeyglobZero" 1 "DWord" "HDCP disable for $($props.DriverDesc)" -PassThru
                    $status = if (-not $result) { "Failed" } elseif ($result.Status -eq "DryRun") { "Previewed" } elseif ($result.Applied) { "Applied" } else { "Failed" }
                    $outcomes.Add([PSCustomObject]@{ Label = "HDCP $($key.PSChildName)"; Status = $status }) | Out-Null
                    if ($status -eq "Failed") { Write-Warn "NVIDIA HDCP registry change was not applied for $($props.DriverDesc)." }
                }
            }
        }

        # ── Enable Write Combining ───────────────────────────────────────────────
        $gfxPath = "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers"
        $writeCombiningResult = Set-RegistryValue $gfxPath "EnableWriteCombining" 1 "DWord" "GPU write combining" -PassThru
        $writeCombiningStatus = if (-not $writeCombiningResult) { "Failed" } elseif ($writeCombiningResult.Status -eq "DryRun") { "Previewed" } elseif ($writeCombiningResult.Applied) { "Applied" } else { "Failed" }
        $outcomes.Add([PSCustomObject]@{ Label = "Write combining"; Status = $writeCombiningStatus }) | Out-Null
        if ($writeCombiningStatus -eq "Applied") { Write-ActionOK "Write Combining registry value applied." }
        elseif ($writeCombiningStatus -eq "Failed") { Write-Warn "Write Combining registry value was not applied." }

        # ── Disable MPO (Multiplane Overlay) ─────────────────────────────────────
        $mpoResult = Set-RegistryValue "HKLM:\SOFTWARE\Microsoft\Windows\Dwm" "OverlayTestMode" 5 "DWord" "MPO disable" -PassThru
        $mpoStatus = if (-not $mpoResult) { "Failed" } elseif ($mpoResult.Status -eq "DryRun") { "Previewed" } elseif ($mpoResult.Applied) { "Applied" } else { "Failed" }
        $outcomes.Add([PSCustomObject]@{ Label = "MPO"; Status = $mpoStatus }) | Out-Null
        if ($mpoStatus -eq "Applied") { Write-ActionOK "MPO registry value applied." }
        elseif ($mpoStatus -eq "Failed") { Write-Warn "MPO registry value was not applied." }

        # ── Disable NVIDIA telemetry services ────────────────────────────────────
        $nvTelServices = @("NvTelemetryContainer", "NvContainerNetworkService")
        foreach ($svc in $nvTelServices) {
            try {
                if (-not $SCRIPT:DryRun) {
                    $service = Get-Service -Name $svc -ErrorAction SilentlyContinue
                    if (-not $service) {
                        $outcomes.Add([PSCustomObject]@{ Label = "Service $svc"; Status = "Skipped" }) | Out-Null
                        continue
                    }
                    $capture = Backup-ServiceState -ServiceName $svc -StepTitle $SCRIPT:CurrentStepTitle -PassThru
                    if (-not $capture -or -not $capture.Captured) {
                        $detail = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
                        throw "Service change blocked because its original state was not captured: $detail"
                    }
                    Flush-BackupBuffer
                    $durableBackup = Get-BackupDataRaw
                    if (@($durableBackup.entries | Where-Object {
                        $_.type -eq 'service' -and [string]$_.step -eq [string]$SCRIPT:CurrentStepTitle -and $_.name -eq $svc
                    }).Count -eq 0) {
                        throw "Service change blocked because backup.json has no restore record for '$svc'."
                    }
                    Stop-Service $svc -Force -ErrorAction Stop
                    Set-Service $svc -StartupType Disabled -ErrorAction Stop
                    $outcomes.Add([PSCustomObject]@{ Label = "Service $svc"; Status = "Applied" }) | Out-Null
                    Write-ActionOK "Stopped and disabled service: $svc"
                } else {
                    Write-ConsoleLine "  [DRY-RUN] Would stop + disable: ${svc}" -ForegroundColor Magenta
                    $outcomes.Add([PSCustomObject]@{ Label = "Service $svc"; Status = "Previewed" }) | Out-Null
                }
            } catch {
                $outcomes.Add([PSCustomObject]@{ Label = "Service $svc"; Status = "Failed" }) | Out-Null
                Write-Warn "NVIDIA telemetry service change failed for ${svc}: $_"
            }
        }

        $failedCount = @($outcomes | Where-Object { $_.Status -eq "Failed" }).Count
        $appliedCount = @($outcomes | Where-Object { $_.Status -eq "Applied" }).Count
        $previewedCount = @($outcomes | Where-Object { $_.Status -eq "Previewed" }).Count
        $skippedCount = @($outcomes | Where-Object { $_.Status -eq "Skipped" }).Count
        if ($SCRIPT:DryRun) {
            Write-Info "Post-install preview: $previewedCount change(s) rendered."
        } elseif ($failedCount -eq 0) {
            Write-Info "Post-install changes completed: $appliedCount applied, $skippedCount not present. Restart recommended."
        } else {
            Write-Warn "Post-install changes were partial: $appliedCount applied, $failedCount failed, $skippedCount not present."
        }
        return [PSCustomObject]@{
            Succeeded = ($failedCount -eq 0)
            Applied = $appliedCount
            Failed = $failedCount
            Previewed = $previewedCount
            Skipped = $skippedCount
        }
    } finally {
        $SCRIPT:CurrentStepTitle = $origTitle
    }
}
