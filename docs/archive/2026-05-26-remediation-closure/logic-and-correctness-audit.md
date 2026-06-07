# Logic and correctness audit

Date: 2026-05-26

Scope: read-only audit of high-risk runtime paths, helper APIs, GUI verification
state, restore logic, benchmark parsing, and tests that claim to protect those
areas. Production code was not modified.

Not a full Windows runtime validation: the audit used source inspection, existing
tests, and small parser/runtime probes on the local macOS host. Findings below
distinguish confirmed issues from suspected issues.

## Confirmed issues

### LC-001 - Write helpers allow false completed steps

- ID: LC-001
- Status: Confirmed
- Location: `helpers/system-utils.ps1:467-501`, `Optimize-RegistryTweaks.ps1:152-231`, `Optimize-Hardware.ps1:20-23`
- Evidence: `Set-RegistryValue` catches `Set-ItemProperty` failures, logs a warning, and returns no failure value. Large phase actions then call `Complete-Step` after many registry writes. `Set-BootConfig` returns `$false` on `bcdedit` failure, but Step 10 ignores the return value and still calls `Complete-Step`.
- Why it matters: progress can say an optimization step is complete even when required registry or boot settings were skipped. Resume and GUI status can then hide silent partial failure.
- Minimal reproduction or reasoning: mock `Set-ItemProperty` to throw in any Step 27 registry write, or mock `bcdedit` failure in Step 10. The action path still reaches `Complete-Step` unless the caller explicitly checks, which these examples do not.
- Existing test coverage: dry-run tests assert no writes happen; `tests/Optimize-Hardware.Tests.ps1` asserts only that `disabledynamictick` is requested, not that failure blocks completion.
- Missing test that should exist: a required write failure should prevent `Complete-Step` and should surface a partial/failure status.
- Suggested minimal fix: make required write helpers return success/failure or throw for required writes; aggregate outcomes in each action and call `Complete-Step` only when required writes succeeded. Keep optional writes explicitly marked optional.
- Risk level: High
- Verification command or strategy: add Pester tests with mocked `Set-ItemProperty` and mocked `bcdedit` failures; run targeted helper and phase tests, then full Pester.
- Confidence: High

### LC-002 - Safe Mode handoff can continue after RunOnce registration fails

- ID: LC-002
- Status: Confirmed
- Location: `helpers/system-utils.ps1:336-390`, `Optimize-GameConfig.ps1:662-693`, `Boot-SafeMode.ps1:101-120`, `SafeMode-DriverClean.ps1:196-209`
- Evidence: `Set-RunOnce` rejects invalid paths, missing files, invalid execution policies, and registry failures by logging and returning no status. Step 38 and `Boot-SafeMode.ps1` do not check whether registration succeeded before setting Safe Mode. `Boot-SafeMode.ps1` unconditionally prints "Phase 2 registered via RunOnce" after calling the helper. Phase 2 also marks Phase 3 complete after calling `Set-RunOnce` without checking registration.
- Why it matters: the machine can be sent into Safe Mode without a reliable Phase 2 autostart, or reboot after driver removal without a reliable Phase 3 autostart. This is a high-risk false-success path around boot state and driver removal.
- Minimal reproduction or reasoning: force `Set-RunOnce` to fail, then allow `bcdedit` to succeed. The code proceeds to set `phase1SafeModeReady` and `Complete-Step`, or prints the registration success line.
- Existing test coverage: helper tests cover invalid `Set-RunOnce` inputs and execution-policy rejection; no caller test asserts that Safe Mode setup halts if RunOnce registration fails.
- Missing test that should exist: Step 38 and `Boot-SafeMode.ps1` should refuse to set Safe Mode and should not complete progress when RunOnce registration fails.
- Suggested minimal fix: have `Set-RunOnce` return `$true` only after the registry write succeeds; callers must require that value before setting Safe Mode, readiness flags, or phase progress.
- Risk level: High
- Verification command or strategy: add Pester tests that mock `Set-RunOnce` failure and assert no `bcdedit`, no readiness flag, and no `Complete-Step`; manually smoke the Safe Mode handoff on Windows.
- Confidence: High

### LC-003 - Driver-clean phase can report complete with no driver removed

