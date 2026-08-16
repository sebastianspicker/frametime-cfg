@echo off
setlocal EnableExtensions EnableDelayedExpansion

rem Two intentionally separate lanes:
rem   /unsigned          structural development or CI package; never authenticated
rem   /release           authenticated Windows release; signing inputs are mandatory
rem   /verify /unsigned  verify an existing structural package
rem   /verify /release   verify an existing authenticated release package
rem
rem The 27 payload paths in package-layout.txt are the release boundary. The
rem authenticated package additionally contains package.manifest.json and, only
rem in /release, package.cat. ZIP checksums and transport manifests stay outside
rem the package and are not authentication metadata.
set "source=%~dp0.."
for %%I in ("%source%") do set "source=%%~fI"
set "dist=%source%\dist"
set "version="
for /f "tokens=2 delims==" %%V in ('findstr /r /b /c:"version = " "%source%\Cargo.toml"') do if not defined version set "version=%%V"
set "version=!version: =!"
set "version=!version:"=!"
if not defined version exit /b 2
set "package_name=frametime-cfg-rust"
set "archive_name=frametime-cfg-rust-v%version%"
set "target=%dist%\%package_name%"
set "staging=%dist%\%package_name%.staging"
set "zip=%dist%\%archive_name%.zip"
set "checksum=%zip%.sha256"
set "transport_manifest=%dist%\%archive_name%.transport.json"
set "package_manifest=%target%\package.manifest.json"
set "catalog=%target%\package.cat"
set "package_mode="
set "verify_only=0"

if /i "%~1"=="/verify" goto parse_verify
if /i "%~1"=="/unsigned" goto parse_unsigned
if /i "%~1"=="/release" goto parse_release
exit /b 2

:parse_verify
set "verify_only=1"
if /i "%~2"=="/unsigned" goto parse_unsigned_verified
if /i "%~2"=="/release" goto parse_release_verified
exit /b 2

:parse_unsigned
if not "%~2"=="" exit /b 2
set "package_mode=unsigned"
goto package_start

:parse_release
if not "%~2"=="" exit /b 2
set "package_mode=release"
goto package_start

:parse_unsigned_verified
if not "%~3"=="" exit /b 2
set "package_mode=unsigned"
goto package_start

:parse_release_verified
if not "%~3"=="" exit /b 2
set "package_mode=release"

:package_start
if /i "%package_mode%"=="release" (
    if "%verify_only%"=="1" (
        call :require_release_verification_inputs || exit /b 2
    ) else (
        call :require_release_inputs || exit /b 2
    )
)
if "%verify_only%"=="1" goto verify_existing

if not exist "%source%\Cargo.toml" exit /b 2
if not exist "%source%\target\x86_64-pc-windows-msvc\release\frametime.exe" exit /b 2
if not exist "%source%\target\x86_64-pc-windows-msvc\release\frametime-gui.exe" exit /b 2
if exist "%staging%" rmdir /s /q "%staging%" || exit /b 1
if not exist "%dist%" mkdir "%dist%" || exit /b 1
mkdir "%staging%" || exit /b 1

rem Reject source additions under payload directories unless they are listed.
for /r "%source%\assets" %%F in (*) do call :assert_source_allowed "%%~fF" || exit /b 1
for /r "%source%\docs" %%F in (*) do call :assert_source_allowed "%%~fF" || exit /b 1
for /r "%source%\licenses" %%F in (*) do call :assert_source_allowed "%%~fF" || exit /b 1

