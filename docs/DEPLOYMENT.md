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

## Alpha test-signing lane

Alpha testers need live authority, and an unsigned package never has it. Until
a public code-signing certificate exists, the alpha uses the `/release` lane
above with a self-signed test certificate. The trust model is unchanged: the
build pins the test certificate's SPKI exactly as it would a production one, and
testers explicitly trust that certificate on their machine.

Use a dedicated release machine or VM. Create the certificate once in
PowerShell 7.4 or later (the release machine may use PowerShell; the package
itself stays PowerShell-free). The certificate is re-read from the store
because PowerShell 7 can return the PKI module's result as a deserialized copy
without methods:

```powershell
$thumb = (New-SelfSignedCertificate -Type CodeSigningCert `
  -Subject "CN=frametime.cfg alpha test signing" `
  -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
  -KeyExportPolicy NonExportable -NotAfter (Get-Date).AddDays(90) `
  -CertStoreLocation Cert:\CurrentUser\My).Thumbprint
$cert = Get-Item "Cert:\CurrentUser\My\$thumb"
[IO.File]::WriteAllBytes("$PWD\frametime-alpha-test-signing.cer", $cert.RawData)
$thumb
(([Security.Cryptography.SHA256]::HashData($cert.PublicKey.ExportSubjectPublicKeyInfo()) |
  ForEach-Object { $_.ToString('x2') }) -join '')
(([Security.Cryptography.SHA256]::HashData($cert.RawData) |
  ForEach-Object { $_.ToString('x2') }) -join '')
```

The printed lines are, in order: the SHA-1 thumbprint for
`FRAMETIME_SIGNING_CERT_SHA1`; the SHA-256 SPKI pin for
`FRAMETIME_PUBLISHER_SPKI_SHA256`; and the certificate's SHA-256 fingerprint,
which testers check before trusting it. An equivalent pin from the exported
certificate is the hex digest printed by
`openssl x509 -inform DER -in frametime-alpha-test-signing.cer -pubkey -noout | openssl pkey -pubin -outform DER | openssl dgst -sha256 -r`
(drop the trailing ` *stdin`). Confirm with `certutil -dump
frametime-alpha-test-signing.cer` that the certificate is not a CA (Basic
Constraints absent or `Subject Type=End Entity`) and lists only the Code Signing
enhanced key usage.

Before the release build, trust the certificate on the release machine too,
because `/release` ends with `package-auth-smoke`. Then follow
[Authenticated release](#authenticated-release) unchanged, using a public RFC
3161 timestamp URL. A wrong pin fails `package-auth-smoke` there instead of on
a tester's machine.

Ship `frametime-alpha-test-signing.cer` next to the ZIP, never inside it, and
publish its SHA-256 fingerprint through a separate channel (for example the
tester invitation), not next to the file. Someone who can replace both the ZIP
and the certificate could otherwise substitute their own build, because each
build carries its own pin. Each tester, from an elevated `cmd.exe`, first checks
the fingerprint and only installs the certificate if it matches:

```bat
certutil -hashfile frametime-alpha-test-signing.cer SHA256
certutil -addstore -f Root frametime-alpha-test-signing.cer
```

After the alpha, each tester removes it again by the thumbprint published with
the release and confirms it is gone:

```bat
certutil -delstore Root <40-hex-certificate-thumbprint>
certutil -store Root <40-hex-certificate-thumbprint>
```

Trusting a root certificate lets anyone holding its private key sign code that
the tester's machine trusts, and a self-signed root cannot be revoked: if the
key leaks, the only remedy is every tester removing the certificate. A
non-exportable key still signs for any process running as that user, so keep it
on the dedicated release machine only (a TPM-backed or smart-card key is
better), keep the validity short, and delete the key when the alpha ends.
Package authentication checks revocation for the whole chain except the root; a
self-signed certificate is its own root, so it needs no CRL, but the timestamp
authority's chain is still checked online. Testers therefore need network
access when a live command authenticates the package.

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
