# ==============================================================================
#  tests/helpers/step-catalog.Tests.ps1  --  Step catalog integrity
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
    . "$PSScriptRoot/../../helpers/step-catalog.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "StepCatalog schema" {

    It "contains the required fields on every entry" {
        $requiredFields = @("Phase", "Step", "Category", "Title", "Tier", "Risk", "Depth", "EstKey", "CheckOnly", "Reboot")

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
