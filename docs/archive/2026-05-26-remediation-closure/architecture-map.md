# Architecture And Flow Map

Generated: 2026-05-26.

Scope inspected: root launchers and PowerShell entrypoints, `helpers/`,
`cfgs/`, `tests/`, `.github/workflows/`, `README.md`,
`docs/architecture.md`, `docs/code-index.md`, and
`docs/verification-baseline.md`.

This map describes current code and documented behavior. Where behavior is not
proved by code, docs, config, or tests, it is marked `UNCLEAR`.

## Runtime Shape

This is an administrator-run Windows PowerShell suite for CS2 optimization. It
has no web server, daemon, database, package build step, or long-running service
owned by the repo. Runtime work is launched from public batch files, root
PowerShell entrypoints, and a WPF dashboard.

High-level structure:

```text
START.bat
  -> Run-Optimize.ps1
       -> Setup-Profile.ps1
       -> Optimize-SystemBase.ps1
       -> Optimize-Hardware.ps1
       -> Optimize-RegistryTweaks.ps1
       -> Optimize-GameConfig.ps1
       -> RunOnce Phase 2 + Safe Mode boot
  -> Cleanup.ps1
  -> FpsCap-Calculator.ps1
  -> Verify-Settings.ps1
  -> Boot-SafeMode.ps1
  -> PostReboot-Setup.ps1
  -> backup summary / restore commands

START-GUI.bat
  -> CS2-Optimize-GUI.ps1
       -> helpers/gui-panels.ps1
       -> helpers/system-analysis.ps1
       -> helpers/step-catalog.ps1
       -> terminal launch of phase scripts when requested
```

`Run-Optimize.ps1` dot-sources the phase files in order. Those files execute
immediately when dot-sourced; they are workflow steps, not passive modules.

Shared helper loading:

```text
config.env.ps1
helpers.ps1
  -> logging.ps1
  -> tier-system.ps1
  -> step-state.ps1
  -> system-utils.ps1
  -> hardware-detect.ps1
  -> debloat.ps1
  -> msi-interrupts.ps1
  -> gpu-driver-clean.ps1
  -> nvidia-driver.ps1
  -> nvidia-drs.ps1
  -> backup-restore.ps1
  -> network-diagnostics.ps1
  -> storage-health.ps1
  -> nvidia-profile.ps1
  -> benchmark-history.ps1
  -> power-plan.ps1
  -> process-priority.ps1
```

Dot-sourcing is a core boundary. Helpers share caller `$SCRIPT:` state such as
`Profile`, `Mode`, `DryRun`, `CurrentPhase`, `CurrentStepTitle`, backup buffers,
and phase counters.

## Main Entry Points

| Entry point | Starts | Runtime role | Important contract |
| --- | --- | --- | --- |
| `START.bat` | User double-clicks or runs as administrator | Terminal menu and elevation wrapper | Menu targets must keep launching shipped scripts. CI checks launcher target text. |
| `START-GUI.bat` | User runs GUI launcher as administrator | Elevates and starts WPF dashboard | Must launch `CS2-Optimize-GUI.ps1`; CI and security workflow check this. |
| `Run-Optimize.ps1` | `START.bat` option 1 or GUI terminal launch | Phase 1 orchestration | `-SmokeTest` marker, dot-source order, backup lock release in `finally`. |
| `Boot-SafeMode.ps1` | `START.bat` option S or GUI button | Re-entry into Phase 2 after Phase 1 handoff | Requires `state.json` and `phase1SafeModeReady`; validates copied RunOnce target. |
| `SafeMode-DriverClean.ps1` | HKLM RunOnce in Safe Mode, or manual launch | Phase 2 driver cleanup | Clears Safe Mode boot flag before driver work; registers Phase 3 after cleanup. |
| `PostReboot-Setup.ps1` | HKLM RunOnce in normal boot, or manual launch | Phase 3 final setup | Refuses Safe Mode, loads state, installs driver, applies device/profile/DNS/process settings. |
| `Cleanup.ps1` | `START.bat` option 2 | Cache cleanup and optional driver-refresh handoff | Driver refresh resets progress and reuses Phase 2/3 handoff. |
| `FpsCap-Calculator.ps1` | `START.bat` option 3 | Benchmark parser, FPS cap calculator, history writer | Parses `[VProf]` text; updates state/history outside dry-run. |
| `Verify-Settings.ps1` | `START.bat` option 6 | Read-only drift verifier | Windows-only compatibility guard; intended read-only. |
| `CS2-Optimize-GUI.ps1` | `START-GUI.bat` | WPF dashboard | `-SmokeTest` marker, named XAML elements, async runspace lifecycle. |

## Domain Primitives

