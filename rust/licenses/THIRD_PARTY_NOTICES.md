# Rust distribution third-party notices

This file records the registry packages selected for the portable Rust
distribution. It replaces the legacy PowerShell/source-provenance notice: the
portable release contains the Rust executables and does not contain the
repository's PowerShell implementation.

## Scope and generation inputs

The component list below is the normal dependency graph selected for the
`x86_64-pc-windows-msvc` target by:

```text
cargo tree --workspace --target x86_64-pc-windows-msvc --edges normal,no-proc-macro
```

Workspace packages, development-only dependencies, and proc-macro build units
are excluded. Package names, versions, and license expressions are taken from
the corresponding local Cargo registry `Cargo.toml` files resolved by
`Cargo.lock`. It is a factual dependency notice, not a legal conclusion.

The distribution carries the full license texts used by these declarations:

- [Apache-2.0](LICENSE-APACHE-2.0)
- [MIT](LICENSE-MIT)

`aho-corasick` and `memchr` declare `Unlicense OR MIT`; the MIT option is
included in this distribution. No package in this target-selected set includes
a separate root `NOTICE` file in the local registry copy.

The native NVIDIA DRS boundary mirrors the structure layouts and interface IDs
from NVIDIA's public NVAPI SDK at commit
`cd6918f60b3c9a0476fdfe7e89bb32330602049d`. NVIDIA publishes those SDK headers
under the MIT License; the distribution's `LICENSE-MIT` text applies to that
small ABI-derived portion.

## Components

| Package | Declared license expression |
| --- | --- |
| `aho-corasick v1.1.5` | `Unlicense OR MIT` |
| `anstream v1.0.0` | `MIT OR Apache-2.0` |
| `anstyle v1.0.14` | `MIT OR Apache-2.0` |
| `anstyle-parse v1.0.0` | `MIT OR Apache-2.0` |
| `anstyle-query v1.1.5` | `MIT OR Apache-2.0` |
| `anstyle-wincon v3.0.11` | `MIT OR Apache-2.0` |
| `anyhow v1.0.104` | `MIT OR Apache-2.0` |
| `bitflags v2.13.1` | `MIT OR Apache-2.0` |
| `block-buffer v0.10.4` | `MIT OR Apache-2.0` |
| `cfg-if v1.0.4` | `MIT OR Apache-2.0` |
| `clap v4.6.6` | `MIT OR Apache-2.0` |
| `clap_builder v4.6.6` | `MIT OR Apache-2.0` |
| `clap_lex v1.1.0` | `MIT OR Apache-2.0` |
| `colorchoice v1.0.5` | `MIT OR Apache-2.0` |
| `cpufeatures v0.2.17` | `MIT OR Apache-2.0` |
| `crypto-common v0.1.7` | `MIT OR Apache-2.0` |
| `deranged v0.5.8` | `MIT OR Apache-2.0` |
| `digest v0.10.7` | `MIT OR Apache-2.0` |
| `equivalent v1.0.2` | `Apache-2.0 OR MIT` |
| `generic-array v0.14.7` | `MIT` |
| `hashbrown v0.17.1` | `MIT OR Apache-2.0` |
| `indexmap v2.14.0` | `Apache-2.0 OR MIT` |
| `is_terminal_polyfill v1.70.2` | `MIT OR Apache-2.0` |
| `itoa v1.0.18` | `MIT OR Apache-2.0` |
| `memchr v2.8.3` | `Unlicense OR MIT` |
| `num-conv v0.2.2` | `MIT OR Apache-2.0` |
| `once_cell_polyfill v1.70.2` | `MIT OR Apache-2.0` |
| `powerfmt v0.2.0` | `MIT OR Apache-2.0` |
| `raw-cpuid v11.6.0` | `MIT` |
| `regex v1.13.1` | `MIT OR Apache-2.0` |
| `regex-automata v0.4.18` | `MIT OR Apache-2.0` |
| `regex-syntax v0.8.11` | `MIT OR Apache-2.0` |
| `serde v1.0.229` | `MIT OR Apache-2.0` |
| `serde_core v1.0.229` | `MIT OR Apache-2.0` |
| `serde_json v1.0.151` | `MIT OR Apache-2.0` |
| `serde_spanned v1.1.1` | `MIT OR Apache-2.0` |
| `sha2 v0.10.9` | `MIT OR Apache-2.0` |
| `strsim v0.11.1` | `MIT` |
| `thiserror v2.0.20` | `MIT OR Apache-2.0` |
| `time v0.3.55` | `MIT OR Apache-2.0` |
| `time-core v0.1.9` | `MIT OR Apache-2.0` |
| `toml v0.9.12+spec-1.1.0` | `MIT OR Apache-2.0` |
| `toml_datetime v0.7.5+spec-1.1.0` | `MIT OR Apache-2.0` |
| `toml_parser v1.1.3+spec-1.1.0` | `MIT OR Apache-2.0` |
| `toml_writer v1.1.2+spec-1.1.0` | `MIT OR Apache-2.0` |
| `typenum v1.20.1` | `MIT OR Apache-2.0` |
| `utf8parse v0.2.2` | `Apache-2.0 OR MIT` |
| `windows v0.62.2` | `MIT OR Apache-2.0` |
| `windows-collections v0.3.2` | `MIT OR Apache-2.0` |
| `windows-core v0.62.2` | `MIT OR Apache-2.0` |
| `windows-future v0.3.2` | `MIT OR Apache-2.0` |
| `windows-link v0.2.1` | `MIT OR Apache-2.0` |
| `windows-numerics v0.3.1` | `MIT OR Apache-2.0` |
| `windows-result v0.4.1` | `MIT OR Apache-2.0` |
| `windows-strings v0.5.1` | `MIT OR Apache-2.0` |
| `windows-sys v0.61.2` | `MIT OR Apache-2.0` |
| `windows-threading v0.2.1` | `MIT OR Apache-2.0` |
| `winnow v0.7.15` | `MIT` |
| `winnow v1.0.4` | `MIT` |
| `zmij v1.0.23` | `MIT` |
