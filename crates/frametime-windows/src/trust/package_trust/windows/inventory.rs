use super::*;

pub(crate) fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    let mut at = Vec::new();
    let mut cursor = path;
    while let Some((parent, _)) = cursor.rsplit_once('/') {
        at.push(parent);
        cursor = parent;
    }
    at.into_iter()
}

pub(crate) fn open(path: &Path, directory: bool) -> Result<RetainedFile, String> {
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

pub(crate) fn validate_kind(file: &RetainedFile, directory: bool) -> Result<(), String> {
    let mut attributes = FILE_ATTRIBUTE_TAG_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            file.handle,
            FileAttributeTagInfo,
            (&mut attributes as *mut FILE_ATTRIBUTE_TAG_INFO).cast::<c_void>(),
            u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>())
                .map_err(|_| "FILE_ATTRIBUTE_TAG_INFO size exceeds u32")?,
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

pub(crate) fn same_retained_file(left: &RetainedFile, right: &RetainedFile) -> bool {
    left.id == right.id
        && left
            .path
            .to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.path.to_string_lossy().replace('/', "\\"))
}

pub(crate) fn verify_tree_inventory(root: &Path) -> Result<(), String> {
    pub(crate) fn walk(
        root: &Path,
        current: &Path,
        files: &mut BTreeSet<String>,
    ) -> Result<(), String> {
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
    let mut expected = super::super::contract::expected_payload_paths();
    expected.insert(PACKAGE_MANIFEST_NAME.into());
    expected.insert(PACKAGE_CATALOG_NAME.into());
    if actual == expected {
        Ok(())
    } else {
        Err("package tree differs from fixed package inventory".into())
    }
}

pub(crate) fn final_path(handle: HANDLE) -> Result<PathBuf, String> {
    let mut units = vec![0_u16; 512];
    loop {
        let needed = unsafe { GetFinalPathNameByHandleW(handle, &mut units, Default::default()) };
        if needed == 0 {
            return Err("resolve package member final path failed".into());
        }
        let needed =
            usize::try_from(needed).map_err(|_| "package member path length exceeds usize")?;
        if needed < units.len() {
            return String::from_utf16(&units[..needed])
                .map(PathBuf::from)
                .map_err(|_| "package member path is not valid UTF-16".into());
        }
        units.resize(needed + 1, 0);
    }
}
pub(crate) fn file_id(handle: HANDLE) -> Result<FILE_ID_INFO, String> {
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast::<c_void>(),
            u32::try_from(size_of::<FILE_ID_INFO>())
                .map_err(|_| "FILE_ID_INFO size exceeds u32")?,
        )
    }
    .map_err(|error| format!("inspect package member identity: {error}"))?;
    Ok(id)
}
pub(crate) fn file_size(file: &RetainedFile) -> Result<u64, String> {
    let mut length = 0_i64;
    unsafe { GetFileSizeEx(file.handle, &mut length) }
        .map_err(|error| format!("inspect package member size: {error}"))?;
    u64::try_from(length).map_err(|_| "package member has negative size".into())
}
pub(crate) fn read_bounded(file: &RetainedFile, maximum: usize) -> Result<Vec<u8>, String> {
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
        offset = offset
            .checked_add(
                usize::try_from(read).map_err(|_| "package metadata read size exceeds usize")?,
            )
            .ok_or("package metadata read offset overflows")?;
    }
    Ok(bytes)
}
pub(crate) fn hash(file: &RetainedFile) -> Result<String, String> {
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
        let read = usize::try_from(read).map_err(|_| "package payload read size exceeds usize")?;
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn is_fixed_local_drive(root: &Path) -> Result<bool, String> {
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
pub(crate) fn wide(value: &Path) -> Vec<u16> {
    value.as_os_str().encode_wide().chain(Some(0)).collect()
}
pub(crate) fn wide_text(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
