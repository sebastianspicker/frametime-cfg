# Refactor And Code-Quality Plan

Generated: 2026-05-26.

Purpose: convert the current audit bundle into small, independently reviewable
implementation slices. This plan does not authorize speculative features or
broad rewrites.

Inputs:

- `AGENTS.md`
- `docs/code-index.md`
- `docs/verification-baseline.md`
- `docs/architecture-map.md`
- `docs/deprecation-and-simplification-audit.md`
- `docs/logic-and-correctness-audit.md`

Current baseline: local verification is partial. PowerShell 7 parser,
PSScriptAnalyzer, Pester, E2E smoke, entrypoint smoke, and security grep checks
ran in the baseline. Stronger confidence is blocked by one CI-style retired
surface grep failure in docs and by Windows-only runtime checks that cannot run
on the current macOS host.

Ordering rules for this plan:

1. Fix verification infrastructure first.
2. Fix false success and silent wrong behavior before style.
3. Fix boot, driver, DNS, restore, and privileged-write paths before cosmetic
   cleanup.
4. Remove dead or deprecated code only when usage evidence is clear.
5. Simplify before abstracting.
6. Keep every slice small enough to review and roll back independently.

Common verification gates:

- Docs-only slices: `git diff --check -- <changed-files>` plus a no-index
  whitespace check for newly created files.
- PowerShell parser gate: parse all `.ps1` files with
  `[System.Management.Automation.Language.Parser]::ParseFile(...)`.
- Lint gate: run PSScriptAnalyzer with `PSScriptAnalyzerSettings.psd1`.
- Unit gate: `pwsh -NoProfile -Command "Invoke-Pester tests -CI"`.
- E2E smoke: `pwsh -NoProfile -Command "Invoke-Pester tests/e2e -CI"`.
- Entrypoint smoke: run the eight `-SmokeTest` entrypoints listed in
  `docs/verification-baseline.md`.
- Windows authority: Windows PowerShell 5.1 and administrator runtime checks are
  required for slices that touch BCD, RunOnce, registry mutation, services,
  DNS, drivers, DRS, WPF, or reboot behavior.

## Execution Sequence

### RP-001 - Restore The Clean Verification Baseline

- ID: RP-001
- Title: Restore the clean verification baseline
- Problem: `docs/verification-baseline.md` records a current CI-style grep
  failure caused by retired exact symbol references in documentation. Later
  implementation work should not start from a known workflow-blocking docs
  failure.
- Findings addressed: verification baseline failure; code-index deprecated
  compatibility note for the retired guard.
- Files affected: `docs/code-index.md`, `docs/verification-baseline.md`.
- Behavior affected: no runtime behavior; CI/doc verification surface only.
- Public contracts affected: the existing retired-surface workflow guard.
- Storage/migration impact, if any: none.
- Tests to add or update: no tests expected; update docs only if the observed
  baseline result changes.
- Verification commands: run the retired-surface grep recorded in
  `docs/verification-baseline.md`; run `git diff --check -- docs/code-index.md
  docs/verification-baseline.md`.
- Rollback strategy: revert the docs wording changes.
- Risk level: Low.
- Ordering rationale: this is the only known current verification blocker and
  must be fixed before relying on CI as a signal for riskier code changes.
- Definition of Done: the retired-surface grep exits 0, the baseline document
  accurately reflects the new result, and no production code changes are in the
  slice.

### RP-002 - Make Required Write Helpers Return Explicit Results

- ID: RP-002
- Title: Make required write helpers return explicit results
- Problem: required registry and RunOnce writes can fail while callers have no
  reliable value to gate progress or reboot decisions.
- Findings addressed: LC-001, LC-002.
- Files affected: `helpers/system-utils.ps1`,
  `tests/helpers/system-utils.Tests.ps1`,
  `tests/integration/dryrun-compliance.Tests.ps1`.
- Behavior affected: shared helper calls expose success, failure, skipped, or
  dry-run result values; existing callers that ignore the return value should
  keep their current behavior until later slices gate them.
- Public contracts affected: dot-sourced helper behavior for
  `Set-RegistryValue`, `Set-RunOnce`, and related tests. Do not remove the
  existing command names.
- Storage/migration impact, if any: none.
- Tests to add or update: helper tests for successful write, terminating
  failure, non-terminating failure when applicable, invalid input, and dry-run.
  Dry-run tests must prove no mutation is performed and that the result is not
  misreported as a real write.
- Verification commands: targeted Pester for system-utils and dry-run
  compliance, then parser and PSScriptAnalyzer.
- Rollback strategy: revert the helper return-shape changes and tests.
- Risk level: Medium.
- Ordering rationale: caller fixes need deterministic helper results before they
  can safely prevent false progress.
