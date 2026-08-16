//! Administrator elevation detection and UAC relaunch.

#[cfg(windows)]
use std::ffi::c_void;

/// True when the current process token is in the Administrators role.
pub fn is_administrator() -> bool {
    #[cfg(windows)]
    {
        windows_is_admin()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Attempt to re-launch the current executable elevated with the given args.
/// Returns `true` if a new process was started (caller should exit).
/// Returns `false` if elevation was cancelled or failed.
pub fn try_relaunch_elevated(args: &[String]) -> bool {
    #[cfg(windows)]
    {
        windows_relaunch_elevated(args)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        false
    }
}

/// Quote a single argument for Windows command lines.
pub fn quote_arg(a: &str) -> String {
    if a.is_empty() {
        return "\"\"".into();
    }
    if a.contains(' ') || a.contains('"') || a.contains('\t') {
        format!("\"{}\"", a.replace('"', "\\\""))
    } else {
        a.to_string()
    }
}

#[cfg(windows)]
fn windows_is_admin() -> bool {
    // CheckTokenMembership evaluates the current access token directly. In particular, it
    // respects UAC's deny-only Administrators SID without invoking a PATH-resolved tool.
    const TOKEN_QUERY: u32 = 0x0008;
    const SECURITY_NT_AUTHORITY: [u8; 6] = [0, 0, 0, 0, 0, 5];
    const SECURITY_BUILTIN_DOMAIN_RID: u32 = 32;
    const DOMAIN_ALIAS_RID_ADMINS: u32 = 544;

    let mut token = std::ptr::null_mut();
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            std::ptr::addr_of_mut!(token),
        )
    };
    if opened == 0 {
        return false;
    }

    let authority = SidIdentifierAuthority {
        value: SECURITY_NT_AUTHORITY,
    };
    let mut administrators_sid = std::ptr::null_mut();
    let created = unsafe {
        AllocateAndInitializeSid(
            std::ptr::addr_of!(authority),
            2,
            SECURITY_BUILTIN_DOMAIN_RID,
            DOMAIN_ALIAS_RID_ADMINS,
            0,
            0,
            0,
            0,
            0,
            0,
            std::ptr::addr_of_mut!(administrators_sid),
        )
    };
    if created == 0 {
        unsafe { CloseHandle(token) };
        return false;
    }

    let mut is_member = 0;
    let checked = unsafe { CheckTokenMembership(token, administrators_sid, &mut is_member) };
    unsafe {
        FreeSid(administrators_sid);
        CloseHandle(token);
    }
    checked != 0 && is_member != 0
}

#[cfg(windows)]
#[repr(C)]
struct SidIdentifierAuthority {
    value: [u8; 6],
}

#[cfg(windows)]
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(
        process_handle: *mut c_void,
        desired_access: u32,
        token_handle: *mut *mut c_void,
    ) -> i32;
    fn AllocateAndInitializeSid(
        identifier_authority: *const SidIdentifierAuthority,
        sub_authority_count: u8,
        sub_authority0: u32,
        sub_authority1: u32,
        sub_authority2: u32,
        sub_authority3: u32,
        sub_authority4: u32,
        sub_authority5: u32,
        sub_authority6: u32,
        sub_authority7: u32,
        sid: *mut *mut c_void,
    ) -> i32;
    fn CheckTokenMembership(
        token_handle: *mut c_void,
        sid_to_check: *mut c_void,
        member: *mut i32,
    ) -> i32;
    fn FreeSid(sid: *mut c_void) -> *mut c_void;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[cfg(windows)]
fn windows_relaunch_elevated(args: &[String]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe_str = exe.to_string_lossy();
    let arg_line: String = args
        .iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ");

    // PowerShell Start-Process -Verb RunAs is reliable for UAC on modern Windows.
    let ps = format!(
        "Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -Wait:$false",
        exe_str.replace('\'', "''"),
        arg_line.replace('\'', "''")
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .status();
    matches!(status, Ok(s) if s.success())
}

/// Result of an execute-mode elevation gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevationGate {
    /// Process is elevated; proceed with mutation.
    Elevated,
    /// Relaunch was started; current process should exit 0.
    Relaunched,
    /// Not elevated and relaunch failed/cancelled; caller exits non-zero.
    Denied { message: String },
}

/// True when UAC relaunch is disabled (CI/non-interactive). Honors `DFOUNDRY_NO_UAC_RELAUNCH=1`.
pub fn uac_relaunch_disabled() -> bool {
    matches!(
        std::env::var("DFOUNDRY_NO_UAC_RELAUNCH").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

/// Gate for `--execute` / live mutation: ensure admin or relaunch.
pub fn gate_execute(args_for_relaunch: &[String], attempt_relaunch: bool) -> ElevationGate {
    if is_administrator() {
        return ElevationGate::Elevated;
    }
    let attempt = attempt_relaunch && !uac_relaunch_disabled();
    if attempt && try_relaunch_elevated(args_for_relaunch) {
        return ElevationGate::Relaunched;
    }
    ElevationGate::Denied {
        message: if uac_relaunch_disabled() {
            "execute requires administrator elevation (UAC relaunch disabled via DFOUNDRY_NO_UAC_RELAUNCH)"
                .into()
        } else {
            "execute requires administrator elevation (UAC relaunch failed or cancelled)".into()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_arg_spaces() {
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg("plain"), "plain");
    }

    #[test]
    fn is_admin_does_not_panic() {
        let _ = is_administrator();
    }

    #[test]
    fn gate_denied_or_elevated_without_relaunch() {
        let g = gate_execute(&[], false);
        match g {
            ElevationGate::Elevated => {}
            ElevationGate::Denied { message } => {
                assert!(message.contains("administrator") || message.contains("elevation"));
            }
            ElevationGate::Relaunched => panic!("should not relaunch when attempt_relaunch=false"),
        }
    }

    #[test]
    fn uac_relaunch_env_disables_relaunch_path() {
        // When env is set, even attempt_relaunch=true must not return Relaunched
        // (unless already elevated).
        std::env::set_var("DFOUNDRY_NO_UAC_RELAUNCH", "1");
        assert!(uac_relaunch_disabled());
        let g = gate_execute(&["clean".into(), "--execute".into()], true);
        std::env::remove_var("DFOUNDRY_NO_UAC_RELAUNCH");
        match g {
            ElevationGate::Elevated => {}
            ElevationGate::Denied { message } => {
                assert!(
                    message.to_ascii_lowercase().contains("admin")
                        || message.to_ascii_lowercase().contains("elevation")
                        || message.contains("DFOUNDRY_NO_UAC_RELAUNCH"),
                    "{message}"
                );
            }
            ElevationGate::Relaunched => {
                panic!("must not relaunch when DFOUNDRY_NO_UAC_RELAUNCH=1")
            }
        }
    }
}
