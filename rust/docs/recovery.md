# Recovery

Recovery data is stored in `C:\FRAMETIME_CFG\backup.json`. Current typed records
cover registry, service, boot, owned power-plan, scheduled-task, HAGS, pagefile,
DNS, interrupt-policy, network-stack, NVIDIA DRS, and complete CS2 CFG
transactions. The pagefile transaction includes its created-object journal;
the CS2 record is a path-free ordered snapshot. Irreversible AppX removal and
driver replacement use separate pending/finalized audit or transaction records
and do not claim lossless rollback. Unknown records and records that fail or
cannot be restored remain for a later retry.

P1:7 HAGS uses the separate typed `hags` receipt. It losslessly records an
absent `HwSchMode` value or DWORD `0`, `1`, or `2`, the exact numeric
DXGI-to-SetupAPI adapter bindings, and an effective-verification pending flag
before it writes the fixed `HKLM\\SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers`
`HwSchMode` DWORD `2` request. The immediate DWORD readback proves only the
request. After reboot, read-only verification requires `D3DKMTIsFeatureEnabled`
to report stable HWSCH support and `Enabled` for every captured nonsoftware
adapter. A registry request is never presented as effective completion. Restore
validates the exact P1:7 receipt before restoring or deleting only the captured
`HwSchMode` value and requiring exact registry readback.

Restore-all processes entries in reverse capture order. Every active restore
requires an exact catalog step, entry type, and identity binding. Exact-bound
registry, SCM service, owned power-plan, Dynamic Tick, Safe Mode BCD, and
token-bearing P1:8 pagefile transaction entries have native dispatch. P1:14 Run values additionally require the
executable-adjacent, validated `frametime.toml` allowlist before restore.
Power-plan cleanup is limited to captured GUIDs whose live name still exactly
matches `frametime.cfg`. Pagefile recovery requires exact CIM object and relative
paths plus expected sizes; tokenless legacy or incomplete create journals are
retained for manual recovery. Current exact adapters also restore P1:13 tasks,
P1:34 CS2 files, P1:16 network settings/QoS/NLA state, P3:2/P3:3 interrupt
policies, P3:4 DRS state, and P3:9 DNS state after reobserving their identities.
Legacy `nic_adapter`, `qos_uro`, `defender`, and tokenless `pagefile` variants
have no exact current catalog capture binding and remain without mutation. The
first captured value for a deduplicated identity is retained. On Windows,
trusted JSON uses fixed-name,
exclusive handles beneath the retained suite-root handle. Writes flush and hash
a protected temporary handle, replace relative to the root handle, then verify
the final path, DACL, and hash through that same renamed handle. A corrupt
trusted file is left byte-for-byte untouched and the operation fails closed.

P1:3 shader-cache cleanup uses an irreversible audit separate from `backup.json`
because shader bytes cannot be restored. The no-follow, handle-relative
deletion primitive is implemented, but production deletion remains build-gated
until its Windows reparse, sharing, race, and disposition matrix is qualified.

An idle or partially completed Phase 1 legacy run may be imported only after
explicit confirmation. Migration fails closed when a legacy PowerShell Phase 2
RunOnce or Phase 3 Run handoff is armed, when Safe Mode is pending, or when a
runtime payload is incomplete. The Rust executable does not cancel, translate,
overwrite, or resume that transaction.

Keep the full `C:\FRAMETIME_CFG` directory until live and restore validation is
complete. Driver packages, removed AppX packages, and user-edited CS2 files may
require separate recovery outside `backup.json`.
