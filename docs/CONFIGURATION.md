# Configuration

## Where the configuration comes from

The root application has exactly one tracked configuration source:
[`frametime.toml`](../frametime.toml). In a release it is part of the
authenticated package inventory.

- A fresh state-changing command uses the immutable byte snapshot retained by
  the authenticated package verifier.
- A resumed reboot command uses the snapshot copied into, and verified with, the
  selected runtime generation.
- Environment variables, per-user files, command-line overrides, and other TOML
  files are not merged in.

Editing `frametime.toml` after packaging invalidates package authentication. The
fixed `C:\FRAMETIME_CFG` work root is a separate Windows trust boundary, not a
configuration field.

## Schema

Every top-level table and value in the tracked file is required unless a default
is listed. Unknown top-level keys, and unknown keys under `[paths]`, are
rejected.

### `autostart_remove`

An array of Windows autostart entry names considered by the relevant workflow
step. The tracked file is the canonical reviewed list.

### `[fps_cap]`

| Key | Meaning | Validation |
| --- | --- | --- |
| `strategy` | `raw` or `vrr` | Required |
| `measured_cap` | Raw-mode measured cap; `0` means uncapped | Defaults to `0`; otherwise `30..=1000` |
| `refresh_hz` | Display refresh used by VRR policy | Defaults to `0`; VRR requires `30..=1000` |
| `ceiling_margin_hz` | Margin below the VRR refresh ceiling | Defaults to `3`; VRR requires a nonzero value below `refresh_hz` |

The raw strategy uses `measured_cap`; the VRR strategy derives its ceiling from
`refresh_hz - ceiling_margin_hz`. The workflow applies additional measurement
and policy checks before it persists a cap.

A new nonzero raw or VRR cap requires at least five valid VProf Avg/P1 result
lines. Every unrounded per-run P1 value must meet the selected cap: an aggregate
average cannot hide a failing run, and a run whose P1 exceeds its Avg cannot
support a cap. Aggregate-only legacy records stay readable for compatibility but
cannot authorize a new nonzero cap. Raw `measured_cap = 0` remains the explicit
uncapped path.

### `[paths]`

The path strings are policy values, not arbitrary filesystem inputs.

- `shader_cache` must be a nonempty, case-insensitively unique subset of the
  five compiled CS2 cache templates shown in `frametime.toml`.
- `nvidia_dx_cache` must equal `%LOCALAPPDATA%\NVIDIA\DXCache`.
- `nvidia_gl_cache` must equal `%LOCALAPPDATA%\NVIDIA\GLCache`.
- `directx_shader_cache` must equal `%LOCALAPPDATA%\D3DSCache`.

Changing these values requires changing the compiled allowlist and its
validation logic. An operator cannot widen the allowed filesystem surface by
editing TOML.

## Other configuration surfaces

User-selected profiles and workflow progress are persisted state, not alternate
configuration files. [`assets/`](../assets/) holds the canonical packaged CFG
and video-data inputs. Optional CS2 CFG deployment is explicit, target-bound,
backed up where supported, and verified after writing.

Publisher pins and signing inputs are release-time environment variables, not
runtime configuration; they are documented in [deployment](DEPLOYMENT.md). Do
not place secrets in `frametime.toml`, assets, state, logs, or diagnostics.
