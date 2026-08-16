use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ffi::{CStr, c_void},
    fs,
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, HANDLE, HWND},
        Security::{
            Cryptography::{CERT_CONTEXT, CRYPT_ALGORITHM_IDENTIFIER, CRYPT_BIT_BLOB},
            WinTrust::{
                WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA,
                WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CHOICE_CATALOG, WTD_CHOICE_FILE,
                WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN,
                WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
                WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
                WTHelperProvDataFromStateData, WinVerifyTrust,
            },
        },
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_BEGIN,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
            FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FileAttributeTagInfo, FileIdInfo,
            GetFileInformationByHandle, GetFileInformationByHandleEx, GetFileSizeEx,
            GetFinalPathNameByHandleW, OPEN_EXISTING, ReadFile, SetFilePointerEx,
        },
    },
    core::PCWSTR,
};

use super::{
    AuthenticatedExecutable, AuthenticatedPackage, CLI_EXECUTABLE_NAME, GUI_EXECUTABLE_NAME,
    PACKAGE_CATALOG_NAME, PACKAGE_MANIFEST_NAME, PackageFile, PackageManifest, VerifiedConfig,
};
use crate::package_catalog::CatalogContext;

// Certificate-provided SPKI components are copied out of WinTrust-owned memory
// before encoding. These limits keep malformed certificates from turning the
// pin comparison path into an unbounded allocation or DER encoder.
const MAX_SPKI_OID_BYTES: usize = 256;
const MAX_SPKI_OID_COMPONENTS: usize = 32;
const MAX_SPKI_PARAMETERS_BYTES: usize = 16 * 1024;
const MAX_SPKI_PUBLIC_KEY_BYTES: usize = 64 * 1024;
const MAX_SPKI_DER_BYTES: usize = MAX_SPKI_OID_BYTES
    + MAX_SPKI_PARAMETERS_BYTES
    + MAX_SPKI_PUBLIC_KEY_BYTES
    + 64;

