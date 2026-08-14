# Operations

`frametime.exe` supports `optimize`, `dry-run <1|2|3|4|all>`, `phase2`,
`phase3`, `boot-safe-mode`, `cleanup`, `fps-cap`, `baseline-benchmark`,
`final-benchmark`,
`hardware`, `driver`, `verify`, `restore`,
`backup-summary`, `reset-progress`, `show-log`, and `smoke-test`.

`hardware` runs bounded, in-process native diagnostics and emits the versioned
`frametime.hardware/v1` JSON envelope. `driver plan --input <json>` validates
exact device, OEM package, SHA-256, and Authenticode evidence and emits a
read-only four-step proposal. Neither command advances workflow progress.
Detailed integration boundaries are in `docs/integrations.md`.

`baseline-benchmark` and `final-benchmark` each accept exactly one complete
VProf text, file, or native clipboard source. The first records the P1:17
baseline. The second requires normal boot, the verified selected runtime, the
verified same-user Phase 3 Run handoff, and an authorized Phase 3 transaction
before it commits the fixed `After all optimizations` record, final receipt,
transaction stage, and P3:13 progress under one lock with progress last. Phase
3 itself runs only P3:1-P3:12, then reads the receipt without writing: absent
evidence directs the operator to `final-benchmark`, a coherent receipt
succeeds, and incoherent evidence fails closed. A coherent receipt permits the
same-user coordinator to delete only the exact retained Phase 3 Run value.

Cleanup is confirmation-gated. `cleanup full` additionally requires
`--acknowledge-irreversible` because it includes non-recoverable temporary,
prefetch, and event-log deletion plus a Winsock reset. The typed cleanup plan
hard-denies suite logs, backup/state/progress/runtime files, DriverStore, CS2
install/content, and non-730 Steam libraries. Native Quick/Full adapters use
fixed handle-backed target families. Winsock reset is the single fixed
`netsh.exe winsock reset` vector and marks the report as restart-required. A
deferred or failed action keeps the cleanup result partial.

Live phases require x64 Windows and use `C:\FRAMETIME_CFG`. The typed reboot
transaction has an initiating-user SID field, and the orchestration model
requires every later phase to prove that identity. P1:38 retains the selected
runtime handles while it records the initiating TokenUser SID, exact runtime
and manifest hashes, writes and reads back the exact HKLM RunOnce Phase 2
value, then sets and reads back Safe Boot before persisting the transaction.
P2:1 clears and reads back Safe Boot before its lock-held
`phase2SafeMode`/progress checkpoint. P2:3 requires completed P2:2 and the
same SID/runtime before writing and reading back the exact HKCU Phase 3 Run
value. `phase3-handoff` validates that same identity and directly invokes the
retained executable with visible `ShellExecuteExW(runas)`, not a shell. Phase
3 retains its handoff until required driver installation and a coherent final
benchmark complete, then deletes and reads back only that exact Run value.
Windows VM interruption, BCD, registry, and elevation qualification remains
required before these native transitions are release evidence.

Every mutable operation follows this boundary:

1. inspect current state;
2. capture the original value;
3. atomically persist and verify the recovery entry;
4. apply through a typed adapter;
5. verify the postcondition;
6. atomically persist phase-qualified progress.

A failure in steps 2 or 3 blocks mutation. A failure in steps 4 or 5 leaves
progress pending. On Windows, the fixed trusted JSON files are read and
replaced through retained, exclusive handles with final-path, DACL, size, and
SHA-256 readback checks. Full dry-run performs only inspection and plan
rendering.

External Windows tools are executed directly with argument vectors. The only
allowed executables are `bcdedit.exe`, `powercfg.exe`, `pnputil.exe`,
`fsutil.exe`, `defrag.exe`, and `netsh.exe`; no command is interpolated through
a shell. `netsh.exe` is exposed only through the fixed Winsock-reset vector.
