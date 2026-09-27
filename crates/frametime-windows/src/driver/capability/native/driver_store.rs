use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Read,
    os::windows::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

use frametime_domain::driver::{OemPublishedName, PublishedDriverPackage, Sha256Digest};
use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Devices::DeviceAndDriverInstallation::SetupGetInfDriverStoreLocationW,
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, MAX_PATH},
        Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
    },
    core::{HRESULT, PCWSTR},
};

use super::super::{AdapterFailure, adapter};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const MAX_IDENTITY_FILES: usize = 128;
const MAX_IDENTITY_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_IDENTITY_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) fn bind_driver_store_identities(
    mut packages: Vec<PublishedDriverPackage>,
) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
    for package in &mut packages {
        package.driver_store_package_sha256 = Some(
            published_inf_package_sha256(&package.published_name)?.ok_or_else(|| {
                adapter(
                    "inspect driver-store package",
                    "a target-bound OEM INF is absent from the Driver Store",
                )
            })?,
        );
    }
    Ok(packages)
}

pub(crate) fn published_inf_is_present(
    published_name: &OemPublishedName,
) -> Result<bool, AdapterFailure> {
    Ok(published_inf_path(published_name)?.is_some())
}

fn published_inf_package_sha256(
    published_name: &OemPublishedName,
) -> Result<Option<Sha256Digest>, AdapterFailure> {
    published_inf_path(published_name)?
        .map(|path| hash_inf_and_catalog_identity(&path))
        .transpose()
}

fn published_inf_path(
    published_name: &OemPublishedName,
) -> Result<Option<PathBuf>, AdapterFailure> {
    let name = published_name
        .as_str()
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut location = vec![0_u16; usize::try_from(MAX_PATH).expect("MAX_PATH fits usize")];
    let mut required = 0_u32;
    match unsafe {
        SetupGetInfDriverStoreLocationW(
            PCWSTR(name.as_ptr()),
            None,
            PCWSTR::null(),
            &mut location,
            Some(&mut required),
        )
    } {
        Ok(()) if required > 1 && required <= MAX_PATH => {
            let end = location
                .iter()
                .position(|unit| *unit == 0)
                .ok_or_else(|| invalid_store_path("SetupAPI returned an unterminated path"))?;
            if end == 0 {
                return Err(invalid_store_path("SetupAPI returned an empty path"));
            }
            Ok(Some(PathBuf::from(OsString::from_wide(&location[..end]))))
        }
        Ok(()) => Err(invalid_store_path(
            "SetupAPI returned an invalid driver-store location length",
        )),
        Err(error)
            if error.code() == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0)
                || error.code() == HRESULT::from_win32(ERROR_PATH_NOT_FOUND.0) =>
        {
            Ok(None)
        }
        Err(error) => Err(adapter("inspect driver-store package", error.to_string())),
    }
}

fn hash_inf_and_catalog_identity(inf_path: &Path) -> Result<Sha256Digest, AdapterFailure> {
    let package_dir = inf_path
        .parent()
        .ok_or_else(|| invalid_store_path("resolved INF has no package directory"))?;
    validate_package_directory(package_dir)?;
    let mut files = identity_files(package_dir, inf_path)?;
    files.sort_by_cached_key(|(name, _)| name.to_ascii_lowercase());
    if files
        .windows(2)
        .any(|pair| pair[0].0.eq_ignore_ascii_case(&pair[1].0))
    {
        return Err(invalid_store_path(
            "driver-store identity files have a case-colliding name",
        ));
    }
    let mut digest = Sha256::new();
    digest.update(b"frametime-driver-store-package-v1\0");
    let mut total = 0_u64;
    for (name, path) in files {
        hash_identity_file(&mut digest, &name, &path, &mut total)?;
    }
    Sha256Digest::parse(format!("{:x}", digest.finalize()))
        .map_err(|error| invalid_store_path(&error.to_string()))
}

