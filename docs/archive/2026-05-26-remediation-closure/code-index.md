# Code Index

Source-file inventory for cleanup, refactor, and audit planning. This file
describes what exists; it does not authorize deletion or refactoring by itself.

Generated: 2026-05-26. Basis: `AGENTS.md`, `README.md`,
`docs/architecture.md`, source scans with `rg --files`, function extraction, and
targeted reference searches. Status words mean:

- **active**: referenced by launchers, phase flow, helpers, docs, or tests.
- **unclear**: present but runtime usage needs stronger proof.
- **generated**: produced artifact or binary asset.
- **deprecated path**: compatibility or retired behavior intentionally retained.

## Coverage Summary

Inspected source areas:

- Root PowerShell and batch launchers.
- Helper modules under `helpers/`.
- CS2 CFG and JSON config files under `cfgs/`.
- Pester tests under `tests/`.
- CI and GitHub repository policy files under `.github/`.
- Maintainer and user documentation under `README.md`, `CONTRIBUTING.md`,
  `CHANGELOG.md`, `AGENTS.md`, and `docs/`.

Not deeply inspected:

- `docs/screenshots/*.png`: binary documentation screenshots; represented as
  generated documentation assets only.
- `testResults.xml` / `test-results.xml`: ignored/generated Pester result
  artifacts when present.

## Root Runtime Files

| File | Language/type | Primary responsibility | Main exports/classes/functions | Runtime role | Direct dependencies worth knowing | Status | Obvious smells |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `START.bat` | Windows batch | Elevated terminal menu for Phase 1, cleanup, FPS cap, logs, verify, progress reset, restore, Safe Mode, and Phase 3. | Batch labels `phase1`, `cleanup`, `fpscap`, `showlog`, `verify`, `resetprogress`, `restore`, `backupsummary`, `safemode`, `phase3`. | entrypoint | `powershell.exe`, `Run-Optimize.ps1`, `Cleanup.ps1`, `FpsCap-Calculator.ps1`, `Verify-Settings.ps1`, `Boot-SafeMode.ps1`, `PostReboot-Setup.ps1`, `config.env.ps1`, `helpers.ps1`. | active | Menu and inline PowerShell are coupled; launcher contract is CI-tested. |
| `START-GUI.bat` | Windows batch | Elevates and launches the WPF dashboard. | No functions. | entrypoint/UI launcher | `powershell.exe`, `CS2-Optimize-GUI.ps1`, UAC `Start-Process -Verb RunAs`. | active | Minimal; launcher target is security-workflow tested. |
| `Run-Optimize.ps1` | PowerShell script | Phase 1 orchestrator; loads config/helpers, initializes state, then dot-sources phase scripts in workflow order. | Script body only; supports `-SmokeTest`. | entrypoint/script | `config.env.ps1`, `helpers.ps1`, `Setup-Profile.ps1`, `Optimize-SystemBase.ps1`, `Optimize-Hardware.ps1`, `Optimize-RegistryTweaks.ps1`, `Optimize-GameConfig.ps1`, backup lock release. | active | Dot-sourced phase scripts execute immediately, so import order is control flow; easy to break with module-style assumptions. |
| `Setup-Profile.ps1` | PowerShell script | Phase 1 Step 1 profile, mode, resume, hardware/FPS inputs, compatibility warning, and state creation. | Script body only. | script/config | `Initialize-Backup`, `Test-SystemCompatibility`, `Load-State`, `Save-SuiteState`, `Complete-Step`, `Get-RamInfo`, `Get-NvidiaDriverVersion`. | active | Backward-compat `mode` derivation remains; empty catches exist and are documented in analyzer settings. |
| `Optimize-SystemBase.ps1` | PowerShell script | Phase 1 Steps 2-9: XMP, shader cache, FSO, NVIDIA driver check, power plan, HAGS, pagefile, ReBAR/SAM checks. | Script body only. | domain logic/script | `Invoke-TieredStep`, `Complete-Step`, `Skip-Step`, `Get-RamInfo`, `Get-CS2InstallPath`, `Set-RegistryValue`, `Backup-PowerPlan`, `Apply-PowerPlan`, `Get-WmiObject` for pagefile. | active | High-risk registry/file deletion path; uses `Get-WmiObject` for PS 5.1 pagefile `.Put()` compatibility; many hardware branches. |
| `Optimize-Hardware.ps1` | PowerShell script | Phase 1 Steps 10-22: timer, MPO, Game Mode, debloat, autostart, Windows Update blocker, NIC stack, baseline benchmark, NVIDIA driver prep. | Script body only. | domain logic/script | `Invoke-TieredStep`, `Set-BootConfig`, `Set-RegistryValue`, `Invoke-GamingDebloat`, `Backup-ServiceState`, `Backup-NicAdapterProperty`, `Backup-QosAndUro`, `Get-LatestNvidiaDriver`, `Invoke-Download`. | active | Large state-changing script with many service/NIC paths; weak-error-handling risk around best-effort service and network commands. |
| `Optimize-RegistryTweaks.ps1` | PowerShell script | Phase 1 Steps 23-33: Fast Startup, dual-channel check, TCP/Nagle, GameConfigStore, scheduler/MMCSS/NTFS, timer resolution, mouse, GPU preference, DVR/overlay/audio. | Script body only. | domain logic/script | `Invoke-TieredStep`, `Set-RegistryValue`, `Backup-RegistryValue`, `Test-DualChannel`, `Get-CS2InstallPath`, `Get-ActiveNicGuid`. | active | Dense registry-tweak sequence; several deprecated/legacy Windows compatibility choices are intentionally modified. |
| `Optimize-GameConfig.ps1` | PowerShell script | Phase 1 Steps 34-38: generated CS2 config, optional CFG deployment, chipset guidance, visual/Defender/Auto HDR, service disables, Safe Mode payload handoff. | Script body only. | domain logic/script | `$CFG_CS2_Autoexec`, `Set-RegistryValue`, `Backup-DefenderExclusions`, `Backup-ServiceState`, `Copy-PhaseRuntimePayload`, `Set-RunOnce`, `bcdedit`. | active | Very large step body; generated CFG text, launch-option guidance, service handling, and reboot handoff are tightly coupled. |
| `SafeMode-DriverClean.ps1` | PowerShell script | Phase 2 Safe Mode flow: clear Safe Mode flag, run native GPU driver removal, register Phase 3. | Script body only; supports `-SmokeTest`. | entrypoint/script | `Load-State`, `Set-RunOnce`, `Set-BootConfig`, `Remove-GpuDriverClean`, `Complete-Step`, `Skip-Step`. | active | Crash-safety state transitions are high-risk; best-effort Phase 3 registration after errors needs careful audit. |
| `PostReboot-Setup.ps1` | PowerShell script | Phase 3 normal-boot flow: clean driver install, MSI/NIC affinity, NVIDIA/AMD settings, DNS, process priority, checklist, benchmark, final restart. | Script body only; supports `-SmokeTest`. | entrypoint/script | `Guide-VideoSettings.ps1`, `Install-NvidiaDriverClean`, `Enable-DeviceMSI`, `Set-NicInterruptAffinity`, `Apply-NvidiaCS2Profile`, `Set-NetworkDiagnosticDnsProfile`, `Set-CS2ProcessPriority`, `Invoke-BenchmarkCapture`. | active | Large high-risk orchestrator; direct `Restart-Computer -Force` after prompt/YOLO; empty catches and vendor branches need targeted audit. |
| `Boot-SafeMode.ps1` | PowerShell script | Shortcut to re-enter Phase 2 after Phase 1 prepared the runtime payload. | Script body only; supports `-SmokeTest`. | entrypoint/script | `Load-State`, `Test-Phase1SafeModeReady`, `Copy-PhaseRuntimePayload`, `Set-RunOnce`, `Set-BootConfig`. | active | Boot-state transition path; correctness depends on `state.json` marker and copied payload. |
| `Cleanup.ps1` | PowerShell script | Soft-reset / cleanup menu for shader/cache/temp/DNS/memory/prefetch/event logs/Steam verify and driver refresh Safe Mode handoff. | Script body only; supports `-SmokeTest`. | entrypoint/script | `config.env.ps1`, `helpers.ps1`, `Invoke-TieredStep`, `Clear-Dir`, `Set-RunOnce`, `Set-BootConfig`, `Copy-PhaseRuntimePayload`, `Restart-Computer`. | active | Cleanup and driver-refresh reboot logic share one script; direct restart after prompt; some destructive cleanup paths rely on path construction and dry-run gates. |
| `FpsCap-Calculator.ps1` | PowerShell script | Parses VProf benchmark text, calculates FPS cap, stores benchmark history/state, prints recommendations. | Script body only; supports `-SmokeTest`. | entrypoint/script | `Parse-BenchmarkOutput`, `Calculate-FpsCap`, `Add-BenchmarkResult`, `Save-SuiteState`, clipboard input. | active | Input parser is format-sensitive; usage is clear via START menu and tests. |
| `Verify-Settings.ps1` | PowerShell script | Post-update verifier for drift across NIC, power, QoS, DNS, TRIM, scheduled tasks, NVIDIA DRS, and runtime compatibility. | `New-VerifyCheckResult`, `Write-VerifyCheckResult`, `Test-VerifyNicAdvancedProperties`, `Test-VerifyPowerPlan`, `Test-VerifyQosPolicies`, `Test-VerifyDnsConfiguration`, `Test-VerifyTrimConfiguration`, `Test-VerifyScheduledTasks`, `Test-VerifyNvidiaDrsProfile`, `Test-VerifyRuntimeCompatibility`, `Invoke-VerifySettings`. | entrypoint/domain logic | `config.env.ps1`, `helpers.ps1`, `process-priority.ps1`, `nvidia-drs.ps1`, `nvidia-profile.ps1`, `power-plan.ps1`. | active | Mixed verifier and output/UI logic; Windows-only behavior is guarded but needs CI evidence. |
| `Guide-VideoSettings.ps1` | PowerShell script | Prints CS2 video settings guidance and optionally writes `video.txt`. | `Show-CS2SettingsGuide`. | domain logic/UI script | `Get-SteamPath`, `Test-TrustedVideoTxtPath`, `docs/video.txt`, `PostReboot-Setup.ps1`. | active | Long console UI and file-write helper in one function; usage is through Phase 3 only. |
| `CS2-Optimize-GUI.ps1` | PowerShell/WPF script | WPF dashboard shell: XAML, runspace pool, shared element lookup, sidebar status, lifecycle, panel-loader import. | `Invoke-Async`, `New-Brush`, `El`, `Update-SidebarStatus`. | UI/entrypoint | WPF assemblies, `config.env.ps1`, `helpers.ps1`, `helpers/step-catalog.ps1`, `helpers/system-analysis.ps1`, `helpers/gui-panels.ps1`. | active | Large XAML/script hybrid; async errors are surfaced through shared state; teardown uses empty catches documented in analyzer settings. |
| `config.env.ps1` | PowerShell config | Central suite constants, runtime paths, risk/profile defaults, NIC/DNS maps, latency target path, CS2 autoexec CVar map. | Variables such as `$CFG_WorkDir`, `$CFG_StateFile`, `$CFG_NIC_Tweaks`, `$CFG_CS2_Autoexec`. | config/storage contract | Dot-sourced by every entrypoint; tests validate CVar counts, paths, DNS, optional CFGs. | active | High-trust admin dot-source boundary; mixes durable config with evidence comments and compatibility stubs. |
| `helpers.ps1` | PowerShell loader | Dot-sources core helper modules into caller script scope. | No functions; loader surface. | adapter/script | `helpers/logging.ps1`, `tier-system.ps1`, `step-state.ps1`, `system-utils.ps1`, hardware/domain helpers. | active | Backward-compatible dot-source loader; shared `$SCRIPT:` state makes module boundaries implicit. |
| `PSScriptAnalyzerSettings.psd1` | PowerShell data/config | Analyzer configuration, excluded rules with justification, compatible syntax targets. | Hashtable settings. | config | GitHub `lint.yml`, local PSScriptAnalyzer command. | active | Several broad exclusions hide style issues by design; exclusions are documented. |