- Definition of Done: helper tests prove failure is observable, dry-run remains
  non-mutating, and no caller completion behavior changes except where tests
  explicitly cover it.

### RP-003 - Block Unsafe RunOnce And Safe Mode Handoffs

- ID: RP-003
- Title: Block unsafe RunOnce and Safe Mode handoffs
- Problem: Phase handoff code can arm Safe Mode or record phase progress after
  RunOnce registration fails.
- Findings addressed: LC-002, DAS-12.
- Files affected: `Optimize-GameConfig.ps1`, `Boot-SafeMode.ps1`,
  `SafeMode-DriverClean.ps1`, `Cleanup.ps1`, `helpers/system-utils.ps1`,
  `tests/helpers/system-utils.Tests.ps1`, `tests/e2e/entrypoints.Tests.ps1`,
  and new `tests/phase-handoff.Tests.ps1`.
- Behavior affected: Safe Mode BCD changes, phase readiness flags, reboot
  prompts, and Phase 2/3 progress are blocked when required RunOnce setup fails.
- Public contracts affected: RunOnce target validation, `phase1SafeModeReady`,
  `progress.json` step completion, public entrypoints, and `-SmokeTest` markers.
- Storage/migration impact, if any: prevents false writes to `state.json` and
  `progress.json`; no schema migration expected.
- Tests to add or update: mocked caller tests where `Set-RunOnce` returns
  failure and callers assert no BCD write, no readiness flag, no reboot prompt,
  and no `Complete-Step`.
- Verification commands: targeted handoff tests, system-utils tests, E2E
  smoke, entrypoint smoke, parser, PSScriptAnalyzer, and Windows Safe Mode smoke
  before release.
- Rollback strategy: revert the caller gates while keeping RP-002 helper result
  tests if they remain valid.
- Risk level: High.
- Ordering rationale: boot-state false success is a high-risk runtime failure
  and should precede lower-risk cleanup.
- Definition of Done: every Safe Mode or Phase 3 handoff path refuses to proceed
  when required RunOnce registration fails, and the user sees a recovery message
  instead of a success message.

### RP-004 - Gate Required Registry And Boot Writes Before Step Completion

- ID: RP-004
- Title: Gate required registry and boot writes before step completion
- Problem: phase steps can call `Complete-Step` after required registry or BCD
  writes fail.
- Findings addressed: LC-001.
- Files affected: `Optimize-Hardware.ps1`, `Optimize-RegistryTweaks.ps1`,
  `helpers/system-utils.ps1`, `tests/Optimize-Hardware.Tests.ps1`, and new
  `tests/Optimize-RegistryTweaks.Tests.ps1`.
- Behavior affected: required write failures surface as step failure or partial
  status; optional or best-effort writes remain explicitly marked optional.
- Public contracts affected: `progress.json` completion semantics,
  `backup.json` flush-before-progress contract, and console summary counters.
- Storage/migration impact, if any: no schema migration; fewer false
  `completedSteps` entries after failed writes.
- Tests to add or update: mock `Set-BootConfig` failure in Step 10; mock
  `Set-RegistryValue` failure in representative required registry steps; assert
  no `Complete-Step` and a visible failure/partial result.
- Verification commands: targeted phase tests, step-state tests, dry-run
  compliance tests, parser, PSScriptAnalyzer, and full Pester.
- Rollback strategy: revert caller gates and tests for this slice only.
- Risk level: High.
- Ordering rationale: progress false success is the root cause for many later
  resume and GUI correctness problems.
- Definition of Done: required write failures cannot be recorded as completed
  steps, and tests show optional writes are still allowed to degrade safely.

### RP-005 - Return Structured Driver Cleanup Results

- ID: RP-005
- Title: Return structured driver cleanup results
- Problem: driver cleanup can print success and allow Phase 2 completion when
  no driver was removed or all removals failed.
- Findings addressed: LC-003, LC-013.
- Files affected: `helpers/gpu-driver-clean.ps1`,
  `SafeMode-DriverClean.ps1`, `tests/helpers/gpu-driver-clean.Tests.ps1`, and
  new `tests/SafeMode-DriverClean.Tests.ps1`.
- Behavior affected: Phase 2 completes driver-clean progress only after a
  successful removal or a verified already-absent state.
- Public contracts affected: Phase 2 progress key, Safe Mode recovery messaging,
  driver-clean console summary, and backup entries for services/tasks/packages.
- Storage/migration impact, if any: no schema migration; prevents false Phase 2
  `completedSteps` entries.
- Tests to add or update: behavior tests for no packages found, all removals
  failed, partial removal failure, already absent, and dry-run. Replace or
  demote source-string assertions that do not protect behavior.