fn validate_package_directory(package_dir: &Path) -> Result<(), AdapterFailure> {
    let repository = package_dir
        .parent()
        .ok_or_else(|| invalid_store_path("driver-store package has no repository parent"))?;
    let expected = crate::system::platform::system_directory_with(|failure| match failure {
        crate::system::platform::SystemDirectoryFailure::Read => {
            invalid_store_path("GetSystemDirectoryW failed")
        }
        crate::system::platform::SystemDirectoryFailure::TooLarge => {
            invalid_store_path("System32 path is too large")
        }
    })?
    .join("DriverStore")
    .join("FileRepository");
    let repository_text = repository
        .to_str()
        .ok_or_else(|| invalid_store_path("driver-store repository path is not Unicode"))?;
    let expected_text = expected
        .to_str()
        .ok_or_else(|| invalid_store_path("expected driver-store path is not Unicode"))?;
    if !crate::trust::io_windows::same_windows_path(repository_text, expected_text) {
        return Err(invalid_store_path(
            "resolved INF is outside the fixed Driver Store repository",
        ));
    }
    validate_ordinary_directory(repository)?;
    validate_ordinary_directory(package_dir)
}

fn validate_ordinary_directory(path: &Path) -> Result<(), AdapterFailure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| adapter("inspect driver-store package", error.to_string()))?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        Err(invalid_store_path(
            "driver-store identity path is not an ordinary directory",
        ))
    } else {
        Ok(())
    }
}

fn identity_files(
    package_dir: &Path,
    resolved_inf: &Path,
) -> Result<Vec<(String, PathBuf)>, AdapterFailure> {
    let mut files = Vec::new();
    let mut found_inf = false;
    let mut catalogs = 0_usize;
    for entry in fs::read_dir(package_dir)
        .map_err(|error| adapter("inspect driver-store package", error.to_string()))?
    {
        let entry =
            entry.map_err(|error| adapter("inspect driver-store package", error.to_string()))?;
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if !extension.eq_ignore_ascii_case("inf") && !extension.eq_ignore_ascii_case("cat") {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid_store_path("driver-store identity leaf is not Unicode"))?;
        if extension.eq_ignore_ascii_case("cat") {
            catalogs += 1;
        }
        if same_path(&path, resolved_inf)? {
            found_inf = true;
        }
        files.push((name, path));
        if files.len() > MAX_IDENTITY_FILES {
            return Err(invalid_store_path(
                "driver-store package has too many INF/catalog identity files",
            ));
        }
    }
    if !found_inf || catalogs == 0 {
        return Err(invalid_store_path(
            "driver-store package lacks its resolved INF or catalog identity",
        ));
    }
    Ok(files)
}

fn hash_identity_file(
    digest: &mut Sha256,
    name: &str,
    path: &Path,
    total: &mut u64,
) -> Result<(), AdapterFailure> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .map_err(|error| adapter("inspect driver-store package", error.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|error| adapter("inspect driver-store package", error.to_string()))?;
    let length = metadata.len();
    *total = total
        .checked_add(length)
        .ok_or_else(|| invalid_store_path("driver-store identity byte count overflows"))?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || length > MAX_IDENTITY_FILE_BYTES
        || *total > MAX_IDENTITY_TOTAL_BYTES
    {
        return Err(invalid_store_path(
            "driver-store identity file violates type or size bounds",
        ));
    }
    let canonical_name = name.to_ascii_lowercase();
    digest.update(
        u64::try_from(canonical_name.len())
            .map_err(|_| invalid_store_path("driver-store identity name is too long"))?
            .to_le_bytes(),
    );
    digest.update(canonical_name.as_bytes());
    digest.update(length.to_le_bytes());
    let mut read = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| adapter("inspect driver-store package", error.to_string()))?;
        if count == 0 {
            break;
        }
        read = read
            .checked_add(u64::try_from(count).expect("buffer count fits u64"))
            .ok_or_else(|| invalid_store_path("driver-store identity read count overflows"))?;
        digest.update(&buffer[..count]);
    }
    if read != length {
        return Err(invalid_store_path(
            "driver-store identity file changed length while hashing",
        ));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> Result<bool, AdapterFailure> {
    let left = left
        .to_str()
        .ok_or_else(|| invalid_store_path("driver-store identity path is not Unicode"))?;
    let right = right
        .to_str()
        .ok_or_else(|| invalid_store_path("driver-store identity path is not Unicode"))?;
    Ok(crate::trust::io_windows::same_windows_path(left, right))
}

fn invalid_store_path(reason: &str) -> AdapterFailure {
    adapter("inspect driver-store package", reason)
}
