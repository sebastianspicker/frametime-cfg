use std::{ffi::c_void, mem::size_of};

use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND},
        Networking::WinHttp::{
            INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
            WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen,
            WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
            WinHttpSendRequest, WinHttpSetOption,
        },
        Security::{
            Cryptography::{
                CALG_SHA_256, CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
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
    Sha256Digest, VerifiedDriverArtifact, adapter,
};

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
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(path.as_ptr()),
            hFile: self.handle,
            pgKnownSubject: std::ptr::null_mut(),
        };
        let mut trust = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
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
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let status = unsafe {
            WinVerifyTrust(
                HWND(std::ptr::null_mut()),
                &mut action,
                (&mut trust as *mut WINTRUST_DATA).cast(),
            )
        };
        if status != 0 {
            return Err(adapter(
                "verify NVIDIA artifact",
                format!("WinVerifyTrust failed: {status}"),
            ));
        }
        let result = signer_from_verified_state(trust.hWVTStateData);
        trust.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = unsafe {
            WinVerifyTrust(
                HWND(std::ptr::null_mut()),
                &mut action,
                (&mut trust as *mut WINTRUST_DATA).cast(),
            )
        };
        result
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }
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
    if length == 0 || length > maximum as u64 {
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
        #[cfg(test)]
        test_bytes: Vec::new(),
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
    if length == 0 || length > maximum as u64 {
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
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast::<c_void>(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    }
    .map_err(|e| adapter("inspect artifact", e.to_string()))?;
    Ok(id)
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
        hash.update(&buffer[..read as usize]);
    }
    Sha256Digest::parse(format!("{:x}", hash.finalize()))
        .map_err(|e| adapter("hash artifact", e.to_string()))
}
fn download_to_handle(
    host: &NvidiaDownloadHost,
    path: &str,
    file: HANDLE,
    maximum: usize,
) -> Result<(), AdapterFailure> {
    let session = http_open()?;
    let authority = wide(host.authority());
    let connection = Http(unsafe {
        WinHttpConnect(
            session.0,
            PCWSTR(authority.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    });
    if connection.0.is_null() {
        return Err(adapter("download NVIDIA artifact", "WinHttpConnect failed"));
    }
    let get = wide("GET");
    let resource = wide(path);
    let request = Http(unsafe {
        WinHttpOpenRequest(
            connection.0,
            PCWSTR(get.as_ptr()),
            PCWSTR(resource.as_ptr()),
            None,
            None,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(adapter(
            "download NVIDIA artifact",
            "WinHttpOpenRequest failed",
        ));
    }
    unsafe {
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_DISABLE_REDIRECTS,
            Some(&[1, 0, 0, 0]),
        )
    }
    .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, 0) }
        .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    let mut status = 0_u32;
    let mut length = size_of::<u32>() as u32;
    let mut index = 0;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some((&mut status as *mut u32).cast()),
            &mut length,
            &mut index,
        )
    }
    .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    if status != 200 {
        return Err(adapter(
            "download NVIDIA artifact",
            format!("HTTPS response status {status} is not permitted"),
        ));
    }
    let mut total = 0_usize;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let mut read = 0;
        unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        }
        .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
        if read == 0 {
            return Ok(());
        }
        let count = read as usize;
        total = total
            .checked_add(count)
            .ok_or_else(|| adapter("download NVIDIA artifact", "response length overflow"))?;
        if total > maximum {
            return Err(adapter(
                "download NVIDIA artifact",
                "response exceeds bounded policy",
            ));
        }
        let mut written = 0;
        unsafe { WriteFile(file, Some(&buffer[..count]), Some(&mut written), None) }
            .map_err(|e| adapter("publish artifact", e.to_string()))?;
        if written as usize != count {
            return Err(adapter(
                "publish artifact",
                "short write of artifact stream",
            ));
        }
    }
}
struct Http(*mut c_void);
impl Drop for Http {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}
fn http_open() -> Result<Http, AdapterFailure> {
    let agent = wide("frametime-cfg/3");
    let session = Http(unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            None,
            None,
            0,
        )
    });
    if session.0.is_null() {
        Err(adapter("download NVIDIA artifact", "WinHttpOpen failed"))
    } else {
        Ok(session)
    }
}
fn signer_from_verified_state(state: HANDLE) -> Result<(String, String), AdapterFailure> {
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return Err(adapter(
            "verify NVIDIA artifact",
            "WinVerifyTrust returned no provider state",
        ));
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, false, 0) };
    if signer.is_null() {
        return Err(adapter(
            "verify NVIDIA artifact",
            "WinVerifyTrust returned no leaf signer",
        ));
    }
    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if certificate.is_null() || unsafe { (*certificate).pCert.is_null() } {
        return Err(adapter(
            "verify NVIDIA artifact",
            "WinVerifyTrust returned no leaf certificate",
        ));
    }
    let context = unsafe { (*certificate).pCert };
    let subject = certificate_subject(context)?;
    let thumbprint = certificate_sha256(context)?;
    Ok((format!("CN={subject}"), thumbprint))
}
fn certificate_subject(
    context: *const windows::Win32::Security::Cryptography::CERT_CONTEXT,
) -> Result<String, AdapterFailure> {
    let needed =
        unsafe { CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None) };
    if needed <= 1 {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf signer subject is absent",
        ));
    }
    let mut units = vec![0_u16; needed as usize];
    unsafe {
        CertGetNameStringW(
            context,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            None,
            Some(&mut units),
        );
    }
    String::from_utf16(&units[..units.len() - 1])
        .map_err(|e| adapter("verify NVIDIA artifact", e.to_string()))
}
fn certificate_sha256(
    context: *const windows::Win32::Security::Cryptography::CERT_CONTEXT,
) -> Result<String, AdapterFailure> {
    let certificate = unsafe { &*context };
    let encoded = unsafe {
        std::slice::from_raw_parts(
            certificate.pbCertEncoded,
            certificate.cbCertEncoded as usize,
        )
    };
    let mut length = 32;
    let mut bytes = [0_u8; 32];
    unsafe {
        CryptHashCertificate(
            None,
            CALG_SHA_256,
            0,
            encoded,
            Some(bytes.as_mut_ptr()),
            &mut length,
        )
    }
    .map_err(|e| adapter("verify NVIDIA artifact", e.to_string()))?;
    if length != 32 {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf certificate SHA-256 length is invalid",
        ));
    }
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
