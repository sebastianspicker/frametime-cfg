//! Windows package-authentication boundary.

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

use super::catalog::CatalogContext;
use super::{
    AuthenticatedExecutable, AuthenticatedPackage, CLI_EXECUTABLE_NAME, GUI_EXECUTABLE_NAME,
    PACKAGE_CATALOG_NAME, PACKAGE_MANIFEST_NAME, PackageFile, PackageManifest, VerifiedConfig,
};

// Certificate-provided SPKI components are copied out of WinTrust-owned memory
// before encoding. These limits keep malformed certificates from turning the
// pin comparison path into an unbounded allocation or DER encoder.
pub(crate) const MAX_SPKI_OID_BYTES: usize = 256;
pub(crate) const MAX_SPKI_OID_COMPONENTS: usize = 32;
pub(crate) const MAX_SPKI_PARAMETERS_BYTES: usize = 16 * 1024;
pub(crate) const MAX_SPKI_PUBLIC_KEY_BYTES: usize = 64 * 1024;
pub(crate) const MAX_SPKI_DER_BYTES: usize =
    MAX_SPKI_OID_BYTES + MAX_SPKI_PARAMETERS_BYTES + MAX_SPKI_PUBLIC_KEY_BYTES + 64;

mod authentication;
mod inventory;
mod retained_file;
mod spki;
#[cfg(test)]
mod tests;
mod wintrust;

pub(crate) use authentication::authenticate;
pub(crate) use retained_file::RetainedFile;

use inventory::*;
use spki::*;
use wintrust::*;