## Helper Modules

| File | Language/type | Primary responsibility | Main exports/classes/functions | Runtime role | Direct dependencies worth knowing | Status | Obvious smells |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `helpers/logging.ps1` | PowerShell helper | Console/log formatting, redaction, banners, phase counters, UTF-8 file writes. | `Set-TextFileUtf8`, `Add-TextFileUtf8Line`, `Test-HostIsWindows`, `Redact-Sensitive`, `Initialize-Log`, `Write-Log`, `Write-OK`, `Write-Warn`, `Write-Err`, `Write-Step`, `Write-Info`, `Write-DebugLog`, `Write-Blank`, `Write-Sub`, `Write-ActionOK`, `Write-TierBadge`, `Write-Section`, `Write-LogoBanner`, `Initialize-PhaseCounters`, `Add-PhaseApplied`, `Add-PhaseSkipped`, `Add-PhaseFailed`, `Write-PhaseSummary`, `Write-Banner`. | adapter/UI | `$CFG_LogFile`, `$CFG_LogDir`, `$SCRIPT:DryRun`, `$SCRIPT:CurrentPhase`. | active | Many global/script-scope reads; Write-Host is intentional and analyzer-excluded. |
| `helpers/tier-system.ps1` | PowerShell helper | Profile/risk gate and tiered step execution, including prompt, dry-run, and skip behavior. | `Test-YoloProfile`, `Get-ProfileMaxRisk`, `Test-RiskAllowed`, `Show-StepInfoCard`, `Invoke-TieredStep`, `Confirm-Risk`. | domain logic | `$SCRIPT:Profile`, `$SCRIPT:DryRun`, `Complete-Step`, `Skip-Step`, `Flush-BackupBuffer`, console input. | active | Central state machine for consent; many callers depend on exact return semantics. |
| `helpers/step-state.ps1` | PowerShell helper | `progress.json` loading, saving, completion, skip, resume prompt, and clear operations. | `Load-Progress`, `Save-Progress`, `Complete-Step`, `Skip-Step`, `Test-StepDone`, `Show-ResumePrompt`, `Clear-Progress`. | storage contract | `$CFG_ProgressFile`, `Save-JsonAtomic`, `Flush-BackupBuffer`. | active | Progress key shape is a contract; tests cover rejection of legacy bare step numbers. |
| `helpers/system-utils.ps1` | PowerShell helper | Download wrapper, atomic JSON, ACL hardening, suite state, runtime payload copy, RunOnce, boot config, registry writes, compatibility, verify counters. | `Invoke-Download`, `Save-JsonAtomic`, `Set-SecureAcl`, `Save-SuiteState`, `Ensure-SecureWorkDir`, `Test-TrustedSuiteScriptPath`, `Copy-PhaseRuntimePayload`, `Test-Phase1SafeModeReady`, `Set-Phase1SafeModeReadyFlag`, `Get-ModeForProfile`, `Load-State`, `Initialize-ScriptDefaults`, `Set-RunOnce`, `Set-BootConfig`, `Test-BootConfigSet`, `Set-RegistryValue`, `Ensure-Dir`, `Set-ClipboardSafe`, `Clear-Dir`, `Test-SystemCompatibility`, `Initialize-VerifyCounters`, `Get-VerifyCounters`, `Test-RegistryCheck`, `Test-ServiceCheck`. | adapter/storage/domain logic | `Invoke-WebRequest`, `bcdedit`, registry provider, `$CFG_WorkDir`, `$CFG_RunOnceExecutionPolicy`, backup helpers. | active | High-risk trust boundary; path validation and dry-run semantics are critical. |
| `helpers/backup-restore.ps1` | PowerShell helper | Backup file lifecycle, lock, backup capture, allowlisted restore, and interactive restore UI. | `New-BackupDataObject`, `New-BackupFile`, `Initialize-Backup`, `Test-BackupLock`, `Set-BackupLock`, `Remove-BackupLock`, `Flush-BackupBuffer`, `Get-BackupDataRaw`, `Get-BackupData`, `Save-BackupData`, `Get-BackupTaskPath`, `Test-ScheduledTaskBackupIdentity`, `Test-ScheduledTaskRestoreAllowed`, `Test-ServiceRestoreAllowed`, `Test-BootConfigRestoreAllowed`, `Test-RegistryRestoreAllowed`, `Backup-*`, `Restore-DrsSettings`, `Invoke-PagefileRestoreAutomation`, `Invoke-PagefileCimUpdate`, `Show-BackupSummary`, `Restore-StepChanges`, `Restore-Interactive`. | storage contract/domain logic | `$CFG_BackupFile`, `$CFG_BackupLockFile`, registry/service/task/DNS/DRS/power/pagefile APIs. | active | Largest helper and highest rollback risk; restore allowlists intentionally narrower than write paths; much branch-heavy logic. |
| `helpers/hardware-detect.ps1` | PowerShell helper | Hardware, Steam/CS2 path, CPU/GPU/RAM/NIC, benchmark parsing, trusted-path, WHEA, motherboard helpers. | `Get-CachedCpuInfo`, `Get-RamInfo`, `Get-NvidiaDriverVersion`, `Parse-BenchmarkOutput`, `Calculate-FpsCap`, `Test-TrustedLocalPath`, `Test-TrustedVideoTxtPath`, `Get-SteamPath`, `Get-CS2InstallPath`, `Get-ActiveNicAdapter`, `Get-ActiveNicGuid`, `Get-IntelHybridCpuName`, `Get-ChipsetVendor`, `Test-DualChannel`, `Get-AmdCpuInfo`, `Get-Ddr5TimingInfo`, `Test-WheaErrors`, `Get-MotherboardInfo`. | adapter/domain logic | WMI/CIM, registry, event logs, `$CFG_VirtualAdapterFilter`, Steam install layout. | active | Hardware detection is inherently branch-heavy; several best-effort failures return null/empty state. |
| `helpers/debloat.ps1` | PowerShell helper | Conservative gaming debloat inventory and action executor for AppX, provisioned packages, telemetry services/tasks, and consumer features. | `Get-GamingDebloatPackageNames`, `Get-GamingDebloatTelemetryServices`, `Get-GamingDebloatTelemetryTaskPaths`, `Get-GamingDebloatInventory`, `Write-GamingDebloatInventorySummary`, `Invoke-GamingDebloat`. | domain logic | AppX cmdlets, services, scheduled tasks, `Set-RegistryValue`, backup helpers. | active | Package allowlist can drift with Windows releases; destructive actions are guarded by inventory/dry-run but need Windows verification. |
| `helpers/msi-interrupts.ps1` | PowerShell helper | Enables MSI for GPU/NIC/audio and configures RSS/NIC interrupt affinity. | `Enable-DeviceMSI`, `Set-NicRssConfig`, `Set-NicInterruptAffinity`. | domain logic | PnP devices, NIC driver class keys, `Set-RegistryValue`, `Get-ActiveNicAdapter`, CPU/core topology. | active | Registry device matching and binary affinity masks are high-risk; many hardware-specific branches. |
| `helpers/gpu-driver-clean.ps1` | PowerShell helper | Native GPU driver cleanup replacing DDU: services/tasks/AppX/driver store/registry/files/shader caches. | `Remove-GpuDriverClean`. | domain logic | `pnputil`, services, AppX, scheduled tasks, registry, filesystem, backup service state. | active | Destructive cleanup path; vendor-specific branching and broad file/registry removals need careful Windows-only verification. |
| `helpers/nvidia-driver.ps1` | PowerShell helper | NVIDIA driver lookup/download metadata, signature validation, clean installer extraction/install, post-install cleanup/tweaks. | `Get-LatestNvidiaDriver`, `Test-NvidiaDriverSignature`, `Install-NvidiaDriverClean`, `Apply-NvidiaPostInstallTweaks`. | adapter/domain logic | NVIDIA web lookup, Authenticode, `Start-Process`, filesystem extraction, services/tasks, `Set-RegistryValue`. | active | Network and executable handling are high-security surfaces; legacy driver package cleanup patterns are branch-heavy. |
| `helpers/nvidia-drs.ps1` | PowerShell/C# interop helper | C# P/Invoke wrapper for `nvapi64.dll` DRS functions and session helper. | `Initialize-NvApiDrs`, `Invoke-DrsSession`; embedded C# `NvApiDrs`. | adapter/domain logic | `Add-Type`, `nvapi64.dll`, unmanaged function pointers, dry-run `NoSave` guard. | active | Complex native interop; API/version mismatch risk; bypasses normal registry dry-run unless guarded. |
| `helpers/nvidia-profile.ps1` | PowerShell helper | Applies CS2 NVIDIA profile through DRS and registry fallback settings. | `Apply-NvidiaCS2Profile`, `Apply-NvidiaCS2ProfileDrs`, `Apply-NvidiaCS2ProfileRegistry`. | domain logic | `Invoke-DrsSession`, `Backup-DrsSettings`, `Set-RegistryValue`, class-key scan, DRS setting tables. | active | Retains legacy registry fallback and legacy FRL IDs; some decoded flags are partially understood by docs. |
| `helpers/power-plan.ps1` | PowerShell helper | Creates and applies native CS2 optimized power plan with tier/vendor-specific settings. | `Set-PowerPlanValue`, `New-CS2PowerPlan`, `Apply-PowerPlan`. | domain logic | `powercfg`, CPU vendor, `$SCRIPT:Profile`, backup power plan. | active | Dense GUID/value matrix; failure modes are external-command dependent. |
| `helpers/process-priority.ps1` | PowerShell helper | Persistent CS2 process priority and dual-CCD Ryzen X3D affinity scheduled task. | `Get-X3DCcdInfo`, `Set-CS2ProcessPriority`, `Install-CS2AffinityTask`. | domain logic | CIM CPU info, `Set-RegistryValue`, scheduled task XML, `Backup-ScheduledTask`, `Get-Process cs2`. | active | Scheduled task script generation is a compatibility surface; X3D topology heuristics can drift with new CPUs. |
| `helpers/network-diagnostics.ps1` | PowerShell helper | Valve-region latency diagnostic, DNS state/profile changes, latency history, comparison rows. | `Get-ValveRegionTargets`, `Get-NetworkDiagnosticAdapterType`, `Get-ActiveNetworkDiagnosticAdapter`, `Get-NetworkDiagnosticDnsState`, `Get-NetworkDiagnosticSummary`, `ConvertTo-LatencySamples`, `Invoke-LatencyCandidateProbe`, `Get-LatencyStatisticMedian`, `Measure-ValveRegionLatency`, `Get-LatencyHistoryData`, `Save-LatencyHistoryRun`, `Invoke-ValveRegionLatencyDiagnostic`, `Get-LatestLatencyRun`, `Get-ValveLatencyComparisonRows`, `Get-LatencyHistoryRows`, `Set-NetworkDiagnosticDnsProfile`, `Restore-LatestDnsBackup`. | domain logic/adapter | `cfgs/valve-latency-targets.json`, `Test-Connection`, DNS client cmdlets, `backup.json`. | active | Target list is heuristic and may drift; diagnostic must not be presented as official match ping. |
| `helpers/benchmark-history.ps1` | PowerShell helper | Benchmark history persistence, FPS cap calculations, comparison display, interactive capture. | `Add-BenchmarkResult`, `Get-BenchmarkHistory`, `Show-BenchmarkComparison`, `Invoke-BenchmarkCapture`. | storage/domain logic | `$CFG_BenchmarkFile`, `Save-JsonAtomic`, `Parse-BenchmarkOutput`, `Calculate-FpsCap`. | active | History file variable is used by scripts; parser is tied to VProf text format. |
| `helpers/storage-health.ps1` | PowerShell helper | TRIM state parser, storage maintenance status, enable TRIM, ReTrim. | `Parse-TrimFsutilOutput`, `Get-TrimHealthStatus`, `Enable-TrimSupport`, `Invoke-StorageRetrim`. | domain logic/adapter | `fsutil`, `Get-Volume`, `Optimize-Volume`. | active | Windows storage behavior cannot be fully verified off Windows; docs intentionally avoid FPS claims. |
| `helpers/system-analysis.ps1` | PowerShell helper | GUI Analyze health checks for hardware, Windows gaming, latency, input, network, services, and CS2 config. | `script:Get-RegVal`, `script:New-CheckItem`, `Invoke-CheckHardware`, `Invoke-CheckWindowsGaming`, `Invoke-CheckSystemLatency`, `Invoke-CheckInput`, `Invoke-CheckNetwork`, `Invoke-CheckServices`, `Invoke-CheckCS2`, `Invoke-SystemAnalysis`. | UI/domain logic | Registry, CIM, services, `Get-SteamPath`, `Get-CS2InstallPath`, `Get-ActiveNicAdapter`, `$CFG_CS2_Autoexec`. | active | Broad read-only health scanner; risk of misleading statuses if runtime evidence is incomplete. |
| `helpers/gui-panels.ps1` | PowerShell/WPF helper | Dashboard panel builders, event handlers, analysis, backup UI, benchmark chart, network, video, settings, terminal launches. | `Get-StateDataSafe`, `Save-StateDataSafe`, `New-DefaultState`, `Should-SkipStartupDriftCheck`, `Test-StartupConfigDrift`, `Update-StartupDriftBanner`, `Get-BenchmarkCapFromText`, `Get-BenchmarkResultFromText`, `Switch-Panel`, `Load-Dashboard`, `Start-Analysis`, `Refresh-StorageHealthCard`, `Load-Optimize`, `Filter-OptimizeGrid`, `Start-InlineVerify`, `Load-Backup`, `Load-Benchmark`, `Draw-BenchChart`, `Load-NetworkDiagnostics`, `Start-LatencyDiagnostic`, `Invoke-GuiDnsProfileChange`, `Test-CurrentVideoTxtPathTrusted`, `Load-Video`, `Get-ResolvedVideoTier`, `Refresh-VideoGrid`, `Load-Settings`, `Save-SettingsToState`, `Launch-Terminal`. | UI | WPF controls from `CS2-Optimize-GUI.ps1`, config/helpers, `system-analysis.ps1`, network/backup/storage helpers. | active | Very large UI module; many panel responsibilities and async callbacks in one file. |
| `helpers/step-catalog.ps1` | PowerShell data helper | Data-only step catalog for GUI Optimize panel; mirrors phase `Invoke-TieredStep` metadata. | `$StepCatalog` array. | config/UI | Phase scripts and GUI panel. | active | Duplicate source of workflow truth; must stay in sync with phase scripts. |

