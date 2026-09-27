use std::{ffi::c_void, mem::size_of};

use frametime_domain::driver::Sha256Digest;
use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND},
        Security::{
            Cryptography::{
                CALG_SHA_256, CERT_CONTEXT, CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
                CryptHashCertificate,
            },
            WinTrust::{
                WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
                WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
                WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
                WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
                WTHelperProvDataFromStateData, WinVerifyTrust,
            },
        },
        Storage::FileSystem::{
            CREATE_NEW, CreateFileW, DeleteFileW, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
            FileIdInfo, FlushFileBuffers, GetFileInformationByHandleEx, GetFileSizeEx,
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_EXISTING,
            ReadFile, SetFilePointerEx, WriteFile,
        },
    },
    core::PCWSTR,
};

use super::{
    AdapterFailure, DRIVER_ARTIFACTS_LEAF, NvidiaArtifactLocation, NvidiaDownloadHost,
    VerifiedDriverArtifact, adapter,
};

mod certificates;
use certificates::signer_from_verified_state;
mod http;
use http::download_to_handle;
mod driver_store;
pub(super) use driver_store::{bind_driver_store_identities, published_inf_is_present};

#[derive(Debug)]
pub(super) struct RetainedArtifact {
    handle: HANDLE,
    path: String,
    id: FILE_ID_INFO,
}

impl Drop for RetainedArtifact {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

impl RetainedArtifact {
    pub(super) fn revalidate(
        &self,
        digest: &Sha256Digest,
        expected_length: u64,
    ) -> Result<(), AdapterFailure> {
        let current_id = file_id(self.handle)?;
        if current_id != self.id
            || file_length(self.handle)? != expected_length
            || digest_handle(self.handle)? != *digest
        {
            return Err(adapter(
                "revalidate artifact",
                "retained file identity, size, or digest changed",
            ));
        }
        crate::trusted_work_dir::validate_descendant_handle(
            self.handle,
            &format!("{DRIVER_ARTIFACTS_LEAF}\\{}", leaf_from_path(&self.path)?),
            false,
        )
        .map_err(|e| adapter("revalidate artifact", e))
    }

    pub(super) fn verify_signature(&self) -> Result<(String, String), AdapterFailure> {
        let path = wide(&self.path);
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: u32::try_from(size_of::<WINTRUST_FILE_INFO>())
                .map_err(|_| adapter("verify NVIDIA artifact", "file info size exceeds u32"))?,
            pcwszFilePath: PCWSTR(path.as_ptr()),
            hFile: self.handle,
            pgKnownSubject: std::ptr::null_mut(),
        };
        let mut trust = WINTRUST_DATA {
            cbStruct: u32::try_from(size_of::<WINTRUST_DATA>())
                .map_err(|_| adapter("verify NVIDIA artifact", "trust data size exceeds u32"))?,
            dwUIChoice: WTD_UI_NONE,
            // Revocation retrieval is deliberately fail-closed: a revoked,
            // offline, or otherwise indeterminate intermediate must make
            // WinVerifyTrust fail rather than authorize an installer.
            fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
            dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        let (mut action, status) = verify_trust_state(&mut trust);
        if status != 0 {
            return Err(adapter(
                "verify NVIDIA artifact",
                format!("WinVerifyTrust failed: {status}"),
            ));
        }
        let result = signer_from_verified_state(trust.hWVTStateData);
        trust.dwStateAction = WTD_STATEACTION_CLOSE;
        close_trust_state(&mut trust, &mut action);
        result
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }
}

fn verify_trust_state(trust: &mut WINTRUST_DATA) -> (windows::core::GUID, i32) {
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND(std::ptr::null_mut()),
            &mut action,
            (trust as *mut WINTRUST_DATA).cast(),
        )
    };
    (action, status)
}

fn close_trust_state(trust: &mut WINTRUST_DATA, action: &mut windows::core::GUID) {
    let _ = unsafe {
        WinVerifyTrust(
            HWND(std::ptr::null_mut()),
            action,
            (trust as *mut WINTRUST_DATA).cast(),
        )
    };
}

fn retain_path(rendered: String) -> Result<RetainedArtifact, AdapterFailure> {
    let handle = open_retained(&rendered)?;
    let mut retained = RetainedArtifact {
        handle,
        path: rendered,
        id: FILE_ID_INFO::default(),
    };
    retained.id = file_id(retained.handle)?;
    Ok(retained)
}