- Profile: `SAFE`, `RECOMMENDED`, `COMPETITIVE`, `CUSTOM`, `YOLO`.
- Mode: `AUTO`, `CONTROL`, `INFORMED`, `YOLO`, `DRY-RUN`. Explicit saved modes
  are preserved; missing modes are derived from profile through
  `Get-ModeForProfile`.
- Tier: `T1`, `T2`, `T3`. `T1` is auto-run in normal profiles; `T2` and `T3`
  are filtered or prompted by profile and risk.
- Risk: `SAFE`, `MODERATE`, `AGGRESSIVE`, `CRITICAL`.
- Depth: `CHECK`, `REGISTRY`, `SERVICE`, `BOOT`, `DRIVER`, `NETWORK`,
  `FILESYSTEM`, `APP`.
- Step identity: `progress.json` uses composite keys shaped as `P{phase}:{step}`.
  Bare step numbers are intentionally not accepted for resume.
- Backup entry types: `registry`, `service`, `bootconfig`, `powerplan`, `drs`,
  `scheduledtask`, `nic_adapter`, `qos_uro`, `defender`, `pagefile`, `dns`.
- Runtime workspace: `C:\CS2_OPTIMIZE`.
- Safe Mode handoff marker: `state.json.phase1SafeModeReady`.

## Storage And State

No database was found.

Runtime files under `C:\CS2_OPTIMIZE`:

| File | Writer | Reader | Contract |
| --- | --- | --- | --- |
| `state.json` | `Save-SuiteState`, GUI settings, phase scripts, FPS calculator | Phase scripts, GUI, cleanup, calculator, verifier | Stores profile/mode/log level, GPU choice, FPS data, driver path, baseline data, handoff flags. Missing or corrupt state either blocks or falls back depending on entrypoint; explicit modes are preserved and missing modes derive from profile. |
| `progress.json` | `Complete-Step`, `Skip-Step`, `Clear-Progress` | phase resume, GUI dashboard, Phase 2/3 guards | Composite `P{phase}:{step}` keys; corrupt file is preserved as `.corrupt` and resume starts fresh. |
| `backup.json` | `Backup-*`, `Flush-BackupBuffer`, `Save-BackupData` | restore UI/CLI, backup summary, DNS restore | First captured value is the rollback source. Restore treats file contents as untrusted and applies allowlists. |
| `backup.lock` | `Set-BackupLock` | backup/restore initializers | Advisory lock for backup writes/restores; stale locks are removed after dead process, PID reuse, corrupt lock, or age. |
| `Logs/optimize_current.log` | logging helpers and tools | user/menu/log view | Current log only; log rotation is in logging helper. |
| `benchmark_history.json` | FPS calculator and benchmark capture | GUI dashboard/benchmark, Phase 3 | Capped history, local timestamps, avg FPS, 1 percent low, labels. |
| `latency_history.json` | network diagnostic helper | GUI network panel | Versioned run history with baseline/post kinds and heuristic latency results. |
| `nvidia_driver.exe` | NVIDIA download flow | Phase 3 driver install | Must be an existing `.exe` path and pass signature checks before execution. |

Other file-system writes:

- CS2 config: `optimization.cfg` and optional CFG files are written to the CS2
  config tree when Step 34 finds a trusted CS2 install path.
- `video.txt`: GUI video panel can write a trusted Steam userdata `video.txt`
  and preserves the first `video.txt.bak`.
- Runtime payload copy: Phase 1 Step 38, `Boot-SafeMode.ps1`, and cleanup driver
  refresh copy required scripts, helpers, CFGs, and selected docs to
  `C:\CS2_OPTIMIZE`.
- Temp/cache cleanup: cleanup and shader-cache steps remove files from CS2,
  NVIDIA, DirectX, Windows temp, prefetch, and related paths.

Windows/system state writes:

- Registry: all normal suite writes should go through `Set-RegistryValue`.
- BCD: normal boot config writes should go through `Set-BootConfig`.
- RunOnce: `Set-RunOnce` writes HKLM RunOnce entries for Phase 2/3.
- Services and scheduled tasks: direct cmdlets are used after matching backup
  helpers capture state.
- DNS/QoS/NIC/power/device state: managed by network, MSI, power, and process
  helpers with backups where implemented.
- NVIDIA DRS binary database: written through `nvapi64.dll` interop when
  available; registry fallback exists.

## External APIs And Dependencies

Runtime dependencies:

- Windows PowerShell 5.1 is the supported full runtime path.
- WPF assemblies are required for `CS2-Optimize-GUI.ps1`.
- Windows command/cmdlet surface includes `bcdedit`, `powercfg`, `fsutil`,
  `netsh`, `pnputil`, `Get-CimInstance`, registry provider, AppX cmdlets,
  scheduled task cmdlets, DNS/NIC cmdlets, `Optimize-Volume`, `Get-MpPreference`,
  and service cmdlets.
