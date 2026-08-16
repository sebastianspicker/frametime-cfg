<#
.SYNOPSIS
    Fixed normal-boot UAC transition for a manifest-verified Phase 3 payload.

.DESCRIPTION
    This file is part of the exact immutable runtime payload. The HKCU Run
    registration supplies only the fixed PostReboot-Setup target; no registry
    text is evaluated as PowerShell source.
#>
param(
    [Parameter(Mandatory)][ValidateSet("PostReboot-Setup.ps1")][string]$Target,
    [Parameter(Mandatory)][ValidateSet("Bypass", "RemoteSigned", "AllSigned")][string]$TargetExecutionPolicy
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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

function Get-TrustedBootstrapPowerShellPath {
    $systemDirectory = [Environment]::SystemDirectory
    if ([string]::IsNullOrWhiteSpace($systemDirectory) -or -not [IO.Path]::IsPathRooted($systemDirectory)) {
        throw "Windows system directory is unavailable."
    }
    $systemRoot = [IO.Path]::GetFullPath($systemDirectory)
    $candidatePath = [IO.Path]::GetFullPath((Join-Path $systemRoot "WindowsPowerShell\\v1.0\\powershell.exe"))
    $rootWithSeparator = $systemRoot.TrimEnd([char[]]@([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)) + [IO.Path]::DirectorySeparatorChar
    if (-not $candidatePath.StartsWith($rootWithSeparator, [StringComparison]::OrdinalIgnoreCase)) {
        throw "PowerShell path escaped the Windows system directory."
    }
    $item = Get-Item -LiteralPath $candidatePath -Force -ErrorAction Stop
    if ($item.PSProvider.Name -ne 'FileSystem' -or $item.PSIsContainer -or
        ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        -not [String]::Equals([IO.Path]::GetFullPath($item.FullName), $candidatePath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Windows PowerShell executable is not a regular trusted system file."
    }
    return $candidatePath
}

try {
    $payloadValidation = Test-PublishedRuntimePayloadBootstrap -RuntimeRoot $PSScriptRoot
    if (-not $payloadValidation.Valid) { throw $payloadValidation.Message }

    $runtimeRoot = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd([char[]]@('\', '/'))
    $targetPath = [IO.Path]::GetFullPath((Join-Path $runtimeRoot $Target))
    if (-not $targetPath.StartsWith($runtimeRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Phase 3 target escaped the verified runtime root."
    }
    $targetItem = Get-Item -LiteralPath $targetPath -Force -ErrorAction Stop
    if ($targetItem.PSProvider.Name -ne 'FileSystem' -or $targetItem.PSIsContainer -or
        ($targetItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        -not [String]::Equals([IO.Path]::GetFullPath($targetItem.FullName), $targetPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Phase 3 target is not a regular trusted runtime file."
    }

    $powershellPath = Get-TrustedBootstrapPowerShellPath
    Start-Process -FilePath $powershellPath -Verb RunAs -Wait -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', $TargetExecutionPolicy, '-WindowStyle', 'Normal', '-File', $targetPath
    ) -ErrorAction Stop
} catch {
    Write-Host "  CRITICAL: Phase 3 elevation bootstrap stopped: $_" -ForegroundColor Red
    Write-Host "  Run Phase 3 manually as Administrator from the published runtime." -ForegroundColor Yellow
    exit 1
}