call :copy_one "target\x86_64-pc-windows-msvc\release\frametime.exe" "frametime.exe" || exit /b 1
call :copy_one "target\x86_64-pc-windows-msvc\release\frametime-gui.exe" "frametime-gui.exe" || exit /b 1
call :copy_one "frametime.toml" "frametime.toml" || exit /b 1
call :copy_one "README.md" "README.md" || exit /b 1
call :copy_one "assets\video.txt" "assets\video.txt" || exit /b 1
call :copy_one "assets\cfgs\audio_lowlatency_001.cfg" "assets\cfgs\audio_lowlatency_001.cfg" || exit /b 1
call :copy_one "assets\cfgs\audio_lowlatency_025.cfg" "assets\cfgs\audio_lowlatency_025.cfg" || exit /b 1
call :copy_one "assets\cfgs\audio_stable.cfg" "assets\cfgs\audio_stable.cfg" || exit /b 1
call :copy_one "assets\cfgs\autoexec.cfg.example" "assets\cfgs\autoexec.cfg.example" || exit /b 1
call :copy_one "assets\cfgs\debug_hud.cfg" "assets\cfgs\debug_hud.cfg" || exit /b 1
call :copy_one "assets\cfgs\debug_hud_off.cfg" "assets\cfgs\debug_hud_off.cfg" || exit /b 1
call :copy_one "assets\cfgs\net_bad.cfg" "assets\cfgs\net_bad.cfg" || exit /b 1
call :copy_one "assets\cfgs\net_highping.cfg" "assets\cfgs\net_highping.cfg" || exit /b 1
call :copy_one "assets\cfgs\net_stable.cfg" "assets\cfgs\net_stable.cfg" || exit /b 1
call :copy_one "assets\cfgs\net_unstable.cfg" "assets\cfgs\net_unstable.cfg" || exit /b 1
call :copy_one "assets\cfgs\optimization.cfg.template" "assets\cfgs\optimization.cfg.template" || exit /b 1
call :copy_one "assets\cfgs\valve-latency-targets.json" "assets\cfgs\valve-latency-targets.json" || exit /b 1
call :copy_one "docs\compatibility-ledger.md" "docs\compatibility-ledger.md" || exit /b 1
call :copy_one "docs\gui.md" "docs\gui.md" || exit /b 1
call :copy_one "docs\integrations.md" "docs\integrations.md" || exit /b 1
call :copy_one "docs\nvidia-drs-settings.md" "docs\nvidia-drs-settings.md" || exit /b 1
call :copy_one "docs\operations.md" "docs\operations.md" || exit /b 1
call :copy_one "docs\recovery.md" "docs\recovery.md" || exit /b 1
call :copy_one "licenses\LICENSE" "licenses\LICENSE" || exit /b 1
call :copy_one "licenses\LICENSE-APACHE-2.0" "licenses\LICENSE-APACHE-2.0" || exit /b 1
call :copy_one "licenses\LICENSE-MIT" "licenses\LICENSE-MIT" || exit /b 1
call :copy_one "licenses\THIRD_PARTY_NOTICES.md" "licenses\THIRD_PARTY_NOTICES.md" || exit /b 1

call :validate_payload "%staging%" || exit /b 1
call :scan_payload "%staging%" || exit /b 1
if /i "%package_mode%"=="release" (
    call :sign_file "%staging%\frametime.exe" || exit /b 1
    call :sign_file "%staging%\frametime-gui.exe" || exit /b 1
)

if exist "%target%" rmdir /s /q "%target%" || exit /b 1
move "%staging%" "%target%" >nul || exit /b 1
call :write_manifest "%target%" "%package_manifest%" || exit /b 1
if /i "%package_mode%"=="release" call :make_and_sign_catalog "%target%" "%catalog%" || exit /b 1
call :validate_tree "%target%" || exit /b 1
call :scan_payload "%target%" || exit /b 1
if /i "%package_mode%"=="release" call :verify_release_authentication "%target%" || exit /b 1

if exist "%zip%" del /f /q "%zip%" || exit /b 1
pushd "%dist%" || exit /b 1
tar.exe -a -c -f "%zip%" "%package_name%" >nul 2>&1
set "tar_error=!errorlevel!"
popd
if not "!tar_error!"=="0" exit /b 1
call :hash_file "%zip%" || exit /b 1
>"%checksum%" echo !hash!  %archive_name%.zip
call :write_transport_manifest "%transport_manifest%" || exit /b 1
call :verify_artifacts || exit /b 1
call :verify_zip || exit /b 1
echo %package_mode% package assembled at %target%
echo ZIP transport artifact: %zip%
echo Authenticated in-package manifest: %package_manifest%
echo ZIP SHA-256: %checksum%
exit /b 0

