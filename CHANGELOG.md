# Changelog

All notable changes to the CS2 Optimization Suite are recorded here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- Phase 3 Step 7: VBS / Core Isolation (Memory Integrity) disable — detects HVCI via `Win32_DeviceGuard`, disables via registry, warns about FACEIT/Vanguard dependency. Replaces the previously reserved step slot.
- DRS profile: rBAR Enable (`983226`) + rBAR Options (`983227`) — per-application Resizable BAR control. Set to `0` (Disabled) for CS2 — ThourCS2 2026: ~6% better 1% lows with rBAR OFF. System-wide BIOS rBAR stays enabled for other titles
- NIC: RSS master switch (`*RSS`) check — creates/enables if absent or disabled (some Realtek drivers ship with `*RSS=0`, silently ignoring all RSS sub-parameters)
- NIC: speed-aware RSS queue count — 5+ GbE NICs (e.g., RTL8126) get 8 queues instead of 4
- NIC: DisplayName fallback for Realtek NICs — tries Intel-style names first, falls back to Realtek-style (`"Energy Efficient Ethernet"` instead of `"EEE"`)
- NIC: 5 GbE buffer sizing — `ReceiveBuffers = 2048` for NICs at 5+ Gbps link speed
- System analysis: WHEA error check (PBO/CO stability indicator — warns if errors in last 24h)
- Process priority: 9900X3D asymmetric CCD mask — 8+4 layout detected per-model instead of assuming symmetric `Floor(totalCores/2)`
- NVIDIA driver: laptop GPU `psid`/`pfid` entries for RTX 30/40/50 Laptop series
- Debloat: `Microsoft.Windows.PhoneLink` (Win11 23H2+ renamed package, alongside existing `Microsoft.YourPhone`)

### Changed
- DRS profile: removed `279476686` (Variable refresh rate) — not present in NPI, likely inert; 6 remaining G-SYNC/VRR settings cover all paths
- DRS profile: removed `1074665807` (CUDA Force P2 State) — undocumented duplicate; `1343646814` (CUDA_STABLE_PERF_LIMIT) handles same override
- DRS profile: `Trilinear optimization` name corrected — value `0` means ON (driver perf shortcut enabled), not OFF
- Power plan: PCIe ASPM GUIDs fixed — previous GUIDs were incorrect, meaning ASPM disable was never applied. Now uses correct subgroup `501a4d13-...` and setting `ee12f906-...`

### Added (previous — initial release)
- `helpers/process-priority.ps1` — replaces Process Lasso (final external tool eliminated). IFEO PerfOptions for persistent High CPU priority; scheduled task for dual-CCD X3D affinity pinning (7900X3D/7950X3D/9900X3D/9950X3D)
- `helpers/power-plan.ps1` — replaces FPSHeaven `.pow` binary import with native `powercfg` calls. Fixed 4 bugs in the original plan (passive cooling, AMD PROCTHROTTLEMIN, duty cycling, PERFAUTONOMOUS)
- `helpers/nvidia-drs.ps1` + `helpers/nvidia-profile.ps1` — direct `nvapi64.dll` DRS write via C# `Add-Type`, replacing NVIDIA Profile Inspector dependency. 52 DWORD settings decoded and written to DRS binary database
- `helpers/benchmark-history.ps1` — iterative FPS tracking across sessions (`benchmark_history.json`)
- `CS2-Optimize-GUI.ps1` + `START-GUI.bat` — WPF dashboard with 7 panels: Dashboard, Analyze, Optimize, Backup, Benchmark, Video, Settings
- `helpers/hardware-detect.ps1` — `Get-IntelHybridCpuName` and `Get-SteamPath` shared helpers (eliminates duplicated registry reads across 7 files)
- Risk/consent system on all steps: `-Risk`, `-Depth`, `-Improvement`, `-SideEffects`, `-Undo` metadata on every `Invoke-TieredStep` call
- `Backup-ScheduledTask` type in backup system for X3D affinity task rollback
- Step 31: `GameDVR_Enabled = 0` (master DVR switch, previously missing)
- Step 33: `UserDuckingPreference = 3` (disable VoIP audio ducking)
- Step 27: FTH disable, Automatic Maintenance disable, NTFS metadata optimizations, `DisableCoInstallers`
- Step 16: URO disable (UDP Receive Offload, Windows 11 build ≥22000), IPv6 left enabled (2026 reversal — Steam/Valve SDR prefers IPv6), `*GreenEthernet`/`*PowerSavingMode` (Realtek-specific), QoS DSCP prerequisite NLA key
- Power plan: PCIe ASPM T1 setting added (prevents Windows software ASPM between frames)
- NVIDIA DRS: `DisableDynamicPstate = 1` in GPU class key, ULL CPL State fixed to `1 (On)`, shader cache 4096→10240 MB
- `docs/video.txt` — annotated copy-ready CS2 video.txt for 2026 competitive meta (three GPU tiers)
- `docs/audio.md`, `docs/process-priority.md`, `docs/backup-restore.md`, `docs/debloat.md` and 10 additional deep-dive documentation files
- `.github/workflows/security.yml` — secret scanning, PowerShell safety patterns, workflow integrity checks
- `.github/CODEOWNERS`, `.github/REPO_SETTINGS.md`, `.github/dependabot.yml`
- `SECURITY.md` with DRY-RUN system documentation and responsible disclosure

