Set-StrictMode -Version Latest

function Parse-TrimFsutilOutput {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]]$OutputLines
    )

    $states = [ordered]@{}
    foreach ($line in $OutputLines) {
        if ($line -match '^\s*(NTFS|ReFS)\s+DisableDeleteNotify\s*=\s*(\d+)\s*$') {
            $fs = [string]$Matches[1]
            $value = [int]$Matches[2]
            $states[$fs] = [PSCustomObject]@{
                FileSystem  = $fs
                RawValue    = $value
                TrimEnabled = ($value -eq 0)
            }
            continue
        }
        if ($line -match '^\s*DisableDeleteNotify\s*=\s*(\d+)\s*$') {
            $value = [int]$Matches[1]
            $states["NTFS"] = [PSCustomObject]@{
                FileSystem  = "NTFS"
                RawValue    = $value
                TrimEnabled = ($value -eq 0)
            }
        }
    }

    return @($states.Values)
}

function Get-TrimHealthStatus {
    [CmdletBinding()]
    param()

    $lines = @(fsutil behavior query DisableDeleteNotify 2>&1 | ForEach-Object { [string]$_ })
    $states = @(Parse-TrimFsutilOutput -OutputLines $lines)
    $retrimmableVolumes = @()
    try {
        $retrimmableVolumes = @(
            Get-Volume -ErrorAction SilentlyContinue | Where-Object {
                $_.DriveType -eq 'Fixed' -and ($_.FileSystem -in @('NTFS', 'ReFS'))
            } | Select-Object -ExpandProperty DriveLetter
        )
    } catch {
        $retrimmableVolumes = @()
    }

    return [PSCustomObject]@{
        States               = $states
        RawOutput            = $lines
        RetrimmableVolumes   = @($retrimmableVolumes | Where-Object { $_ })
        RetrimAvailable      = ($retrimmableVolumes.Count -gt 0)
        AnyTrimDisabled      = (@($states | Where-Object { -not $_.TrimEnabled }).Count -gt 0)
        Summary              = if ($states.Count -eq 0) {
            "TRIM state not readable"
        } else {
            (($states | ForEach-Object { "$($_.FileSystem): $(if ($_.TrimEnabled) { 'enabled' } else { 'disabled' })" }) -join ' · ')
        }
    }
}

function Enable-TrimSupport {
    [CmdletBinding()]
    param()

    $output = @(fsutil behavior set DisableDeleteNotify 0 2>&1 | ForEach-Object { [string]$_ })
    return [PSCustomObject]@{
        Success = ($LASTEXITCODE -eq 0)
        Output  = $output
    }
}

function Invoke-StorageRetrim {
    [CmdletBinding(SupportsShouldProcess = $true)]
    param(
        [Parameter(Mandatory)][ValidatePattern('^[A-Z]$')][string]$DriveLetter
    )

    if ($PSCmdlet.ShouldProcess("$DriveLetter`:", "Run ReTrim")) {
        Optimize-Volume -DriveLetter $DriveLetter -ReTrim -ErrorAction Stop | Out-Null
    }
}