- Verification commands: targeted GPU driver cleanup tests, Phase 2 smoke,
  parser, PSScriptAnalyzer, full Pester, and Windows Safe Mode driver-clean
  smoke before release.
- Rollback strategy: revert the structured result and caller gate; keep any
  test-only source-shape reductions only if they still match the old behavior.
- Risk level: High.
- Ordering rationale: driver cleanup is destructive and currently has confirmed
  false-success behavior.
- Definition of Done: Phase 2 cannot mark driver cleanup complete when removal
  failed, and tests fail if result counts are ignored.

### RP-006 - Treat Partial NVIDIA DRS Writes As Partial Failure

- ID: RP-006
- Title: Treat partial NVIDIA DRS writes as partial failure
- Problem: one or more failed required DRS writes can still return success and
  produce checkmarked summary text.
- Findings addressed: LC-004, LC-013, DAS-08.
- Files affected: `helpers/nvidia-profile.ps1`,
  `helpers/nvidia-drs.ps1` if session result plumbing is needed,
  `PostReboot-Setup.ps1`, `tests/helpers/nvidia-profile.Tests.ps1`,
  `tests/helpers/nvidia-drs.Tests.ps1`, and `tests/PostReboot-Setup.Tests.ps1`.
- Behavior affected: required DRS setting failures produce partial or failed
  status; Phase 3 Step 4 does not fully complete on partial DRS application.
- Public contracts affected: NVIDIA profile result semantics, Phase 3 progress,
  and user-facing summary text. Registry fallback remains a compatibility path
  until a later evidence-gated cleanup slice.
- Storage/migration impact, if any: no schema migration; backup entries for DRS
  and registry fallback must remain restorable.
- Tests to add or update: mock one required DRS write failure, all DRS write
  failures, DRS unavailable with registry fallback, and backup failure. Tests
  must assert result status and displayed summary, not only no-throw behavior.
- Verification commands: targeted NVIDIA profile and DRS tests,
  PostReboot-Setup tests, parser, PSScriptAnalyzer, full Pester, and Windows
  NVIDIA hardware verification with an external profile inspector or equivalent
  driver evidence before release.
- Rollback strategy: revert result plumbing and caller gate; leave no partial
  result shape in public docs unless the code remains.
- Risk level: High.
- Ordering rationale: DRS profile application is a high-risk hardware path and
  currently allows silent partial success.
- Definition of Done: failed required DRS writes are visible, progress is not
  marked complete on partial application, and tests fail if the function returns
  unconditional success.

### RP-007 - Reuse Verified DNS Write Semantics In Phase 3

- ID: RP-007
- Title: Reuse verified DNS write semantics in Phase 3
- Problem: Phase 3 DNS writes can report success after non-terminating or
  partial adapter failures, while the network diagnostic helper already has
  stronger error handling.
- Findings addressed: LC-005.
- Files affected: `PostReboot-Setup.ps1`,
  `helpers/network-diagnostics.ps1` if a helper seam is needed,
  `tests/PostReboot-Setup.Tests.ps1`,
  `tests/helpers/network-diagnostics.Tests.ps1`.
- Behavior affected: DNS step succeeds only when selected adapters are written
  and verified; partial adapter failure remains visible.
- Public contracts affected: DNS profile behavior, Phase 3 progress, DNS backup
  entries, and GUI/network helper semantics if reused.
- Storage/migration impact, if any: no schema migration; backup entries must
  still be captured before DNS mutation.
- Tests to add or update: one-adapter failure, second-adapter failure, no active
  adapter, successful write with post-state verification, and dry-run.
- Verification commands: targeted PostReboot and network diagnostic tests,
  backup DNS restore tests, parser, PSScriptAnalyzer, full Pester, and Windows
  DNS smoke on a disposable adapter/profile before release.
- Rollback strategy: revert Phase 3 DNS caller changes while keeping network
  helper behavior unchanged.
- Risk level: Medium.
- Ordering rationale: DNS mutation is less destructive than boot or driver work
  but still writes external state and currently has confirmed false-success
  behavior.
- Definition of Done: Phase 3 cannot print or record DNS success unless all
  required selected-adapter writes and postcondition checks pass.

### RP-008 - Preserve Partial Restore Failures

- ID: RP-008
- Title: Preserve partial restore failures
- Problem: restore can count service entries as restored when delayed-start or
  restart restoration silently fails; DNS restore may also fall back to a stale
  interface index when adapter identity is not proven.
- Findings addressed: LC-006, LC-011, DAS-13.
- Files affected: `helpers/backup-restore.ps1`,
  `tests/helpers/backup-restore.Tests.ps1`,
  `tests/integration/backup-restore-roundtrip.Tests.ps1`.
- Behavior affected: restore entries are retained for retry when ancillary
  state cannot be restored; DNS restore fails closed when adapter identity is
  not proved.