pub(super) fn verify_microsoft_signed_path(path: &std::path::Path) -> Result<(), AdapterFailure> {
    let rendered = path.to_string_lossy().into_owned();
    let retained = retain_path(rendered)?;
    let (subject, _) = retained.verify_signature()?;
    if !subject.to_ascii_lowercase().contains("microsoft") {
        return Err(adapter(
            "verify WDK tool",
            "authenticated tool signer is not Microsoft",
        ));
    }
    Ok(())
}

pub(super) fn run_microsoft_signed_system_tool(
    path: &std::path::Path,
    argv: &[String],
) -> Result<super::ProcessOutcome, AdapterFailure> {
    let rendered = path.to_string_lossy().into_owned();
    let retained = retain_path(rendered)?;
    let length = file_length(retained.handle)?;
    let digest = digest_handle(retained.handle)?;
    let (subject, _) = retained.verify_signature()?;
    if !subject.to_ascii_lowercase().contains("microsoft") {
        return Err(adapter(
            "verify Windows system tool",
            "authenticated system-tool signer is not Microsoft",
        ));
    }
    let status = std::process::Command::new(path)
        .args(argv)
        .status()
        .map_err(|error| adapter("launch Windows system tool", error.to_string()))?;
    if file_id(retained.handle)? != retained.id
        || file_length(retained.handle)? != length
        || digest_handle(retained.handle)? != digest
    {
        return Err(adapter(
            "revalidate Windows system tool",
            "system-tool identity, size, or digest changed across launch",
        ));
    }
    Ok(super::ProcessOutcome {
        exit_code: status.code(),
    })
}

pub(super) fn launch_prepared_nvidia_package(
    expected: &frametime_domain::driver::SignedArtifactDescriptor,
    expected_package_sha256: &Sha256Digest,
    observed_at_utc: &str,
) -> Result<super::PreparedNvidiaLaunchEvidence, AdapterFailure> {
    let package = std::path::Path::new(crate::WINDOWS_WORK_DIR).join("driver-package");
    let manifest = super::super::package_builder::verify_prepared_nvidia_package(&package)
        .map_err(|reason| adapter("verify prepared NVIDIA package", reason))?;
    let package_sha256 = super::super::package_builder::prepared_nvidia_package_digest(&manifest)
        .map_err(|reason| adapter("verify prepared NVIDIA package", reason))?;
    if &package_sha256 != expected_package_sha256 {
        return Err(adapter(
            "verify prepared NVIDIA package",
            "package manifest does not match the durable transaction",
        ));
    }
    let manifest_setup = manifest
        .files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case("setup.exe"))
        .ok_or_else(|| {
            adapter(
                "verify prepared NVIDIA package",
                "package manifest lacks setup.exe identity",
            )
        })?;
    let setup = package.join("setup.exe");
    let rendered = setup.to_string_lossy().into_owned();
    let retained = retain_path(rendered)?;
    let length = file_length(retained.handle)?;
    let digest = digest_handle(retained.handle)?;
    let manifest_digest = Sha256Digest::parse(&manifest_setup.sha256)
        .map_err(|error| adapter("verify prepared NVIDIA installer", error.to_string()))?;
    if manifest_setup.bytes != length || manifest_digest != digest {
        return Err(adapter(
            "verify prepared NVIDIA installer",
            "setup.exe does not match its authenticated package manifest",
        ));
    }
    crate::trusted_work_dir::validate_descendant_handle(
        retained.handle,
        "driver-package\\setup.exe",
        false,
    )
    .map_err(|reason| adapter("verify prepared NVIDIA installer", reason))?;
    let (subject, thumbprint) = retained.verify_signature()?;
    if subject != expected.authenticode.signer_subject
        || thumbprint.to_ascii_lowercase()
            != expected.authenticode.signer_thumbprint_sha256.as_str()
        || !super::artifact::accepts_compiled_nvidia_policy(&subject, &thumbprint)
    {
        return Err(adapter(
            "verify prepared NVIDIA installer",
            "prepared setup.exe does not match the exact NVIDIA signer policy",
        ));
    }
    let status = std::process::Command::new(&setup)
        .current_dir(&package)
        .args(["-s", "-noreboot"])
        .status()
        .map_err(|error| adapter("launch prepared NVIDIA installer", error.to_string()))?;
    if file_id(retained.handle)? != retained.id
        || file_length(retained.handle)? != length
        || digest_handle(retained.handle)? != digest
    {
        return Err(adapter(
            "revalidate prepared NVIDIA installer",
            "setup.exe identity, size, or digest changed across launch",
        ));
    }
    let thumbprint = Sha256Digest::parse(thumbprint.to_ascii_lowercase())
        .map_err(|error| adapter("record prepared NVIDIA installer", error.to_string()))?;
    Ok(super::PreparedNvidiaLaunchEvidence {
        outcome: super::ProcessOutcome {
            exit_code: status.code(),
        },
        artifact: frametime_domain::driver::ArtifactIdentity {
            artifact_id: expected.locator.artifact_id.clone(),
            artifact_file_name: "setup.exe".into(),
            payload_sha256: digest,
            signer_thumbprint_sha256: thumbprint.clone(),
        },
        authenticode: frametime_domain::driver::AuthenticodeEvidence {
            status: frametime_domain::driver::AuthenticodeStatus::Valid,
            signer_subject: subject,
            signer_thumbprint_sha256: thumbprint,
            observed_at_utc: observed_at_utc.into(),
            extensions: Default::default(),
        },
    })
}

