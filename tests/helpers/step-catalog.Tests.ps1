# ==============================================================================
#  tests/helpers/step-catalog.Tests.ps1  --  Step catalog integrity
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
    . "$PSScriptRoot/../../helpers/step-catalog.ps1"

    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/../..").Path

    function Get-StepCatalogSourceDeclarations {
        $phaseSources = @(
            [PSCustomObject]@{ Phase = 1; Path = "Setup-Profile.ps1" },
            [PSCustomObject]@{ Phase = 1; Path = "Optimize-SystemBase.ps1" },
            [PSCustomObject]@{ Phase = 1; Path = "Optimize-Hardware.ps1" },
            [PSCustomObject]@{ Phase = 1; Path = "Optimize-RegistryTweaks.ps1" },
            [PSCustomObject]@{ Phase = 1; Path = "Optimize-GameConfig.ps1" },
            [PSCustomObject]@{ Phase = 3; Path = "PostReboot-Setup.ps1" }
        )
        $stepPattern = [regex]'(?s)Write-Section\s+["'']Step\s+(?<Step>\d+)\s+[—-]\s+(?<Title>[^"'']+)["''](?<Body>.*?)(?=Write-Section\s+["'']Step\s+\d+\s+[—-]|\z)'

        foreach ($source in $phaseSources) {
            $fullPath = Join-Path $script:ProjectRoot $source.Path
            $content = Get-Content $fullPath -Raw
            foreach ($match in $stepPattern.Matches($content)) {
                [PSCustomObject]@{
                    Phase = $source.Phase
                    Step  = [int]$match.Groups["Step"].Value
                    Title = $match.Groups["Title"].Value.Trim()
                    Body  = $match.Groups["Body"].Value
                    Path  = $source.Path
                }
            }
        }
    }

    function ConvertTo-StepCatalogToken {
        param([string]$Token)
        $value = $Token.ToLowerInvariant()
        if ($value.Length -gt 4 -and $value.EndsWith("ies")) {
            return "$($value.Substring(0, $value.Length - 3))y"
        }
        if ($value.Length -gt 4 -and $value.EndsWith("es")) {
            return $value.Substring(0, $value.Length - 2)
        }
        if ($value.Length -gt 3 -and $value.EndsWith("s")) {
            return $value.Substring(0, $value.Length - 1)
        }
        return $value
    }

    function Get-StepCatalogTokens {
        param([string]$Text)

        $stopWords = @("and", "the", "for", "with", "plus")
        [regex]::Matches($Text.ToLowerInvariant(), "[a-z0-9]+") |
            ForEach-Object { ConvertTo-StepCatalogToken $_.Value } |
            Where-Object { $_.Length -ge 2 -and $_ -notin $stopWords } |
            Select-Object -Unique
    }

    function Test-StepCatalogTokenInSource {
        param(
            [string]$Token,
            [string[]]$SourceTokens
        )

        $tokensToMatch = @($Token)
        if ($Token -eq "off") { $tokensToMatch += @("disable", "disabled") }
        if ($Token -eq "on") { $tokensToMatch += @("enable", "enabled") }

        foreach ($candidate in $tokensToMatch) {
            foreach ($sourceToken in $SourceTokens) {
                if ($sourceToken -eq $candidate -or
                    $sourceToken.StartsWith($candidate) -or
                    $candidate.StartsWith($sourceToken)) {
                    return $true
                }
            }
        }
        return $false
    }

    $script:StepCatalogSourceDeclarations = @(Get-StepCatalogSourceDeclarations)
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "StepCatalog schema" {

    It "contains the required fields on every entry" {
        $requiredFields = @("Phase", "Step", "Category", "Title", "Tier", "Risk", "Depth", "CheckOnly", "Reboot")

        foreach ($entry in $SCRIPT:StepCatalog) {
            foreach ($field in $requiredFields) {
                $entry.PSObject.Properties.Name | Should -Contain $field -Because "$($entry.Title) must define $field"
            }
        }
    }

    It "uses unique phase-step identifiers" {
        $ids = @($SCRIPT:StepCatalog | ForEach-Object { "P$($_.Phase):$($_.Step)" })
        @($ids | Select-Object -Unique).Count | Should -Be $ids.Count
    }

    It "only uses allowed tier, risk, and depth values" {
        $validTiers = @(1, 2, 3)
        $validRisks = @("SAFE", "MODERATE", "AGGRESSIVE", "CRITICAL")
        $validDepths = @("SETUP", "CHECK", "FILESYSTEM", "REGISTRY", "DRIVER", "BOOT", "APP", "SERVICE", "NETWORK")

        foreach ($entry in $SCRIPT:StepCatalog) {
            $entry.Tier | Should -BeIn $validTiers -Because "$($entry.Title) has invalid tier"
            $entry.Risk | Should -BeIn $validRisks -Because "$($entry.Title) has invalid risk"
            $entry.Depth | Should -BeIn $validDepths -Because "$($entry.Title) has invalid depth"
        }
    }

    It "keeps CheckOnly and Reboot as booleans" {
        foreach ($entry in $SCRIPT:StepCatalog) {
            $entry.CheckOnly | Should -BeOfType [bool]
            $entry.Reboot    | Should -BeOfType [bool]
        }
    }
}

Describe "StepCatalog source drift" {

    It "declares the same phase-step identifiers as the represented phase scripts" {
        $sourceIds = @(
            $script:StepCatalogSourceDeclarations |
                ForEach-Object { "P$($_.Phase):$($_.Step)" } |
                Sort-Object -Unique
        )
        $catalogIds = @(
            $SCRIPT:StepCatalog |
                ForEach-Object { "P$($_.Phase):$($_.Step)" } |
                Sort-Object -Unique
        )

        $diff = @(Compare-Object -ReferenceObject $sourceIds -DifferenceObject $catalogIds)

        $diff | Should -BeNullOrEmpty -Because "the GUI catalog must not miss or invent represented phase steps"
    }

    It "keeps catalog titles backed by source step declarations" {
        $sourceTextById = @{}
        foreach ($declaration in $script:StepCatalogSourceDeclarations) {
            $id = "P$($declaration.Phase):$($declaration.Step)"
            if (-not $sourceTextById.ContainsKey($id)) {
                $sourceTextById[$id] = [System.Collections.Generic.List[string]]::new()
            }
            $sourceTextById[$id].Add("$($declaration.Title)`n$($declaration.Body)") | Out-Null
        }

        foreach ($entry in $SCRIPT:StepCatalog) {
            $id = "P$($entry.Phase):$($entry.Step)"
            $sourceTextById.ContainsKey($id) | Should -BeTrue -Because "$id must have a source step declaration"

            $sourceTokens = @(Get-StepCatalogTokens (@($sourceTextById[$id]) -join "`n"))
            $titleTokens = @(Get-StepCatalogTokens $entry.Title)
            $missingTokens = @(
                $titleTokens |
                    Where-Object { -not (Test-StepCatalogTokenInSource -Token $_ -SourceTokens $sourceTokens) }
            )

            $missingTokens | Should -BeNullOrEmpty -Because "$id catalog title '$($entry.Title)' must describe the source step"
        }
    }
}
