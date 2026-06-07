# 2026-05-26 Remediation Closure

This folder contains the completed repository audit and remediation packet that
used to live in the active `docs/` root.

## Contents

- `code-index.md`: source-file inventory.
- `verification-baseline.md`: discovered verification commands and current
  baseline.
- `architecture-map.md`: runtime architecture, flows, contracts, and coupling.
- `deprecation-and-simplification-audit.md`: dead-code, deprecated-path,
  duplication, and simplification findings.
- `logic-and-correctness-audit.md`: correctness and silent-wrong-behavior
  findings.
- `refactor-plan.md`: prioritized implementation slice plan.
- `remediation-ledger.md`: completed per-slice execution ledger.
- `remediation-status.md`: final remediation status.

## Final State

The remediation workflow closed with 19 completed slices and 3 deferred
evidence-gated slices. The deferred items require proof that cannot be produced
from this macOS checkout:

- Windows pagefile replacement proof on Windows PowerShell 5.1 and PowerShell 7.
- Current CS2 CVar/runtime proof and manual console verification.
- Live Windows/NVIDIA driver, DRS, registry fallback, restore, and external
  profile-inspection evidence.

These files are retained for historical traceability and should not be treated
as the current execution surface for new work.