- Public contracts affected: `backup.json` restore semantics, restore summary
  counters, service allowlists, DNS backup entries, and GUI/terminal restore
  messaging.
- Storage/migration impact, if any: no schema migration; more failed or partial
  entries may remain in `backup.json` instead of being removed.
- Tests to add or update: delayed-start restore failure, service restart
  failure, missing adapter name with stored interface index, changed interface
  index with matching adapter name, and retry retention.
- Verification commands: targeted backup-restore helper tests, integration
  roundtrip tests, parser, PSScriptAnalyzer, full Pester, and Windows restore
  smoke with harmless registry/service/DNS fixtures before release.
- Rollback strategy: revert only the changed restore case handling and tests.
- Risk level: High.
- Ordering rationale: rollback trust is a core safety contract; false restore
  success can hide unrecovered system state.
- Definition of Done: restore success counters only include fully restored
  entries, partial failures remain retryable, and adapter identity must be
  proven before DNS state is written.

### RP-009 - Stop GUI Verification From Completing Runtime Steps

- ID: RP-009
- Title: Stop GUI verification from completing runtime steps
- Problem: GUI inline verification writes observed state into
  `progress.json.completedSteps`, letting resume skip steps that the suite did
  not actually run.
- Findings addressed: LC-007, DAS-11.
- Files affected: `helpers/gui-panels.ps1`,
  `helpers/step-state.ps1` only if a read-only observed-state helper is needed,
  `tests/helpers/gui-panels.Tests.ps1`,
  `tests/helpers/step-state.Tests.ps1`.
- Behavior affected: GUI can display observed state without mutating completed
  runtime progress.
- Public contracts affected: `progress.json` `completedSteps`, GUI Optimize
  panel status, and resume behavior.
- Storage/migration impact, if any: avoid schema changes if possible. If an
  observed-state field is necessary, make it additive and ensure old progress
  files still load.
- Tests to add or update: pre-seeded registry value with empty progress should
  display observed/applied state but must not add `P{phase}:{step}` to
  `completedSteps`; resume should not skip that step because of GUI verify.
- Verification commands: targeted GUI panel and step-state tests, parser,
  PSScriptAnalyzer, full Pester, GUI smoke, and manual Windows GUI resume smoke
  before release.
- Rollback strategy: revert GUI progress write changes and any additive storage
  field.
- Risk level: Medium.
- Ordering rationale: progress truth should be repaired before deduplicating
  verification tables or cleaning GUI code.
- Definition of Done: inline verification no longer causes runtime steps to be
  marked complete without suite provenance.

### RP-010 - Represent Throttled Startup Drift As Unknown

- ID: RP-010
- Title: Represent throttled startup drift as unknown
- Problem: startup drift throttling returns `HasDrift = false` when no check
  was performed.
- Findings addressed: LC-008.
- Files affected: `helpers/gui-panels.ps1`,
  `tests/helpers/gui-panels.Tests.ps1`.
- Behavior affected: dashboard state distinguishes skipped/stale checks from
  verified healthy state.
- Public contracts affected: GUI dashboard copy/status only; no runtime
  mutation contract.
- Storage/migration impact, if any: `startup_last_verified` remains compatible.
- Tests to add or update: recent timestamp returns a skipped/unknown state, UI
  rendering does not label it healthy, forced refresh detects drift, and old
  tests that asserted skipped equals no drift are updated.
- Verification commands: targeted GUI panel tests, parser, PSScriptAnalyzer,
  full Pester, and GUI smoke.
- Rollback strategy: revert the GUI helper and tests to old throttling behavior.
- Risk level: Medium.
- Ordering rationale: this fixes a false healthy UI state after progress truth
  is repaired.
- Definition of Done: no caller can treat a skipped drift check as verified no
  drift unless a fresh check actually ran.

### RP-011 - Load Partial State Files Field By Field

- ID: RP-011
- Title: Load partial state files field by field
- Problem: a `state.json` with `profile` but missing `mode` can fall through
  the catch path and reset valid profile data to defaults.
- Findings addressed: LC-010, DAS-06.
- Files affected: `helpers/system-utils.ps1`,
  `tests/helpers/system-utils.Tests.ps1`,
  `tests/integration/state-persistence.Tests.ps1`.
- Behavior affected: missing or malformed fields default independently instead
  of discarding the whole state object.
- Public contracts affected: `state.json` compatibility, profile/mode banners,
  GUI settings, and terminal Phase 1 defaults.
- Storage/migration impact, if any: no migration; improves tolerance for older
  or partial state files while preserving existing schema fields.
- Tests to add or update: missing `mode`, missing `profile`, missing `logLevel`,
  malformed single field, corrupt JSON file, and dry-run mode preservation.