- ID: LC-003
- Status: Confirmed
- Location: `helpers/gpu-driver-clean.ps1:304-340`, `helpers/gpu-driver-clean.ps1:472-483`, `SafeMode-DriverClean.ps1:202-209`
- Evidence: `Remove-GpuDriverClean` warns when no display driver packages are found, counts failed removals, and still prints "GPU DRIVER CLEAN REMOVAL COMPLETE" and "Ready for clean driver installation." The caller does not inspect a return value and marks Phase 2 Step 2 complete immediately after the call.
- Why it matters: a failed or empty driver removal can be recorded as successful, then Phase 3 can proceed as if the driver was removed cleanly.
- Minimal reproduction or reasoning: mock driver enumeration to return no packages, or make every `pnputil /delete-driver` call fail. The helper still reaches the green summary and the caller still calls `Complete-Step`.
- Existing test coverage: several GPU clean tests assert source text patterns such as service names, precise folder patterns, and fallback text. They do not assert behavior for "all removals failed" or "no package found but driver still installed."
- Missing test that should exist: all driver package removal failures should return a failed or partial result and prevent Step 2 completion unless a postcondition proves the driver was already absent.
- Suggested minimal fix: return a structured result with found/removed/failed/alreadyAbsent counts; make Phase 2 complete only when removal succeeded or a verified already-removed state exists.
- Risk level: High
- Verification command or strategy: add mocked Pester tests for no packages, partial failures, and all failures; on Windows, verify Device Manager or WMI display adapter state after a real Safe Mode run.
- Confidence: High

### LC-004 - Partial NVIDIA DRS writes are treated as successful

- ID: LC-004
- Status: Confirmed
- Location: `helpers/nvidia-profile.ps1:190-229`, `helpers/nvidia-profile.ps1:370-395`, `PostReboot-Setup.ps1:382-396`
- Evidence: per-setting DRS write failures increment `$errors` and are described as "non-fatal", but `Apply-NvidiaCS2ProfileDrs` returns `$true` after the session. The summary still prints checkmarked setting lines, and Phase 3 Step 4 calls `Complete-Step` after `Apply-NvidiaCS2Profile` without checking for partial success.
- Why it matters: the NVIDIA profile can be partially applied while progress and summary copy imply the profile step is complete. This can leave key driver settings missing without forcing a retry.
- Minimal reproduction or reasoning: mock `[NvApiDrs]::SetDwordSetting` to throw for one setting while the session itself succeeds. `$SCRIPT:_drsErrors` becomes nonzero, but the function returns `$true` and the caller completes the step.
- Existing test coverage: `tests/helpers/nvidia-profile.Tests.ps1` covers table shape, selected IDs, registry fallback calls, and dry-run no-throw. It does not cover DRS partial failure semantics.
- Missing test that should exist: one rejected required DRS setting should produce a partial or failed result and should not mark the step fully complete.
- Suggested minimal fix: return a structured DRS result; classify required setting failures as partial/failure; remove checkmarks for failed settings; only complete progress when required settings and required registry locks are verified.
- Risk level: High
- Verification command or strategy: add mocked DRS-session tests; on Windows/NVIDIA, verify the written DRS profile with `Verify-Settings.ps1` or NVIDIA Profile Inspector after applying.
- Confidence: High

### LC-005 - Phase 3 DNS can report success after failed or partial writes

- ID: LC-005
- Status: Confirmed
- Location: `PostReboot-Setup.ps1:611-694`, contrasted with safer helper `helpers/network-diagnostics.ps1:387-442`
- Evidence: Phase 3 calls `Set-DnsClientServerAddress` without `-ErrorAction Stop`, prints `Write-OK` immediately after, catches only around the whole block, and calls `Complete-Step` after the catch. The GUI/network helper uses `-ErrorAction Stop`, but Phase 3 does not reuse it.
- Why it matters: DNS can remain unchanged, be changed on only some adapters, or fail with a non-terminating error while the phase records DNS as complete.
- Minimal reproduction or reasoning: mock `Set-DnsClientServerAddress` to emit a non-terminating error or fail on the second selected adapter. The loop can still report OK for attempted writes, and the action reaches `Complete-Step`.
- Existing test coverage: tests cover `Set-NetworkDiagnosticDnsProfile`, not the Phase 3 DNS branch.
- Missing test that should exist: Phase 3 DNS should fail or record partial status when any selected adapter write fails, and should verify the post-write DNS state.
- Suggested minimal fix: reuse `Set-NetworkDiagnosticDnsProfile` or a shared adapter-write helper; use `-ErrorAction Stop`, collect per-adapter results, verify with `Get-DnsClientServerAddress`, and withhold completion on partial failure.
- Risk level: Medium
- Verification command or strategy: add Pester tests for one-adapter failure and multi-adapter partial failure; on Windows, run the DNS step against a disposable adapter profile and verify post-state.
- Confidence: High

