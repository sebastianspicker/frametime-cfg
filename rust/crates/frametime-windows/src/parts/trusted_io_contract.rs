// Portable validation for the user-selected backup-export destination.

use std::path::Path;

use crate::WINDOWS_WORK_DIR;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportDestination {
    pub(crate) drive_root: String,
    pub(crate) directories: Vec<String>,
    pub(crate) file_name: String,
}

impl ExportDestination {
    #[cfg(windows)]
    pub(crate) fn absolute_path(&self) -> String {
        let mut path = self.drive_root.clone();
        for directory in &self.directories {
            path.push_str(directory);
            path.push('\\');
        }
        path.push_str(&self.file_name);
        path
    }
}

pub(crate) fn parse_export_destination(path: &Path) -> Result<ExportDestination, String> {
    let text = path.to_string_lossy().replace('/', "\\");
    if text.is_empty() || text.len() > 32_767 || text.contains('\0') {
        return Err("backup export destination is empty, oversized, or contains NUL".into());
    }
    if text.starts_with(r"\\") || text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
        return Err("backup export destination must not be a UNC or device path".into());
    }
    let drive = text.as_bytes().first().copied();
    let bytes = text.as_bytes();
    if bytes.len() < 4
        || !drive.is_some_and(|value| value.is_ascii_alphabetic())
        || bytes[1] != b':'
        || bytes[2] != b'\\'
    {
        return Err("backup export destination must be an absolute local drive path".into());
    }
    let mut components = text[3..].split('\\').map(str::to_owned).collect::<Vec<_>>();
    if components.is_empty()
        || components.iter().any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.contains(':')
                || part.contains(['<', '>', '"', '|', '?', '*'])
                || part.ends_with(' ')
                || part.ends_with('.')
                || reserved_dos_device_name(part)
        })
    {
        return Err("backup export destination contains an unsafe path component".into());
    }
    if normalized_is_within_work_dir(&text) {
        return Err("backup export destination must not be inside C:\\FRAMETIME_CFG".into());
    }
    let file_name = components
        .pop()
        .ok_or("backup export destination has no file name")?;
    Ok(ExportDestination {
        drive_root: format!(
            "{}:\\",
            char::from(drive.ok_or("backup export destination has no drive")?).to_ascii_uppercase()
        ),
        directories: components,
        file_name,
    })
}

fn reserved_dos_device_name(component: &str) -> bool {
    let base = component.split('.').next().unwrap_or_default();
    let upper = base.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn normalized_is_within_work_dir(path: &str) -> bool {
    let root = WINDOWS_WORK_DIR.to_ascii_lowercase();
    let path = path.to_ascii_lowercase();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

/// `windows::core::Error::code()` is an HRESULT, not a raw Win32 error.
pub(crate) fn is_missing_path_hresult(code: i32) -> bool {
    matches!(code, -2_147_024_894 | -2_147_024_893)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_absolute_local_file_path() {
        let destination = parse_export_destination(Path::new(r"D:\Exports\backup.json"))
            .expect("local export path");
        assert_eq!(destination.drive_root, "D:\\");
        assert_eq!(destination.directories, ["Exports"]);
        assert_eq!(destination.file_name, "backup.json");
    }

    #[test]
    fn rejects_devices_network_paths_ads_and_traversal() {
        for path in [
            r"\\server\share\backup.json",
            r"\\?\C:\temp\backup.json",
            r"\\.\PhysicalDrive0",
            r"C:\temp\backup.json:stream",
            r"C:\temp\..\backup.json",
            r"C:\temp\\backup.json",
            r"C:\temp\NUL.json",
            r"C:backup.json",
        ] {
            assert!(parse_export_destination(Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn rejects_the_trusted_work_directory_and_its_descendants() {
        for path in [
            r"C:\FRAMETIME_CFG\backup.json",
            r"c:/frametime_cfg/Logs/backup.json",
        ] {
            assert!(parse_export_destination(Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn recognizes_hresult_wrapped_missing_file_and_directory_errors() {
        assert!(is_missing_path_hresult(-2_147_024_894)); // ERROR_FILE_NOT_FOUND
        assert!(is_missing_path_hresult(-2_147_024_893)); // ERROR_PATH_NOT_FOUND
        assert!(!is_missing_path_hresult(2));
    }
}