## CFG and Data Files

| File | Language/type | Primary responsibility | Main exports/classes/functions | Runtime role | Direct dependencies worth knowing | Status | Obvious smells |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `cfgs/autoexec.cfg.example` | CS2 CFG example | Annotated example autoexec with competitive CVar baseline and excluded launch options. | CS2 console variables and comments. | config | README/docs; not directly executed by suite. | active reference | Contains personal sections/binds by design; usage is reference/manual, not generated output. |
| `cfgs/net_stable.cfg` | CS2 CFG | Low-ping stable connection/reset profile. | `cl_interp_ratio`, `cl_net_buffer_ticks`, timeout, SDR override. | config | Deployed by Step 34, documented and tested. | active | None obvious. |
| `cfgs/net_highping.cfg` | CS2 CFG | Stable high-ping profile with moderate buffer and longer timeout. | CS2 network CVars. | config | Deployed by Step 34, documented and tested. | active | Manual-use correctness depends on user selecting the right network condition. |
| `cfgs/net_unstable.cfg` | CS2 CFG | Jitter/loss profile for acceptable base ping. | CS2 network CVars. | config | Deployed by Step 34, documented and tested. | active | Adds latency by design; must remain clearly framed. |
| `cfgs/net_bad.cfg` | CS2 CFG | High-ping plus jitter/loss survival profile. | CS2 network CVars. | config | Deployed by Step 34, documented and tested. | active | Worst-case compromise; runtime benefit depends on network condition. |
| `cfgs/debug_hud.cfg` | CS2 CFG | Optional telemetry overlay and console diagnostic snapshot. | CS2 telemetry CVars and console commands. | config/script | Deployed by Step 34, documented and tested. | active | Diagnostic overlay can clutter play; docs say to revert. |
| `cfgs/debug_hud_off.cfg` | CS2 CFG | Resets debug telemetry controls to quiet defaults. | CS2 telemetry CVars. | config/script | Paired with `debug_hud.cfg`. | active | None obvious. |
| `cfgs/audio_stable.cfg` | CS2 CFG | Stable audio baseline and reset path. | `snd_autodetect_latency`, `snd_mixahead`. | config | Deployed by Step 34, documented and tested. | active | None obvious. |
| `cfgs/audio_lowlatency_025.cfg` | CS2 CFG | Optional lower audio buffer experiment. | `snd_autodetect_latency`, `snd_mixahead`. | config | Deployed by Step 34, documented and tested. | active | Experimental setting can cause crackle/dropouts; docs warn. |
| `cfgs/audio_lowlatency_001.cfg` | CS2 CFG | Optional aggressive low audio buffer experiment. | `snd_autodetect_latency`, `snd_mixahead`. | config | Deployed by Step 34, documented and tested. | active | High instability risk; manual proof is full-match listening/benchmark. |
| `cfgs/valve-latency-targets.json` | JSON data | Versioned heuristic Valve/Steam region candidates for GUI latency diagnostic. | `version`, `notes`, `targets[]`. | config/storage | `helpers/network-diagnostics.ps1`, tests, docs. | active | Heuristic endpoints may drift; ICMP reachability is not authoritative match ping. |

