# Evidence policy

The repository keeps implemented behavior separate from claimed performance
benefit. Settings depend on the installed CS2 build, Windows version, driver,
hardware, display, network, accessibility requirements, and user preferences. No
asset or policy here is a universal performance profile.

## Sources and measurement

- Prefer current Valve, Microsoft, and hardware-vendor documentation over old
  tuning lists.
- Keep primary sources with any proposed setting change, and record uncertainty
  and counterexamples.
- Use repeated measurements on the target system before making a performance
  claim; source structure and host checks are not hardware evidence.
- Preserve image quality, accessibility, stability, and anti-cheat constraints.

## CFG assets

[`assets/cfgs/autoexec.cfg.example`](../../assets/cfgs/autoexec.cfg.example) is a
manually reviewed starter asset — not an automatically deployed file and not a
claim that every command is optimal. Optional CFG deployment is explicit: the
workflow validates the CS2 target, captures supported prior bytes, writes only
selected managed files, and verifies the written bytes. Execution by the game
stays outside that file-level verification boundary.

CVar names, defaults, and effects depend on the game build. Validate them against
the installed client and current Valve material before use.

## Excluded and conditional controls

Legacy launch flags, blanket timer or scheduler changes, generic TCP tuning for a
UDP game workload, indiscriminate cache deletion, and universal driver profiles
are omitted or conditional when target identity, recovery, current behavior, or
repeatable benefit is not well established.

A new state-changing control needs a typed owner, a bounded target, a validation
and recovery plan, current source evidence, and target-system measurement.
Otherwise it stays advisory, conditional, or excluded.