pub(super) fn acquire(
    root: &crate::TrustedWorkDir,
    location: &NvidiaArtifactLocation,
    protected_leaf: &str,
    maximum: usize,
) -> Result<VerifiedDriverArtifact, AdapterFailure> {
    let leaf = checked_leaf(protected_leaf)?;
    let directory = root
        .driver_artifact_directory_handle()
        .map_err(|e| adapter("open artifact store", e))?;
    let path = format!(
        "{}\\{}\\{}",
        crate::WINDOWS_WORK_DIR,
        DRIVER_ARTIFACTS_LEAF,
        leaf
    );
    let handle = match location {
        NvidiaArtifactLocation::Official {
            host,
            path: server_path,
        } => {
            // A partial download never occupies the retained final leaf. A
            // retry removes only this fixed sibling, flushes and hashes it,
            // then atomically publishes the verified bytes.
            let temporary = format!("{path}.download.tmp");
            let _ = delete_if_present(&temporary);
            let handle = create_new(&temporary)?;
            let published = download_to_handle(host, server_path, handle, maximum)
                .and_then(|_| flush(handle))
                .and_then(|_| verify_download(handle, maximum))
                .and_then(|_| rename_atomically(&temporary, &path));
            if let Err(error) = published {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                let _ = delete_if_present(&temporary);
                return Err(error);
            }
            unsafe {
                let _ = CloseHandle(handle);
            }
            open_retained(&path)?
        }
        NvidiaArtifactLocation::LocalLeaf(_) => open_retained(&path)?,
    };
    let _directory = directory;
    crate::trusted_work_dir::validate_descendant_handle(
        handle,
        &format!("{DRIVER_ARTIFACTS_LEAF}\\{leaf}"),
        false,
    )
    .map_err(|e| adapter("open artifact", e))?;
    let length = file_length(handle)?;
    let maximum = u64::try_from(maximum)
        .map_err(|_| adapter("acquire artifact", "maximum artifact size exceeds u64"))?;
    if length == 0 || length > maximum {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(adapter(
            "acquire artifact",
            "artifact length is outside the bounded policy",
        ));
    }
    let digest = digest_handle(handle)?;
    let id = file_id(handle)?;
    Ok(VerifiedDriverArtifact {
        protected_leaf: protected_leaf.into(),
        length,
        payload_sha256: digest,
        retained: Some(RetainedArtifact { handle, path, id }),
    })
}

pub(super) fn launch(
    artifact: &VerifiedDriverArtifact,
    argv: &[String],
) -> Result<super::ProcessOutcome, AdapterFailure> {
    artifact.revalidate()?;
    let status = std::process::Command::new(artifact.retained()?.path())
        .args(argv)
        .status()
        .map_err(|e| adapter("launch NVIDIA artifact", e.to_string()))?;
    artifact.revalidate()?;
    Ok(super::ProcessOutcome {
        exit_code: status.code(),
    })
}

