BeforeAll {
    Set-StrictMode -Version Latest

    $script:RepoRoot = Split-Path $PSScriptRoot -Parent
    $script:ProductPath = Join-Path $script:RepoRoot "PRODUCT.md"
    $script:DesignPath = Join-Path $script:RepoRoot "DESIGN.md"
    $script:XamlPath = Join-Path $script:RepoRoot "ui/CS2-Optimize-GUI.xaml"
    $script:GuiScriptPath = Join-Path $script:RepoRoot "CS2-Optimize-GUI.ps1"
    $script:GuiPanelsPath = Join-Path $script:RepoRoot "helpers/gui-panels.ps1"

function script:Get-DesignColor {
    param(
        [Parameter(Mandatory)][string]$Source,
        [Parameter(Mandatory)][string]$Name
    )

    $escapedName = [regex]::Escape($Name)
    $match = [regex]::Match($Source, "(?m)^  ${escapedName}: `"(#[0-9A-Fa-f]{6})`"$")
    if (-not $match.Success) {
        throw "Design color '$Name' is missing from DESIGN.md frontmatter."
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
    $script:XamlDocument = [xml]$script:XamlSource
}

Describe "GUI product and design contracts" {

    It "defines the product register and required strategic sections" {
        $script:ProductSource | Should -Match '(?ms)^## Register\s+product\s*$'
        foreach ($heading in @(
                "Users",
                "Product Purpose",
                "Brand Personality",
                "Anti-references",
                "Design Principles",
                "Accessibility & Inclusion")) {
            $script:ProductSource | Should -Match "(?m)^## $([regex]::Escape($heading))$"
        }
    }

    It "uses the required DESIGN.md section order" {
        $positions = foreach ($heading in @(
                "Overview",
                "Colors",
                "Typography",
                "Elevation",
                "Components",
                "Do's and Don'ts")) {
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

    It "records the selected desktop accessibility and anti-slop constraints" {
        $script:ProductSource | Should -Match 'WCAG 2\.2 AA-equivalent'
        $script:ProductSource | Should -Match '960 by 540 effective pixels'
        $script:DesignSource | Should -Match 'The Match Engineer'
        $script:DesignSource | Should -Match 'RGB or neon gaming-launcher styling'
        $script:DesignSource | Should -Match 'generic SaaS card grids'
    }

    It "loads a maintainable external XAML layout with native window chrome" {
        $script:GuiScriptSource | Should -Match 'ui\\CS2-Optimize-GUI\.xaml'
        $script:GuiScriptSource | Should -Not -Match '(?m)^\[xml\]\$XAML = @'
        $script:XamlSource | Should -Match 'WindowStyle="SingleBorderWindow"'
        $script:XamlSource | Should -Not -Match 'x:Name="TitleBar"'
        $script:XamlSource | Should -Not -Match 'x:Key="WinBtn'
    }

    It "keeps every literal GUI element reference backed by a named XAML element" {
        $namespaceManager = [System.Xml.XmlNamespaceManager]::new($script:XamlDocument.NameTable)
        $namespaceManager.AddNamespace("wpf", "http://schemas.microsoft.com/winfx/2006/xaml/presentation")
        $namespaceManager.AddNamespace("x", "http://schemas.microsoft.com/winfx/2006/xaml")
        $names = @($script:XamlDocument.SelectNodes("//*[@x:Name]", $namespaceManager) |
            ForEach-Object { $_.GetAttribute("Name", "http://schemas.microsoft.com/winfx/2006/xaml") })

        $references = @([regex]::Matches(($script:GuiScriptSource + $script:GuiPanelsSource), '\(El\s+"([^"]+)"\)') |
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

    It "keeps verification, latency, and recovery cleanup outside success-only callbacks" {
        $script:GuiPanelsSource | Should -Match '(?s)function Start-InlineVerify.*?-OnFinally'
        $script:GuiPanelsSource | Should -Match '(?s)function Start-LatencyDiagnostic.*?-OnFinally'
        $script:GuiPanelsSource | Should -Match '(?s)Restoring all recorded changes.*?Invoke-Async.*?-OnFinally'
        $script:GuiPanelsSource | Should -Match '(?s)Restoring \$stepTitle.*?Invoke-Async.*?-OnFinally'
        $script:GuiScriptSource | Should -Match '(?s)Add_Closing.*?CriticalOperation.*?Cancel = \$true'
    }
}