- NVIDIA flow uses NVIDIA web/download endpoints, Authenticode signatures,
  installer extraction/execution, and `nvapi64.dll` DRS calls.
- Steam integration is file/protocol based: registry path discovery,
  `steam://validate/730`, CS2 workshop benchmark output, CS2 config files.
- Network diagnostics use `Test-Connection` ICMP against repo-owned heuristic
  target definitions.

Development dependencies:

- Pester 5.x for tests.
- PSScriptAnalyzer for static checks.
- GitHub Actions workflows for Windows PowerShell compatibility, smoke, Pester,
  parser, security, and retired-surface grep checks.

## Configuration Sources

| Source | Meaning |
| --- | --- |
| `config.env.ps1` | Central suite constants, workdir paths, DNS maps, NIC tweak maps, benchmark URLs, CS2 autoexec map, latency-target file path, RunOnce execution policy. |
| `cfgs/valve-latency-targets.json` | Heuristic Valve/Steam region candidate hosts for diagnostics. Not an official in-match ping API. |
| `cfgs/*.cfg` | Optional CS2 network, audio, and telemetry/debug presets. |
| `docs/video.txt` | Reference video settings content used by docs/guide flows. |
| `state.json` | User-selected or GUI-selected profile/mode/GPU/FPS/driver state. |
| `progress.json` | Resume decisions and dashboard progress. |
| User prompts | Profile, risk consent, GPU/FPS inputs, DNS choices, manual driver path, restore choices, reboot decisions. |
| Windows runtime state | Registry, services, BCD, devices, power plans, Steam/CS2 paths, WMI/CIM hardware info. |

## Public Contracts That Must Not Break

- Public entrypoint names and `-SmokeTest` output markers are CI contracts.
- `START.bat` and `START-GUI.bat` launcher targets are CI/security contracts.
- `helpers.ps1` remains a backward-compatible dot-source loader.
- `config.env.ps1` is dot-sourced by entrypoints and copied to the runtime
  payload; arbitrary code in it runs with administrator privileges.
- `C:\CS2_OPTIMIZE` is the runtime workspace contract.
- `state.json`, `progress.json`, `backup.json`, `backup.lock`,
  `benchmark_history.json`, and `latency_history.json` schemas are runtime
  compatibility contracts.
- `progress.json` step keys must remain `P{phase}:{step}`.
- Backup first-value preservation must remain intact; reruns must not replace a
  pre-suite original value with a suite-mutated value.
- `Complete-Step` must flush backup entries before saving progress.
- Restore allowlists must remain narrower than live write surfaces because
  `backup.json` can be tampered with.
- RunOnce script paths must remain under `C:\CS2_OPTIMIZE\` and end in `.ps1`.
- Safe Mode handoff depends on copied payload files existing before RunOnce is
  registered.
- Phase 2 must clear the Safe Mode boot flag before driver cleanup.
- Phase 3 must not continue real driver/setup work while Safe Mode is active.
- Dry-run must not persist registry, BCD, backup, RunOnce, DNS, device, or file
  mutations except the explicit Safe Mode recovery override in Phase 2.
- Optional CFG files must remain non-auto-executed presets unless behavior is
  explicitly changed and tested.
- `cfgs/valve-latency-targets.json` schema must expose version, notes, targets,
  labels, and candidate hosts.
- CI's retired estimate-key guard remains a public workflow contract while
  branch protection depends on it.

## Compatibility And Deprecation Layers

- `helpers.ps1` is an intentional compatibility loader, not a module boundary.
- `mode` is still stored/derived for compatibility with saved state and banners.
- Windows PowerShell 5.1 is the supported full path; PowerShell 7 has known gaps
  such as pagefile handling.
- `Test-SystemCompatibility` degrades on ARM64, Constrained Language Mode,
  Server/LTSC, missing AppX cmdlets, and PowerShell 7.
- `Get-WmiObject` remains where PS 5.1 behavior is needed.
- BCD handling uses raw hex element IDs where possible to avoid localized text.
- CS2 autoexec keeps some no-op/deprecated CVars as documented stubs.
- NVIDIA profile application retains registry fallback where DRS is unavailable.
- Some docs include legacy/debunked options for education; do not treat old
  changelog entries as current runtime behavior.

## Hidden Coupling

- Dot-sourced helpers share `$SCRIPT:` state. Moving helpers to modules would
  break implicit state unless all callers are updated.
- Phase script import order is workflow order. Reordering dot-sources changes
  behavior.
- `Invoke-TieredStep` sets `CurrentStepTitle`, which write wrappers use to label
  backup entries.
- `Set-RegistryValue` and `Set-BootConfig` are coupled to backup helpers through
  `CurrentStepTitle`.
- `Complete-Step` is coupled to backup flushing; progress should not be written
  if backup persistence fails.
- GUI panels depend on XAML element names, `El()`, `$Window`, `$Script:Root`,
  `$Script:UISync`, and helper functions existing in the same scope.
- GUI settings write `state.json`; terminal Phase 1 reads the same profile/mode
  and may skip menus for YOLO.
- The GUI Optimize panel depends on `helpers/step-catalog.ps1` staying aligned
  with phase scripts and docs.
- `Boot-SafeMode.ps1` trusts `phase1SafeModeReady` as proof that the runtime
  payload was prepared by Phase 1.
- Phase 3 Step 1 checks Phase 2 progress key `P2:2`; wrong progress state can
  make it warn incorrectly about clean driver removal.
- Network diagnostic DNS changes use backup/restore infrastructure but create
  GUI-specific backup step names.
- `config.env.ps1` path constants are copied into the runtime payload. A copied
  payload can diverge from the checkout if changed after Phase 1.

## State Transitions

Primary phase states:

```text
No state
  -> Phase 1 Step 1 creates state.json
  -> Phase 1 steps write backup.json/progress.json
  -> Phase 1 Step 38 copies runtime payload, registers Phase 2, sets Safe Mode,
     marks phase1SafeModeReady
  -> reboot into Safe Mode
  -> Phase 2 clears Safe Mode, removes GPU driver, registers Phase 3
  -> reboot into normal mode
  -> Phase 3 installs driver and applies final settings
  -> final restart may be requested
