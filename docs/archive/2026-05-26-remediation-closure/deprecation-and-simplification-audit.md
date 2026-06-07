# Deprecation And Simplification Audit

Generated: 2026-05-26.

Scope inspected: root PowerShell entrypoints, `helpers/`, `cfgs/`, tests,
workflows, `AGENTS.md`, `docs/code-index.md`,
`docs/verification-baseline.md`, and `docs/architecture-map.md`.

This audit identifies deletion, inlining, deduplication, and simplification
candidates. It does not authorize changes by itself. When runtime usage cannot
be proven from source, docs, tests, config, or workflows, the finding says
`needs runtime or git-history verification`.

## Method

Evidence sources:

- `rg --files` for source inventory.
- PowerShell AST scans for production function definitions and command calls.
- `rg` searches for deprecated, legacy, compatibility, fallback, no-op, and
  empty-catch patterns.
- Targeted reads of the referenced source and test ranges.
- Local parser and PSScriptAnalyzer checks.

Static check results during this audit:

- PowerShell parser over all `.ps1` files: passed.
- PSScriptAnalyzer with `PSScriptAnalyzerSettings.psd1`: passed.

No package manifest was found, so there were no project dependencies to audit
for unused package-level dependencies.

## Summary

No tracked production source file was proven fully unused.

Likely strongest deletion or simplification candidates:

1. Inactive backup-version pruning helpers and config.
2. Uncalled full-restore wrapper.
3. Obsolete RAM/XMP wrapper and test-only cache reset in production helper code.
4. Unread `last_verified` state field.
5. Deprecated/no-op CS2 config lines and NVIDIA fallback paths that need current
   runtime proof.

Highest-risk simplification areas:

1. Backup/restore switch logic and restore allowlists.
2. Safe Mode handoff paths.
3. NVIDIA DRS/registry fallback behavior.
4. Pagefile WMI/CIM behavior across Windows PowerShell 5.1 and PowerShell 7.
5. Duplicated verification logic across CLI verifier, GUI quick checks, and
   system analysis.

## Findings

### DAS-01: Inactive Backup-Version Pruning

- Category: unused internal feature / stale config
- Location: `helpers/backup-restore.ps1:41`, `helpers/backup-restore.ps1:53`,
  `config.env.ps1:26`, `tests/helpers/storage-hardening.Tests.ps1:31`
- Evidence: `Get-BackupVersionFiles` is only called by
  `Prune-BackupVersions`; `Prune-BackupVersions` has no production PowerShell
  caller in the AST scan. `CFG_BackupMaxVersions` is only used by
  `Prune-BackupVersions` and tests. `Initialize-Backup` calls
  `New-BackupFile` or `Set-SecureAcl`, but does not call the pruning function.
- Why likely obsolete or harmful: the retention feature is implemented and
  tested but appears inactive at runtime, so it adds maintenance cost and a
  false sense that old backup versions are pruned automatically.
- What could break if changed: an external dot-sourced use of
  `Prune-BackupVersions`, or an intended but currently unwired backup rotation
  behavior.
- Suggested action: investigate, then delete the pruning helpers, tests, and
  `CFG_BackupMaxVersions` if no git-history or user-facing requirement exists.
  If backup rotation is still required, wire it explicitly instead of leaving
  dormant code.
- Risk level: medium
- Verification needed: `rg` for the three symbols, git-history review for the
  backup rotation requirement, focused backup tests, `Invoke-Pester
  tests/helpers/storage-hardening.Tests.ps1 -CI`, and backup initialization
  smoke on Windows.

### DAS-02: Uncalled Full-Restore Wrapper

- Category: unused export / duplicate restore control flow
- Location: `helpers/backup-restore.ps1:1391`, `helpers/backup-restore.ps1:1432`,
  `START.bat:106`, `helpers/gui-panels.ps1:731`
- Evidence: the AST scan found no production PowerShell command call to
  `Restore-AllChanges`. `START.bat` invokes `Restore-Interactive`; the GUI
  explicitly avoids `Restore-AllChanges` because it uses `Read-Host`, then loops
  over step names and calls `Restore-StepChanges` directly.
- Why likely obsolete or harmful: it keeps a second "restore everything" path
  that is only exercised by tests, while the shipped terminal and GUI paths use
  other control flow.
