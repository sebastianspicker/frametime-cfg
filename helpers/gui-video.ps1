# ==============================================================================
#  helpers/gui-video.ps1  -  Video panel functions and event handlers
#  Loaded by helpers/gui-panels.ps1 after the shared GUI helpers are defined.
# ==============================================================================

# ==============================================================================
# VIDEO
# ==============================================================================
$Script:VideoTxtPath = $null
$Script:VideoSteamPath = $null

function Test-CurrentVideoTxtPathTrusted {
    if (-not $Script:VideoTxtPath) { return $false }
    $steamPath = if ($Script:VideoSteamPath) { $Script:VideoSteamPath } else { Get-SteamPath }
    return (Test-TrustedVideoTxtPath -Path $Script:VideoTxtPath -SteamPath $steamPath)
}

function Load-Video {
    # Populate tier picker
    if ((El "VideoTierPicker").Items.Count -eq 0) {
        foreach ($t in @("Auto (vendor heuristic)","HIGH","MID","LOW")) { (El "VideoTierPicker").Items.Add($t) | Out-Null }
        (El "VideoTierPicker").SelectedIndex = 0
    }

    $steamPath = Get-SteamPath
    $vtxt = if ($steamPath) {
        Get-ChildItem "$steamPath\userdata\*\730\local\cfg\video.txt" -ErrorAction SilentlyContinue |
            Where-Object { Test-TrustedVideoTxtPath -Path $_.FullName -SteamPath $steamPath } |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }

    if ($vtxt) {
        $Script:VideoTxtPath = $vtxt.FullName
        $Script:VideoSteamPath = $steamPath
        (El "VideoTxtPath").Text = $vtxt.FullName
        (El "BtnVideoWrite").IsEnabled = $true
        (El "BtnVideoWriteFooter").IsEnabled = $true
    } else {
        $Script:VideoTxtPath = $null
        $Script:VideoSteamPath = $null
        (El "VideoTxtPath").Text = "video.txt not found - launch CS2 once to generate it"
        (El "BtnVideoWrite").IsEnabled = $false
        (El "BtnVideoWriteFooter").IsEnabled = $false
        return
    }

    Refresh-VideoGrid
}

