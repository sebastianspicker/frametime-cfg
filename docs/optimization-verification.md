# Package and CI optimization verification

This record covers the package-inventory hashing and native Rust CI scope
changes. It is host evidence — not Windows package, signing, driver, or hardware
qualification.

## Package inventory hashing

Measurements used Rust 1.96.0 on macOS 26.6.2 arm64. The ignored
`package_inventory_performance_fixture` test creates one 256 MiB file filled
with `0x5a` using 64 KiB writes, synchronizes it, then inventories and hashes
it. The `read-all` fixture mode reproduces the former whole-file allocation; the
default `streaming` mode calls the production recursive inventory and its fixed
64 KiB hashing buffer.

The pre-change debug test binary recorded 10,055 ms and 272,449,536 bytes peak
RSS for the former implementation. After the change, the debug binary recorded
32,952 ms and 3,571,712 bytes peak RSS while other workspace builds were running.
That pair is kept as chronological evidence but is not a useful speed comparison
because the scheduling load differed.

A paired run from the same optimized test binary gave comparable results:

| Fixture mode | Hash elapsed | Peak RSS |
| --- | ---: | ---: |
| `read-all` | 2,360 ms | 271,007,744 bytes |
| `streaming` | 1,116 ms | 2,621,440 bytes |

Three more alternating optimized runs recorded read-all hash times of 4,035,
1,130, and 904 ms and streaming times of 2,902, 943, and 1,040 ms. The warm-run
medians were 1,130 ms and 1,040 ms. Filesystem caching and shared-host scheduling
affect elapsed time, so the durable result is the bounded-memory behavior: peak
process RSS fell by more than 99% on this fixed fixture. `/usr/bin/time -l`
includes the test harness in peak RSS.

The focused package-builder suite covers multi-buffer hashing, inspected-length
mismatch rejection, sorted persisted and returned manifest forms, independent
tamper detection, and partial-output cleanup. It passed with 8 tests and one
ignored manual performance fixture.

## Native Rust CI scope

The classifier's shell suite passed documentation-only, packaged-document,
source, old-and-new rename path, newline path, changed inventory, malformed
inventory, NUL inventory, Windows absolute path, empty diff, unavailable SHA,
and manual-event cases. `shellcheck` passed for both classifier scripts and
`actionlint` passed for the native Rust and security workflows.

The cache action is pinned to commit
`0057852bfaa89a56745cba8c7296529d2fc39830`, verified as the upstream
`actions/cache` `v4.3.0` tag with `git ls-remote`. Cache keys separate host debug
all-feature builds from Windows debug all-feature plus default release builds,
and include runner OS, architecture, Rust version, and the lock and Cargo
configuration hash.

## Integrated root checks

The complete root workspace passed these checks with Rust 1.96.0:

- `cargo fmt --all -- --check`
- `bash scripts/check-domain-boundary.sh`
- `bash scripts/check-architecture-boundaries.sh`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity`
- `cargo test --workspace --all-targets --all-features --locked`
- `cargo run -p frametime-cli --locked -- dry-run all`
- `cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --locked`

GUI model tests cover injected reads, filtering without additional reads,
resource coalescing, obsolete generations, mutation invalidation, retained stale
snapshots, reader panic recovery, safe close during reads, non-actionable
metadata and unknown recovery records, and bounded Unicode input. Recovery
selectors are separate from display text and usable only in a ready snapshot.

Benchmark tests cover exact input limits, unrounded thresholds, ordered evidence
round trips, aggregate collisions, and baseline/final crash-prefix retries. A
direct CLI check rejected P1 values `100,100,100,100,600` at 180 FPS with four
failing runs, and accepted five runs at 180 P1 FPS with persistence disabled.

The native product, package boundary, and workflow-integrity shell checks from
`.github/workflows/security.yml` also passed locally. Live CI cache hit rates,
Windows GUI timings and accessibility, native clipboard behavior, and protected
Windows persistence were not measured or qualified on this macOS host.

## Maintenance follow-up, 2026-11-01

The cache pin and source state described above are retained as historical
verification evidence. The prepared workflow now uses the reviewed
`actions/cache` v6.1.0 commit, and generic CI runs only for Dependabot-authored
pull requests or manual dispatch. Each CI job retains an explicit
eligibility guard (the classifier was removed later). Pages publication is manual
on `main` only; security-only scans remain independent.

The grouped Cargo upgrade was validated on the actual Rust 1.96 compiler/Clippy
binaries, with host formatting, strict Clippy, workspace tests, all-branch
read-only preview, and Windows-target check/Clippy. RustSec reported no advisory
findings. Windows-native packaging, signing, Safe Mode and hardware qualification
were not executed on this Mac. Existing numeric observations above were not
recomputed or reclassified by this maintenance pass.