```

Progress states:

```text
missing/corrupt progress.json -> start at step 1
completed step -> backup flush -> progress completedSteps += P{phase}:{step}
skipped step -> progress skippedSteps += P{phase}:{step}
resume -> first unprocessed step in current phase
clear -> reset progress object to empty state
```

Backup states:

```text
no backup.json -> Initialize-Backup creates it
write step starts -> backup lock acquired by phase script
write wrappers/Backup-* add pending entries
step boundary -> Flush-BackupBuffer writes unique entries
restore -> backup lock acquired, allowlisted entries restored
success -> restored entries removed
failure/partial -> entries retained for retry
```

Dry-run states:

```text
DRY-RUN selected -> state.mode = DRY-RUN
Invoke-TieredStep previews allowed actions
write wrappers print intended writes and avoid mutation
Complete-Step/Skip-Step do not record progress
Phase 2 Safe Mode flag removal can override dry-run for boot safety
```

## Error-Handling Strategy

- Entry scripts generally set `Set-StrictMode -Version Latest`.
- Some entrypoints use `$ErrorActionPreference = "Stop"`; Phase 3 uses
  `"Continue"` around a broad try/catch.
- `Invoke-TieredStep` catches action failures, reports the failed step, increments
  failed counters, flushes backups if possible, and continues.
- High-risk phase scripts use outer `try/finally` blocks to release
  `backup.lock`.
- Phase 2 has explicit crash-recovery instructions and registers Phase 3
  best-effort on unexpected errors.
- Phase 3 refuses Safe Mode and attempts to clear Safe Mode plus re-register
  itself before exiting.
- Corrupt `state.json`, `progress.json`, and `backup.json` are preserved where
  implemented before reset or fallback.
- Many hardware/Windows probes are best-effort and return null/info/warnings
  rather than crashing.
- Restore reports partial/failure counts and retains unresolved backup entries.
- GUI async work surfaces background errors in message boxes and disposes
  runspaces/timers on close.

Failure can still be silent or misleading when a best-effort path logs a warning
but later code records progress, when external commands return localized or
unexpected output, when hardware heuristics pick the wrong device, or when GUI
status reflects stale files rather than live runtime state.

## Major Flows

### 1. Terminal Menu Flow

- Starts from: `START.bat`, after an administrator check and self-elevation.
- Inputs: user menu choice; untrusted but constrained by batch labels.
- Validation: `net session` admin check; exact menu choices route to labels.
- Reads: repo script paths relative to `%~dp0`, current log path via config for
  log viewing.
- Writes: none directly except reset progress/restore/backup summary inline
  commands can load helpers and mutate runtime state.
- Can fail: elevation denied, missing scripts, PowerShell unavailable, helper
  load failure, invalid runtime state for selected flow.
- Failure surfaced: console output, pause, underlying script errors.
- Tests: workflow contract tests for launcher targets; CI launcher contract
  check; E2E smoke covers script targets, not interactive menu.
- Wrong-result risk: menu can launch correctly while target script later
  silently skips or partially applies settings; batch tests do not prove
  interactive menu labels still match user-facing text.

### 2. Phase 1 Optimization Flow

- Starts from: `Run-Optimize.ps1`.
- Inputs: user profile/mode/GPU/FPS prompts, existing `state.json`,
  `progress.json`, hardware/WMI/registry state, config constants, Steam/CS2
  paths, NVIDIA metadata.
- Trusted/untrusted: repo code/config are high-trust; user prompts, Windows
  state, network responses, and existing JSON files are untrusted or
  environment-derived.
- Validation: profile choice loops, numeric FPS parse, compatibility warnings,
  profile/risk filtering, trusted path checks for CS2/Steam/video paths, URL
  allowlisting for downloads, registry/BCD path/name validation in wrappers.
- Reads: `config.env.ps1`, helper modules, runtime state/progress/backup,
  registry, BCD, services, tasks, NICs, WMI/CIM, filesystem caches, Steam paths.
- Writes: `state.json`, `progress.json`, `backup.json`, logs, registry, BCD,
  services/tasks, AppX/provisioned packages, NIC/QoS/URO, power plan, CS2 CFGs,
  runtime payload, RunOnce, downloaded NVIDIA installer.
- Can fail: bad permissions, corrupt JSON, backup lock, missing Windows cmdlets,
  unsupported hardware, failed registry/BCD writes, network/download/signature
  failure, path discovery failure, file locks, user cancellation.
- Failure surfaced: step warnings/errors, `Invoke-TieredStep` failure messages,
  debug log, phase summary, manual recovery instructions.
- Tests: profile matrix, dry-run compliance, state persistence, backup/restore,
  config tests, helper-specific tests for hardware, power, NIC, NVIDIA, debloat,
  system utils, E2E smoke, workflow smoke.
- Wrong-result risk: hardware detection can choose wrong GPU/NIC/RAM state;
  progress may skip rerun if an earlier version recorded a step incorrectly;
  best-effort write failures can leave partial settings; docs/step catalog can
  drift from phase scripts; PowerShell 7 checks cannot prove PS 5.1 runtime.

### 3. Safe Mode Handoff Flow

- Starts from: Phase 1 Step 38 or `Boot-SafeMode.ps1`.
- Inputs: `state.json`, `phase1SafeModeReady`, source checkout path,
  `C:\CS2_OPTIMIZE`, user restart confirmation, BCD state.
- Trusted/untrusted: state and copied runtime files are trusted only after
  secure directory/path checks; user confirmation and BCD result are external.
- Validation: `Boot-SafeMode.ps1` requires `state.json` and
  `phase1SafeModeReady`; `Copy-PhaseRuntimePayload` requires specific files;
  `Set-RunOnce` validates name, path, policy, target existence, and ACL;
  BCD result is checked by exit code or `Test-BootConfigSet`.
- Reads: runtime state, source files, BCD, profile/dry-run mode.
- Writes: copied scripts/helpers/cfgs/docs, ACLs, RunOnce Phase 2,
  BCD `safeboot`, `phase1SafeModeReady`, progress Step 38.
- Can fail: missing state, stale `phase1SafeModeReady`, missing payload files,
  ACL failure, RunOnce rejection, BCD failure, restart denied.
- Failure surfaced: console warnings/errors and manual `bcdedit` instructions.
- Tests: system-utils payload/RunOnce tests, storage-hardening tests, boot config
  dry-run tests, E2E smoke, workflow smoke.
- Wrong-result risk: copied payload can become stale relative to checkout;
  `phase1SafeModeReady` proves a previous handoff but not necessarily that the
  current payload still matches source; BCD verification can be incomplete on
  unusual Windows builds.

### 4. Phase 2 Driver Clean Flow

- Starts from: HKLM RunOnce in Safe Mode, or manual
  `SafeMode-DriverClean.ps1`.
- Inputs: `state.json`, `SAFEBOOT_OPTION`, GPU choice/detection, user consent,
  BCD state, installed drivers/services/tasks/packages.
- Trusted/untrusted: `state.json` can be missing/corrupt and falls back to
  defaults if user agrees; installed device/service state is external.
- Validation: Safe Mode is checked via `SAFEBOOT_OPTION`; if absent, user must
  confirm; GPU choice maps to known vendors; BCD clearing verifies state when
  delete fails.
- Reads: state, progress, backup, BCD, environment, GPU/driver state.
- Writes: clears Safe Mode BCD, backup/progress/logs, driver/service/task/AppX
  removals via `Remove-GpuDriverClean`, RunOnce Phase 3.
- Can fail: corrupt state, BCD delete failure, not actually in Safe Mode, driver
  removal errors, missing RunOnce target, backup lock, shutdown failure.
- Failure surfaced: red recovery block, manual BCD commands, warnings, best-
  effort Phase 3 registration, retained normal-boot recovery path.
- Tests: GPU driver clean helper tests, backup/restore tests, state/progress
  tests, storage-hardening RunOnce tests, E2E/CI smoke. The Windows-only
  PostReboot smoke test is skipped locally on non-Windows.
- Wrong-result risk: continuing outside Safe Mode can run without crashing but
  not cleanly remove driver files; fallback GPU detection can choose NVIDIA
  defaults; Phase 3 registration can fail after driver cleanup, requiring manual
  user action.

### 5. Phase 3 Post-Reboot Flow

- Starts from: HKLM RunOnce in normal boot, `START.bat` option P, or GUI
  terminal launch.
- Inputs: `state.json`, progress, driver path, GPU choice, profile/dry-run mode,
  Windows/NVIDIA/AMD/device/network state, user choices.
- Trusted/untrusted: `state.json.nvidiaDriverPath` is explicitly treated as
  tamperable and validated; manual driver path and network/DNS choices are
  untrusted.
- Validation: Safe Mode guard, driver path traversal/null/extension/existence
  checks, NVIDIA signature checks, profile/risk filtering, DNS adapter filtering,
  backup helpers, registry wrappers.
- Reads: state/progress/backup, driver files, NVIDIA metadata, registry, DRS,
  devices, services, DNS/NICs, benchmark history, CS2/Steam paths.
- Writes: driver install and post-install tweaks, AppX cleanup, MSI/NIC affinity,
  DRS/registry profile, VBS registry, DNS, process priority/scheduled task,
  video guidance outputs, benchmark history, progress/logs, optional final
  restart.
- Can fail: Safe Mode still active, missing/invalid/unsigned driver, NVIDIA site
  unavailable, DRS unavailable, hardware cmdlets fail, DNS adapter not found,
  user cancels, reboot failure.
- Failure surfaced: warnings/errors, message text, step skip, fatal error block,
  retained backup/progress behavior.
- Tests: NVIDIA driver/profile/DRS tests, MSI/process/network/power helper
  tests, state/progress tests, backup/restore tests, dry-run compliance, E2E
  smoke, CI Windows smoke.
- Wrong-result risk: installer can report success while driver/device state is
  still unhealthy; DRS registry fallback may not match true DRS behavior; DNS
  changes can apply to the wrong active adapter if adapter identity changes;
  final benchmark claims depend on user-supplied `[VProf]` text.

### 6. Backup And Restore Flow

- Starts from: `Initialize-Backup` during phases, `Set-RegistryValue`,
  `Set-BootConfig`, explicit `Backup-*` helpers, `START.bat` restore/summary,
  GUI Backup panel, GUI DNS restore.
- Inputs: current system state before writes, pending backup buffer,
  `backup.json`, selected step/title, user confirmation.
- Trusted/untrusted: live state is external; `backup.json` is explicitly treated
  as untrusted restore input.
- Validation: backup lock, stale lock handling, dedupe rules, restore allowlists
  for registry/services/tasks/BCD/script paths, GUID/path/name/value checks,
  adapter identity checks, binary byte-range checks.
- Reads: `backup.json`, lock file, registry/services/tasks/BCD/power/DNS/DRS.
- Writes: `backup.json`, `backup.lock`, restored registry/services/tasks/BCD/
  power/DNS/DRS/pagefile state, removal of successfully restored entries.
- Can fail: lock held, corrupt backup, unsupported/tampered entry, service/task
  missing, DRS unavailable, BCD/powercfg failure, pagefile automation failure.
- Failure surfaced: warnings, partial counts, retained failed entries, GUI
  message boxes.
- Tests: backup helper tests, integration backup-restore roundtrip, storage
  hardening, security validation, DNS restore tests.
- Wrong-result risk: backup buffer can be lost if process crashes before flush;
  first-value dedupe can preserve an already-mutated value if the first run did
  not capture pre-suite state; pagefile restore can be partial/manual; GUI clear
  backup deletes rollback data after confirmation.

### 7. GUI Dashboard Flow

- Starts from: `START-GUI.bat` -> `CS2-Optimize-GUI.ps1`.
- Inputs: XAML events, state/progress/backup/history files, Windows hardware and
  registry state, user settings, benchmark text, DNS/profile/video actions.
- Trusted/untrusted: UI input and runtime JSON are untrusted; XAML element names
  are internal contracts; background probes read external Windows state.
- Validation: named element lookup warns on missing elements; settings writes
  use `Save-SuiteState`; video path trust checks; DNS/profile helpers validate
  adapters and backup; terminal launch uses repo-root script names hardcoded in
  event handlers.
- Reads: state, progress, backup, benchmark/latency history, hardware/registry,
  CS2/Steam/video files, step catalog.
- Writes: settings to `state.json`, startup drift timestamp, DNS settings and
  DNS backup entries, benchmark history, `video.txt`, backup restore/clear
  changes, launch of terminal scripts.
- Can fail: missing WPF assemblies, missing XAML element, background runspace
  errors, stale JSON, Windows cmdlet failure, path trust failure, backup lock.
- Failure surfaced: message boxes, collapsed/error UI state, debug log.
- Tests: GUI panel helper tests, system-analysis tests, benchmark/network/video
  helper tests, E2E smoke. There is no screenshot or full WPF interaction test
  in the current local baseline.
- Wrong-result risk: dashboard cards can show stale file-based state; startup
  drift check is throttled for 60 minutes; GUI can save a profile that Phase 1
  later interprets through compatibility mode logic; visual correctness is not
  automatically tested.

### 8. Network Diagnostic And DNS Flow

- Starts from: GUI Network panel or Phase 3 DNS step.
- Inputs: active adapters, DNS provider choice, `cfgs/valve-latency-targets.json`,
  ICMP results, latency history.
- Trusted/untrusted: target JSON is repo-owned; network reachability, DNS state,
  adapter state, and user provider choices are external/untrusted.
- Validation: target JSON requires labels and candidate hosts; adapters are
  filtered against virtual/VPN regex; DNS provider is a validate-set; GUI DNS
  writes backup unless skipped.
- Reads: latency targets, active adapters, DNS state, latency history,
  `backup.json`.
- Writes: `latency_history.json`, DNS server settings, DNS backup entries.
- Can fail: target file missing/corrupt, all ICMP probes time out, DNS cmdlets
  unavailable, no active adapter, backup lock.
- Failure surfaced: thrown errors in helper, GUI message boxes, timeout-only
  diagnostic rows, warning text in Phase 3.
- Tests: `tests/helpers/network-diagnostics.Tests.ps1`, backup restore DNS
  integration, GUI panel tests.
- Wrong-result risk: heuristic ICMP targets are not official in-match ping;
  ICMP can be blocked or routed differently from CS2 traffic; adapter selection
  can pick a non-game path; DNS changes do not prove lower in-match latency.

### 9. FPS Cap And Benchmark Flow

- Starts from: `FpsCap-Calculator.ps1`, Phase 1 baseline prompt, Phase 3 final
  benchmark, or GUI Benchmark panel.
- Inputs: user-pasted `[VProf]` output, manual average FPS, clipboard, existing
  benchmark history, config cap percentage/minimum.
- Trusted/untrusted: all benchmark text is user-supplied; history is local JSON.
- Validation: parser requires recognized `Avg` and `P1` formats; manual average
  must be positive where prompted; cap uses config range guards from
  `config.env.ps1`.
- Reads: config, state, benchmark history, clipboard.
- Writes: clipboard, `state.json` FPS fields, `benchmark_history.json`, log.
- Can fail: malformed text, empty clipboard, clipboard unavailable, corrupt
  history, state write failure.
- Failure surfaced: warnings, skipped history comparison, debug log.
- Tests: hardware-detect benchmark parsing/FPS cap tests, benchmark-history
  tests, GUI panel benchmark tests, E2E smoke.
- Wrong-result risk: copied benchmark text can be from the wrong map/run;
  single-run values can be variance; GUI cap parse does not prove the user
  applied the cap in NVIDIA/AMD/CS2.

### 10. Cleanup And Driver Refresh Flow

- Starts from: `Cleanup.ps1`.
- Inputs: cleanup mode choice, Steam/CS2 paths, cache/temp paths, GPU state,
  existing `state.json`, user Safe Mode confirmation.
- Trusted/untrusted: user mode choice is constrained; path discovery and state
  are environment-derived; cache paths must not be broadened without tests.
- Validation: menu loop, dry-run gates, path existence checks, GPU choice
  fallback prompt for driver refresh, BCD verification for Safe Mode.
- Reads: config paths, Steam path, state/progress/backup, filesystem caches,
  event logs, DNS/Winsock state.
- Writes: deletes caches/temp/prefetch/event logs, flushes DNS, trims RAM,
  triggers Steam validation protocol, copies runtime payload, clears progress,
  registers Phase 2, sets Safe Mode BCD.
- Can fail: locked files, missing Steam, Windows command failure, missing GPU
  state, payload copy failure, BCD failure, backup lock.
- Failure surfaced: warnings, summary, thrown copy errors, manual Safe Mode
  instructions.
- Tests: E2E smoke, system-utils tests, dry-run compliance, hardware path tests.
  UNCLEAR whether cleanup modes have full destructive-path unit coverage for all
  cache/event-log variants.
- Wrong-result risk: file deletion can report success while locked files remain;
  Steam validation protocol launch does not prove Steam verified files; driver
  refresh reuses Phase 2/3 risks.

### 11. Verify Settings Flow

- Starts from: `Verify-Settings.ps1` or GUI Optimize verify action.
- Inputs: current Windows registry, BCD, power, QoS, DNS, TRIM, scheduled tasks,
  NVIDIA DRS state, runtime compatibility.
- Trusted/untrusted: all checked Windows state is external; local state is read
  only for runtime defaults.
- Validation: `Test-VerifyRuntimeCompatibility` blocks non-Windows and missing
  service cmdlet; individual checks normalize or classify missing/changed/info.
- Reads: registry, BCD, powercfg, QoS/DNS, storage, scheduled tasks, DRS,
  hardware detection.
- Writes: none intended.
- Can fail: unsupported runtime, missing cmdlets, unreadable registry/DRS/BCD,
  or unavailable runtime checks.
- Failure surfaced: console status rows, summary counters, compatibility message,
  and debug logs from individual checks.
- Tests: `tests/Verify-Settings.Tests.ps1`, system-utils verify counters,
  helper-specific tests, E2E smoke.
- Wrong-result risk: `OK` means "matches suite expectation", not "best for every
  machine"; missing hardware-specific checks can be `INFO`; DRS verification
  depends on `nvapi64.dll` availability.

### 12. CI And Verification Flow

- Starts from: GitHub Actions push/PR/workflow_dispatch or local developer
  commands from docs.
- Inputs: repository files, workflows, Pester/PSScriptAnalyzer modules, Windows
  and Ubuntu runners.
- Trusted/untrusted: CI runner environment is external; workflows pin actions
  and use read-only token permissions.
- Validation: parser, PSScriptAnalyzer, Windows PowerShell 5.1 smoke, Pester,
  process E2E smoke, entrypoint smoke, launcher contracts, retired-surface grep,
  secret/safety/workflow scans.
- Reads: all `.ps1`, `.psd1`, `.bat`, CFG, docs, workflow files, tests.
- Writes: CI `test-results.xml` artifact only; local runs may create ignored
  test result files.
- Can fail: module install/cache failure, Windows-only behavior unavailable
  locally, pattern-scan false positive, stale branch-protection required checks.
- Failure surfaced: CI job failures, local command exit codes, baseline doc.
- Tests: workflow-contract tests protect the workflow surface itself.
- Wrong-result risk: smoke markers only prove startup short-circuit; mocked
  Pester tests do not prove live admin/device behavior; grep scans can miss
  semantic security issues or flag harmless text.

## Dependency Boundaries

Keep these boundaries narrow:

- Entry scripts own user prompts and orchestration.
- `config.env.ps1` owns static configuration only; adding runtime logic here
  broadens the admin trust boundary.
- `helpers/system-utils.ps1` owns privileged write wrappers, state persistence,
  secure ACLs, payload copy, RunOnce, and BCD/registry validation.
- `helpers/backup-restore.ps1` owns rollback persistence and restore allowlists.
- Domain helpers should own one Windows surface each: NVIDIA, GPU cleanup, MSI,
  power, process priority, network, storage, hardware detection.
- GUI helpers should present or trigger existing domain helpers; they should not
  create separate optimization logic without tests and docs.

## Known Unclear Areas

- UNCLEAR: whether every destructive cleanup path has focused tests for all
  possible Windows path variants. Proof would be tests or Windows evidence for
  each deletion target and locked-file case.
- UNCLEAR: whether GUI screenshots match the current WPF UI. Proof would be a
  fresh screenshot comparison or GUI smoke with captured visuals.
- UNCLEAR: live NVIDIA DRS and driver installer behavior across driver versions.
  Proof would require Windows/NVIDIA hardware runs, not only source-shape tests.
- UNCLEAR: live NIC/QoS/MSI behavior across hardware vendors. Proof would require
  Windows hardware verification and rollback tests.
- UNCLEAR: whether heuristic latency targets still represent useful route
  proxies over time. Proof would require refreshed target evidence.
- UNCLEAR: full Windows PowerShell 5.1 compatibility from this macOS host. The
  verification baseline marks it unavailable locally.

## Do Not Break First

Highest-risk contracts to protect before refactoring:

1. `backup.json` first-value rollback and restore allowlists.
2. `progress.json` `P{phase}:{step}` semantics.
3. Safe Mode clearing before Phase 2 driver removal.
4. Runtime payload copy plus RunOnce path validation.
5. Dry-run interception for registry, BCD, RunOnce, DNS, and external writes.
6. Public entrypoint names and smoke markers.
7. GUI/state coupling for profile/mode.
8. NVIDIA download/signature/DRS fallback validation.
9. DNS/NIC adapter selection and backup before mutation.
10. CI workflow contracts, including launcher targets and retired-surface guard.
