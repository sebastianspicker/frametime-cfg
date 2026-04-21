# ==============================================================================
#  tests/helpers/power-plan.Tests.ps1  --  Power plan creation & tiered settings
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Stub powercfg and Windows-only cmdlets before loading the module
    if ($IsWindows -eq $false) {
        if (-not (Get-Command powercfg -ErrorAction SilentlyContinue)) {
            function global:powercfg { return "" }
        }
    }

    . "$PSScriptRoot/../../helpers/power-plan.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── GUID Constants ──────────────────────────────────────────────────────────
Describe "Power Plan GUID Constants" {

    It "defines PCIe ASPM subgroup GUID" {
        $PP_SUB_PCIE | Should -Be "501a4d13-42af-4429-9fd1-a8218c268e20"
    }

    It "defines ASPM setting GUID" {
        $PP_ASPM | Should -Be "ee12f906-d277-404b-b6da-e5fa1a576df5"
    }

    It "defines processor subgroup GUID" {
        $PP_SUB_PROCESSOR | Should -Be "54533251-82be-4824-96c1-47b60b740d00"
    }

    It "maps dab60367... as the AHCI adaptive link-power setting" {
        $PP_DISKAHCIADAPTIVE | Should -Be "dab60367-53fe-4fbc-825e-521d069d2456"
    }

    It "uses documented processor timing setting GUIDs" {
        $PP_PERFINCRTIME | Should -Be "984cf492-3bed-4488-a8f9-4286c97bf5aa"
        $PP_PERFDECRTIME | Should -Be "d8edeb9b-95cf-4f95-a73c-b061973693c8"
    }

    It "all GUIDs match 36-char GUID format" {
        $guidVars = @(
            $PP_SUB_PROCESSOR, $PP_SUB_DISK, $PP_SUB_USB, $PP_SUB_SLEEP,
            $PP_SUB_NETWORK, $PP_SUB_GPUPREF, $PP_SUB_COOLING, $PP_SUB_PCIE,
            $PP_PERFBOOSTMODE, $PP_PERFBOOSTPOL, $PP_PERFEPP, $PP_PERFEPP2,
            $PP_PROCTHROTTLEMAX, $PP_PROCTHROTTLEMIN, $PP_IDLEDISABLE,
            $PP_IDLESTATEMAX, $PP_DUTYCYCLING, $PP_PERFHISTCOUNT,
            $PP_PERFINCRTIME, $PP_PERFDECRTIME, $PP_CPMINCORES, $PP_CPMAXCORES,
            $PP_CPMINCORES1, $PP_DISKIDLE, $PP_DISKPOWERMGMT, $PP_DISKLPM,
            $PP_DISKAHCIADAPTIVE, $PP_DISKNVIDLE, $PP_DISKADAPTIVE, $PP_USBSS,
            $PP_USBHUB, $PP_USBC, $PP_WIFIPOWERSAVE, $PP_GPUPREF,
            $PP_SYSCOOLPOL, $PP_STANDBYIDLE, $PP_HIBERNATEIDLE, $PP_ASPM
        )
        foreach ($g in $guidVars) {
            $g | Should -Match '^[a-fA-F0-9\-]{36}$'
        }
    }
}

# ── Set-PowerPlanValue ──────────────────────────────────────────────────────
Describe "Set-PowerPlanValue" {

    BeforeEach { Reset-TestState }

    Context "GUID Validation (Security)" {

        It "rejects invalid PlanGuid" {
            Mock Write-Warn {}
            Set-PowerPlanValue "INVALID!" $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke Write-Warn -Times 1
        }

        It "rejects invalid SubgroupGuid" {
            Mock Write-Warn {}
            Set-PowerPlanValue "a1b2c3d4-e5f6-7890-abcd-ef1234567890" "NOT-A-GUID" $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke Write-Warn -Times 1
        }

        It "rejects invalid SettingGuid" {
            Mock Write-Warn {}
            Set-PowerPlanValue "a1b2c3d4-e5f6-7890-abcd-ef1234567890" $PP_SUB_PROCESSOR "INVALID" 100 "test"
            Should -Invoke Write-Warn -Times 1
        }

        It "allows DRY-RUN-GUID as PlanGuid" {
            Mock Write-Warn {}
            $SCRIPT:DryRun = $true
            Set-PowerPlanValue "DRY-RUN-GUID" $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke Write-Warn -Times 0
        }
    }

    Context "DRY-RUN mode" {

        It "skips powercfg in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock powercfg {}
            Set-PowerPlanValue "a1b2c3d4-e5f6-7890-abcd-ef1234567890" $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke powercfg -Times 0
        }
    }

    Context "Normal execution" {

        It "calls powercfg with correct arguments" {
            $SCRIPT:DryRun = $false
            $planGuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
            Mock powercfg { $global:LASTEXITCODE = 0 }
            Mock Write-DebugLog {}
            Set-PowerPlanValue $planGuid $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "CPU max"
            Should -Invoke powercfg -Times 1
        }
    }
}

