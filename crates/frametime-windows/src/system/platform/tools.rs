use crate::*;
#[cfg(any(test, windows))]
pub(crate) fn resolve_system_tool_path(
    system_directory: &Path,
    program: &str,
) -> Result<PathBuf, String> {
    let command = CommandName::from_program(program)?;
    Ok(system_directory.join(command.program()))
}

#[cfg(windows)]
pub(crate) fn system_directory() -> Result<PathBuf, String> {
    system_directory_with(|failure| match failure {
        SystemDirectoryFailure::Read => format!(
            "read Windows System32 directory: {}",
            windows::core::Error::from_thread()
        ),
        SystemDirectoryFailure::TooLarge => "Windows System32 path is too large".into(),
    })
}

#[cfg(windows)]
pub(crate) enum SystemDirectoryFailure {
    Read,
    TooLarge,
}

#[cfg(windows)]
pub(crate) fn system_directory_with<E>(
    error: impl Fn(SystemDirectoryFailure) -> E,
) -> Result<PathBuf, E> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = vec![0_u16; 260];
    loop {
        let copied = unsafe { GetSystemDirectoryW(Some(&mut buffer)) };
        if copied == 0 {
            return Err(error(SystemDirectoryFailure::Read));
        }
        let copied =
            usize::try_from(copied).map_err(|_| error(SystemDirectoryFailure::TooLarge))?;
        if copied < buffer.len() {
            return Ok(PathBuf::from(OsString::from_wide(&buffer[..copied])));
        }
        let required = copied
            .checked_add(1)
            .ok_or_else(|| error(SystemDirectoryFailure::TooLarge))?;
        buffer.resize(required, 0);
    }
}

#[cfg(windows)]
pub(crate) fn execute_allowlisted(
    command: CommandName,
    arguments: &[String],
) -> Result<String, String> {
    use std::process::Command;
    let system_directory = system_directory()?;
    let program = resolve_system_tool_path(&system_directory, command.program())?;
    let output = Command::new(&program)
        .args(arguments)
        // Keep an allowlisted inbox tool's current-directory DLL search away
        // from the portable package or caller-controlled working directory.
        .current_dir(&system_directory)
        .output()
        .map_err(|error| format!("{} execution failed: {error}", command.program()))?;
    if !output.status.success() {
        return Err(format!(
            "{} exited with {}: {}",
            command.program(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
#[cfg(not(windows))]
pub(crate) fn execute_allowlisted(_: CommandName, _: &[String]) -> Result<String, String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(windows)]
pub(crate) fn require_elevation() -> Result<(), String> {
    use windows::Win32::UI::Shell::IsUserAnAdmin;
    if unsafe { IsUserAnAdmin() }.as_bool() {
        Ok(())
    } else {
        Err("live commands require an elevated administrator token".into())
    }
}
#[cfg(not(windows))]
pub(crate) fn require_elevation() -> Result<(), String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(windows)]
pub(crate) fn discover_hardware() -> Result<HardwareInfo, String> {
    let devices = crate::system::device_bindings::windows_setupapi::display_devices()
        .map_err(|error| format!("enumerate display adapters: {error}"))?;
    let vendors = devices
        .iter()
        .filter_map(|(_, vendor)| *vendor)
        .collect::<Vec<_>>();
    Ok(HardwareInfo {
        display_adapters: devices.into_iter().map(|(name, _)| name).collect(),
        gpu_branch: gpu_branch_from_vendors(&vendors),
    })
}

#[cfg(any(test, windows))]
fn gpu_branch_from_vendors(vendors: &[u16]) -> Option<GpuBranch> {
    [
        (0x10de, GpuBranch::Nvidia),
        (0x1002, GpuBranch::Amd),
        (0x8086, GpuBranch::IntelArc),
    ]
    .into_iter()
    .find_map(|(id, branch)| vendors.contains(&id).then_some(branch))
}
#[cfg(not(windows))]
pub(crate) fn discover_hardware() -> Result<HardwareInfo, String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(test)]
mod gpu_branch_tests {
    use super::*;

    #[test]
    fn vendor_precedence_is_nvidia_amd_intel() {
        assert_eq!(
            gpu_branch_from_vendors(&[0x8086, 0x10de]),
            Some(GpuBranch::Nvidia)
        );
        assert_eq!(
            gpu_branch_from_vendors(&[0x8086, 0x1002]),
            Some(GpuBranch::Amd)
        );
        assert_eq!(
            gpu_branch_from_vendors(&[0x8086]),
            Some(GpuBranch::IntelArc)
        );
        assert_eq!(gpu_branch_from_vendors(&[0x1414]), None);
        assert_eq!(gpu_branch_from_vendors(&[]), None);
    }
}
