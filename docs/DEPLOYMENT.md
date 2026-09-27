# Release packaging

## Scope

The repository produces a portable Windows directory and ZIP. It does not
contain an installer or a deployment service. The packaging script has two
deliberately separate modes:

- `/unsigned` assembles and structurally verifies a development/CI package. It
  is never mutation authority.
- `/release` signs the two PE files and the catalog, verifies every catalog
  member, and runs the package-authentication smoke command.

CI exercises only `/unsigned`. A green CI package job is not evidence of
authenticated signing or live Windows qualification.

## Prerequisites

Run release packaging from a Windows x64 checkout with:

- the pinned Rust 1.96.0 MSVC toolchain and Windows target;
- Windows SDK `signtool.exe` and `MakeCat.exe` at explicit paths;
- a code-signing certificate available to SignTool by SHA-1 thumbprint;
- an RFC 3161 timestamp URL;
- one or two distinct SHA-256 SPKI publisher pins for certificates the compiled
  application accepts.

The certificate private key and credentials are external release assets. Never
copy them into the repository or `dist/`.

## Authenticated release

From the repository root in `cmd.exe`, set the release inputs from the
controlled release environment:

```bat
set "FRAMETIME_PUBLISHER_SPKI_SHA256=<64-hex-pin>[;<second-64-hex-pin>]"
set "FRAMETIME_SIGNTOOL_PATH=<absolute-path-to-signtool.exe>"
set "FRAMETIME_MAKECAT_PATH=<absolute-path-to-MakeCat.exe>"
set "FRAMETIME_SIGNING_CERT_SHA1=<40-hex-certificate-thumbprint>"
set "FRAMETIME_SIGNING_TIMESTAMP_URL=<https-timestamp-url>"
cargo build --release -p frametime-cli -p frametime-gui --locked --target x86_64-pc-windows-msvc
call scripts\package.cmd /release
call scripts\package.cmd /verify /release
```

`FRAMETIME_PUBLISHER_SPKI_SHA256` must be present during compilation because the
application embeds the accepted pin set. The packaging script reads the same
variable and rejects missing, malformed, duplicate, or more-than-two pins. The
two-pin form supports a controlled certificate transition; both pins stay
accepted by that build.

The script signs `frametime.exe`, `frametime-gui.exe`, and `package.cat` with
SHA-256 file digests and timestamp digests. It then catalogs
`package.manifest.json` and every entry in
[`package-layout.txt`](../package-layout.txt), and verifies direct signatures,
catalog membership, the extracted ZIP tree, and the executable's
`package-auth-smoke` path.

## Unsigned structural package

The CI-equivalent lane clears the publisher pin, builds the default fail-closed
binaries, and checks the output structure:

```bat
set "FRAMETIME_PUBLISHER_SPKI_SHA256="
cargo build --release -p frametime-cli -p frametime-gui --locked --target x86_64-pc-windows-msvc
call scripts\package.cmd /unsigned
call scripts\package.cmd /verify /unsigned
```

Do not distribute this output, or describe it, as an authenticated release.

## Outputs

The script derives the archive version from the root Cargo workspace version and
writes generated output under `dist\`:

- `frametime-cfg-rust\` — unpacked package directory;
- `frametime-cfg-rust-v<VERSION>.zip` — portable archive;
- `.zip.sha256` — transport checksum;
- `.transport.json` — external archive and manifest hashes.

The package directory contains `package.manifest.json`; `/release` additionally
contains `package.cat`. The ZIP checksum and transport manifest sit outside the
authenticated in-package boundary.

## Release evidence

Before publishing, retain the package-script output and record:

- the Windows version and architecture;
- the certificate identity and accepted pin set, without private material;
- the package-authentication result and privilege level;
- the exercised entrypoint;
- focused VM or hardware recovery evidence.

The repository does not define an upload destination, release-creation command,
support window, or automatic rollback procedure. Do not infer one.