#[derive(Debug)]
pub(super) struct RetainedFile {
    pub(super) handle: HANDLE,
    path: PathBuf,
    id: FILE_ID_INFO,
}
impl Drop for RetainedFile {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

pub(super) fn authenticate(
    root: &Path,
    pins: &[String],
    current_image: &Path,
) -> Result<AuthenticatedPackage, String> {
    if !is_fixed_local_drive(root)? {
        return Err("package root must be on a local fixed drive".into());
    }
    let root_handle = open(root, true)?;
    let root_path = root_handle.path.clone();
    verify_tree_inventory(&root_path)?;
    let current = open(current_image, false)?;
    if !matches!(
        current.path.file_name().and_then(|name| name.to_str()),
        Some(name) if name.eq_ignore_ascii_case(GUI_EXECUTABLE_NAME) || name.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME)
    ) {
        return Err("current image is not a package GUI or CLI role".into());
    }
    let manifest_file = open(&root_path.join(PACKAGE_MANIFEST_NAME), false)?;
    let manifest = PackageManifest::parse(&read_bounded(&manifest_file, 1024 * 1024)?)?;
    let catalog = open(&root_path.join(PACKAGE_CATALOG_NAME), false)?;
    let mut retained = vec![root_handle, manifest_file, catalog];
    let mut ids = HashSet::new();
    for file in &retained {
        if !ids.insert((file.id.VolumeSerialNumber, file.id.FileId.Identifier)) {
            return Err("package contains duplicate file identities".into());
        }
    }
    let mut payload = Vec::new();
    let mut config_bytes = None;
    let mut directories = BTreeSet::new();
    for entry in manifest.files() {
        for ancestor in ancestors(entry.path()) {
            directories.insert(ancestor);
        }
        let file = open(&root_path.join(entry.path()), false)?;
        if !ids.insert((file.id.VolumeSerialNumber, file.id.FileId.Identifier)) {
            return Err("package contains duplicate file identities".into());
        }
        if file_size(&file)? != entry.size() || hash(&file)? != entry.sha256() {
            return Err(format!(
                "package payload hash or size differs: {}",
                entry.path()
            ));
        }
        config_bytes = config_bytes.or(read_config_snapshot(entry, &file)?);
        payload.push((entry.path(), file));
    }
    for directory in directories {
        retained.push(open(&root_path.join(directory), true)?);
    }
    let manifest_member = &retained[1];
    let catalog_member = &retained[2];
    let gui_index = payload
        .iter()
        .position(|(path, _)| path.eq_ignore_ascii_case(GUI_EXECUTABLE_NAME))
        .ok_or("package manifest omits GUI executable")?;
    let cli_index = payload
        .iter()
        .position(|(path, _)| path.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME))
        .ok_or("package manifest omits CLI executable")?;
    if !same_retained_file(&current, &payload[gui_index].1)
        && !same_retained_file(&current, &payload[cli_index].1)
    {
        return Err("current image does not match the retained package GUI or CLI".into());
    }
    let gui_signer = verify_file(&payload[gui_index].1)?;
    let cli_signer = verify_file(&payload[cli_index].1)?;
    let catalog_signer = verify_catalog_member(catalog_member, manifest_member)?;
    for (_, file) in &payload {
        if verify_catalog_member(catalog_member, file)? != catalog_signer {
            return Err("package catalog signer differs across members".into());
        }
    }
    if gui_signer != cli_signer
        || gui_signer != catalog_signer
        || !pins.iter().any(|pin| pin == &gui_signer)
    {
        return Err("GUI, CLI, and catalog signer do not match a pinned publisher SPKI".into());
    }
    let gui = payload.swap_remove(gui_index).1;
    let cli_index = payload
        .iter()
        .position(|(path, _)| path.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME))
        .expect("CLI index remains after GUI extraction");
    let cli = payload.swap_remove(cli_index).1;
    let payload = payload
        .into_iter()
        .map(|(path, file)| (path.to_ascii_lowercase(), file))
        .collect::<BTreeMap<_, _>>();
    Ok(AuthenticatedPackage {
        root: root_path,
        manifest,
        config: bind_config_snapshot(config_bytes)?,
        gui: AuthenticatedExecutable {
            path: gui.path.clone(),
            _retained: gui,
        },
        cli: AuthenticatedExecutable {
            path: cli.path.clone(),
            _retained: cli,
        },
        payload,
        _retained: retained,
    })
}

fn read_config_snapshot(
    entry: &PackageFile,
    file: &RetainedFile,
) -> Result<Option<(Vec<u8>, u64, String)>, String> {
    if !entry.path().eq_ignore_ascii_case("frametime.toml") {
        return Ok(None);
    }
    Ok(Some((
        read_bounded(file, 1024 * 1024)?,
        entry.size(),
        entry.sha256().to_owned(),
    )))
}

fn bind_config_snapshot(
    snapshot: Option<(Vec<u8>, u64, String)>,
) -> Result<VerifiedConfig, String> {
    let (bytes, size, sha256) = snapshot.ok_or("package manifest omits frametime.toml")?;
    // The byte binder runs only after the exact tree, retained-handle hashes,
    // catalog membership, signer equality, and publisher pin all succeeded.
    VerifiedConfig::from_verified_bytes(bytes, size, &sha256)
}

fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    let mut at = Vec::new();
    let mut cursor = path;
    while let Some((parent, _)) = cursor.rsplit_once('/') {
        at.push(parent);
        cursor = parent;
    }
    at.into_iter()
}

fn open(path: &Path, directory: bool) -> Result<RetainedFile, String> {
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            Default::default()
        };
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map_err(|error| format!("open package member: {error}"))?;
    let retained = RetainedFile {
        handle,
        path: final_path(handle)?,
        id: file_id(handle)?,
    };
    validate_kind(&retained, directory)?;
    Ok(retained)
}