:verify_existing
if not exist "%target%" exit /b 2
if not exist "%zip%" exit /b 2
if not exist "%checksum%" exit /b 2
if not exist "%transport_manifest%" exit /b 2
if not exist "%package_manifest%" exit /b 2
call :validate_tree "%target%" || exit /b 1
call :scan_payload "%target%" || exit /b 1
call :verify_manifest "%target%" "%package_manifest%" || exit /b 1
if /i "%package_mode%"=="release" call :verify_release_authentication "%target%" || exit /b 1
call :hash_file "%zip%" || exit /b 1
set "expected_checksum=%checksum%.verify.tmp"
if exist "%expected_checksum%" del /f /q "%expected_checksum%" >nul || exit /b 1
>"%expected_checksum%" echo !hash!  %archive_name%.zip
fc /b "%expected_checksum%" "%checksum%" >nul
set "fc_error=!errorlevel!"
del /f /q "%expected_checksum%" >nul 2>&1
if not "!fc_error!"=="0" exit /b 1
call :verify_transport_manifest "%transport_manifest%" || exit /b 1
call :verify_artifacts || exit /b 1
call :verify_zip || exit /b 1
echo Existing %package_mode% package verified at %target%
exit /b 0

:copy_one
if not exist "%source%\%~1" (
    echo Missing required release input: %~1 1>&2
    exit /b 1
)
for %%D in ("%staging%\%~dp2") do if not exist "%%~fD" mkdir "%%~fD" || exit /b 1
copy /b /y "%source%\%~1" "%staging%\%~2" >nul || exit /b 1
exit /b 0

:assert_source_allowed
set "relative=%~1"
set "relative=!relative:%source%\=!"
call :is_allowed "!relative!"
if errorlevel 1 (
    echo Unlisted source file under a payload directory: !relative! 1>&2
    exit /b 1
)
exit /b 0

:validate_payload
set "payload_count=0"
for /r "%~1" %%F in (*) do (
    set /a payload_count+=1
    call :assert_payload_allowed "%%~fF" "%~1" || exit /b 1
)
if not "!payload_count!"=="27" (
    echo Package payload must contain exactly 27 files; found !payload_count!. 1>&2
    exit /b 1
)
exit /b 0

:validate_tree
set "tree_count=0"
for /r "%~1" %%F in (*) do (
    set /a tree_count+=1
    call :assert_staged_allowed "%%~fF" "%~1" || exit /b 1
)
set "expected_tree_count=28"
if /i "%package_mode%"=="release" set "expected_tree_count=29"
if not "!tree_count!"=="!expected_tree_count!" (
    echo Package inventory must contain !expected_tree_count! files; found !tree_count!. 1>&2
    exit /b 1
)
exit /b 0

:assert_payload_allowed
set "relative=%~1"
set "relative=!relative:%~2\=!"
set "relative=!relative:\=/!"
call :is_allowed "!relative!"
if errorlevel 1 (
    echo Unlisted file in package payload: !relative! 1>&2
    exit /b 1
)
exit /b 0

:assert_staged_allowed
set "relative=%~1"
set "relative=!relative:%~2\=!"
set "relative=!relative:\=/!"
if /i "!relative!"=="package.manifest.json" exit /b 0
if /i "%package_mode%"=="release" if /i "!relative!"=="package.cat" exit /b 0
call :assert_payload_allowed "%~1" "%~2"
exit /b !errorlevel!

:is_allowed
set "candidate=%~1"
set "candidate=!candidate:\=/!"
findstr /l /i /x /c:"!candidate!" "%source%\package-layout.txt" >nul
exit /b !errorlevel!

:scan_payload
for /r "%~1" %%F in (*) do call :scan_file "%%~fF" || exit /b 1
exit /b 0