- What could break if changed: users or scripts that dot-source helpers and call
  `Restore-AllChanges` directly; integration tests that currently cover it.
- Suggested action: investigate. If no external/public use is required, delete
  `Restore-AllChanges` and its dedicated tests, and keep
  `Restore-Interactive` plus GUI step restore as the public surfaces.
- Risk level: medium
- Verification needed: git-history search, docs search for public support
  claims, `Invoke-Pester tests/integration/backup-restore-entrypoints.Tests.ps1
  -CI`, `Invoke-Pester tests/integration/backup-restore-roundtrip.Tests.ps1
  -CI`, and manual terminal restore smoke with a harmless backup fixture.
  Usage needs runtime or git-history verification.

### DAS-03: Obsolete `Test-XmpActive` Wrapper

- Category: unused export / wrapper with little value
- Location: `helpers/hardware-detect.ps1:20`, `helpers/hardware-detect.ps1:60`,
  `helpers/hardware-detect.ps1:69`, `tests/helpers/hardware-detect.Tests.ps1:301`
- Evidence: production code uses `Get-RamInfo` and reads `AtRatedSpeed` or
  `XmpActive` directly. The AST scan found no production caller for
  `Test-XmpActive`; references are tests and source inventory docs.
- Why likely obsolete or harmful: the wrapper only returns
  `Get-RamInfo().XmpActive`, while the richer RAM object is the active contract.
  It makes tests preserve an API that runtime code no longer uses.
- What could break if changed: external dot-sourced users, and tests that still
  describe the old wrapper.
- Suggested action: delete the wrapper and replace wrapper-specific tests with
  `Get-RamInfo` behavior tests, unless git history shows an external support
  contract.
- Risk level: low
- Verification needed: `rg Test-XmpActive`, git-history check, `Invoke-Pester
  tests/helpers/hardware-detect.Tests.ps1 -CI`, and GUI dashboard smoke for RAM
  status. Usage needs runtime or git-history verification.

### DAS-04: Test-Only CPU Cache Reset In Production Helper

- Category: test-only helper in production code / single-use abstraction
- Location: `helpers/hardware-detect.ps1:7`, `helpers/hardware-detect.ps1:16`,
  `tests/helpers/hardware-detect.Tests.ps1:17`,
  `tests/helpers/process-priority.Tests.ps1:30`
- Evidence: `Reset-CachedCpuInfo` has no production caller. The source comment
  says to call it in tests, and references are in Pester `BeforeEach` blocks.
- Why likely obsolete or harmful: production helper code exposes a reset API
  solely for tests. That broadens the dot-sourced surface without runtime value.
- What could break if changed: tests that rely on clearing cached CIM results,
  and external callers if they discovered the helper.
- Suggested action: investigate moving the reset behavior into test setup by
  clearing `$Script:_cachedCpuInfo` directly, or keep it only if the project
  accepts test seams inside production helpers.
- Risk level: low
- Verification needed: `rg Reset-CachedCpuInfo`, `Invoke-Pester
  tests/helpers/hardware-detect.Tests.ps1 -CI`, `Invoke-Pester
  tests/helpers/process-priority.Tests.ps1 -CI`, and one dashboard hardware
  analysis smoke. Usage needs runtime or git-history verification.

### DAS-05: Unread `last_verified` State Field

- Category: stale storage field / compatibility branch
- Location: `Verify-Settings.ps1:369`, `Verify-Settings.ps1:379`,
  `helpers/gui-panels.ps1:44`, `helpers/gui-panels.ps1:86`,
  `helpers/gui-panels.ps1:596`, `tests/helpers/gui-panels.Tests.ps1:257`
- Evidence: source search found `last_verified` only in
  `Update-LastVerifiedTimestamp` and a debug message. GUI startup drift uses
  `startup_last_verified`; tests explicitly assert that GUI drift writes
  `startup_last_verified` and does not create `last_verified`.
- Why likely obsolete or harmful: the CLI verifier persists a field that no
  current code reads, while the GUI maintains a separate timestamp for quick
  drift suppression. That creates unclear state semantics.
- What could break if changed: external tooling or user scripts reading
  `state.json.last_verified`, or a planned future doc surface not present in
  current source.
