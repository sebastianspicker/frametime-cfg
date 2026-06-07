# Remediation Status

Generated: 2026-05-26.

Source of truth: `docs/refactor-plan.md`.

Overall state: COMPLETE

Current slice: none

Last slice: RP-022 - Narrow empty-catch analyzer suppression

## Counts By Status

| Status | Count |
| --- | ---: |
| NOT_STARTED | 0 |
| IN_PROGRESS | 0 |
| BLOCKED | 0 |
| DEFERRED | 3 |
| IMPLEMENTED | 0 |
| VERIFIED | 0 |
| COMPLETE | 19 |

Highest remaining priority: none locally actionable. Deferred evidence-gated
slices are RP-019, RP-020, and RP-021.

Last commands/result: RP-022 exact empty-catch search found no matches;
`PSAvoidUsingEmptyCatchBlock` no longer appears in analyzer settings; parser
passed; recursive PSScriptAnalyzer still hit the known internal range-position
error; per-file PSScriptAnalyzer passed for all `.ps1` files excluding
`tests/helpers/_TestInit.ps1`, which triggers the analyzer internal error;
targeted Pester passed 142, failed 0, skipped 1; e2e smoke passed 2/2; full
Pester passed 796, failed 0, skipped 1; whitespace check passed;
retired-surface guard passed.

Uncertainty:

- The current worktree already has unrelated modified and untracked files.
- Windows PowerShell 5.1, live Safe Mode driver-clean smoke, live driver removal,
  and administrator/runtime checks are unavailable on this host until a Windows
  environment is used.
- The one-shot recursive PSScriptAnalyzer command returned an internal analyzer
  error during RP-005; an individual-file analyzer sweep passed instead.
- Live NVIDIA hardware, DRS database inspection, and external NVIDIA Profile
  Inspector verification were not run during RP-006 on this macOS host.
- Windows restore smoke with harmless service and DNS fixtures was not run
  during RP-008 on this macOS host.
- Windows DNS smoke on a disposable adapter/profile was not run during RP-007
  on this macOS host; DHCP reset post-state remains command-success based.
- Windows WPF GUI smoke and manual GUI resume smoke were not run during RP-009
  on this macOS host.
- Windows WPF GUI smoke was not run during RP-010 on this macOS host.
- Windows PowerShell 5.1 and administrator/runtime profile-loading smoke were
  not run during RP-011 on this macOS host.
- Interactive FPS-cap fallback prompts and Windows GUI benchmark-panel smoke
  were not run during RP-012 on this macOS host.
- Phase 2 Safe Mode steps remain outside the Optimize-grid catalog scope; Windows
  WPF GUI manual smoke was not run during RP-013 on this macOS host.
- Windows verifier smoke and WPF GUI manual verification were not run during
  RP-014 on this macOS host. One accidental parallel targeted Pester run was
  discarded and the same targets were rerun sequentially.
- Windows backup initialization smoke was not run during RP-015 on this macOS
  host; external dot-sourced use outside this repository cannot be proven from
  repo search.
- Windows manual terminal restore smoke was not run during RP-016 on this macOS
  host; external dot-sourced use outside this repository cannot be proven from
  repo search.
- Windows GUI hardware analysis smoke was not run during RP-017 on this macOS
  host; external dot-sourced use outside this repository cannot be proven from
  repo search.
- Windows verifier smoke and Windows GUI settings smoke were not run during
  RP-018 on this macOS host; external consumers of removed `last_verified`
  cannot be proven from repo search.
- RP-019 is deferred until Windows PowerShell 5.1, PowerShell 7 Windows smoke,
  and disposable VM pagefile UI proof are available.
- RP-020 is deferred until current CS2 convar/runtime proof and manual CS2
  console verification are available.
- RP-021 is deferred until live Windows/NVIDIA driver, DRS, registry fallback,
  restore, and external profile-inspection evidence are available.
- Recursive PSScriptAnalyzer remains unreliable because of an internal analyzer
  error on `tests/helpers/_TestInit.ps1`; the per-file analyzer sweep excluding
  that file passed during RP-022.
- Windows manual GUI close/teardown smoke was not run during RP-022 on this
  macOS host.
- Cleanup/deletion slices require usage, git-history, or runtime proof before
  code removal.

Next slice: none until deferred evidence is available.