:scan_file
if /i "%~nx1"=="package.manifest.json" exit /b 0
if /i "%~nx1"=="package.cat" exit /b 0
if /i "%~x1"==".ps1" (
    echo PowerShell source file is not permitted in the package: %~1 1>&2
    exit /b 1
)
findstr /m /i /c:"powershell.exe" /c:"pwsh.exe" /c:"system.management.automation" /c:"microsoft.powershell" /c:"invoke-expression" /c:"start-process" "%~1" >nul 2>&1
if not errorlevel 1 (
    echo PowerShell runtime or source dependency marker in package file: %~1 1>&2
    exit /b 1
)
exit /b 0

:write_manifest
set "manifest_path=%~2"
>"%manifest_path%" echo {
>>"%manifest_path%" echo   "schema_version": 1,
>>"%manifest_path%" echo   "version": "%version%",
>>"%manifest_path%" echo   "files": [
set "manifest_first=1"
for /f "delims=" %%F in ('dir /b /s /a-d "%~1" ^| sort') do call :manifest_entry "%%~fF" "%~1" || exit /b 1
>>"%manifest_path%" echo   ]
>>"%manifest_path%" echo }
exit /b 0

:manifest_entry
set "relative=%~1"
set "relative=!relative:%~2\=!"
set "relative=!relative:\=/!"
if /i "!relative!"=="package.manifest.json" exit /b 0
if /i "!relative!"=="package.cat" exit /b 0
call :hash_file "%~1" || exit /b 1
if "!manifest_first!"=="0" >>"%manifest_path%" echo     ,
>>"%manifest_path%" echo     {"path":"!relative!","size":%~z1,"sha256":"!hash!"}
set "manifest_first=0"
exit /b 0

:verify_manifest
rem Keep the regenerated comparison outside the authenticated tree. Otherwise
rem the temporary file would enumerate itself as an additional payload member.
set "expected_manifest=%dist%\package.manifest.verify.tmp"
set "manifest_error=0"
if exist "%expected_manifest%" del /f /q "%expected_manifest%" >nul
if exist "%expected_manifest%" set "manifest_error=1"
if "!manifest_error!"=="0" call :write_manifest "%~1" "%expected_manifest%" || set "manifest_error=1"
if "!manifest_error!"=="0" (
    fc /b "%expected_manifest%" "%~2" >nul
    if errorlevel 1 set "manifest_error=1"
)
if exist "%expected_manifest%" del /f /q "%expected_manifest%" >nul 2>&1
if exist "%expected_manifest%" set "manifest_error=1"
if not "!manifest_error!"=="0" (
    echo In-package manifest does not exactly match the 27 payload files, sizes, and SHA-256 hashes. 1>&2
    exit /b 1
)
exit /b 0

:write_transport_manifest
call :hash_file "%zip%" || exit /b 1
set "zip_hash=!hash!"
call :hash_file "%package_manifest%" || exit /b 1
set "package_manifest_hash=!hash!"
>"%~1" echo {
>>"%~1" echo   "schema_version": 1,
>>"%~1" echo   "artifact": "%archive_name%.zip",
>>"%~1" echo   "sha256": "!zip_hash!",
>>"%~1" echo   "authenticated_package_manifest": "package.manifest.json",
>>"%~1" echo   "authenticated_package_manifest_sha256": "!package_manifest_hash!"
>>"%~1" echo }
exit /b 0

:verify_transport_manifest
set "expected_transport=%~1.verify.tmp"
set "transport_error=0"
if exist "%expected_transport%" del /f /q "%expected_transport%" >nul
if exist "%expected_transport%" set "transport_error=1"
if "!transport_error!"=="0" call :write_transport_manifest "%expected_transport%" || set "transport_error=1"
if "!transport_error!"=="0" (
    fc /b "%expected_transport%" "%~1" >nul
    if errorlevel 1 set "transport_error=1"
)
if exist "%expected_transport%" del /f /q "%expected_transport%" >nul 2>&1
if exist "%expected_transport%" set "transport_error=1"
if not "!transport_error!"=="0" (
    echo External ZIP transport manifest does not match the archive or in-package manifest. 1>&2
    exit /b 1
)
exit /b 0