- Verification commands: targeted system-utils and state-persistence tests,
  parser, PSScriptAnalyzer, full Pester, entrypoint smoke, and Windows smoke for
  profile loading before release.
- Rollback strategy: revert field-level loading and tests.
- Risk level: Medium.
- Ordering rationale: state compatibility affects every entrypoint and should
  be fixed before removing or migrating stale state fields.
- Definition of Done: valid fields survive when unrelated state fields are
  missing, and tests prove no silent profile downgrade occurs.

### RP-012 - Harden Benchmark Numeric Parsing

- ID: RP-012
- Title: Harden benchmark numeric parsing
- Problem: malformed benchmark-like numeric tokens can match the parser regex
  and then throw during numeric conversion.
- Findings addressed: LC-009.
- Files affected: `helpers/hardware-detect.ps1`,
  `helpers/gui-panels.ps1`, `FpsCap-Calculator.ps1`,
  `tests/helpers/hardware-detect.Tests.ps1`,
  `tests/helpers/gui-panels.Tests.ps1`.
- Behavior affected: malformed benchmark input returns invalid input or warning
  behavior instead of crashing an interactive calculator or GUI handler.
- Public contracts affected: accepted `[VProf]` text formats, FPS cap
  calculator output, GUI benchmark panel behavior, and benchmark history writes.
- Storage/migration impact, if any: no migration; invalid parses must not append
  to `benchmark_history.json`.
- Tests to add or update: `300..0`, `.`, trailing decimal, comma decimal input,
  huge values, empty input, garbage input, and valid integer/decimal input.
  Add parity tests if duplicate parser code remains.
- Verification commands: targeted hardware-detect and GUI panel tests, FPS cap
  smoke, parser, PSScriptAnalyzer, full Pester, and entrypoint smoke.
- Rollback strategy: revert parser changes and tests.
- Risk level: Low.
- Ordering rationale: this is a confirmed crash edge case, but it is less
  safety-critical than boot, driver, DNS, restore, and progress correctness.
- Definition of Done: malformed matched numbers cannot throw, valid formats
  still parse, and invalid input does not write benchmark history or state.

### RP-013 - Add Step Catalog Drift Protection

- ID: RP-013
- Title: Add step catalog drift protection
- Problem: the GUI step catalog mirrors executable phase steps by hand, so GUI
  status can drift from runtime scripts while current tests still pass.
- Findings addressed: DAS-10.
- Files affected: `helpers/step-catalog.ps1`,
  `tests/helpers/step-catalog.Tests.ps1`, and phase scripts only if tests prove
  metadata is currently wrong.
- Behavior affected: no intended runtime behavior change; verification catches
  metadata drift.
- Public contracts affected: GUI Optimize panel rows, progress key shape, step
  titles, and docs that describe phases.
- Storage/migration impact, if any: none.
- Tests to add or update: drift test that compares catalog `P{phase}:{step}`
  entries and titles against phase script declarations or a deterministic
  source scan. Keep the test deterministic and avoid a new runtime abstraction
  unless it replaces the duplicated source of truth.
- Verification commands: step-catalog tests, parser, PSScriptAnalyzer, full
  Pester, and GUI smoke.
- Rollback strategy: revert the drift test and any catalog corrections.
- Risk level: Medium.
- Ordering rationale: after progress semantics are fixed, metadata drift should
  be caught before larger GUI or phase edits continue.
- Definition of Done: a mismatched, missing, or duplicate phase step in the GUI
  catalog fails a deterministic test.

### RP-014 - Add Verification Table Drift Protection

- ID: RP-014
- Title: Add verification table drift protection
- Problem: GUI quick checks, GUI inline verification, CLI verification, and
  system analysis duplicate many registry and service checks.
- Findings addressed: DAS-11.
- Files affected: `helpers/gui-panels.ps1`, `Verify-Settings.ps1`,
  `helpers/system-analysis.ps1`, `tests/helpers/gui-panels.Tests.ps1`,
  `tests/Verify-Settings.Tests.ps1`,
  `tests/helpers/system-analysis.Tests.ps1`.
- Behavior affected: no user-visible behavior should change in this slice
  unless the drift test finds a proven mismatch that must be corrected.
- Public contracts affected: GUI dashboard truth, CLI verifier status, and
  user-facing analysis rows.
- Storage/migration impact, if any: none.
- Tests to add or update: deterministic comparison tests for shared check
  definitions such as MPO, Game Mode, Game DVR, Fast Startup, timer resolution,
  mouse acceleration, audio ducking, and Steam overlay. Do not create a new
  shared abstraction unless at least two active call sites use it immediately.
- Verification commands: targeted GUI, verifier, and system-analysis tests,
  parser, PSScriptAnalyzer, full Pester, and Windows verifier smoke before
  release.