### LC-006 - Service restore can lose delayed-start or running-state restoration silently

- ID: LC-006
- Status: Confirmed
- Location: `helpers/backup-restore.ps1:1069-1080`
- Evidence: service startup type restore uses `Set-Service -ErrorAction Stop`, but restoring `DelayedAutostart` and restarting originally running services use `-ErrorAction SilentlyContinue`. The function still prints "Restored" and increments `$restoreOk`.
- Why it matters: rollback can say a service is restored while the delayed-start qualifier or running status was not restored. That is silent wrong rollback state.
- Minimal reproduction or reasoning: make `Set-ItemProperty` for `DelayedAutostart` fail or make `Start-Service` fail. The code has no postcondition check and still counts the service as restored.
- Existing test coverage: tests capture the delayed-auto flag during backup, but no test forces delayed-auto restore failure or verifies the restored delayed flag.
- Missing test that should exist: delayed-auto restore and restart failure should increment partial/fail counters and keep the backup entry for retry.
- Suggested minimal fix: use `-ErrorAction Stop` for delayed-start and restart operations, or verify post-state after each; report partial restore if ancillary state cannot be restored.
- Risk level: Medium
- Verification command or strategy: add Pester tests for delayed-start and restart failure; on Windows, round-trip a disposable allowed service or mock service state with postcondition verification.
- Confidence: High

### LC-007 - GUI inline verification rewrites progress from observed state, not suite execution

- ID: LC-007
- Status: Confirmed
- Location: `helpers/gui-panels.ps1:480-591`
- Evidence: `Start-InlineVerify` maps registry/service checks to step keys, adds matching keys to `progress.json.completedSteps`, updates `phase`, and saves progress. It does not prove the suite made those changes, that backup entries exist, or that all step side effects were completed. The DRS-related check for `P3:4` only checks `PerfLevelSrc`, not the DRS database.
- Why it matters: progress can be recovered for values already present due to defaults, manual edits, group policy, or a partial prior run. Resume can then skip steps with missing backups or missing side effects.
- Minimal reproduction or reasoning: pre-seed one expected registry value manually, delete progress, and run inline verify. The step key is added to completed progress without executing the step.
- Existing test coverage: startup drift helpers are tested; `Start-InlineVerify` progress mutation and provenance are not directly tested.
- Missing test that should exist: inline verification should not mutate `completedSteps` unless it also proves the step was applied by the suite, or it should write a separate `verifiedSteps`/`observedState` field.
- Suggested minimal fix: stop writing to `completedSteps` from GUI verification, or separate "observed applied" from "completed by suite"; require backup/provenance evidence before resume skips a step.
- Risk level: Medium
- Verification command or strategy: add GUI helper tests with mocked registry checks and progress file; verify resume behavior after inline verify.
- Confidence: High

### LC-008 - Startup drift throttle returns "no drift" when no check ran

- ID: LC-008
- Status: Confirmed
- Location: `helpers/gui-panels.ps1:39-64`
- Evidence: when `startup_last_verified` is less than 60 minutes old, `Test-StartupConfigDrift` returns `Skipped = true`, `HasDrift = false`, `DriftCount = 0`, and `CheckedCount = 0` without reading registry state.
- Why it matters: the dashboard can hide drift introduced after the last quick check while reporting a false non-drift result to downstream UI logic.
- Minimal reproduction or reasoning: create state with `startup_last_verified` 10 minutes ago, change one checked registry value, and call `Test-StartupConfigDrift`; the function exits before checking and returns `HasDrift = false`.
- Existing test coverage: `tests/helpers/gui-panels.Tests.ps1:233-238` explicitly asserts that recent timestamps skip the probe.
- Missing test that should exist: skipped checks should surface an "unknown/stale" state instead of `HasDrift = false`.
- Suggested minimal fix: represent skipped as unknown/not checked, or keep the banner neutral until a check runs; do not treat skipped as healthy.
- Risk level: Medium
- Verification command or strategy: add tests for skipped state rendering and for forced refresh after registry drift.
- Confidence: High

