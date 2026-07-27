BeforeAll {
    Set-StrictMode -Version Latest

    $script:RepoRoot = Split-Path $PSScriptRoot -Parent
    $script:ProductPath = Join-Path $script:RepoRoot "docs/product.md"
    $script:DesignPath = Join-Path $script:RepoRoot "docs/ui-design.md"
    $script:XamlPath = Join-Path $script:RepoRoot "ui/frametime-gui.xaml"
    $script:GuiScriptPath = Join-Path $script:RepoRoot "frametime-gui.ps1"
    $script:GuiPanelsPath = Join-Path $script:RepoRoot "helpers/gui-panels.ps1"
    $script:GuiNetworkPath = Join-Path $script:RepoRoot "helpers/gui-network.ps1"
    $script:GuiVideoPath = Join-Path $script:RepoRoot "helpers/gui-video.ps1"

function script:Get-DesignColor {
    param(
        [Parameter(Mandatory)][string]$Source,
        [Parameter(Mandatory)][string]$Name
    )

    $escapedName = [regex]::Escape($Name)
    $match = [regex]::Match($Source, "(?m)^  ${escapedName}: `"(#[0-9A-Fa-f]{6})`"$")
    if (-not $match.Success) {
        throw "Design color '$Name' is missing from docs/ui-design.md frontmatter."
    }
    return $match.Groups[1].Value
}

function script:Get-RelativeLuminance {
    param([Parameter(Mandatory)][string]$Hex)

    $channels = foreach ($offset in @(1, 3, 5)) {
        $value = [Convert]::ToInt32($Hex.Substring($offset, 2), 16) / 255.0
        if ($value -le 0.04045) {
            $value / 12.92
        } else {
            [Math]::Pow((($value + 0.055) / 1.055), 2.4)
        }
    }

    return (0.2126 * $channels[0]) + (0.7152 * $channels[1]) + (0.0722 * $channels[2])
}

function script:Get-ContrastRatio {
    param(
        [Parameter(Mandatory)][string]$Foreground,
        [Parameter(Mandatory)][string]$Background
    )

    $foregroundLuminance = Get-RelativeLuminance -Hex $Foreground
    $backgroundLuminance = Get-RelativeLuminance -Hex $Background
    $lighter = [Math]::Max($foregroundLuminance, $backgroundLuminance)
    $darker = [Math]::Min($foregroundLuminance, $backgroundLuminance)
    return ($lighter + 0.05) / ($darker + 0.05)
}

function script:Get-NormalizedText {
    param([Parameter(Mandatory)][string]$Path)

    return (Get-Content $Path -Raw) -replace "`r`n?", "`n"
}

    $script:ProductSource = Get-NormalizedText -Path $script:ProductPath
    $script:DesignSource = Get-NormalizedText -Path $script:DesignPath
    $script:XamlSource = Get-NormalizedText -Path $script:XamlPath
    $script:GuiScriptSource = Get-NormalizedText -Path $script:GuiScriptPath
    $script:GuiPanelsSource = Get-NormalizedText -Path $script:GuiPanelsPath
    $script:GuiNetworkSource = Get-NormalizedText -Path $script:GuiNetworkPath
    $script:GuiVideoSource = Get-NormalizedText -Path $script:GuiVideoPath
    $script:XamlDocument = [xml]$script:XamlSource
}

Describe "GUI product and design contracts" {

    It "defines the product purpose and operational constraints" {
        foreach ($heading in @(
                "Purpose",
                "Users",
                "Operational model",
                "Interface constraints",
                "Accessibility requirements",
                "Non-goals")) {
            $script:ProductSource | Should -Match "(?m)^## $([regex]::Escape($heading))$"
        }
    }

    It "uses the required UI design section order" {
        $positions = foreach ($heading in @(
                "Overview",
                "Colors",
                "Typography",
                "Elevation",
                "Components",
                "Usage rules")) {
            $script:DesignSource.IndexOf("## $heading", [StringComparison]::Ordinal)
        }

        $positions | Should -Not -Contain -1
        for ($index = 1; $index -lt $positions.Count; $index++) {
            $positions[$index] | Should -BeGreaterThan $positions[$index - 1]
        }
    }

    It "keeps normal text tokens at AA contrast on every dark surface" {
        $textColors = @(
            Get-DesignColor -Source $script:DesignSource -Name "text-primary"
            Get-DesignColor -Source $script:DesignSource -Name "text-secondary"
            Get-DesignColor -Source $script:DesignSource -Name "text-muted"
        )
        $surfaces = @(
            Get-DesignColor -Source $script:DesignSource -Name "app-background"
            Get-DesignColor -Source $script:DesignSource -Name "surface"
            Get-DesignColor -Source $script:DesignSource -Name "surface-raised"
        )

        foreach ($textColor in $textColors) {
            foreach ($surface in $surfaces) {
                Get-ContrastRatio -Foreground $textColor -Background $surface |
                    Should -BeGreaterOrEqual 4.5 -Because "$textColor on $surface is normal operational text"
            }
        }
    }

    It "keeps primary button text at AA contrast in every interaction state" {
        $buttonText = Get-DesignColor -Source $script:DesignSource -Name "app-background"
        foreach ($state in @("accent", "accent-hover", "accent-pressed")) {
            $fill = Get-DesignColor -Source $script:DesignSource -Name $state
            Get-ContrastRatio -Foreground $buttonText -Background $fill |
                Should -BeGreaterOrEqual 4.5 -Because "$state is used behind normal-size button text"
        }
    }

    It "records the selected desktop accessibility and presentation constraints" {
        $script:ProductSource | Should -Match 'Windows UI\s+Automation labels'
        $script:ProductSource | Should -Match '960 by 540 effective pixels'
        $script:ProductSource | Should -Match 'manual validation on Windows before compatibility'
        $script:DesignSource | Should -Match 'RGB or neon styling'
        $script:DesignSource | Should -Match 'decorative metrics'
    }

    It "uses the frametime.cfg wordmark and amber frame trace without a slogan" {
        $script:ProductSource | Should -Match 'frametime\.cfg provides a Windows PowerShell workflow'
        $script:XamlSource | Should -Match 'Text="frametime\.cfg"'
        $script:XamlSource | Should -Match 'Text="WINDOWS CONFIGURATION STATUS"'
        $script:XamlSource | Should -Not -Match 'FLAT FRAMETIMES\. CLEAN ROUNDS\.'
        $script:XamlSource | Should -Match '<Polyline[^>]+Points="0,7 34,7 39,3 44,10 49,7 144,7"'
        Get-DesignColor -Source $script:DesignSource -Name "accent" | Should -Be '#D6A43B'
        Get-DesignColor -Source $script:DesignSource -Name "accent-hover" | Should -Be '#E2B24B'
        Get-DesignColor -Source $script:DesignSource -Name "accent-pressed" | Should -Be '#B98527'
    }

    It "loads a maintainable external XAML layout with native window chrome" {
        $script:GuiScriptSource | Should -Match 'ui\\frametime-gui\.xaml'
        $script:GuiScriptSource | Should -Not -Match '(?m)^\[xml\]\$XAML = @'
        $script:XamlSource | Should -Match 'WindowStyle="SingleBorderWindow"'
        $script:XamlSource | Should -Not -Match 'x:Name="TitleBar"'
        $script:XamlSource | Should -Not -Match 'x:Key="WinBtn'
    }

    It "loads the network and video controllers only through the GUI panel controller" {
        Test-Path -LiteralPath $script:GuiNetworkPath -PathType Leaf | Should -BeTrue
        Test-Path -LiteralPath $script:GuiVideoPath -PathType Leaf | Should -BeTrue
        $script:GuiPanelsSource | Should -Match '\. "\$Script:Root\\helpers\\gui-network\.ps1"'
        $script:GuiPanelsSource | Should -Match '\. "\$Script:Root\\helpers\\gui-video\.ps1"'
        (Get-NormalizedText -Path (Join-Path $script:RepoRoot "helpers.ps1")) |
            Should -Not -Match 'gui-(network|video)\.ps1'
    }

    It "keeps every literal GUI element reference backed by a named XAML element" {
        $namespaceManager = [System.Xml.XmlNamespaceManager]::new($script:XamlDocument.NameTable)
        $namespaceManager.AddNamespace("wpf", "http://schemas.microsoft.com/winfx/2006/xaml/presentation")
        $namespaceManager.AddNamespace("x", "http://schemas.microsoft.com/winfx/2006/xaml")
        $names = @($script:XamlDocument.SelectNodes("//*[@x:Name]", $namespaceManager) |
            ForEach-Object { $_.GetAttribute("Name", "http://schemas.microsoft.com/winfx/2006/xaml") })

        $guiControllerSource = $script:GuiScriptSource + $script:GuiPanelsSource +
            $script:GuiNetworkSource + $script:GuiVideoSource
        $references = @([regex]::Matches($guiControllerSource, '\(El\s+"([^"]+)"\)') |
            ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
        foreach ($reference in $references) {
            $names | Should -Contain $reference -Because "literal El references must resolve after XAML extraction"
        }
    }

    It "defines keyboard focus, live status, labels, and High Contrast adaptation" {
        $script:XamlSource | Should -Match 'x:Key="KeyboardFocusVisual"'
        $script:XamlSource | Should -Match 'AutomationProperties\.LiveSetting="Polite"'
        $script:XamlSource | Should -Match '<Label[^>]+Target='
        $script:GuiScriptSource | Should -Match 'SystemParameters\]::HighContrast'
        $script:GuiScriptSource | Should -Match 'add_StaticPropertyChanged'
    }

    It "exposes cancellation for the long-running assessment operation" {
        $script:XamlSource | Should -Match 'x:Name="BtnCancelAnalysis"'
        $script:GuiScriptSource | Should -Match 'function Stop-AsyncOperation'
        (Get-Content (Join-Path $script:RepoRoot "helpers/gui-panels.ps1") -Raw) |
            Should -Match 'Stop-AsyncOperation -Operation \$Script:AnalysisOperation'
    }

    It "initializes event-handler state before StrictMode reads it" {
        foreach ($stateName in @(
                "AnalysisInFlight",
                "AnalysisOperation",
                "VerifyInFlight",
                "LatencyInFlight",
                "NetworkRegionPickerUpdating",
                "CriticalOperation")) {
            $script:GuiScriptSource | Should -Match "(?m)^\`$Script:$stateName\s*=\s*\`$(?:false|null)$"
        }
    }

    It "shows legacy handoff startup failures even though the launcher is hidden" {
        $script:GuiScriptSource | Should -Match '(?s)try\s*\{\s*Assert-NoLegacyPhaseHandoff\s*\}\s*catch\s*\{.*?MessageBox\]::Show\(.*?Startup blocked.*?throw'
    }

    It "keeps verification, latency, and recovery cleanup outside success-only callbacks" {
        $script:GuiPanelsSource | Should -Match '(?s)function Start-InlineVerify.*?-OnFinally'
        $script:GuiNetworkSource | Should -Match '(?s)function Start-LatencyDiagnostic.*?-OnFinally'
        $script:GuiPanelsSource | Should -Match '(?s)Restoring all recorded changes.*?Invoke-Async.*?-OnFinally'
        $script:GuiPanelsSource | Should -Match '(?s)Restoring \$stepTitle.*?Invoke-Async.*?-OnFinally'
        $script:GuiScriptSource | Should -Match '(?s)Add_Closing.*?CriticalOperation.*?Cancel = \$true'
    }
}