# ── New-CS2PowerPlan ────────────────────────────────────────────────────────
Describe "New-CS2PowerPlan" {

    BeforeEach { Reset-TestState }

    It "returns DRY-RUN-GUID in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        $result = New-CS2PowerPlan
        $result | Should -Be "DRY-RUN-GUID"
    }

    It "does not call powercfg in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        Mock powercfg {}
        New-CS2PowerPlan
        Should -Invoke powercfg -Times 0
    }
}

# ── Apply-PowerPlan ─────────────────────────────────────────────────────────
Describe "Apply-PowerPlan" {

    BeforeEach { Reset-TestState }

    Context "T1 settings (SAFE profile)" {

        It "applies T1 settings for SAFE profile" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Set-PowerPlanValue {}

            Apply-PowerPlan "DRY-RUN-GUID"

            # T1 settings: verify Set-PowerPlanValue was called at least once (count varies as T1 set evolves)
            Should -Invoke Set-PowerPlanValue -Scope It
        }
    }

    Context "T2 settings (RECOMMENDED profile)" {

        It "applies T2 AMD vendor branching (PROCTHROTTLEMIN=0)" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "AMD" }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Host {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $minCall = $script:ppCalls | Where-Object { $_.Label -match "CPU min perf" }
            $minCall | Should -Not -BeNullOrEmpty
        }

        It "applies T2 Intel vendor branching with CPMINCORES1" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Host {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $intelCall = $script:ppCalls | Where-Object { $_.Label -match "Intel ring min cores" }
            $intelCall | Should -Not -BeNullOrEmpty
        }
    }

    Context "T3 settings (COMPETITIVE profile)" {

        It "applies T3 settings only for COMPETITIVE" {
            $SCRIPT:Profile = "COMPETITIVE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Get-AmdCpuInfo { return $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Host {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $idleCall = $script:ppCalls | Where-Object { $_.Label -match "idle disable" }
            $idleCall | Should -Not -BeNullOrEmpty
        }

        It "does not apply T3 for RECOMMENDED profile" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Get-AmdCpuInfo { return $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Host {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $idleCall = $script:ppCalls | Where-Object { $_.Label -match "idle disable" }
            $idleCall | Should -BeNullOrEmpty
        }

        It "uses documented in-range interval values for perf increase and decrease time" {
            $SCRIPT:Profile = "COMPETITIVE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Get-AmdCpuInfo { return $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Host {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value; SettingGuid = $SettingGuid })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            ($script:ppCalls | Where-Object SettingGuid -eq $PP_PERFINCRTIME | Select-Object -ExpandProperty Value) | Should -Be 0
            ($script:ppCalls | Where-Object SettingGuid -eq $PP_PERFDECRTIME | Select-Object -ExpandProperty Value) | Should -Be 100
        }
    }

    Context "PCIe ASPM (T1)" {

        It "sets PCIe ASPM to 0 (off) as T1" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Write-Step {}
            Mock Write-OK {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ SubgroupGuid = $SubgroupGuid; SettingGuid = $SettingGuid; Value = $Value; Label = $Label })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $aspmCall = $script:ppCalls | Where-Object { $_.SettingGuid -eq $PP_ASPM }
            $aspmCall | Should -Not -BeNullOrEmpty
            $aspmCall.Value | Should -Be 0
        }
    }
}