### Changed
- All 8 external tool dependencies eliminated — pure PowerShell with zero external binaries (except NVIDIA driver .exe download)
- Step 12 (Game Mode): reversed from Disable to Enable — WU suppression + MMCSS `Games` scheduling; IFEO supersedes any priority interference
- Step 16 (NIC): interrupt moderation set to Medium for ALL profiles (djdallmann empirical result — Disabled causes interrupt storms under background traffic)
- `mouclass queue size`: 16 → 50 (2025 testing showed values below 30 cause event skipping)
- `Win32PrioritySeparation`: `0x26` (variable) → `0x2A` (fixed) — 2025 Blur Busters: short fixed quantum improves 1% lows
- Step 34 autoexec: 56 CVars (8 categories) → 74 CVars (10 categories). Added HUD/QoL (7), Privacy (5), updated Gameplay (17). `cl_predict_body/head_shot_fx` set to `0` per 95% pro consensus (ThourCS2 120-player study)
- Network stack: TCP-only settings removed from scope; `netsh int tcp` commands documented as no-op for CS2's UDP traffic

### Fixed
- `Cleanup.ps1` line 161: undefined `$steamReg` crash on Steam Verification path (changed to `$steamBase`)
- 3× Intel hybrid CPU detection duplicated inline → `Get-IntelHybridCpuName` shared function
- 9× Steam path lookup duplicated inline → `Get-SteamPath` shared function
- Power plan: passive cooling bug (FPSHeaven shipped `SYSCOOLPOL=0`; corrected to Active)
- Power plan: AMD PROCTHROTTLEMIN bug (100% bypasses Precision Boost 2; corrected to 0% on AMD)
- NVIDIA DRS: `PerfLevelSrc=0x2222` registry key correctly placed in GPU class key `{4d36e968}` (not `d3d\` path which is a no-op on modern drivers)

### Removed
- All external tool downloads: NVCleanstall, NVIDIA Profile Inspector, FPSHeaven, Process Lasso, GoInterruptPolicy, and 3 others
- `FPSHEAVEN2026.pow` dependency (binary `.pow` import replaced by native `powercfg` calls)
- Debunked settings removed from autoexec: `-tickrate 128`, `-threads N`, `cl_cmdrate`, `net_graph 1`, `r_dynamic 0`, `mat_queue_mode 2`

### Polish (March 2026)

**Backup/restore coverage:**
- 5 new backup types: `nic_adapter`, `qos_uro`, `defender`, `pagefile`, `dns` — every state-changing operation now has a matching restore path
- `Backup-NicAdapterProperty`: records NIC adapter settings (DisplayName and RegistryKeyword) before modification
- `Backup-QosAndUro`: records QoS policies and URO state before DSCP/network changes
- `Backup-DefenderExclusions`: records Defender exclusion paths/processes added by Step 36
- `Backup-PagefileConfig`: records pagefile auto-management and size before Step 8 changes
- `Backup-DnsConfig`: records DNS server addresses per adapter before Phase 3 Step 9 changes
- `Restore-StepChanges`: full restore handlers for all 5 new backup types
- `Show-BackupSummary`: displays all new backup types in the summary view
- `Backup-RegistryValue`: fallback type detection now returns `String` for `\Run` paths instead of defaulting to `DWord`
- DRS restore: `$s.id` cast to `[uint32]` before `.ToString('X')` to handle JSON round-trip `[double]` conversion

**Bug fixes:**
- `msi-interrupts.ps1` line 265: `$targetCore:` parsed as scope-qualified variable reference (fixed with `${targetCore}:` brace delimiter)
- `hardware-detect.ps1`: NVIDIA driver version parser required only 2 dot-separated parts; now requires 4 (matching actual `31.0.15.5762` format)
- `step-state.ps1`: `timestamps=@{}` hashtable replaced with `[PSCustomObject]@{}` for consistent JSON serialization
- Resume prompt (`Show-ResumePrompt`): filtered completedSteps/skippedSteps by current phase prefix to prevent Phase 1/3 step mixing

**Reliability:**
- `Set-NicInterruptAffinity`: 3-strategy PnP device matching (exact name, substring, PCI hardware path) for multi-instance NICs
- `Save-JsonAtomic`: atomic write via NTFS rename documented in code comment

**Code quality:**
- All PSScriptAnalyzer warnings resolved (test stubs: `$Args` renamed to `$CmdArgs`, `$input` to `$vprofOutput`, pipeline stubs given `process {}` blocks)
- AI filler text removed from docs and scripts (`robust`, `comprehensive` replaced with concrete terms)
- `.gitignore`: added `.claude/loops/`, `progress.md`, `testResults.xml`

### Audit (March 2026)
Multi-round code audit covering infrastructure, phase scripts, specialized modules, GUI, tests, documentation, CI/CD, UX, code simplification, security hardening, and compatibility guards.

**Bug fixes:**
- Step 29: mouse acceleration curves were 20 bytes instead of required 40 bytes (INT64 per entry)
- Step 9 DNS: only fastest adapter was configured; now sets DNS on all active adapters
- Phase 3 DRY-RUN switch: mode derivation was hardcoded CONTROL instead of matching profile
- Phase 3 fallback state missing `baselineAvg`, `baselineP1`, `appliedSteps` fields
- GPU driver clean: CIM/WMI primary enumeration for non-English Windows (pnputil labels are locale-dependent)
- NVIDIA registry fallback missing `DisableDynamicPstate=1` in GPU class key
- Install-NvidiaDriverClean: unconditional `$true` return on non-zero exit codes
- Step 37 services: SysMain/WSearch did not check service existence before disable
- Step 38: silent failure on missing helpers/ directory during Safe Mode file copy
- Step 13 debloat: duplicate autostart cleanup overlapped with Step 14
- Phase 2 bcdedit: re-run caused false CRITICAL error when safeboot value already cleared
- Legacy bare step-number keys in progress.json caused P1:5/P3:5 collision on resume
- Pester test parse errors: `$key:` inside double-quoted strings parsed as scope qualifier
- Verify-Settings counter leaks: HAGS and PowerThrottling branches missed counter increment

**Reliability improvements:**
- `-ErrorAction Stop` on all critical JSON readers (state.json, progress.json, backup.json)
- Missing `-SkipAction` on T1 steps 2, 3, 4, 6 (CUSTOM profile decline left progress unrecorded)
- Backup I/O batching: ~60 read/write cycles reduced to 38 (one flush per step)
- Advisory lockfile for concurrent backup.json access with 4-hour auto-expire
- GUI: async timers stopped on window close to prevent post-dispose crashes
- Restart-Computer gated by DRY-RUN at Phase 1 completion
- Initialize-Backup moved inside try/finally in all entry points

**UX improvements:**
- Step progress bar (`[Step 5/38]`) in Phase 1 and Phase 3 banners
- DRY-RUN completion banners clarified (no longer imply changes were applied)
- Resume prompt shows previously skipped steps for context
- Verify-Settings summary with OK/MISSING/WARN counts
- `Write-ActionOK` helper consolidates 17 single-line DRY-RUN success-message guards

**Simplification:**
- DNS virtual-adapter filter extracted to `$CFG_VirtualAdapterFilter` in config.env.ps1
- Removed dead functions: `Write-ToolInfo`, `Write-RiskBadge`
- Removed 12 WHAT-docstrings that restated function names

**New:**
- Pester 5.x test suite: 7 test files, 233 test cases covering core helpers
- CI: Pester test job, EstimateKey cross-reference check, enhanced PSScriptAnalyzer rules
- CI: security workflow with Restart-Computer/Remove-Item gate checks
- Verify-Settings: added missing qWave + 4 Xbox service checks (was only SysMain + WSearch)
- GUI system-analysis: 9 additional checks for GUI-CLI parity

**Documentation accuracy (F1 pass):**
- `docs/video-settings.md`: removed `-novid` from recommended launch options (no-op in CS2, confirmed in code)
- `README.md`: fixed docs/ file count from 16 to 17 (includes `video.txt`)
- `README.md`: fixed nvidia-drs-settings.md description to match actual doc structure (12 sections, not "10-cluster/8 findings")
- `CHANGELOG.md`: updated test count from 203 to 233, test files from 6 to 7

