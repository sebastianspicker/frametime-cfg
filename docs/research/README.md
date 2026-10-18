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
  claim; source structure and host tests are not hardware evidence.
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

The `repo-checks` suite fails when a cfg asset uses a name that is not a
player-settable convar or command in the pinned snapshot
`repo-checks/data/cs2-console-names.txt`: built from the GameTracking-CS2
`DumpSource2` convar and command dumps, without cheat, development-only, or
hidden entries. When Valve changes CVars, regenerate the snapshot from the
newer dump and update the assets in the same change. The snapshot proves a
name exists, not that a value helps; that still needs measurement.

`assets/videosettings.txt` is a reference for the in-game video menu. Its
`setting.*` keys are checked against `repo-checks/data/cs2-video-settings.txt`,
the keys found in the shipped CS2 binaries of the same build. The tool still
never writes CS2 video files.

## Excluded and conditional controls

Legacy launch flags, blanket timer or scheduler changes, generic TCP tuning for a
UDP game workload, indiscriminate cache deletion, and universal driver profiles
are omitted or conditional when target identity, recovery, current behavior, or
repeatable benefit is not well established.

A new state-changing control needs a typed owner, a bounded target, a validation
and recovery plan, current source evidence, and target-system measurement.
Otherwise it stays advisory, conditional, or excluded.
