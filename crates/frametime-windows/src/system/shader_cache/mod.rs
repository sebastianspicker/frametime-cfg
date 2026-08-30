use crate::*;

pub(crate) mod handles;
pub(crate) mod validation;

const PROGRAM_FILES_X86: &str = "%ProgramFiles(x86)%";
const PROGRAM_FILES: &str = "%ProgramFiles%";
const LOCAL_APP_DATA: &str = "%LOCALAPPDATA%";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownFolders {
    program_files_x86: PathBuf,
    program_files: PathBuf,
    local_app_data: PathBuf,
}

impl KnownFolders {
    pub(crate) fn local_app_data(&self) -> &Path {
        &self.local_app_data
    }
}

pub(crate) fn resolve_cache_template(
    value: &str,
    folders: &KnownFolders,
) -> Result<PathBuf, String> {
    let (root, suffix) = if let Some(suffix) = value.strip_prefix(PROGRAM_FILES_X86) {
        (&folders.program_files_x86, suffix)
    } else if let Some(suffix) = value.strip_prefix(PROGRAM_FILES) {
        (&folders.program_files, suffix)
    } else if let Some(suffix) = value.strip_prefix(LOCAL_APP_DATA) {
        (&folders.local_app_data, suffix)
    } else if exact_local_drive_template(value) {
        return checked_windows_path(value).map(PathBuf::from);
    } else {
        return Err(
            "cleanup cache template is not a known-folder or exact local-drive path".into(),
        );
    };
    let suffix = suffix
        .strip_prefix('\\')
        .ok_or("cleanup cache template lacks a bounded relative suffix")?;
    validate_relative_components(suffix)?;
    let root_text = root.to_string_lossy();
    let root = checked_windows_path(&root_text)?;
    Ok(PathBuf::from(format!(r"{root}\{suffix}")))
}

fn exact_local_drive_template(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && !value.starts_with("\\\\")
}

fn checked_windows_path(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.starts_with("\\\\")
        || value.starts_with(r"\\?\\")
        || value.starts_with(r"\\.\\")
        || value.contains('\0')
        || value.contains('/')
        || value.ends_with([' ', '.'])
        || !exact_local_drive_template(value)
    {
        return Err("cleanup cache path is not an exact local drive path".into());
    }
    validate_relative_components(&value[3..])?;
    Ok(value)
}

fn validate_relative_components(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("cleanup cache path has no component".into());
    }
    for component in value.split('\\') {
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with([' ', '.'])
            || component.contains(':')
        {
            return Err(
                "cleanup cache path has traversal, alternate-stream, or alias components".into(),
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_shader_cache_entry_name(name: &[u16]) -> Result<(), String> {
    if name.is_empty()
        || name == [b'.' as u16]
        || name == [b'.' as u16, b'.' as u16]
        || name.iter().any(|unit| {
            *unit == 0
                || *unit == u16::from(b'\\')
                || *unit == u16::from(b'/')
                || *unit == u16::from(b':')
        })
        || name
            .last()
            .is_some_and(|unit| *unit == u16::from(b' ') || *unit == u16::from(b'.'))
    {
        return Err("cleanup directory entry has a hostile or alias name".into());
    }
    Ok(())
}

pub(crate) fn normalize_windows_dos_path(path: &[u16]) -> Result<Vec<u16>, String> {
    const UPPER_A: u16 = b'A' as u16;
    const UPPER_Z: u16 = b'Z' as u16;
    const LOWER_A: u16 = b'a' as u16;
    const LOWER_Z: u16 = b'z' as u16;
    let path = if path.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16]) {
        &path[4..]
    } else if path.starts_with(&[b'\\' as u16, b'\\' as u16]) {
        return Err("cleanup final path has an unapproved device or UNC prefix".into());
    } else {
        path
    };
    if path.len() < 4
        || !matches!(path[0], UPPER_A..=UPPER_Z | LOWER_A..=LOWER_Z)
        || path[1] != u16::from(b':')
        || path[2] != u16::from(b'\\')
        || path.contains(&0)
    {
        return Err("cleanup final path is not an exact DOS drive path".into());
    }
    Ok(path
        .iter()
        .map(|unit| match *unit {
            upper @ UPPER_A..=UPPER_Z => upper + LOWER_A - UPPER_A,
            unit => unit,
        })
        .collect())
}

pub(crate) fn known_folders() -> Result<KnownFolders, String> {
    use windows::{
        Win32::UI::Shell::{
            FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
            SHGetKnownFolderPath,
        },
        core::PWSTR,
    };
    struct KnownFolderAllocation(PWSTR);
    impl Drop for KnownFolderAllocation {
        fn drop(&mut self) {
            unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(self.0.0.cast())) };
        }
    }
    fn get(id: &windows::core::GUID) -> Result<PathBuf, String> {
        let raw = KnownFolderAllocation(
            unsafe { SHGetKnownFolderPath(id, Default::default(), None) }
                .map_err(|error| format!("resolve known cleanup folder: {error}"))?,
        );
        let value = unsafe { raw.0.to_string() }
            .map_err(|error| format!("decode known cleanup folder: {error}"))?;
        checked_windows_path(&value).map(PathBuf::from)
    }
    Ok(KnownFolders {
        program_files_x86: get(&FOLDERID_ProgramFilesX86)?,
        program_files: get(&FOLDERID_ProgramFiles)?,
        local_app_data: get(&FOLDERID_LocalAppData)?,
    })
}
