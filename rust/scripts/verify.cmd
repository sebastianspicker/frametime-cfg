@echo off
setlocal EnableExtensions EnableDelayedExpansion

rem Source and toolchain gate. Packaging verification is explicit so a source
rem check never silently treats an old dist directory as a fresh release.
if /i "%~1"=="/package" if /i "%~2"=="/unsigned" if "%~3"=="" (
    call "%~dp0package.cmd" /verify /unsigned
    if errorlevel 2 exit /b 2
    if errorlevel 1 exit /b 1
    exit /b 0
)
if /i "%~1"=="/package" if /i "%~2"=="/release" if "%~3"=="" (
    call "%~dp0package.cmd" /verify /release
    if errorlevel 2 exit /b 2
    if errorlevel 1 exit /b 1
    exit /b 0
)
if not "%~1"=="" exit /b 2

cargo fmt --all --check || exit /b 1
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity || exit /b 1
cargo test --workspace --all-targets --all-features --locked || exit /b 1
cargo check --workspace --all-targets --all-features --locked --target x86_64-pc-windows-msvc || exit /b 1
cargo clippy --workspace --all-targets --all-features --locked --target x86_64-pc-windows-msvc -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity || exit /b 1

rem The workspace suite runs frametime-core's production-only source_hygiene
rem test, which rejects PowerShell runtime markers and files over 600 lines.

where cargo-audit >nul 2>&1 || exit /b 2
cargo audit --deny warnings || exit /b 1
exit /b 0