:make_and_sign_catalog
set "catalog_definition=%~1\package.cdf"
call :write_catalog_definition "%~1" "%catalog_definition%" || exit /b 1
pushd "%~1" || exit /b 1
"%makecat_tool%" -r -v "%catalog_definition%"
set "makecat_error=!errorlevel!"
popd
del /f /q "%catalog_definition%" >nul 2>&1
if exist "%catalog_definition%" exit /b 1
if not "!makecat_error!"=="0" exit /b 1
if not exist "%~2" exit /b 1
call :sign_file "%~2" || exit /b 1
exit /b 0

:write_catalog_definition
set "catalog_definition=%~2"
>"%catalog_definition%" echo [CatalogHeader]
>>"%catalog_definition%" echo Name=package.cat
>>"%catalog_definition%" echo ResultDir=%~1
>>"%catalog_definition%" echo PublicVersion=0x0000001
>>"%catalog_definition%" echo CatalogVersion=2
>>"%catalog_definition%" echo HashAlgorithms=SHA256
>>"%catalog_definition%" echo EncodingType=0x00010001
>>"%catalog_definition%" echo CATATTR1=0x10010001:OSAttr:2:10.0
>>"%catalog_definition%" echo [CatalogFiles]
>>"%catalog_definition%" echo ^<HASH^>package.manifest.json=package.manifest.json
for /f "usebackq delims=" %%F in ("%source%\package-layout.txt") do call :catalog_entry "%%F" || exit /b 1
exit /b 0

:catalog_entry
>>"%catalog_definition%" echo ^<HASH^>%~1=%~1
exit /b 0

:sign_file
"%sign_tool%" sign /fd SHA256 /sha1 "%signing_cert_sha1%" /tr "%signing_timestamp_url%" /td SHA256 "%~1"
if errorlevel 1 exit /b 1
exit /b 0

:verify_release_authentication
if not exist "%~1\package.cat" exit /b 1
call :verify_direct_signature "%~1\frametime.exe" || exit /b 1
call :verify_direct_signature "%~1\frametime-gui.exe" || exit /b 1
call :verify_direct_signature "%~1\package.cat" || exit /b 1
call :verify_catalog_member "%~1\package.cat" "%~1\package.manifest.json" || exit /b 1
for /f "usebackq delims=" %%F in ("%source%\package-layout.txt") do call :verify_catalog_member "%~1\package.cat" "%~1\%%F" || exit /b 1
"%~1\frametime.exe" package-auth-smoke || exit /b 1
exit /b 0

:verify_direct_signature
"%sign_tool%" verify /pa /all /v "%~1"
if errorlevel 1 exit /b 1
exit /b 0

:verify_catalog_member
"%sign_tool%" verify /pa /all /v /c "%~1" "%~2"
if errorlevel 1 exit /b 1
exit /b 0

:verify_artifacts
if not exist "%zip%" exit /b 1
if not exist "%checksum%" exit /b 1
if not exist "%transport_manifest%" exit /b 1
if not exist "%package_manifest%" exit /b 1
if /i "%package_mode%"=="release" if not exist "%catalog%" exit /b 1
exit /b 0

