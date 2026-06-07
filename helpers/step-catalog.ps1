# ==============================================================================
#  helpers/step-catalog.ps1  —  Step metadata table for GUI display
#  Each entry mirrors the Invoke-TieredStep call in the phase scripts.
#  Does NOT contain executable logic — pure data for the Optimize panel.
#  When a phase step changes, update this table with the phase script and docs
#  so the GUI does not drift from the actual terminal workflow.
# ==============================================================================

$SCRIPT:StepCatalog = @(
    # ── Phase 1 ───────────────────────────────────────────────────────────────
    [PSCustomObject]@{ Phase=1; Step=1;  Category="System"; Title="Configuration";  Tier=1; Risk="SAFE"; Depth="SETUP"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=2;  Category="Hardware"; Title="XMP/EXPO Check";              Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=3;  Category="GPU";      Title="Clear Shader Cache";           Tier=1; Risk="SAFE";       Depth="FILESYSTEM"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=4;  Category="Display";  Title="Fullscreen Optimizations";     Tier=1; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=5;  Category="GPU";      Title="NVIDIA Driver Version";        Tier=2; Risk="AGGRESSIVE"; Depth="DRIVER"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=6;  Category="System";   Title="CS2 Power Plan";               Tier=1; Risk="MODERATE";   Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=7;  Category="GPU";      Title="HAGS";                         Tier=2; Risk="MODERATE";   Depth="REGISTRY"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=1; Step=8;  Category="System";   Title="Pagefile";                     Tier=2; Risk="MODERATE";   Depth="REGISTRY"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=1; Step=9;  Category="GPU";      Title="Resizable BAR";                Tier=2; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$true  }
    [PSCustomObject]@{ Phase=1; Step=10; Category="System";   Title="Dynamic Tick";                 Tier=3; Risk="MODERATE";   Depth="BOOT"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=1; Step=11; Category="Display";  Title="Disable MPO";                  Tier=3; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=1; Step=12; Category="System";   Title="Game Mode";                    Tier=3; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=13; Category="System";   Title="Gaming Debloat";               Tier=2; Risk="MODERATE";   Depth="APP"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=14; Category="System";   Title="Autostart Cleanup";            Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=15; Category="System";   Title="Windows Update Blocker";       Tier=3; Risk="CRITICAL";   Depth="SERVICE"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=16; Category="Network";  Title="NIC Latency Stack";            Tier=2; Risk="MODERATE";   Depth="NETWORK"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=1; Step=17; Category="Benchmark";Title="Baseline Benchmark";           Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=18; Category="GPU";      Title="GPU Driver Clean (prep)";      Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=19; Category="GPU";      Title="NVIDIA Driver Download";       Tier=1; Risk="SAFE";       Depth="FILESYSTEM"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=20; Category="GPU";      Title="NVIDIA Profile (prep)";        Tier=3; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=21; Category="Hardware"; Title="MSI Interrupts (prep)";        Tier=2; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=22; Category="Network";  Title="NIC Interrupt Affinity (prep)";Tier=3; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=23; Category="System";   Title="Disable Fast Startup";         Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=24; Category="Hardware"; Title="Dual-Channel RAM";             Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=25; Category="Network";  Title="Disable Nagle";                Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=26; Category="Display";  Title="GameConfigStore FSE";          Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=27; Category="System";   Title="MMCSS + Gaming Priority";      Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=28; Category="System";   Title="Timer Resolution";             Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=29; Category="Input";    Title="Mouse Acceleration Off";       Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=30; Category="GPU";      Title="CS2 GPU Preference";           Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=31; Category="System";   Title="Disable Game DVR";             Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=32; Category="System";   Title="Disable Overlays";             Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=33; Category="Audio";    Title="Audio Optimization";           Tier=2; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=34; Category="CS2";      Title="Autoexec (73 CVars)";          Tier=2; Risk="SAFE";       Depth="APP"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=35; Category="System";   Title="Chipset Driver Check";         Tier=2; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=36; Category="Display";  Title="Visual Effects Performance";   Tier=3; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=37; Category="System";   Title="Disable SysMain + Windows Search"; Tier=3; Risk="MODERATE"; Depth="SERVICE"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=1; Step=38; Category="System";   Title="Activate Safe Mode";           Tier=1; Risk="MODERATE";   Depth="BOOT"; CheckOnly=$false; Reboot=$true  }
    # ── Phase 3 ───────────────────────────────────────────────────────────────
    [PSCustomObject]@{ Phase=3; Step=1;  Category="GPU";      Title="Install NVIDIA Driver";        Tier=1; Risk="MODERATE";   Depth="DRIVER"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=3; Step=2;  Category="GPU";      Title="MSI Interrupts";               Tier=2; Risk="MODERATE";   Depth="REGISTRY"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=3; Step=3;  Category="Network";  Title="NIC Interrupt Affinity";       Tier=3; Risk="MODERATE";   Depth="REGISTRY"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=3; Step=4;  Category="GPU";      Title="NVIDIA DRS Profile";           Tier=3; Risk="SAFE";       Depth="DRIVER"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=5;  Category="CS2";      Title="FPS Cap Info";                  Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=6;  Category="CS2";      Title="Launch Options + Video";        Tier=2; Risk="SAFE";       Depth="APP"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=7;  Category="Security";  Title="VBS / Core Isolation";           Tier=2; Risk="MODERATE";   Depth="REGISTRY"; CheckOnly=$false; Reboot=$true  }
    [PSCustomObject]@{ Phase=3; Step=8;  Category="GPU";      Title="AMD GPU Settings";              Tier=2; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=9;  Category="Network";  Title="DNS Configuration";             Tier=3; Risk="SAFE";       Depth="NETWORK"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=10; Category="CPU";      Title="Process Priority + X3D CCD";    Tier=3; Risk="SAFE";       Depth="REGISTRY"; CheckOnly=$false; Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=11; Category="System";   Title="VRAM Leak Awareness";           Tier=2; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=12; Category="System";   Title="Knowledge Summary + Checklist"; Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
    [PSCustomObject]@{ Phase=3; Step=13; Category="Benchmark";Title="Final Benchmark + FPS Cap";     Tier=1; Risk="SAFE";       Depth="CHECK"; CheckOnly=$true;  Reboot=$false }
)