- Suggested action: investigate and delete the `last_verified` write if there is
  no external compatibility requirement. If a full-verification timestamp is
  needed, document and read it explicitly instead of leaving it write-only.
- Risk level: medium
- Verification needed: git-history search, docs search, `Invoke-Pester
  tests/Verify-Settings.Tests.ps1 -CI`, `Invoke-Pester
  tests/helpers/gui-panels.Tests.ps1 -CI`, and Windows verifier smoke.
  Compatibility needs runtime or git-history verification.

### DAS-06: Duplicated Profile-To-Mode Mapping

- Category: duplicated logic / obsolete compatibility state
- Location: `Setup-Profile.ps1:106`, `helpers/gui-panels.ps1:1287`,
  `helpers/gui-panels.ps1:1292`, `helpers/system-utils.ps1:305`
- Evidence: the same profile-to-mode mapping appears in terminal setup and GUI
  settings save. `Load-State` still defaults missing `mode` to `CONTROL`, and
  comments state that `mode` is retained for compatibility with state loading
  and banners.
- Why likely obsolete or harmful: `mode` is mostly derived from `profile` except
  for dry-run. Keeping both stored values can drift and requires duplicate
  mapping in separate UI and terminal paths.
- What could break if changed: dry-run behavior, saved `state.json`
  compatibility, logging banners, tests that assert mode preservation, and any
  external state consumers.
- Suggested action: investigate a small migration: either derive `mode` from
  `profile` plus an explicit dry-run flag, or centralize the existing mapping in
  one existing helper. Do not do this as a broad rewrite.
- Risk level: high
- Verification needed: state migration tests, GUI settings tests, dry-run
  compliance tests, profile behavior matrix, entrypoint smoke, and Windows
  runtime smoke. Compatibility needs runtime or git-history verification.

### DAS-07: Deprecated Or No-Op CS2 CVars Kept In Generated Config

- Category: deprecated internal config / compatibility no-op
- Location: `config.env.ps1:152`, `config.env.ps1:157`,
  `config.env.ps1:163`, `config.env.ps1:240`,
  `Optimize-GameConfig.ps1:12`, `tests/config.Tests.ps1:57`
- Evidence: comments say `m_rawinput` is a harmless documentation stub because
  current CS2 forces raw input, and interpolation-style CVars are deprecated in
  CS2 Source 2 while `cl_net_buffer_ticks` is the actual control. Tests assert
  that `m_rawinput` remains documented as a no-op stub.
- Why likely obsolete or harmful: generated configs can look authoritative even
  when some lines are explicitly no-ops. That can preserve old tweak folklore
  and make future audits harder.
- What could break if changed: documented config count expectations, users who
  rely on the generated file as annotated education, and tests that lock the
  current explanatory text.
- Suggested action: investigate current CS2 convar behavior, then delete no-op
  lines from generated runtime config if they have no practical effect. Keep
  educational notes in docs if needed.
- Risk level: medium
- Verification needed: current CS2 convar proof, `Invoke-Pester
  tests/config.Tests.ps1 -CI`, Step 34 config-generation smoke, and docs update
  for any user-visible compatibility impact. Runtime proof is required before
  deletion.

### DAS-08: NVIDIA Registry Fallback And Legacy Driver Settings

- Category: deprecated compatibility path / likely ineffective fallback
- Location: `helpers/nvidia-profile.ps1:62`, `helpers/nvidia-profile.ps1:134`,
  `helpers/nvidia-profile.ps1:187`, `helpers/nvidia-profile.ps1:403`,
  `helpers/nvidia-profile.ps1:421`, `helpers/nvidia-profile.ps1:449`,
  `docs/nvidia-optimization.md:94`, `tests/helpers/nvidia-profile.Tests.ps1:137`
- Evidence: the primary NVIDIA path writes DRS through `nvapi64.dll`. The
  fallback path writes many `d3d` registry values; code comments and docs state
  that only GPU class P-state keys are confirmed effective on modern drivers and
  that `d3d` keys are best-effort. The DRS settings table also keeps a legacy
  frame-rate-limiter setting alongside the current NVCPL limiter.
- Why likely obsolete or harmful: fallback writes can imply a stronger runtime
  effect than the code/docs can prove, and they expand restore/test surface.
- What could break if changed: systems without DRS access, old drivers, 32-bit
  PowerShell fallback behavior, rollback entries for registry fallback writes,
  and tests that expect the registry fallback.