- Rollback strategy: revert comparison tests and any proven mismatch fixes.
- Risk level: Medium.
- Ordering rationale: drift protection comes after false progress/healthy-state
  bugs so it can protect future simplification without changing semantics first.
- Definition of Done: duplicated verifier tables have a failing test for drift,
  or a documented keep decision exists for intentionally different checks.

### RP-015 - Prove And Remove Inactive Backup Version Pruning

- ID: RP-015
- Title: Prove and remove inactive backup version pruning
- Problem: backup-version pruning helpers and config appear implemented but
  unwired, creating false confidence that old backup versions are pruned.
- Findings addressed: DAS-01.
- Files affected: `helpers/backup-restore.ps1`, `config.env.ps1`,
  `tests/helpers/storage-hardening.Tests.ps1`, backup-related docs if they
  mention retention.
- Behavior affected: remove inactive retention code only if usage proof confirms
  it has no runtime caller; otherwise wire it explicitly in a separate plan.
- Public contracts affected: possible external dot-sourced helper use; no
  shipped entrypoint contract if usage is unproven.
- Storage/migration impact, if any: none if deleted as inactive; if retained and
  wired, storage behavior changes and needs a new plan.
- Tests to add or update: remove pruning-specific tests when code is deleted;
  keep backup initialization tests proving current backup creation still works.
- Verification commands: `rg` for pruning symbols, git-history review, targeted
  backup/storage tests, parser, PSScriptAnalyzer, full Pester, and backup
  initialization smoke on Windows.
- Rollback strategy: restore the deleted helper/config/tests.
- Risk level: Medium.
- Ordering rationale: deletion waits until correctness fixes and usage evidence
  are in place.
- Definition of Done: either the inactive pruning surface is deleted with usage
  proof and passing tests, or the slice closes with a documented keep decision
  and no code deletion.

### RP-016 - Prove And Remove The Uncalled Full-Restore Wrapper

- ID: RP-016
- Title: Prove and remove the uncalled full-restore wrapper
- Problem: `Restore-AllChanges` appears unused by shipped entrypoints and
  duplicates restore-all control flow that terminal and GUI paths implement
  differently.
- Findings addressed: DAS-02.
- Files affected: `helpers/backup-restore.ps1`,
  `tests/integration/backup-restore-entrypoints.Tests.ps1`,
  `tests/integration/backup-restore-roundtrip.Tests.ps1`, and docs if they
  mention direct helper usage.
- Behavior affected: removes only the uncalled wrapper if no public/external use
  is found; `Restore-Interactive` and GUI restore stay intact.
- Public contracts affected: possible dot-sourced helper API for external
  users; terminal and GUI restore entrypoints must not change.
- Storage/migration impact, if any: none.
- Tests to add or update: delete wrapper-only tests; keep entrypoint tests for
  terminal and GUI restore flows.
- Verification commands: `rg Restore-AllChanges`, git-history and docs search,
  targeted backup restore entrypoint/roundtrip tests, parser,
  PSScriptAnalyzer, full Pester, and manual terminal restore smoke with a
  harmless fixture on Windows.
- Rollback strategy: restore the wrapper and tests.
- Risk level: Medium.
- Ordering rationale: cleanup waits until restore partial-failure behavior is
  fixed so deletion does not hide correctness defects.
- Definition of Done: the duplicate wrapper is deleted only with usage proof,
  and the remaining public restore surfaces are verified.

### RP-017 - Remove Test-Only Hardware Wrapper Surfaces When Proven Unused

- ID: RP-017
- Title: Remove test-only hardware wrapper surfaces when proven unused
- Problem: `Test-XmpActive` and `Reset-CachedCpuInfo` appear unused by
  production code or exist only to support tests.
- Findings addressed: DAS-03, DAS-04.
- Files affected: `helpers/hardware-detect.ps1`,
  `tests/helpers/hardware-detect.Tests.ps1`,
  `tests/helpers/process-priority.Tests.ps1`.
- Behavior affected: no shipped runtime behavior should change if usage proof
  is correct; tests should target `Get-RamInfo` and production behavior instead
  of wrapper presence.
- Public contracts affected: possible external dot-sourced helper use.
- Storage/migration impact, if any: none.
- Tests to add or update: replace wrapper-specific tests with `Get-RamInfo`
  behavior tests; update cache-related tests to reset state through test setup
  without adding production-only test helpers.
- Verification commands: `rg` for both symbols, git-history search, targeted
  hardware-detect and process-priority tests, parser, PSScriptAnalyzer, full
  Pester, and GUI hardware analysis smoke.
- Rollback strategy: restore deleted wrappers and tests.
- Risk level: Low.
- Ordering rationale: low-risk cleanup follows correctness and restore work.
- Definition of Done: wrappers are deleted only if no runtime or public usage is
  found, and behavior tests still cover RAM/cache decisions.