# Single source of truth for video tier presets (V=value, N=note for display)
$Script:VideoPresets = @{
    "HIGH" = @{
        "setting.msaa_samples"              = @{ V="4";  N="4x MSAA; benchmark against 2x or CMAA2" }
        "setting.mat_vsync"                 = @{ V="0";  N="VSync off in the repository preset" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen in the repository preset" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On; compare with a repeatable capture" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR disabled" }
        "setting.shaderquality"             = @{ V="1";  N="High shader quality" }
        "setting.r_texturefilteringquality" = @{ V="5";  N="AF16x; repository high-tier image-quality default" }
        "setting.r_csgo_cmaa_enable"        = @{ V="0";  N="Off - MSAA handles AA" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off; compare image and frame-time results if enabled" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance - suite default; compare visually" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low particle detail" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows enabled" }
        "setting.videocfg_dynamic_shadows"  = @{ V="1";  N="Dynamic Shadows All" }
    }
    "MID" = @{
        "setting.msaa_samples"              = @{ V="4";  N="4x - or 2x if FPS budget is tight" }
        "setting.mat_vsync"                 = @{ V="0";  N="Off - fixed-refresh default" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF - native clarity default" }
        "setting.shaderquality"             = @{ V="0";  N="Low shader quality" }
        "setting.r_texturefilteringquality" = @{ V="5";  N="AF16x" }
        "setting.r_csgo_cmaa_enable"        = @{ V="0";  N="Off - MSAA handles AA" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance - suite default" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON" }
        "setting.videocfg_dynamic_shadows"  = @{ V="1";  N="Dynamic Shadows All" }
    }
    "LOW" = @{
        "setting.msaa_samples"              = @{ V="0";  N="MSAA disabled; CMAA2 enabled separately" }
        "setting.mat_vsync"                 = @{ V="0";  N="Off - fixed-refresh default" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen in the repository preset" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF - lower resolution first" }
        "setting.shaderquality"             = @{ V="0";  N="Low" }
        "setting.r_texturefilteringquality" = @{ V="0";  N="Bilinear filtering" }
        "setting.r_csgo_cmaa_enable"        = @{ V="1";  N="CMAA2 on; compare against MSAA with the same workload" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows enabled" }
        "setting.videocfg_dynamic_shadows"  = @{ V="1";  N="Dynamic Shadows All" }
    }
}

function Get-ResolvedVideoTier {
    # Auto selects HIGH when an NVIDIA driver is detected and MID otherwise.
    # This is a vendor heuristic, not hardware-performance detection.
    param([string]$TierSel)
    if ($TierSel -like "Auto*") {
        $nv = Get-NvidiaDriverVersion
        if ($nv) { return "HIGH" }
        return "MID"
    }
    return $TierSel
}

function Refresh-VideoGrid {
    $tier = Get-ResolvedVideoTier (El "VideoTierPicker").SelectedItem
    $recommended = $Script:VideoPresets[$tier]

    $current = @{}
    if ((Test-CurrentVideoTxtPathTrusted) -and (Test-Path $Script:VideoTxtPath)) {
        Get-Content $Script:VideoTxtPath | ForEach-Object {
            if ($_ -match '^\s*"([^"]+)"\s+"([^"]*)"') { $current[$Matches[1]] = $Matches[2] }
        }
    }

    $rows = foreach ($kv in $recommended.GetEnumerator() | Sort-Object Key) {
        $cur  = $current[$kv.Key]
        $rec  = $kv.Value.V
        $note = $kv.Value.N
        $st   = if ($null -eq $cur) { "-  Missing" } elseif ($cur -eq $rec) { "OK" } else { "Differs" }
        $sc   = if ($st -match "OK") { Get-GuiSemanticBrush "Success" "#22C55E" } elseif ($st -match "Missing") { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }
        [PSCustomObject]@{
            Setting     = $kv.Key -replace "^setting\.",""
            YourValue   = if ($null -eq $cur) { "(not set)" } else { $cur }
            Recommended = $rec
            StatusLabel = $st
            StatusColor = $sc
            Notes       = $note
        }
    }

    (El "VideoGrid").ItemsSource = $rows
    $diffs = @($rows | Where-Object { $_.StatusLabel -notmatch "OK" }).Count
    (El "VideoSummary").Text = "$diffs setting(s) differ from the $tier preset"
}

(El "VideoTierPicker").Add_SelectionChanged({ if ((El "VideoTierPicker").SelectedItem) { Refresh-VideoGrid } })

$writeVideo = {
    if (-not $Script:VideoTxtPath) { [System.Windows.MessageBox]::Show("video.txt not found.","Write"); return }
    if (-not (Test-CurrentVideoTxtPathTrusted)) {
        [System.Windows.MessageBox]::Show("video.txt path is outside the trusted Steam userdata tree.","Write","OK","Error")
        return
    }

    $tier = Get-ResolvedVideoTier (El "VideoTierPicker").SelectedItem

    # Derive values-only hashtable from shared presets
    $managed = @{}
    foreach ($kv in $Script:VideoPresets[$tier].GetEnumerator()) { $managed[$kv.Key] = $kv.Value.V }

    # Read existing file - preserve unmanaged keys (resolution, Hz, etc.)
    $existing = [System.Collections.Generic.Dictionary[string,string]]::new([StringComparer]::OrdinalIgnoreCase)
    if (Test-Path $Script:VideoTxtPath) {
        Get-Content $Script:VideoTxtPath | ForEach-Object {
            if ($_ -match '^\s*"([^"]+)"\s+"([^"]*)"') { $existing[$Matches[1]] = $Matches[2] }
        }
    }

    # Merge: apply managed overrides onto existing keys
    foreach ($kv in $managed.GetEnumerator()) { $existing[$kv.Key] = $kv.Value }

    $summary = ($managed.Keys | ForEach-Object { "$($_ -replace '^setting\.',''): $($managed[$_])" }) -join "`n"
    $r = [System.Windows.MessageBox]::Show(
        "Write the $tier repository preset to video.txt?`n`nOriginal -> video.txt.bak`n`nSettings:`n$summary",
        "Confirm Write","YesNo","Question")
    if ($r -ne "Yes") { return }

    try {
        $bakPath = "$Script:VideoTxtPath.bak"
        # Only create backup if one doesn't already exist - preserve the original
        $bakMade = $false
        if ((Test-Path $Script:VideoTxtPath) -and -not (Test-Path $bakPath)) {
            Copy-Item $Script:VideoTxtPath $bakPath -Force
            $bakMade = $true
        }

        $dir = Split-Path $Script:VideoTxtPath
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force -ErrorAction SilentlyContinue | Out-Null }

        $lines = @(
            '"VideoConfig"'
            '{'
            "    // frametime.cfg - $(Get-Date -Format 'yyyy-MM-dd HH:mm')  Tier: $tier"
            "    // Original backed up as video.txt.bak"
            ""
        )
        foreach ($kv in $existing.GetEnumerator() | Sort-Object Key) {
            $lines += "    `"$($kv.Key)`"`t`"$($kv.Value)`""
        }
        $lines += "}"
        # Steam Cloud can set video.txt read-only - clear the flag before writing
        if ((Test-Path $Script:VideoTxtPath) -and (Get-Item $Script:VideoTxtPath).IsReadOnly) {
            try { (Get-Item $Script:VideoTxtPath).IsReadOnly = $false }
            catch {
                [System.Windows.MessageBox]::Show(
                    "video.txt is read-only (Steam Cloud may be syncing).`n`nTry disabling Steam Cloud sync for CS2:`nSteam -> CS2 -> Properties -> General -> Steam Cloud",
                    "Read-Only File", "OK", "Warning")
                return
            }
        }
        [System.IO.File]::WriteAllLines($Script:VideoTxtPath, [string[]]$lines, [System.Text.UTF8Encoding]::new($false))

        $backupMsg = if ($bakMade) { "Original saved as video.txt.bak" } else { "Backup preserved as video.txt.bak (from first run)" }
        [System.Windows.MessageBox]::Show("video.txt written ($tier tier).`n$backupMsg`n`n$Script:VideoTxtPath","Done")
        Load-Video
    } catch { [System.Windows.MessageBox]::Show("Error: $($_.Exception.Message)","Write Failed") }
}
(El "BtnVideoWrite"      ).Add_Click($writeVideo)
(El "BtnVideoWriteFooter").Add_Click($writeVideo)