- Suggested action: investigate with live NVIDIA driver evidence. If fallback
  keys are not effective, delete or narrow the fallback to confirmed GPU class
  keys and clearly surface partial success.
- Risk level: high
- Verification needed: NVIDIA hardware runtime test, DRS success/failure test,
  registry fallback test, restore roundtrip, `Invoke-Pester
  tests/helpers/nvidia-profile.Tests.ps1 -CI`, and manual verification with
  NVIDIA Profile Inspector or equivalent driver evidence. Needs runtime or
  git-history verification.

### DAS-09: Deprecated WMI Pagefile Write Path

- Category: deprecated library API / compatibility branch
- Location: `Optimize-SystemBase.ps1:535`, `Optimize-SystemBase.ps1:565`,
  `Optimize-SystemBase.ps1:569`, `helpers/backup-restore.ps1:856`,
  `helpers/backup-restore.ps1:903`, `PSScriptAnalyzerSettings.psd1:45`
- Evidence: Phase 1 pagefile writes use `Get-WmiObject`, `[wmiclass]`, and
  `.Put()` with comments saying this is for Windows PowerShell 5.1 and is
  removed in PowerShell 7. Pagefile restore already has a CIM-based helper path
  using `Set-CimInstance`.
- Why likely obsolete or harmful: the write path is tied to a deprecated API
  while the restore path proves a CIM shape exists. The analyzer exclusion hides
  future WMI use.
- What could break if changed: Windows PowerShell 5.1 pagefile mutation,
  PowerShell 7 behavior, rollback capture before mutation, and reboot-required
  semantics.
- Suggested action: investigate replacing the write path with the existing CIM
  update helper approach if Windows PowerShell 5.1 proves equivalent. Keep the
  current path until that proof exists.
- Risk level: high
- Verification needed: focused pagefile tests, Windows PowerShell 5.1 runtime
  test on a disposable Windows VM, PowerShell 7 parser/lint, backup/restore
  roundtrip, and manual pagefile UI verification after reboot.

### DAS-10: Hand-Maintained Step Catalog Mirrors Phase Scripts

- Category: duplicate metadata / boilerplate
- Location: `helpers/step-catalog.ps1:1`, `helpers/step-catalog.ps1:9`,
  `helpers/gui-panels.ps1:414`, `tests/helpers/step-catalog.Tests.ps1:16`
- Evidence: the catalog comment says every entry mirrors `Invoke-TieredStep`
  calls in phase scripts and must be updated with scripts and docs. Tests cover
  schema, uniqueness, and allowed values, but not drift against the phase
  scripts.
- Why likely obsolete or harmful: phase metadata exists in both executable
  steps and GUI data. Drift can make the GUI status grid lie even when tests
  pass.
- What could break if changed: GUI Optimize panel rows, filters, status colors,
  and tests expecting the current catalog shape.
- Suggested action: investigate generating or validating the catalog from the
  phase step declarations. If that is too broad, add a drift test before
  changing step metadata. Do not add a new abstraction unless it replaces the
  duplicated source of truth.
- Risk level: medium
- Verification needed: catalog drift test, GUI Optimize panel smoke,
  `Invoke-Pester tests/helpers/step-catalog.Tests.ps1 -CI`, and entrypoint smoke.

### DAS-11: Duplicated Verification Logic Across GUI And CLI

- Category: duplicated logic / copy-paste checks
- Location: `helpers/gui-panels.ps1:53`, `helpers/gui-panels.ps1:491`,
  `Verify-Settings.ps1:432`, `Verify-Settings.ps1:448`,
  `Verify-Settings.ps1:486`, `helpers/system-analysis.ps1:127`,
  `helpers/system-analysis.ps1:142`
- Evidence: the same registry settings appear in GUI startup drift checks, GUI
  inline verify, CLI verification, and system analysis. Examples include MPO,
  Game Mode, Game DVR, Fast Startup, timer resolution, mouse acceleration, audio
  ducking, and Steam overlay checks.
- Why likely obsolete or harmful: each verifier can drift independently, causing
  different surfaces to claim different status for the same setting.
- What could break if changed: dashboard drift banner, Optimize inline verify,
  CLI `Verify-Settings.ps1`, tests for each surface, and user trust in status
  indicators.