fn checked_leaf(protected_leaf: &str) -> Result<&str, AdapterFailure> {
    let leaf = protected_leaf
        .strip_prefix("driver-artifacts/")
        .ok_or_else(|| adapter("acquire artifact", "artifact escaped protected directory"))?;
    if leaf.is_empty()
        || leaf.len() > 128
        || !leaf
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(adapter(
            "acquire artifact",
            "artifact name is not a fixed safe leaf",
        ));
    }
    Ok(leaf)
}

fn leaf_from_path(path: &str) -> Result<&str, AdapterFailure> {
    path.rsplit('\\')
        .next()
        .filter(|leaf| !leaf.is_empty())
        .ok_or_else(|| adapter("revalidate artifact", "artifact path has no fixed leaf"))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn create_new(path: &str) -> Result<HANDLE, AdapterFailure> {
    let path = wide(path);
    unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            Default::default(),
            None,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(|e| adapter("publish artifact", e.to_string()))
}
fn delete_if_present(path: &str) -> Result<(), AdapterFailure> {
    let path = wide(path);
    match unsafe { DeleteFileW(PCWSTR(path.as_ptr())) } {
        Ok(()) => Ok(()),
        Err(error) if error.code().0 == 2 => Ok(()),
        Err(error) => Err(adapter("remove partial NVIDIA artifact", error.to_string())),
    }
}
fn verify_download(handle: HANDLE, maximum: usize) -> Result<(), AdapterFailure> {
    let length = file_length(handle)?;
    let maximum = u64::try_from(maximum)
        .map_err(|_| adapter("publish artifact", "maximum artifact size exceeds u64"))?;
    if length == 0 || length > maximum {
        return Err(adapter(
            "publish artifact",
            "download length is outside bounded policy",
        ));
    }
    let _ = digest_handle(handle)?;
    Ok(())
}
fn rename_atomically(temporary: &str, final_path: &str) -> Result<(), AdapterFailure> {
    let temporary = wide(temporary);
    let final_path = wide(final_path);
    unsafe {
        MoveFileExW(
            PCWSTR(temporary.as_ptr()),
            PCWSTR(final_path.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| adapter("publish artifact", error.to_string()))
}
fn open_retained(path: &str) -> Result<HANDLE, AdapterFailure> {
    let path = wide(path);
    unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(|e| adapter("open artifact", e.to_string()))
}
fn flush(handle: HANDLE) -> Result<(), AdapterFailure> {
    unsafe { FlushFileBuffers(handle) }.map_err(|e| adapter("publish artifact", e.to_string()))
}
fn file_id(handle: HANDLE) -> Result<FILE_ID_INFO, AdapterFailure> {
    let mut id = FILE_ID_INFO::default();
    let byte_count = file_id_info_byte_count()?;
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast::<c_void>(),
            byte_count,
        )
    }
    .map_err(|e| adapter("inspect artifact", e.to_string()))?;
    Ok(id)
}

fn file_id_info_byte_count() -> Result<u32, AdapterFailure> {
    u32::try_from(size_of::<FILE_ID_INFO>())
        .map_err(|_| adapter("inspect artifact", "file ID size exceeds u32"))
}
fn file_length(handle: HANDLE) -> Result<u64, AdapterFailure> {
    let mut size = 0_i64;
    unsafe { GetFileSizeEx(handle, &mut size) }
        .map_err(|e| adapter("inspect artifact", e.to_string()))?;
    u64::try_from(size).map_err(|_| adapter("inspect artifact", "artifact size is negative"))
}
fn digest_handle(handle: HANDLE) -> Result<Sha256Digest, AdapterFailure> {
    unsafe { SetFilePointerEx(handle, 0, None, FILE_BEGIN) }
        .map_err(|e| adapter("hash artifact", e.to_string()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let mut read = 0;
        unsafe { ReadFile(handle, Some(&mut buffer), Some(&mut read), None) }
            .map_err(|e| adapter("hash artifact", e.to_string()))?;
        if read == 0 {
            break;
        }
        let count = usize::try_from(read)
            .map_err(|_| adapter("hash artifact", "read count exceeds address space"))?;
        hash.update(&buffer[..count]);
    }
    Sha256Digest::parse(format!("{:x}", hash.finalize()))
        .map_err(|e| adapter("hash artifact", e.to_string()))
}
