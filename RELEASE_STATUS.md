# Release Status

**Evidence cutoff:** 2026-07-14  
**Verdict:** LOCAL RELEASE BLOCKERS CLEARED; WINDOWS RC VALIDATION REQUIRED

## Status Summary

The previously documented Safe Mode, runtime-payload, recovery, driver-clean,
state-accounting, and rollback blockers are fixed in the current candidate.
The repository is ready for an owner-run Windows release-candidate validation,
but it is not yet release-proven because this macOS host cannot execute the
privileged Windows, Safe Mode, WPF, BCD, registry, driver, or power-plan paths.

Draft PR #22 packages the reviewed candidate on branch
`remediation/full-release-hardening-2026-07-10` against the current `main`.
The exact three-commit candidate identity is recorded in GitHub; no release tag
has been created.

## Locally Verified Evidence

- Phase 1 now publishes a fixed 35-file payload as an immutable runtime
  generation, with a manifest, per-file hashes, lock ownership, validation, and
  an atomic current-generation pointer. Prior generations remain available to
  already-armed handoffs. Phase 2 and Phase 3 execute only a published runtime.
- Safe Mode entry and exit now require verified BCD state and a verified phase
  handoff. Failure paths retain recovery artifacts and do not restart or claim
  completion.
- GPU cleanup uses CIM-owned display packages, strict OEM INF validation, an
  absolute inbox `pnputil.exe`, verified package absence, partitioned outcome
  counters, safe recursive-delete boundaries, and explicit Normal-Mode AppX
  deferral. Phase 3 verifies deferred AppX cleanup before driver installation.
- NVIDIA extraction and setup validate Authenticode identity, trusted paths,
  package contents, process-tree timeout termination, and post-install driver
  state. Unverified termination retains the secured work directory.
- Backup, power-plan, Run/RunOnce, process-priority, MSI/NIC, benchmark, and
  step/tier outcomes now report structured, verified success, skip, partial,
  and failure states without promoting best-effort work to completion.
- Repository syntax parsing, the pinned PSScriptAnalyzer gate, XAML XML
  validation, and focused WPF contracts pass on macOS. The final Pester run
  discovered 1,037 tests: 1,036 passed, zero failed, and one Windows-only smoke
  test was skipped in 872.83 seconds.
- Local Codacy analysis reports zero issues from Markdownlint, Jackson,
  Spectral, Trivy, Checkov, and Opengrep across their routed files. Checkov also
  emitted a non-code warning because its optional Prisma guideline lookup was
  unavailable in the sandbox.

## Required Windows RC Validation

- Run the CI matrix on Windows, including Windows PowerShell 5.1 parse/smoke,
  the complete Pester suite, and shipped entrypoint smoke tests.
- Exercise clean, interrupted, and recovery boots through Phase 1, Safe Mode
  Phase 2, and Normal-Mode Phase 3 on a disposable supported Windows system.
- Verify live `bcdedit`, Run/RunOnce registry values, NTFS ACLs/reparse checks,
  `powercfg`, service/task control, AppX, MSI/NIC, and NVIDIA installer behavior.
- Validate WPF UI Automation, keyboard-only use, High Contrast, 200% scaling,
  loading/empty/error states, and the documented 960-by-540 viewport.
- Archive logs, manifests, state, backup evidence, and driver/power snapshots
  from the exact candidate, then tag or build only that reviewed tree.

## Known Verification Boundaries

- No live Windows, Safe Mode, reboot, WPF, BCD, registry, Driver Store,
  Authenticode installer, `powercfg`, service, scheduled-task, or AppX operation
  was executed on this macOS host; those contracts were exercised with mocks and
  filesystem tests where possible.
- Local Codacy Checkov completed with zero findings but could not download its
  optional guideline mapping. No dependency or analyzer was installed during
  this audit.
- The candidate remains intentionally untagged pending Windows validation.
  Packaging or publishing the reviewed tree does not add Windows-native runtime
  evidence and must not be treated as release approval.