- Suggested action: deduplicate by deleting redundant check tables only after a
  shared source of check definitions is proven useful across at least the GUI
  and CLI. Until then, add drift tests between existing tables before changing
  behavior.
- Risk level: high
- Verification needed: GUI panel tests, verifier tests, system-analysis tests,
  Windows runtime verifier smoke, and explicit checks that changed/missing
  statuses remain user-visible.

### DAS-12: Duplicated Safe Mode Handoff Paths

- Category: duplicated control flow / compatibility branches
- Location: `Optimize-GameConfig.ps1:654`, `Optimize-GameConfig.ps1:662`,
  `Setup-Profile.ps1:138`, `Boot-SafeMode.ps1:101`,
  `Boot-SafeMode.ps1:111`, `Cleanup.ps1:288`, `Cleanup.ps1:300`,
  `Cleanup.ps1:302`
- Evidence: Safe Mode arming appears in Phase 1 Step 38, the
  already-completed Phase 1 branch, `Boot-SafeMode.ps1`, and cleanup driver
  refresh. These paths overlap on runtime payload copy, RunOnce registration,
  BCD safeboot writes, retry/fallback behavior, and recovery messaging.
- Why likely obsolete or harmful: boot-state transitions are high risk, and
  multiple arming paths make it easy for one path to miss a validation or
  rollback behavior used by another.
- What could break if changed: Phase 1 to Phase 2 handoff, manual Safe Mode
  shortcut, cleanup driver refresh, BCD backup/restore, RunOnce registration,
  and recovery from failed Safe Mode entry.
- Suggested action: investigate which entry paths are still required. Delete
  obsolete compatibility branches only with evidence; otherwise reduce drift by
  making all paths use the same existing write wrappers and tests.
- Risk level: high
- Verification needed: dry-run compliance tests, system-utils security tests,
  e2e entrypoint smoke, manual Windows Safe Mode handoff smoke on disposable
  machine/VM, and git-history review for why each path exists. Needs runtime or
  git-history verification.

### DAS-13: Large Backup Restore Switch Chain

- Category: endless switch chain / mixed responsibilities
- Location: `helpers/backup-restore.ps1:951`, `helpers/backup-restore.ps1:972`,
  `helpers/backup-restore.ps1:1032`, `helpers/backup-restore.ps1:1083`,
  `helpers/backup-restore.ps1:1138`, `helpers/backup-restore.ps1:1221`,
  `helpers/backup-restore.ps1:1247`, `helpers/backup-restore.ps1:1278`,
  `helpers/backup-restore.ps1:1294`, `helpers/backup-restore.ps1:1314`
- Evidence: `Restore-StepChanges` handles registry, service, boot config, power
  plan, DRS, scheduled task, NIC, QoS/URO, Defender, pagefile, and DNS restore
  cases in one function. Tests cover many cases in
  `tests/integration/backup-restore-roundtrip.Tests.ps1`.
- Why likely obsolete or harmful: one function owns storage parsing, security
  validation, restore side effects, output, partial-success retention, and
  cleanup for many unrelated system surfaces.
- What could break if changed: rollback safety for nearly every state-changing
  optimization, tamper rejection, partial restore retention, and user-facing
  restore summaries.
- Suggested action: do not broad-rewrite. First remove proven dead restore
  surfaces such as inactive version pruning or uncalled wrappers. If a future
  change touches one restore type, simplify only that case with focused tests.
- Risk level: high
- Verification needed: full backup/restore unit and integration tests,
  tampered-backup tests, dry-run tests, Windows restore smoke with harmless
  registry/service fixtures, and manual restore UX review.

### DAS-14: Stale Broad Empty-Catch Analyzer Exclusion

- Category: boilerplate / weak error handling / stale config comment
- Location: `PSScriptAnalyzerSettings.psd1:17`,
  `helpers/gui-panels.ps1:536`, `helpers/gui-panels.ps1:546`,
  `helpers/gui-panels.ps1:563`, `Verify-Settings.ps1:88`,
  `Verify-Settings.ps1:279`, `CS2-Optimize-GUI.ps1:56`,
  `CS2-Optimize-GUI.ps1:1190`