fn validate_kind(file: &RetainedFile, directory: bool) -> Result<(), String> {
    let mut attributes = FILE_ATTRIBUTE_TAG_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            file.handle,
            FileAttributeTagInfo,
            (&mut attributes as *mut FILE_ATTRIBUTE_TAG_INFO).cast::<c_void>(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    }
    .map_err(|error| format!("inspect package member: {error}"))?;
    if attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || (attributes.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0) != directory
    {
        return Err(
            "package members may not be reparse points and must have the expected type".into(),
        );
    }
    let mut basic = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(file.handle, &mut basic) }
        .map_err(|error| format!("inspect package member links: {error}"))?;
    if !directory && basic.nNumberOfLinks != 1 {
        return Err("package files may not be hardlinked".into());
    }
    Ok(())
}

fn same_retained_file(left: &RetainedFile, right: &RetainedFile) -> bool {
    left.id == right.id
        && left
            .path
            .to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.path.to_string_lossy().replace('/', "\\"))
}

fn verify_tree_inventory(root: &Path) -> Result<(), String> {
    fn walk(root: &Path, current: &Path, files: &mut BTreeSet<String>) -> Result<(), String> {
        for entry in
            fs::read_dir(current).map_err(|error| format!("enumerate package tree: {error}"))?
        {
            let entry = entry.map_err(|error| format!("enumerate package tree entry: {error}"))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("inspect package tree entry: {error}"))?;
            if metadata.file_type().is_symlink() {
                return Err("package tree may not contain reparse points".into());
            }
            if metadata.is_dir() {
                walk(root, &path, files)?;
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| "package inventory escapes root")?
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase();
                if !files.insert(relative) {
                    return Err("package tree has case-colliding paths".into());
                }
            } else {
                return Err("package tree contains a non-file member".into());
            }
        }
        Ok(())
    }
    let mut actual = BTreeSet::new();
    walk(root, root, &mut actual)?;
    let mut expected = super::package_trust_contract::expected_payload_paths();
    expected.insert(PACKAGE_MANIFEST_NAME.into());
    expected.insert(PACKAGE_CATALOG_NAME.into());
    if actual == expected {
        Ok(())
    } else {
        Err("package tree differs from fixed package inventory".into())
    }
}