### RP-018 - Clean Up State Timestamp And Profile-Mode Duplication

- ID: RP-018
- Title: Clean up state timestamp and profile-mode duplication
- Problem: `last_verified` is written but not read, while profile-to-mode
  mapping is duplicated between terminal setup, GUI settings, and state loading.
- Findings addressed: DAS-05, DAS-06, LC-010 follow-up.
- Files affected: `Verify-Settings.ps1`, `Setup-Profile.ps1`,
  `helpers/gui-panels.ps1`, `helpers/system-utils.ps1`,
  `tests/Verify-Settings.Tests.ps1`,
  `tests/helpers/gui-panels.Tests.ps1`,
  `tests/integration/state-persistence.Tests.ps1`.
- Behavior affected: either remove the write-only timestamp or make it an
  explicit read contract; centralize existing profile-to-mode mapping only if
  at least two active call sites use the helper immediately.
- Public contracts affected: `state.json`, GUI settings, terminal profile
  setup, verifier timestamp behavior, and dry-run mode handling.
- Storage/migration impact, if any: possible additive or removal change to
  `state.json.last_verified`; document compatibility impact before deletion.
- Tests to add or update: state migration/load tests, GUI settings save tests,
  verifier timestamp tests, dry-run profile matrix tests.
- Verification commands: targeted state, GUI, verifier, and profile matrix
  tests; parser; PSScriptAnalyzer; full Pester; entrypoint smoke; Windows
  verifier smoke.
- Rollback strategy: restore old state writes and mapping code.
- Risk level: High.
- Ordering rationale: storage cleanup waits until partial-state loading is
  fixed and verified.
- Definition of Done: there is one documented source of truth for profile-mode
  mapping, write-only timestamp behavior is either removed or read, and old
  state files still load predictably.

### RP-019 - Replace Deprecated Pagefile WMI Writes Only After Windows Proof

- ID: RP-019
- Title: Replace deprecated pagefile WMI writes only after Windows proof
- Problem: pagefile writes use deprecated WMI APIs while restore already has a
  CIM-based path, but Windows PowerShell 5.1 compatibility is an active
  contract.
- Findings addressed: DAS-09.
- Files affected: `Optimize-SystemBase.ps1`,
  `helpers/backup-restore.ps1`, `PSScriptAnalyzerSettings.psd1`,
  pagefile-related tests.
- Behavior affected: pagefile mutation and restore should remain equivalent
  across supported Windows PowerShell runtimes.
- Public contracts affected: Windows PowerShell 5.1 support, PowerShell 7
  parser/lint compatibility, pagefile backup/restore, and reboot-required
  messaging.
- Storage/migration impact, if any: no schema migration; backup entries must
  remain restorable.
- Tests to add or update: mocked pagefile write success/failure, backup capture,
  restore roundtrip, reboot-required summary, and PSScriptAnalyzer suppression
  narrowing if WMI is removed.
- Verification commands: targeted pagefile tests, backup restore roundtrip,
  parser, PSScriptAnalyzer, full Pester, Windows PowerShell 5.1 runtime test on
  a disposable VM, PowerShell 7 runtime smoke, and manual pagefile UI check
  after reboot.
- Rollback strategy: restore the WMI path and analyzer exclusion.
- Risk level: High.
- Ordering rationale: deprecated API cleanup is important but must not break the
  supported Windows runtime contract.
- Definition of Done: CIM replacement is proven equivalent on Windows
  PowerShell 5.1 and PowerShell 7, or the slice closes with a keep decision and
  documented evidence.

### RP-020 - Remove No-Op Generated CS2 Config Lines Only With Current Proof

- ID: RP-020
- Title: Remove no-op generated CS2 config lines only with current proof
- Problem: generated config includes documented no-op or deprecated CVars, which
  can look authoritative even when they are educational stubs.
- Findings addressed: DAS-07.
- Files affected: `config.env.ps1`, `Optimize-GameConfig.ps1`,
  `tests/config.Tests.ps1`, `docs/audio.md`, `docs/video-settings.md`,
  `docs/network-cfgs.md`, README sections that describe generated config.
- Behavior affected: generated `optimization.cfg` content and documentation.
- Public contracts affected: CS2 config generation, documented CVar count tests,
  optional CFG guidance, and user-visible explanations.
- Storage/migration impact, if any: no suite storage migration; generated CS2
  config content changes.
- Tests to add or update: config generation tests for the new count/content,
  docs cross-reference tests if present, and proof that educational notes moved
  to docs if removed from runtime config.
- Verification commands: current CS2 convar proof, targeted config tests,
  parser, PSScriptAnalyzer, full Pester, Step 34 dry-run/config-generation
  smoke, and manual CS2 console verification if available.