- Evidence: analyzer settings say the empty-catch exclusion is for 7 instances,
  but `rg 'catch \{\}' -g '*.ps1'` found more exact empty catches across GUI,
  verifier, logging, setup, NVIDIA driver cleanup, and teardown paths. Several
  are benign teardown or best-effort checks, but the central exclusion now hides
  new cases by default.
- Why likely obsolete or harmful: a broad exclusion plus stale count makes it
  easy to add silent failures in runtime paths without review.
- What could break if changed: best-effort GUI teardown, logging fallback,
  unsupported runtime probes, and tests that do not expect warnings.
- Suggested action: replace non-teardown empty catches with `Write-DebugLog` or
  explicit comments, then narrow the analyzer suppression to documented cases.
  Keep truly best-effort teardown quiet if user-visible noise would be worse.
- Risk level: medium
- Verification needed: PSScriptAnalyzer, focused GUI/verifier tests, entrypoint
  smoke, and manual GUI close/teardown smoke.

### DAS-15: Ignored Local Agent Documentation Archive

- Category: likely unused local files / generated or superseded artifacts
- Location: `.gitignore:66`, `docs/agent/Documentation.md`,
  `docs/agent/archive/2026-05-26-superseded-agent-docs/`
- Evidence: `git check-ignore -v` reports `docs/agent/` is ignored. `git
  ls-files docs/agent` returns no tracked files. The directory contains archived
  agent docs and superseded audit/planning artifacts.
- Why likely obsolete or harmful: ignored local docs can confuse repository-wide
  searches and audit results, especially when they mention retired surfaces that
  CI intentionally excludes under `docs/agent/`.
- What could break if changed: local-only agent history or a user workflow that
  expects those ignored notes to remain on this machine.
- Suggested action: investigate with the user before deleting local ignored
  artifacts. Do not treat these as production source.
- Risk level: low
- Verification needed: user confirmation, `git check-ignore -v docs/agent/...`,
  and `git status --ignored docs/agent`. Usage needs local/runtime verification.

## Non-Findings And Do-Not-Delete Notes

- `script:Get-RegVal` and `script:New-CheckItem` initially looked unused in a
  naive function scan because their definitions include a scope prefix. Direct
  text search shows they are actively called throughout `helpers/system-analysis.ps1`.
- `Restore-Interactive` looked uncalled in the PowerShell AST scan because the
  shipped call is inline inside `START.bat`. It is active through the terminal
  restore menu.
- The optional CS2 network/audio/debug CFG files are referenced by Step 34,
  README, docs, and tests. They are not deletion candidates from current
  evidence. Some of those files are currently untracked in the worktree, which
  is a source-control state issue, not dead-code evidence.
- `helpers.ps1` is a compatibility loader, but it is an active public contract
  for every entrypoint. Do not delete it without a migration plan.
- Windows PowerShell 5.1 compatibility is an active supported runtime contract,
  so PowerShell 7-only simplifications need Windows proof before adoption.

## Recommended Next Audit Targets

1. Backup/restore public surface: decide whether inactive backup-version
   pruning and the uncalled full-restore wrapper can be deleted.
2. State schema cleanup: prove whether `last_verified`, `mode`, and
   `startup_last_verified` should be merged, migrated, or documented.
3. Runtime config cleanup: verify current CS2 convars and remove no-op generated
   settings if they are only educational.
4. NVIDIA fallback cleanup: prove which registry fallback settings still affect
   current drivers.
5. Verification-table drift: compare GUI quick checks, GUI inline verify,
   `Verify-Settings.ps1`, and system-analysis checks.
6. Safe Mode handoff consolidation: audit all BCD/RunOnce/payload-copy paths
   before changing any of them.

## Coverage Gaps And Uncertainty

- No Windows runtime, administrator, reboot, Safe Mode, driver, WPF, Steam, CS2,
  NVIDIA, NIC, DNS, or registry mutation checks were run for this audit.
- Git history was not exhaustively reviewed. Findings that could affect public
  or external dot-sourced helper usage are marked for git-history verification.
- Dynamic PowerShell calls from batch files were inspected by text search, but
  the AST scan only covers `.ps1` production files.
- Ignored local files under `docs/agent/` were represented only as local
  artifacts; they were not treated as production source.
- Untracked active CFG files were not modified. Their presence should be
  reviewed separately if preparing a commit.
