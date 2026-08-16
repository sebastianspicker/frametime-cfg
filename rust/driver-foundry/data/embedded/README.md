# Embedded helpers (local only)

Optional binaries used for 7z extract/SFX, package tooling, and try-sign probes. **Not checked into git.** The install path falls back to pure Rust zip when helpers are missing.

## Expected layout after you materialize

```
data/embedded/
  README.md          # this file
  7zr.exe            # 7-Zip standalone
  7zSD.sfx           # SFX stub
  installer.exe      # optional package tooling
  SetupWrapper.exe
  packages.xml
  signing.zip        # or expanded signing/
  signing/
    Inf2Cat.exe
    signtool.exe
    *.dll
    WindowsProtectedFiles.xml
```

## Where to get them

Supply `7zr.exe` from an authorized local 7-Zip distribution and obtain Inf2Cat or signtool from the Windows SDK. Do not commit proprietary closed-product dumps.

Zip-only install workflows need none of this.