- Rollback strategy: restore removed config lines and tests.
- Risk level: Medium.
- Ordering rationale: config cleanup waits for current runtime proof; do not
  delete educational stubs based on names alone.
- Definition of Done: every removed generated line has current evidence that it
  is no-op/deprecated, docs explain compatibility impact, and generated config
  tests pass.

### RP-021 - Narrow NVIDIA Registry Fallback Only With Driver Evidence

- ID: RP-021
- Title: Narrow NVIDIA registry fallback only with driver evidence
- Problem: registry fallback writes may imply more runtime effect than modern
  drivers provide, but they may still be needed when DRS is unavailable.
- Findings addressed: DAS-08, RP-006 follow-up.
- Files affected: `helpers/nvidia-profile.ps1`,
  `docs/nvidia-optimization.md`, `docs/nvidia-drs-settings.md`,
  `tests/helpers/nvidia-profile.Tests.ps1`,
  `tests/integration/backup-restore-roundtrip.Tests.ps1`.
- Behavior affected: NVIDIA fallback writes and restore entries are narrowed or
  deleted only for keys proven ineffective.
- Public contracts affected: NVIDIA profile fallback behavior, DRS/registry
  restore, user-facing partial-success messages, and docs.
- Storage/migration impact, if any: existing backup entries for removed fallback
  keys must still be safely ignored or restorable; document compatibility
  impact.
- Tests to add or update: DRS-unavailable fallback tests, backup/restore
  roundtrip for remaining fallback keys, partial-success messaging tests, and
  docs updates.
- Verification commands: targeted NVIDIA profile tests, backup restore
  roundtrip, parser, PSScriptAnalyzer, full Pester, and live Windows/NVIDIA
  driver verification before release.
- Rollback strategy: restore deleted fallback keys and tests.
- Risk level: High.
- Ordering rationale: fallback deletion waits until partial DRS result semantics
  are truthful and live evidence exists.
- Definition of Done: fallback keys are narrowed only with driver evidence, DRS
  unavailable behavior remains truthful, and restore compatibility is verified.

### RP-022 - Narrow Empty-Catch Analyzer Suppression

- ID: RP-022
- Title: Narrow empty-catch analyzer suppression
- Problem: the analyzer exclusion for empty catches is broad and stale, hiding
  future silent failures.
- Findings addressed: DAS-14.
- Files affected: `PSScriptAnalyzerSettings.psd1`,
  `helpers/gui-panels.ps1`, `Verify-Settings.ps1`, `CS2-Optimize-GUI.ps1`, and
  any other exact empty-catch sites found by the slice.
- Behavior affected: non-teardown empty catches should log debug context or
  become explicit best-effort comments; true teardown noise can remain quiet
  with narrow justification.
- Public contracts affected: debug log content, GUI teardown behavior, verifier
  best-effort probes, and lint rules.
- Storage/migration impact, if any: none.
- Tests to add or update: focused tests for any catch whose behavior changes;
  no test needed for comment-only justification changes.
- Verification commands: `rg 'catch \\{\\}' -g '*.ps1'`, PSScriptAnalyzer,
  parser, targeted GUI/verifier tests, full Pester, entrypoint smoke, and manual
  GUI close/teardown smoke before release.
- Rollback strategy: restore prior analyzer settings and catch blocks.
- Risk level: Medium.
- Ordering rationale: lint hardening is valuable after confirmed false-success
  paths are fixed, because it prevents reintroducing the same class of bug.
- Definition of Done: broad suppression is narrowed, every remaining empty
  catch has a local reason, and PSScriptAnalyzer still passes.

## Deferred Or Explicitly Not Planned Yet

- Broad backup-restore rewrite: not planned. `helpers/backup-restore.ps1` is
  overcomplicated, but DAS-13 recommends changing one restore case at a time
  with focused tests.
- Broad GUI split: not planned. `helpers/gui-panels.ps1` and
  `CS2-Optimize-GUI.ps1` are large, but current confirmed issues are progress
  truth, drift state, and duplicated checks.
- Broad phase-script rewrite: not planned. Phase scripts are long and
  imperative, but each slice above touches only the steps needed for a confirmed
  issue.
- Local ignored agent documentation cleanup: not planned without explicit user
  confirmation because DAS-15 concerns ignored local artifacts, not production
  source.
- Generated screenshot refresh: not planned because no screenshot correctness
  finding has been proven in the current audits.

## First Implementation Recommendation

Start with RP-001. It is docs-only, removes the known verification blocker, and
gives later slices a clean local/CI signal. The first production-code slice
should be RP-002, followed immediately by RP-003 and RP-004 so helper result
semantics are used to prevent false progress rather than merely exposed.
