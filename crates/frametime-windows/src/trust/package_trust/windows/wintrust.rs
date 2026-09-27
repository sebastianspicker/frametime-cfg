use super::*;

pub(crate) fn verify_file(file: &RetainedFile) -> Result<String, String> {
    let wide = wide(&file.path);
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: u32::try_from(size_of::<WINTRUST_FILE_INFO>())
            .map_err(|_| "WINTRUST_FILE_INFO size exceeds u32")?,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: file.handle,
        pgKnownSubject: std::ptr::null_mut(),
    };
    verify(WTD_CHOICE_FILE, WINTRUST_DATA_0 { pFile: &mut info })
}
pub(crate) fn verify_catalog_member(
    catalog: &RetainedFile,
    member: &RetainedFile,
) -> Result<String, String> {
    let context = CatalogContext::acquire()?;
    let hash = context.hash(member.handle)?;
    let catalog_path = wide(&catalog.path);
    let member_path = wide(&member.path);
    let tag = wide_text(
        &hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    );
    let mut info = WINTRUST_CATALOG_INFO {
        cbStruct: u32::try_from(size_of::<WINTRUST_CATALOG_INFO>())
            .map_err(|_| "WINTRUST_CATALOG_INFO size exceeds u32")?,
        pcwszCatalogFilePath: PCWSTR(catalog_path.as_ptr()),
        pcwszMemberTag: PCWSTR(tag.as_ptr()),
        pcwszMemberFilePath: PCWSTR(member_path.as_ptr()),
        hMemberFile: member.handle,
        pbCalculatedFileHash: hash.as_ptr().cast_mut(),
        cbCalculatedFileHash: u32::try_from(hash.len())
            .map_err(|_| "catalog member hash length exceeds u32")?,
        hCatAdmin: context.handle,
        ..Default::default()
    };
    verify(
        WTD_CHOICE_CATALOG,
        WINTRUST_DATA_0 {
            pCatalog: &mut info,
        },
    )
}
pub(crate) fn verify(
    choice: windows::Win32::Security::WinTrust::WINTRUST_DATA_UNION_CHOICE,
    subject: WINTRUST_DATA_0,
) -> Result<String, String> {
    let mut trust = WINTRUST_DATA {
        cbStruct: u32::try_from(size_of::<WINTRUST_DATA>())
            .map_err(|_| "WINTRUST_DATA size exceeds u32")?,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: choice,
        Anonymous: subject,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
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
        return Err(trust_error(status));
    }
    let result = signer(trust.hWVTStateData);
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
pub(crate) fn trust_error(status: i32) -> String {
    match u32::from_ne_bytes(status.to_ne_bytes()) {
        0x8009_2013 | 0x800b_010e => "signature revocation status is offline or unknown".into(),
        _ => format!("WinVerifyTrust rejected package signature: {status}"),
    }
}
pub(crate) fn signer(state: HANDLE) -> Result<String, String> {
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return Err("WinVerifyTrust returned no provider state".into());
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, false, 0) };
    if signer.is_null() {
        return Err("WinVerifyTrust returned no primary signer".into());
    }
    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if certificate.is_null() || unsafe { (*certificate).pCert.is_null() } {
        return Err("WinVerifyTrust returned no primary signer certificate".into());
    }
    spki_sha256(unsafe { (*certificate).pCert })
}