## Tests

| File | Language/type | Primary responsibility | Main exports/classes/functions | Runtime role | Direct dependencies worth knowing | Status | Obvious smells |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `tests/helpers/_TestInit.ps1` | PowerShell/Pester helper | Shared test bootstrap, temp Windows-style paths, global stubs, config defaults, helper load order. | `New-TestStateFile`, `New-TestProgressFile`, `New-TestBackupFile`, `Reset-TestState`. | test | Test temp root, core helpers, mockable Windows command stubs. | active | Central test harness; changes can invalidate many tests. |
| `tests/config.Tests.ps1` | Pester | Tests CS2 autoexec config, optional CFG files, NIC tweak config, path formats, DNS, FPS cap settings. | Pester `Describe` blocks. | test | `config.env.ps1`, `cfgs/*`, `Optimize-GameConfig.ps1`. | active | Hardcoded counts are intentional contract tests but can churn with config changes. |
| `tests/Optimize-Hardware.Tests.ps1` | Pester | Tests Phase 1 hardware module contracts around Step 10/dynamic tick path. | Pester tests. | test | `_TestInit.ps1`, `Optimize-Hardware.ps1`. | active | Narrow coverage for a broad script. |
| `tests/PostReboot-Setup.Tests.ps1` | Pester | Direct smoke contract for `PostReboot-Setup.ps1`. | Pester tests. | test | Real child `powershell`, `PostReboot-Setup.ps1 -SmokeTest`. | active | Windows-skipped locally when not on Windows. |
| `tests/Verify-Settings.Tests.ps1` | Pester | Verifier output/status tests and focused checks for NIC/power/task/DRS/runtime compatibility. | Pester tests. | test | `Verify-Settings.ps1`, mocks for system APIs. | active | Large test file; mocks can drift from real Windows behavior. |
| `tests/workflow-contracts.Tests.ps1` | Pester | Asserts CI workflow jobs and launcher targets remain present. | Pester tests. | test | `.github/workflows/lint.yml`, `.github/workflows/security.yml`, `START.bat`, `START-GUI.bat`. | active | Tests YAML by regex; catches contract drift but not full workflow validity. |
| `tests/e2e/entrypoints.Tests.ps1` | Pester E2E | Starts every shipped entrypoint in `-SmokeTest` mode as real `pwsh` child processes and checks markers/stderr/artifacts. | `Invoke-EntrypointSmokeProcess`. | test | `pwsh`, public entrypoint scripts. | active | Smoke-only; does not prove live Windows/admin behavior. |
| `tests/helpers/backup-dryrun.Tests.ps1` | Pester | Verifies every `Backup-*` helper self-guards in dry-run. | Pester tests. | test | Backup helpers. | active | Focused and valuable. |
| `tests/helpers/backup-restore.Tests.ps1` | Pester | Tests backup initialization, lock, backup capture, restore allowlists, scheduled tasks, registry/service/pagefile/DRS restore behavior. | Pester tests. | test | `helpers/backup-restore.ps1`, mocks for registry/services/tasks. | active | Very large; strongest coverage for rollback contract. |
| `tests/helpers/benchmark-history.Tests.ps1` | Pester | Tests benchmark parsing, FPS cap calculation, history persistence/comparison. | Pester tests. | test | `helpers/benchmark-history.ps1`, `hardware-detect.ps1`. | active | None obvious. |
| `tests/helpers/debloat.Tests.ps1` | Pester | Tests debloat allowlists, dry-run, inventory, service/task disable, AppX-unavailable handling. | Pester tests. | test | `helpers/debloat.ps1`, Windows command mocks. | active | Windows package surface can drift faster than tests. |
| `tests/helpers/gpu-driver-clean.Tests.ps1` | Pester | Tests native GPU driver clean helper behavior. | Pester tests. | test | `helpers/gpu-driver-clean.ps1`. | active | Needs Windows/manual validation for destructive live paths. |
| `tests/helpers/gui-panels.Tests.ps1` | Pester | Tests GUI panel helpers for switching, video path trust, network rows, storage card, benchmark parsing, startup drift, settings save. | Pester tests. | test | `helpers/gui-panels.ps1`, WPF-like mocks. | active | UI rendering itself is not browser/screenshot verified. |
| `tests/helpers/hardware-detect.Tests.ps1` | Pester | Tests hardware/CPU/GPU/RAM/NIC/video path detection and benchmark parsing. | Pester tests. | test | `helpers/hardware-detect.ps1`. | active | Heuristic CPU model coverage can drift with new hardware. |
| `tests/helpers/logging.Tests.ps1` | Pester | Tests logging helpers and output/log file behavior. | Pester tests. | test | `helpers/logging.ps1`. | active | Console-color UX not deeply validated. |
| `tests/helpers/logging-security.Tests.ps1` | Pester | Tests sensitive-output redaction/logging security behavior. | Pester tests. | test | `helpers/logging.ps1`. | active | None obvious. |
| `tests/helpers/msi-interrupts.Tests.ps1` | Pester | Tests MSI, RSS, and NIC interrupt affinity dry-run/device-filter behavior. | Pester tests. | test | `helpers/msi-interrupts.ps1`, PnP/NIC mocks. | active | Hardware-specific registry paths need Windows confirmation. |
| `tests/helpers/network-diagnostics.Tests.ps1` | Pester | Tests latency target load, probe fallback, diagnostic persistence, comparison rows, DNS profile changes. | Pester tests. | test | `helpers/network-diagnostics.ps1`, latency targets JSON. | active | ICMP/live routing cannot be deterministically tested. |
| `tests/helpers/nvidia-driver.Tests.ps1` | Pester | Tests NVIDIA driver lookup mapping, URL security, signature validation, install guards, post-install tweaks. | Pester tests. | test | `helpers/nvidia-driver.ps1`, web/signature/process mocks. | active | Live NVIDIA site/API and installer behavior still need manual/CI proof. |
| `tests/helpers/nvidia-drs.Tests.ps1` | Pester | Tests NVAPI DRS helper guards and interop assumptions. | Pester tests. | test | `helpers/nvidia-drs.ps1`. | active | Native API cannot be fully exercised without NVIDIA driver. |
| `tests/helpers/nvidia-profile.Tests.ps1` | Pester | Tests NVIDIA profile registry/DRS setting behavior. | Pester tests. | test | `helpers/nvidia-profile.ps1`, `nvidia-drs.ps1`, backup mocks. | active | DRS setting effect remains partly hardware/driver dependent. |
| `tests/helpers/power-plan.Tests.ps1` | Pester | Tests power-plan creation/apply command selection and vendor/tier decisions. | Pester tests. | test | `helpers/power-plan.ps1`, `powercfg` mocks. | active | GUID matrix correctness depends on external Windows powercfg behavior. |
| `tests/helpers/process-priority.Tests.ps1` | Pester | Tests X3D topology detection, IFEO writes, and scheduled task creation behavior. | Pester tests. | test | `helpers/process-priority.ps1`. | active | CPU-model heuristic drift risk. |
| `tests/helpers/security-validation.Tests.ps1` | Pester | Tests trusted path and security validation helpers. | Pester tests. | test | `helpers/system-utils.ps1`, `hardware-detect.ps1`. | active | None obvious. |
| `tests/helpers/step-catalog.Tests.ps1` | Pester | Tests GUI step catalog contracts. | Pester tests. | test | `helpers/step-catalog.ps1`. | active | Duplicates phase metadata by design. |
| `tests/helpers/step-state.Tests.ps1` | Pester | Tests progress state load/save/complete/skip/resume/clear behavior and legacy bare-step rejection. | Pester tests. | test | `helpers/step-state.ps1`. | active | Important storage-contract tests. |
| `tests/helpers/storage-hardening.Tests.ps1` | Pester | Tests secure runtime directory and RunOnce/path hardening. | Pester tests. | test | `helpers/system-utils.ps1`. | active | Windows ACL semantics need Windows authority. |
| `tests/helpers/storage-health.Tests.ps1` | Pester | Tests TRIM parser and storage health helper behavior. | Pester tests. | test | `helpers/storage-health.ps1`. | active | Runtime storage cmdlets need Windows proof. |
| `tests/helpers/system-analysis.Tests.ps1` | Pester | Tests Analyze panel check item/status behavior for hardware, network, services. | Pester tests. | test | `helpers/system-analysis.ps1`. | active | Read-only checks can become misleading if Windows output changes. |
| `tests/helpers/system-utils.Tests.ps1` | Pester | Tests registry/BCD dry-run, state, defaults, runtime payload, RunOnce, secure path utilities. | Pester tests. | test | `helpers/system-utils.ps1`. | active | Broad helper contract coverage. |
| `tests/helpers/tier-system.Tests.ps1` | Pester | Tests profile risk ceilings, tiered step execution, dry-run, custom, YOLO, skip callbacks. | Pester tests. | test | `helpers/tier-system.ps1`. | active | Consent semantics are central; regressions can affect all phases. |
| `tests/integration/_IntegrationInit.ps1` | Pester helper | Integration test setup and reset helpers. | `Reset-MockTracker`, `Reset-IntegrationState`. | test | Core helpers and temp state. | active | Central integration harness. |
| `tests/integration/backup-restore-entrypoints.Tests.ps1` | Pester integration | Tests backup/restore entrypoint-level contracts. | Pester tests. | test | Backup helpers and entrypoint flows. | active | Mocked integration, not live Windows restore. |
| `tests/integration/backup-restore-roundtrip.Tests.ps1` | Pester integration | Tests backup/restore round trips across registry/service/task/power/boot/DNS/DRS/pagefile cases. | Pester tests. | test | Backup/restore helpers, mocks. | active | Very broad; still mocked for dangerous operations. |
| `tests/integration/dryrun-compliance.Tests.ps1` | Pester integration | Tests dry-run compliance for registry, BCD, tiered steps, external commands, RunOnce. | Pester tests. | test | `Set-RegistryValue`, `Set-BootConfig`, `Invoke-TieredStep`, `Set-RunOnce`. | active | High-value safety contract. |
| `tests/integration/profile-behavior-matrix.Tests.ps1` | Pester integration | Tests profile behavior across SAFE/RECOMMENDED/COMPETITIVE/CUSTOM/YOLO. | Pester tests. | test | `tier-system.ps1`, prompt mocks. | active | Matrix can be verbose but protects consent policy. |
| `tests/integration/state-persistence.Tests.ps1` | Pester integration | Tests state/progress/backup persistence and corruption handling. | Pester tests. | test | `Load-State`, `Save-SuiteState`, progress/backup helpers. | active | Storage-path behavior can differ on Windows vs macOS. |