:verify_zip
set "zip_temp=%dist%\%package_name%.zip.verify"
set "zip_error=0"
if exist "%zip_temp%" rmdir /s /q "%zip_temp%"
if exist "%zip_temp%" set "zip_error=1"
if "!zip_error!"=="0" mkdir "%zip_temp%" || set "zip_error=1"
if "!zip_error!"=="0" tar.exe -xf "%zip%" -C "%zip_temp%" >nul 2>&1 || set "zip_error=1"
set "archive_root_entry="
if "!zip_error!"=="0" for /f "delims=" %%A in ('dir /b /a "%zip_temp%"') do (
    if defined archive_root_entry set "zip_error=1"
    if not defined archive_root_entry set "archive_root_entry=%%A"
)
if not defined archive_root_entry set "zip_error=1"
if defined archive_root_entry if /i not "!archive_root_entry!"=="%package_name%" set "zip_error=1"
if "!zip_error!"=="0" if not exist "%zip_temp%\%package_name%" set "zip_error=1"
if "!zip_error!"=="0" call :validate_tree "%zip_temp%\%package_name%" || set "zip_error=1"
if "!zip_error!"=="0" call :scan_payload "%zip_temp%\%package_name%" || set "zip_error=1"
if "!zip_error!"=="0" call :verify_manifest "%zip_temp%\%package_name%" "%zip_temp%\%package_name%\package.manifest.json" || set "zip_error=1"
if "!zip_error!"=="0" if /i "%package_mode%"=="release" call :verify_release_authentication "%zip_temp%\%package_name%" || set "zip_error=1"
if exist "%zip_temp%" rmdir /s /q "%zip_temp%" >nul 2>&1
if exist "%zip_temp%" set "zip_error=1"
if not "!zip_error!"=="0" exit /b 1
exit /b 0

:require_release_inputs
call :require_release_verification_inputs || exit /b 1
set "makecat_tool=%FRAMETIME_MAKECAT_PATH%"
set "signing_cert_sha1=%FRAMETIME_SIGNING_CERT_SHA1%"
set "signing_timestamp_url=%FRAMETIME_SIGNING_TIMESTAMP_URL%"
set "publisher_pins=%FRAMETIME_PUBLISHER_SPKI_SHA256%"
if not defined makecat_tool goto missing_release_input
if not defined signing_cert_sha1 goto missing_release_input
if not defined signing_timestamp_url goto missing_release_input
if not defined publisher_pins goto missing_release_input
if not exist "%makecat_tool%" goto missing_release_input
call :valid_sha1 "%signing_cert_sha1%" || goto missing_release_input
for /f "tokens=1,2,3 delims=;" %%A in ("%publisher_pins%") do (
    set "publisher_pin_one=%%A"
    set "publisher_pin_two=%%B"
    set "publisher_pin_three=%%C"
)
if not defined publisher_pin_one goto missing_release_input
if defined publisher_pin_three goto missing_release_input
call :valid_sha256 "%publisher_pin_one%" || goto missing_release_input
if defined publisher_pin_two call :valid_sha256 "%publisher_pin_two%" || goto missing_release_input
if defined publisher_pin_two if /i "%publisher_pin_one%"=="%publisher_pin_two%" goto missing_release_input
exit /b 0

:require_release_verification_inputs
set "sign_tool=%FRAMETIME_SIGNTOOL_PATH%"
if not defined sign_tool goto missing_verification_input
if not exist "%sign_tool%" goto missing_verification_input
exit /b 0

:missing_verification_input
echo Authenticated release verification requires FRAMETIME_SIGNTOOL_PATH. 1>&2
exit /b 1

:missing_release_input
echo Authenticated release requires FRAMETIME_PUBLISHER_SPKI_SHA256, FRAMETIME_SIGNTOOL_PATH, FRAMETIME_MAKECAT_PATH, FRAMETIME_SIGNING_CERT_SHA1, and FRAMETIME_SIGNING_TIMESTAMP_URL. 1>&2
exit /b 1

:valid_sha1
set "candidate=%~1"
if "!candidate:~39,1!"=="" exit /b 1
if not "!candidate:~40,1!"=="" exit /b 1
for /f "delims=0123456789abcdefABCDEF" %%A in ("!candidate!") do exit /b 1
exit /b 0

:valid_sha256
set "candidate=%~1"
if "!candidate:~63,1!"=="" exit /b 1
if not "!candidate:~64,1!"=="" exit /b 1
for /f "delims=0123456789abcdefABCDEF" %%A in ("!candidate!") do exit /b 1
exit /b 0