### LC-009 - Benchmark parsers can crash on malformed numeric tokens

- ID: LC-009
- Status: Confirmed
- Location: `helpers/hardware-detect.ps1:113-125`, `helpers/gui-panels.ps1:118-137`, `FpsCap-Calculator.ps1:79-84`
- Evidence: the regex accepts `[\d.]+`, then casts/parses without `TryParse`. A local PowerShell probe confirmed `[float]::Parse("300..0", InvariantCulture)` throws. Manual FPS-cap input calls `Parse-BenchmarkOutput` without a local try/catch in that branch.
- Why it matters: pasted benchmark-like text with malformed numbers can crash an interactive calculator or GUI event handler instead of returning "invalid input."
- Minimal reproduction or reasoning: input `[VProf] FPS: Avg=300..0, P1=200` matches the regex; parsing `300..0` throws.
- Existing test coverage: tests cover standard, whitespace, multiple-run, empty, garbage, and integer-only values, but not malformed matched numbers.
- Missing test that should exist: invalid matched numeric tokens should return `$null` or a warning without throwing.
- Suggested minimal fix: replace `[\d.]+` with a stricter invariant-culture number pattern, or use `TryParse` after matching and reject invalid tokens.
- Risk level: Low
- Verification command or strategy: add parser tests for `300..0`, `.`, `300.`, comma decimal input, and very large values; run benchmark and GUI helper tests.
- Confidence: High

### LC-010 - Soft state loader drops valid profile data when `mode` is absent

- ID: LC-010
- Status: Confirmed
- Location: `helpers/system-utils.ps1:316-333`
- Evidence: `Initialize-ScriptDefaults` reads `$st.mode` directly under `Set-StrictMode -Version Latest`. A local PowerShell probe confirmed missing property access throws `PropertyNotFoundException`. The catch resets mode, log level, profile, and dry-run to defaults, so a state file with `profile` but no `mode` loses the profile.
- Why it matters: older or partially written `state.json` can silently change future entry points back to `RECOMMENDED`/`CONTROL`, changing profile behavior without telling the user which field was missing.
- Minimal reproduction or reasoning: write `state.json` as `{ "profile": "SAFE" }`, run an entry point that calls `Initialize-ScriptDefaults`, and the catch path defaults profile to `RECOMMENDED`.
- Existing test coverage: state tests cover missing `profile` and missing `logLevel`, but not missing `mode`.
- Missing test that should exist: missing `mode` should preserve existing `profile` and default only `mode`.
- Suggested minimal fix: mirror `Load-State` property checks: read each property only after `PSObject.Properties[...]` exists, and warn/debug on field-level defaults rather than catching the entire state load.
- Risk level: Medium
- Verification command or strategy: add state-persistence tests for missing `mode`, missing `profile`, and malformed single fields.
- Confidence: High

## Suspected issues needing runtime or history verification

### LC-011 - DNS restore can target a stale interface index if adapter name is gone

- ID: LC-011
- Status: Suspected
- Location: `helpers/backup-restore.ps1:1314-1343`
- Evidence: DNS restore tries to resolve the adapter by name. If the adapter is not found, it logs that it is using the stored `InterfaceIndex` and proceeds to call `Set-DnsClientServerAddress`. Windows can reuse interface indexes after adapter changes, but this was not validated in this audit.
- Why it matters: a restore could apply old DNS settings to the wrong adapter if the original adapter was removed and another adapter owns the stored index.
- Minimal reproduction or reasoning: create a DNS backup for adapter A with index N, remove/rename A, create adapter B with index N, then restore. The code will not fail closed when A is absent.
- Existing test coverage: integration tests cover changed interface index when adapter name resolves, and retained entries when DNS restore throws. They do not cover missing adapter plus reused stored index.
- Missing test that should exist: when adapter name cannot be resolved, restore should fail or require confirmation instead of applying to the stored index.
- Suggested minimal fix: fail closed when the named adapter is absent, unless an additional immutable adapter identifier proves the stored index still belongs to the original adapter.
- Risk level: Medium
- Verification command or strategy: mock `Get-NetAdapter` absent and assert no DNS write; on Windows, validate interface index reuse behavior before deciding compatibility impact.
- Confidence: Medium

### LC-012 - Registry check helper is scalar-only but looks generic