fn final_path(handle: HANDLE) -> Result<PathBuf, String> {
    let mut units = vec![0_u16; 512];
    loop {
        let needed = unsafe { GetFinalPathNameByHandleW(handle, &mut units, Default::default()) };
        if needed == 0 {
            return Err("resolve package member final path failed".into());
        }
        let needed = needed as usize;
        if needed < units.len() {
            return String::from_utf16(&units[..needed])
                .map(PathBuf::from)
                .map_err(|_| "package member path is not valid UTF-16".into());
        }
        units.resize(needed + 1, 0);
    }
}
fn file_id(handle: HANDLE) -> Result<FILE_ID_INFO, String> {
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast::<c_void>(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    }
    .map_err(|error| format!("inspect package member identity: {error}"))?;
    Ok(id)
}
fn file_size(file: &RetainedFile) -> Result<u64, String> {
    let mut length = 0_i64;
    unsafe { GetFileSizeEx(file.handle, &mut length) }
        .map_err(|error| format!("inspect package member size: {error}"))?;
    u64::try_from(length).map_err(|_| "package member has negative size".into())
}
fn read_bounded(file: &RetainedFile, maximum: usize) -> Result<Vec<u8>, String> {
    let length = usize::try_from(file_size(file)?).map_err(|_| "package metadata is too large")?;
    if length > maximum {
        return Err("package metadata exceeds bounded size".into());
    }
    unsafe { SetFilePointerEx(file.handle, 0, None, FILE_BEGIN) }
        .map_err(|error| format!("seek package metadata: {error}"))?;
    let mut bytes = vec![0; length];
    let mut offset = 0;
    while offset < length {
        let mut read = 0;
        unsafe {
            ReadFile(
                file.handle,
                Some(&mut bytes[offset..]),
                Some(&mut read),
                None,
            )
        }
        .map_err(|error| format!("read package metadata: {error}"))?;
        if read == 0 {
            return Err("short package metadata read".into());
        }
        offset += read as usize;
    }
    Ok(bytes)
}
fn hash(file: &RetainedFile) -> Result<String, String> {
    unsafe { SetFilePointerEx(file.handle, 0, None, FILE_BEGIN) }
        .map_err(|error| format!("seek package payload: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let mut read = 0;
        unsafe { ReadFile(file.handle, Some(&mut buffer), Some(&mut read), None) }
            .map_err(|error| format!("read package payload: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read as usize]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_file(file: &RetainedFile) -> Result<String, String> {
    let wide = wide(&file.path);
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: file.handle,
        pgKnownSubject: std::ptr::null_mut(),
    };
    verify(WTD_CHOICE_FILE, WINTRUST_DATA_0 { pFile: &mut info })
}
fn verify_catalog_member(catalog: &RetainedFile, member: &RetainedFile) -> Result<String, String> {
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
        cbStruct: size_of::<WINTRUST_CATALOG_INFO>() as u32,
        pcwszCatalogFilePath: PCWSTR(catalog_path.as_ptr()),
        pcwszMemberTag: PCWSTR(tag.as_ptr()),
        pcwszMemberFilePath: PCWSTR(member_path.as_ptr()),
        hMemberFile: member.handle,
        pbCalculatedFileHash: hash.as_ptr().cast_mut(),
        cbCalculatedFileHash: hash.len() as u32,
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
fn verify(
    choice: windows::Win32::Security::WinTrust::WINTRUST_DATA_UNION_CHOICE,
    subject: WINTRUST_DATA_0,
) -> Result<String, String> {
    let mut trust = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
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
fn trust_error(status: i32) -> String {
    match status as u32 {
        0x8009_2013 | 0x800b_010e => "signature revocation status is offline or unknown".into(),
        _ => format!("WinVerifyTrust rejected package signature: {status}"),
    }
}
fn signer(state: HANDLE) -> Result<String, String> {
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
fn spki_sha256(context: *const CERT_CONTEXT) -> Result<String, String> {
    let info = unsafe { (*context).pCertInfo.as_ref() }
        .ok_or("signer certificate lacks certificate information")?;
    let encoded = encode_spki(
        &info.SubjectPublicKeyInfo.Algorithm,
        &info.SubjectPublicKeyInfo.PublicKey,
    )?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}
fn encode_spki(
    algorithm: &CRYPT_ALGORITHM_IDENTIFIER,
    key: &CRYPT_BIT_BLOB,
) -> Result<Vec<u8>, String> {
    if algorithm.pszObjId.0.is_null() {
        return Err("signer SPKI OID is null".into());
    }
    let oid = unsafe { CStr::from_ptr(algorithm.pszObjId.0.cast()) }
        .to_str()
        .map_err(|_| "signer SPKI OID is invalid")?;
    if oid.len() > MAX_SPKI_OID_BYTES {
        return Err("signer SPKI OID exceeds bounded size".into());
    }
    let mut algorithm_der = der_oid(oid)?;
    let params = checked_bytes(
        algorithm.Parameters.pbData,
        algorithm.Parameters.cbData,
        MAX_SPKI_PARAMETERS_BYTES,
        "signer SPKI parameters",
    )?;
    checked_extend(&mut algorithm_der, &params, "signer SPKI algorithm")?;
    let algorithm_der = der(0x30, &algorithm_der)?;
    let key_bytes = checked_bytes(
        key.pbData,
        key.cbData,
        MAX_SPKI_PUBLIC_KEY_BYTES,
        "signer SPKI key",
    )?;
    if key.cUnusedBits > 7 {
        return Err("signer SPKI key has invalid unused bits".into());
    }
    let mut bits = vec![key.cUnusedBits as u8];
    checked_extend(&mut bits, &key_bytes, "signer SPKI key")?;
    let mut content = algorithm_der;
    checked_extend(&mut content, &der(0x03, &bits)?, "signer SPKI")?;
    if content.len() > MAX_SPKI_DER_BYTES {
        return Err("signer SPKI exceeds bounded size".into());
    }
    der(0x30, &content)
}
fn checked_bytes(
    pointer: *const u8,
    length: u32,
    maximum: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let length = usize::try_from(length).map_err(|_| format!("{label} is too large"))?;
    if length > maximum {
        return Err(format!("{label} exceeds bounded size"));
    }
    if length == 0 {
        Ok(Vec::new())
    } else if pointer.is_null() {
        Err(format!("{label} is null"))
    } else {
        Ok(unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec())
    }
}
fn der_oid(oid: &str) -> Result<Vec<u8>, String> {
    let mut values = oid.split('.');
    let first = values
        .next()
        .ok_or("signer SPKI OID is invalid")?
        .parse::<u64>()
        .map_err(|_| "signer SPKI OID is invalid")?;
    let second = values
        .next()
        .ok_or("signer SPKI OID is invalid")?
        .parse::<u64>()
        .map_err(|_| "signer SPKI OID is invalid")?;
    if first > 2 || (first < 2 && second > 39) {
        return Err("signer SPKI OID is invalid".into());
    }
    let first_value = first
        .checked_mul(40)
        .and_then(|value| value.checked_add(second))
        .ok_or("signer SPKI OID is invalid")?;
    let mut body = base128(first_value);
    let mut components: usize = 2;
    for value in values {
        components = components
            .checked_add(1)
            .ok_or("signer SPKI OID is invalid")?;
        if components > MAX_SPKI_OID_COMPONENTS {
            return Err("signer SPKI OID exceeds bounded size".into());
        }
        let value = value
            .parse::<u64>()
            .map_err(|_| "signer SPKI OID is invalid")?;
        checked_extend(&mut body, &base128(value), "signer SPKI OID")?;
    }
    if body.len() > MAX_SPKI_OID_BYTES {
        return Err("signer SPKI OID exceeds bounded size".into());
    }
    der(0x06, &body)
}
fn base128(mut value: u64) -> Vec<u8> {
    let mut out = vec![(value & 127) as u8];
    value >>= 7;
    while value > 0 {
        out.push(((value & 127) as u8) | 128);
        value >>= 7;
    }
    out.reverse();
    out
}
fn checked_extend(target: &mut Vec<u8>, bytes: &[u8], label: &str) -> Result<(), String> {
    target
        .try_reserve(bytes.len())
        .map_err(|_| format!("{label} exceeds bounded size"))?;
    target.extend_from_slice(bytes);
    Ok(())
}
fn der(tag: u8, body: &[u8]) -> Result<Vec<u8>, String> {
    let length_bytes = if body.len() < 128 {
        1
    } else {
        let mut length = body.len();
        let mut bytes = 0_usize;
        while length > 0 {
            bytes = bytes
                .checked_add(1)
                .ok_or("signer SPKI DER length overflows")?;
            length >>= 8;
        }
        1_usize
            .checked_add(bytes)
            .ok_or("signer SPKI DER length overflows")?
    };
    let capacity = 1_usize
        .checked_add(length_bytes)
        .and_then(|value| value.checked_add(body.len()))
        .ok_or("signer SPKI DER length overflows")?;
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| "signer SPKI DER exceeds bounded size")?;
    out.push(tag);
    if body.len() < 128 {
        out.push(body.len() as u8);
    } else {
        let mut length = body.len();
        let mut bytes = Vec::new();
        while length > 0 {
            bytes.push((length & 255) as u8);
            length >>= 8;
        }
        bytes.reverse();
        let length_of_length =
            u8::try_from(bytes.len()).map_err(|_| "signer SPKI DER length overflows")?;
        out.push(128 | length_of_length);
        out.extend(bytes);
    }
    out.extend(body);
    Ok(out)
}
fn is_fixed_local_drive(root: &Path) -> Result<bool, String> {
    let path = wide(root);
    let mut volume = [0_u16; 32768];
    unsafe {
        windows::Win32::Storage::FileSystem::GetVolumePathNameW(PCWSTR(path.as_ptr()), &mut volume)
    }
    .map_err(|error| format!("resolve package volume root: {error}"))?;
    let kind =
        unsafe { windows::Win32::Storage::FileSystem::GetDriveTypeW(PCWSTR(volume.as_ptr())) };
    Ok(kind == windows::Win32::System::WindowsProgramming::DRIVE_FIXED)
}
fn wide(value: &Path) -> Vec<u16> {
    value.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn wide_text(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn algorithm(oid: &[u8]) -> CRYPT_ALGORITHM_IDENTIFIER {
        let mut algorithm = CRYPT_ALGORITHM_IDENTIFIER::default();
        algorithm.pszObjId.0 = oid.as_ptr().cast_mut();
        algorithm
    }

    #[test]
    fn spki_encoding_rejects_null_oid_and_parameter_pointer() {
        let mut null_oid = CRYPT_ALGORITHM_IDENTIFIER::default();
        assert_eq!(
            encode_spki(&null_oid, &CRYPT_BIT_BLOB::default()),
            Err("signer SPKI OID is null".into())
        );

        let oid = b"1.2.840.113549.1.1.1\0";
        null_oid = algorithm(oid);
        null_oid.Parameters.cbData = 1;
        assert_eq!(
            encode_spki(&null_oid, &CRYPT_BIT_BLOB::default()),
            Err("signer SPKI parameters is null".into())
        );
    }

    #[test]
    fn spki_encoding_rejects_malformed_and_oversized_components() {
        assert!(der_oid("2.18446744073709551615").is_err());
        assert!(der_oid("1.2.3.4.5.6.7.8.9.10.11.12.13.14.15.16.17.18.19.20.21.22.23.24.25.26.27.28.29.30.31.32").is_err());

        let oid = b"1.2.840.113549.1.1.1\0";
        let mut oversized_parameters_algorithm = algorithm(oid);
        let mut params = vec![0_u8; MAX_SPKI_PARAMETERS_BYTES + 1];
        oversized_parameters_algorithm.Parameters.pbData = params.as_mut_ptr();
        oversized_parameters_algorithm.Parameters.cbData = params.len() as u32;
        assert_eq!(
            encode_spki(&oversized_parameters_algorithm, &CRYPT_BIT_BLOB::default()),
            Err("signer SPKI parameters exceeds bounded size".into())
        );

        let mut key = vec![0_u8; MAX_SPKI_PUBLIC_KEY_BYTES + 1];
        let oversized_key = CRYPT_BIT_BLOB {
            cbData: key.len() as u32,
            pbData: key.as_mut_ptr(),
            ..Default::default()
        };
        assert_eq!(
            encode_spki(&algorithm(oid), &oversized_key),
            Err("signer SPKI key exceeds bounded size".into())
        );
    }

    #[test]
    fn spki_encoding_rejects_invalid_unused_bits() {
        let oid = b"1.2.840.113549.1.1.1\0";
        let key = CRYPT_BIT_BLOB {
            cUnusedBits: 8,
            ..Default::default()
        };
        assert_eq!(
            encode_spki(&algorithm(oid), &key),
            Err("signer SPKI key has invalid unused bits".into())
        );
    }

    #[test]
    fn spki_encoding_keeps_canonical_pinned_publisher_input() {
        let oid = b"1.2.840.113549.1.1.1\0";
        let params = [0x05, 0x00];
        let mut algorithm = algorithm(oid);
        algorithm.Parameters.pbData = params.as_ptr().cast_mut();
        algorithm.Parameters.cbData = params.len() as u32;
        let key_bytes = [0x01, 0x02, 0x03];
        let key = CRYPT_BIT_BLOB {
            cbData: key_bytes.len() as u32,
            pbData: key_bytes.as_ptr().cast_mut(),
            ..Default::default()
        };

        assert_eq!(
            encode_spki(&algorithm, &key),
            Ok(vec![
                0x30, 0x15, 0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d,
                0x01, 0x01, 0x01, 0x05, 0x00, 0x03, 0x04, 0x00, 0x01, 0x02, 0x03,
            ])
        );
    }
}
