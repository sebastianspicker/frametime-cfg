# Package inventory optimization evidence

This record covers the package-inventory hashing change. It is host evidence —
not Windows package, signing, driver, or hardware qualification.

## Package inventory hashing

Measurements used Rust 1.96.0 on macOS 26.6.2 arm64. The measurement fixture
used one 256 MiB file filled with `0x5a` using 64 KiB writes, synchronized it,
then inventoried and hashed it. The `read-all` mode reproduced the former
whole-file allocation; the `streaming` mode called the production recursive
inventory and its fixed 64 KiB hashing buffer.

The pre-change debug measurement recorded 10,055 ms and 272,449,536 bytes peak
RSS for the former implementation. After the change, the debug measurement recorded
32,952 ms and 3,571,712 bytes peak RSS while other workspace builds were running.
That pair is kept as chronological evidence but is not a useful speed comparison
because the scheduling load differed.

A paired run from the same optimized measurement binary gave comparable results:

| Fixture mode | Hash elapsed | Peak RSS |
| --- | ---: | ---: |
| `read-all` | 2,360 ms | 271,007,744 bytes |
| `streaming` | 1,116 ms | 2,621,440 bytes |

Three more alternating optimized runs recorded read-all hash times of 4,035,
1,130, and 904 ms and streaming times of 2,902, 943, and 1,040 ms. The warm-run
medians were 1,130 ms and 1,040 ms. Filesystem caching and shared-host scheduling
affect elapsed time, so the durable result is the bounded-memory behavior: peak
process RSS fell by more than 99% on this fixed fixture. `/usr/bin/time -l`
includes the measurement harness in peak RSS.