- ID: LC-012
- Status: Suspected
- Location: `helpers/system-utils.ps1:608-650`, `Verify-Settings.ps1:566-572`
- Evidence: `Test-RegistryCheck` compares `$result -eq $Expected`. `Verify-Settings.ps1` comments that this is reference equality for `byte[]`, so `UserPreferencesMask` uses a special inline binary comparison instead.
- Why it matters: future binary or array registry checks could falsely report changed/missing state if routed through the generic-looking helper. Current code appears to avoid this for the known binary check.
- Minimal reproduction or reasoning: compare two distinct `[byte[]](1,2)` arrays via the helper; scalar equality does not perform the intended positional binary comparison.
- Existing test coverage: helper tests cover scalar OK/changed/missing cases only.
- Missing test that should exist: binary registry values should either be rejected by `Test-RegistryCheck` or compared with a byte-aware path.
- Suggested minimal fix: rename the helper to indicate scalar-only behavior, or add type-aware comparisons for byte arrays and arrays.
- Risk level: Low
- Verification command or strategy: add byte-array comparison tests and search for current/future binary callers before changing behavior.
- Confidence: Medium

## Misleading or weak tests

### LC-013 - High-risk driver and DRS tests assert implementation trivia

- ID: LC-013
- Status: Confirmed
- Location: `tests/helpers/gpu-driver-clean.Tests.ps1:123-169`, `tests/helpers/nvidia-drs.Tests.ps1:19-90`, `tests/helpers/nvidia-profile.Tests.ps1:117-135`
- Evidence: several tests read source text and assert that strings or constants exist. The DRS dry-run test only asserts no throw. These tests can pass while the failure semantics in LC-003 and LC-004 remain wrong.
- Why it matters: these tests create confidence around driver removal and NVAPI interop without proving runtime behavior, postconditions, or failure handling.
- Minimal reproduction or reasoning: remove the return/status handling entirely from driver clean or keep DRS returning success after partial failure; the source-string tests still pass.
- Existing test coverage: mostly structural and smoke-level.
- Missing test that should exist: behavior tests for no driver found, all driver removals failed, partial DRS setting rejection, backup failure, and postcondition verification.
- Suggested minimal fix: keep a few source-shape tests only where they protect constants, but add behavior tests around outcomes and failure propagation.
- Risk level: Medium
- Verification command or strategy: write mocked Pester tests for LC-003 and LC-004; run the existing suite to prove these tests fail before the fix and pass after.
- Confidence: High

## Highest-risk files

- `helpers/system-utils.ps1`: shared write helpers, RunOnce, state loading.
- `Optimize-GameConfig.ps1`: Safe Mode handoff and many completion decisions.
- `Boot-SafeMode.ps1`: direct Safe Mode shortcut with unchecked RunOnce success.
- `SafeMode-DriverClean.ps1` and `helpers/gpu-driver-clean.ps1`: driver removal and phase handoff.
- `helpers/nvidia-profile.ps1`: NVAPI/DRS partial failure handling.
- `PostReboot-Setup.ps1`: DNS and Phase 3 completion logic.
- `helpers/backup-restore.ps1`: rollback correctness and state restoration.
- `helpers/gui-panels.ps1`: GUI progress and drift state.

## Areas not fully inspected

- Every phase step was not exhaustively simulated with failing Windows APIs.
- The audit did not run a real Windows Safe Mode, DNS, DRS, bcdedit, service, or driver-removal flow.
- External APIs and current NVIDIA/NVAPI behavior were not verified against live primary documentation.
- CFG files and docs were not audited for gameplay correctness beyond code references.
- Existing dirty/untracked worktree files were not changed and were not normalized.

## Recommended next audit targets

1. Build a mocked failure matrix for required side-effect helpers: registry, boot config, RunOnce, service, DNS, scheduled task, DRS, and driver removal.
2. Decide a repository-wide result contract for phase actions: success, skipped, partial, failed, and verified.
3. Separate "observed applied state" from "suite completed this step" in GUI and resume state.
4. Add Windows-only smoke/manual verification scripts for Safe Mode handoff, DNS write/restore, and DRS verification.

## Verification performed for this audit

- Source inspection with `rg`, `sed`, and `nl` across runtime helpers, phase scripts, GUI code, verifier code, and relevant tests.
- Local PowerShell probe confirmed malformed numeric parse throws.
- Local PowerShell probe confirmed missing PSCustomObject property access throws under `Set-StrictMode -Version Latest`.

No production code was changed. No full test suite was run for this audit.