## CI, GitHub, and Repository Metadata

| File | Language/type | Primary responsibility | Main exports/classes/functions | Runtime role | Direct dependencies worth knowing | Status | Obvious smells |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.github/workflows/lint.yml` | GitHub Actions YAML | CI lint/type/compat/test gate for PowerShell files, docs, CFGs, batch files, and workflows. | Jobs: `psscriptanalyzer`, `syntax-check`, `windows-powershell-compat`, `pester`, `estimate-keys`, `e2e`, `entrypoint-smoke`. | config/CI | PSScriptAnalyzer, Pester, Windows PowerShell, `grep` retired estimate-key check. | active | Retired-surface cross-reference guard is a branch-protection compatibility check; keep non-excluded docs free of guarded retired symbols. |
| `.github/workflows/security.yml` | GitHub Actions YAML | Secret scan, PowerShell dangerous-pattern scan, launcher safety, workflow integrity. | Jobs: `secret-scan`, `script-safety`, `workflow-integrity`. | config/CI/security | `grep`, pinned actions, batch launchers, workflow files. | active | Pattern-based scanning can false-positive/false-negative; still useful as guardrail. |
| `.github/PULL_REQUEST_TEMPLATE.md` | Markdown | PR checklist for evidence, CI, dry-run, backup/restore, docs, security. | Checklist sections. | config/docs | GitHub PR UI. | active | None obvious. |
| `.github/REPO_SETTINGS.md` | Markdown | Manual repository settings and branch protection checklist. | Settings checklist. | config/docs | GitHub web settings. | active | Required-check list can drift from actual branch protection; needs periodic verification. |
| `.github/SECURITY.md` | Markdown | Security policy and design notes. | Policy sections. | docs/security | GitHub security tab. | active | None obvious. |
| `.github/CODEOWNERS` | GitHub metadata | Ownership/review routing. | CODEOWNERS rules. | config | GitHub pull request review assignment. | active | Not deeply inspected; verify exact owners before relying on enforcement. |
| `.github/ISSUE_TEMPLATE/bug_report.yml` | GitHub issue form YAML | Structured bug-report template. | Issue form fields. | config/docs | GitHub issue UI. | active | None obvious. |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | GitHub issue form YAML | Structured evidence-first feature request template. | Issue form fields. | config/docs | GitHub issue UI. | active | None obvious. |
| `.gitignore` | Git ignore config | Excludes runtime state, secrets, local agent artifacts, generated test results, and debug/temp files. | Ignore patterns. | config | Git status/commit hygiene. | active | `AGENTS.md` was previously ignored; current root guidance should stay trackable. |
| `LICENSE` | Text/legal | MIT license. | License text. | docs/legal | Repository distribution. | active | Not source logic. |

## Documentation Files

| File | Language/type | Primary responsibility | Main exports/classes/functions | Runtime role | Direct dependencies worth knowing | Status | Obvious smells |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `AGENTS.md` | Markdown | Durable agent guidance for this repository. | Rules/commands/contracts. | docs/config | Future agent behavior. | active | Needs periodic update when commands/contracts change. |
| `README.md` | Markdown | Main user and maintainer overview: purpose, profiles, quick start, file overview, verification, phases, deep dives. | User docs. | docs | Links to all major docs and entrypoints. | active | Large user-facing source of truth; must stay in sync with phase scripts. |
| `CONTRIBUTING.md` | Markdown | Contributor rules: evidence, no external tools, backup, dry-run, tier system, verification commands. | Contributor policy. | docs | PSScriptAnalyzer/Pester commands. | active | Duplicates some AGENTS/README guidance. |
| `CHANGELOG.md` | Markdown | Release history and unreleased changes. | Changelog sections. | docs | Human release process. | active | Includes historical/retired surfaces; avoid treating old entries as current behavior. |
| `docs/architecture.md` | Markdown | Maintainer map of entrypoints, runtime state, helper boundaries, phase handoff, backup rules, verification. | Architecture sections. | docs | README, AGENTS, code-index. | active | Central source; should be updated with workflow changes. |
| `docs/evidence.md` | Markdown | Per-optimization impact/risk table and profile decision matrix. | Evidence/risk tables. | docs | README, tier system. | active | Performance estimates can become stale; claims need evidence refresh. |
| `docs/debunked.md` | Markdown | Debunked/contested settings and out-of-scope guidance. | Debunked setting list. | docs | CONTRIBUTING, README. | active | Evidence can drift as CS2/Windows changes. |
| `docs/gui.md` | Markdown | GUI dashboard usage, panels, and technical notes. | Panel docs. | docs/UI | `CS2-Optimize-GUI.ps1`, `helpers/gui-panels.ps1`. | active | Must track UI panel changes. |
| `docs/network-diagnostics.md` | Markdown | Latency diagnostic scope, history model, target definitions, DNS workflow, evidence posture. | Diagnostic docs. | docs | `helpers/network-diagnostics.ps1`, `cfgs/valve-latency-targets.json`. | active | Explicitly heuristic; avoid stronger claims. |
| `docs/storage-health.md` | Markdown | TRIM/ReTrim maintenance framing and verification surface. | Storage health docs. | docs | `helpers/storage-health.ps1`, GUI, verifier. | active | Correctly avoids FPS claims. |
| `docs/audio.md` | Markdown | Audio CVar and Windows audio rationale, low-latency CFG warnings. | Audio docs. | docs | `$CFG_CS2_Autoexec`, CFG audio files, Step 33/34. | active | Audio recommendations need current CS2 CVar validation. |
| `docs/video-settings.md` | Markdown | `video.txt`, autoexec, launch option, Reflex, and video setting rationale. | Video docs. | docs | `docs/video.txt`, `Optimize-GameConfig.ps1`, `Guide-VideoSettings.ps1`. | active | Large evidence-heavy doc; stale CVar/launch option risk. |
| `docs/video.txt` | CS2 config documentation | Copy-ready annotated `video.txt` for low/mid/high GPU tiers. | CS2 video settings. | config/docs | Video settings docs and GUI video panel. | active | It is a config artifact in `docs/`; CS2 may overwrite user copies. |
| `docs/network-cfgs.md` | Markdown | Network condition CFG matrix and diagnostics guidance. | CFG docs. | docs | `cfgs/net_*.cfg`, `debug_hud*.cfg`. | active | User must select condition manually. |
| `docs/nic-latency-stack.md` | Markdown | NIC tuning rationale: PHY, interrupt moderation, RSS, URO, QoS, IPv6. | Network stack docs. | docs | `Optimize-Hardware.ps1`, `helpers/msi-interrupts.ps1`. | active | Hardware/driver evidence can drift. |
| `docs/msi-interrupts.md` | Markdown | MSI/MSI-X and NIC affinity explanation and verification. | Interrupt docs. | docs | Phase 3 Steps 2-3, `helpers/msi-interrupts.ps1`. | active | Hardware-specific. |
| `docs/windows-scheduler.md` | Markdown | HAGS, Game Mode, Fast Startup, MPO, scheduler/MMCSS, FTH, maintenance, timer, mouse. | Scheduler docs. | docs | `Optimize-SystemBase.ps1`, `Optimize-Hardware.ps1`, `Optimize-RegistryTweaks.ps1`. | active | Windows behavior can change by build. |
| `docs/process-priority.md` | Markdown | IFEO process priority and X3D CCD affinity rationale/rollback. | Process-priority docs. | docs | `helpers/process-priority.ps1`. | active | CPU-model assumptions can drift. |
| `docs/nvidia-optimization.md` | Markdown | NVIDIA driver install, native DDU replacement, DRS profile architecture, MSI, verification. | NVIDIA docs. | docs | `helpers/nvidia-driver.ps1`, `gpu-driver-clean.ps1`, `nvidia-drs.ps1`, `nvidia-profile.ps1`. | active | Driver/API behavior can drift quickly. |
| `docs/nvidia-drs-settings.md` | Markdown | Complete DRS setting table, decoded/unknown flags, registry keys, verification. | DRS setting docs. | docs | `helpers/nvidia-profile.ps1`. | active | Some flags are partially decoded/unknown; keep uncertainty explicit. |
| `docs/power-plan.md` | Markdown | Power plan rationale, FPSHeaven bug fixes, tier structure, verification. | Power plan docs. | docs | `helpers/power-plan.ps1`. | active | Legacy FPSHeaven comparison is intentional historical context. |
| `docs/services.md` | Markdown | Service disable rationale, impact, rollback. | Service docs. | docs | `Optimize-GameConfig.ps1`, `helpers/debloat.ps1`. | active | Service impact can vary by user peripherals and Windows edition. |
| `docs/debloat.md` | Markdown | AppX/services/tasks debloat rationale, fresh baseline, rollback. | Debloat docs. | docs | `helpers/debloat.ps1`, `Optimize-Hardware.ps1`. | active | Package names drift with Windows releases. |
| `docs/backup-restore.md` | Markdown | Backup/restore model, backup types, access, manual restore. | Backup docs. | docs | `helpers/backup-restore.ps1`. | active | Manual DRS restore mentions external Profile Inspector as manual reference; suite runtime remains native. |
| `docs/fresh-windows-baseline.md` | Markdown | Recommended official Windows install baseline before running suite. | Baseline docs. | docs | README/debloat docs. | active | Time-sensitive Windows version note; verify before future updates. |
| `docs/screenshots/*.png` | PNG assets | GUI screenshots embedded in README/docs. | Binary assets. | generated/docs | README and `docs/gui.md`. | active/generated | Not source-inspected; visual freshness is UNCLEAR without opening screenshots. |

## Highest-Risk Files

1. `helpers/backup-restore.ps1`: owns rollback correctness and restore allowlists
   for registry, services, tasks, boot config, DRS, DNS, pagefile, and power
   plan state. A false restore success or unsafe allowlist is high impact.
2. `helpers/system-utils.ps1`: contains admin-scope write primitives,
   `RunOnce`, `bcdedit`, registry writes, ACL hardening, and runtime payload
   copy.
3. `Optimize-GameConfig.ps1`, `SafeMode-DriverClean.ps1`, and
   `PostReboot-Setup.ps1`: handle reboot handoff, Safe Mode state, service
   changes, driver install/removal, and final restart.
4. `helpers/gpu-driver-clean.ps1`, `helpers/nvidia-driver.ps1`,
   `helpers/nvidia-drs.ps1`, and `helpers/nvidia-profile.ps1`: driver cleanup,
   downloaded executable handling, native interop, and DRS profile writes.
5. `Optimize-Hardware.ps1`, `helpers/msi-interrupts.ps1`,
   `helpers/process-priority.ps1`, and `helpers/network-diagnostics.ps1`:
   NIC/interrupt/QoS/DNS/scheduled-task paths with hardware and Windows-version
   variability.

## Likely Dead Files

No likely dead source files were proven in this pass.

UNCLEAR:

- `cfgs/autoexec.cfg.example` is a reference/example rather than a deployed
  runtime file. It is linked from README-style docs and should not be called dead
  unless docs no longer need a copy-ready reference.
- `docs/screenshots/*.png` are active documentation assets by path, but visual
  freshness was not validated.
- `testResults.xml` / `test-results.xml` are generated/ignored artifacts, not
  source. Delete only as local cleanup, not as source refactor.

Proof needed before deleting anything: no references from README/docs/tests/CI,
no runtime copy/deploy path, and a passing full relevant test/smoke gate after
removal.

## Likely Overcomplicated Files

1. `helpers/backup-restore.ps1`: many backup types, allowlists, restore cases,
   and interactive UI in one file.
2. `helpers/gui-panels.ps1`: all dashboard panels and event handlers are in one
   large WPF helper.
3. `CS2-Optimize-GUI.ps1`: XAML shell, runspace management, shared UI lookup,
   and lifecycle logic in one script.
4. `Optimize-GameConfig.ps1`, `Optimize-Hardware.ps1`,
   `Optimize-RegistryTweaks.ps1`, `Optimize-SystemBase.ps1`,
   `PostReboot-Setup.ps1`: phase scripts are long, imperative, and branch-heavy.
5. `helpers/hardware-detect.ps1`: many hardware/vendor heuristics in one helper.

Do not split these just for size. Refactor only with a written plan and
behavioral tests around the contract being moved.

## Likely Deprecated Compatibility Paths

- `helpers.ps1`: explicitly a backward-compatible dot-source loader.
- `Setup-Profile.ps1`: derives `mode` for backward compatibility with state
  load/banner behavior.
- `Optimize-SystemBase.ps1`: uses `Get-WmiObject` for pagefile `.Put()` support
  in Windows PowerShell 5.1; PowerShell 7 lacks this path.
- `config.env.ps1` and `Optimize-GameConfig.ps1`: keep CS2 no-op/forward-compat
  CVars such as `m_rawinput` and deprecated interp-style CVars as documented
  stubs.
- `helpers/nvidia-profile.ps1`: keeps legacy NVIDIA registry fallback and
  legacy frame-rate-limiter IDs alongside DRS writes.
- `cfgs/autoexec.cfg.example` and docs include legacy/debunked launch-option
  examples for user education.
- `.github/workflows/lint.yml`: retired-surface cross-reference guard is likely
  retained for branch-protection/CI compatibility.

## Recommended Next Audit Targets

1. Backup/restore correctness audit: prove restore allowlists, first-value
   preservation, false-success handling, corrupt JSON behavior, and dry-run
   interaction in `helpers/backup-restore.ps1`.
2. Reboot and phase-handoff audit: inspect `Optimize-GameConfig.ps1`,
   `Boot-SafeMode.ps1`, `SafeMode-DriverClean.ps1`, `PostReboot-Setup.ps1`, and
   `Copy-PhaseRuntimePayload`/`Set-RunOnce` together.
3. Driver/NVIDIA audit: validate executable download/signature/install flow,
   native driver cleanup, DRS native interop, and registry fallback paths.
4. GUI state truth audit: verify dashboard status, startup drift, inline verify,
   backup restore, network diagnostics, and video writes do not show false
   healthy/connected/applied states.
5. Deprecated compatibility audit: decide which compatibility paths are still
   required, especially CS2 deprecated CVars, NVIDIA legacy registry fallback,
   and the retired-surface CI guard.
6. Windows-only verification baseline: run CI-equivalent Windows PowerShell 5.1
   smoke, Pester, parser, and PSScriptAnalyzer gates on a clean Windows host.

## Coverage Gaps and Uncertainty

- This pass did not execute Pester, PSScriptAnalyzer, parser checks, GUI smoke,
  Windows PowerShell 5.1 smoke, or live runtime flows.
- Windows-specific system behavior was read from code/tests/docs, not verified
  on a Windows host.
- Usage classifications are based on current references and tests. A file marked
  active can still contain unused functions; proving function-level dead code
  requires call-graph analysis and targeted tests.
- CFG usefulness depends on current CS2 CVar behavior and Valve changes; future
  CVar validation is required before removing or strengthening claims.
- Documentation/screenshots were represented by file/path and header-level
  inspection; screenshot visual freshness was not validated.
