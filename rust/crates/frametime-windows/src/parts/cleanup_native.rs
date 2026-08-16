//! Standalone cleanup execution.  Selection is entirely core-owned; this
//! module resolves only fixed Windows resources and returns one typed outcome
//! per selected action so an earlier failure cannot hide later work.

use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

use frametime_core::{
    CleanupAction, CleanupActionOutcome, CleanupActionResult, CleanupMode, CleanupReport,
    cleanup_actions, require_phase_one_handoff_ready,
};

#[cfg(windows)]
use crate::cleanup_shader::{CleanupShaderCacheKind, clear_cleanup_shader_cache};
use crate::{
    AuthenticatedPackage, Progress, VerifiedConfig, launch_published_safe_mode_handoff,
    load_driver_transaction, load_progress, publish_current_packaged_runtime, require_elevation,
};
#[cfg(windows)]
use crate::{CommandName, CommandVector, GpuBranch, copy_text_to_clipboard, discover_hardware};

#[cfg(any(test, windows))]
const STEAM_VALIDATE_730_URI: &str = "steam://validate/730";

pub(crate) fn run(mode: CleanupMode, _work_dir: &Path, config: &VerifiedConfig) -> CleanupReport {
    if matches!(mode, CleanupMode::Driver) {
        return CleanupReport {
            action_results: vec![CleanupActionResult {
                action: CleanupAction::ArmDriverRefreshRuntimeHandoff,
                outcome: CleanupActionOutcome::Failed {
                    reason: "driver cleanup requires an authenticated package capability".into(),
                },
            }],
            restart_required: false,
        };
    }
    let action_results = cleanup_actions(mode)
        .into_iter()
        .map(|spec| CleanupActionResult {
            action: spec.action,
            outcome: action_outcome(spec.action, config),
        })
        .collect::<Vec<_>>();
    let restart_required = action_results.iter().any(|result| {
        result.action == CleanupAction::ResetWinsockCatalog
            && matches!(result.outcome, CleanupActionOutcome::Completed { .. })
    });
    CleanupReport {
        action_results,
        restart_required,
    }
}

pub(crate) fn run_driver(work_dir: &Path, package: &AuthenticatedPackage) -> CleanupReport {
    driver_report(work_dir, package)
}

fn driver_report(work_dir: &Path, package: &AuthenticatedPackage) -> CleanupReport {
    let outcome = (|| {
        require_elevation()?;
        let progress = load_progress(work_dir)?;
        require_phase_one_handoff_ready(&progress)
            .map_err(|error| format!("driver cleanup requires resolved P1:1-37: {error:?}"))?;
        let transaction = load_driver_transaction(work_dir)?
            .ok_or("driver cleanup requires a coherent prepared NVIDIA transaction")?;
        if transaction.plan.target_gpu.vendor != frametime_driver::GpuVendor::Nvidia {
            return Err("driver cleanup requires a prepared NVIDIA transaction".into());
        }
        if transaction.capture.is_some()
            || transaction.removal.is_some()
            || transaction.installation.is_some()
        {
            return Err(
                "driver cleanup requires a prepared transaction before any P2 or P3 work".into(),
            );
        }
        let runtime = publish_current_packaged_runtime(package)?;
        launch_published_safe_mode_handoff(&runtime)?;
        let verified = load_progress(work_dir)?;
        if !verified.completed_steps.contains(&Progress::key(1, 38)) {
            return Err("published runtime returned without verified P1:38 progress".into());
        }
        Ok(())
    })();
    let restart_required = outcome.is_ok();
    CleanupReport {
        action_results: vec![CleanupActionResult {
            action: CleanupAction::ArmDriverRefreshRuntimeHandoff,
            outcome: outcome.map_or_else(
                |reason| CleanupActionOutcome::Failed { reason },
                |_| CleanupActionOutcome::Completed { affected_items: 0 },
            ),
        }],
        restart_required,
    }
}

#[cfg(windows)]
fn action_outcome(action: CleanupAction, config: &VerifiedConfig) -> CleanupActionOutcome {
    match action_effect(action, config) {
        Ok(affected_items) => CleanupActionOutcome::Completed { affected_items },
        Err(CleanupDisposition::Inapplicable(reason)) => {
            CleanupActionOutcome::Inapplicable { reason }
        }
        Err(CleanupDisposition::Deferred(reason)) => CleanupActionOutcome::Deferred { reason },
        Err(CleanupDisposition::Failed(reason)) => CleanupActionOutcome::Failed { reason },
    }
}

#[cfg(not(windows))]
fn action_outcome(_action: CleanupAction, _config: &VerifiedConfig) -> CleanupActionOutcome {
    CleanupActionOutcome::Deferred {
        reason: "native cleanup requires supported Windows x64".into(),
    }
}

#[cfg(windows)]
enum CleanupDisposition {
    Inapplicable(String),
    Deferred(String),
    Failed(String),
}

#[cfg(windows)]
fn action_effect(
    action: CleanupAction,
    config: &VerifiedConfig,
) -> Result<usize, CleanupDisposition> {
    use CleanupAction::{
        ClearAmdDxShaderCache, ClearApplicationEventLog, ClearCs2App730ShaderCache,
        ClearCurrentUserTemp, ClearDirectXShaderCache, ClearNvidiaDxShaderCache,
        ClearNvidiaGlShaderCache, ClearSetupEventLog, ClearSystemEventLog, ClearWindowsTemp,
        DeleteWindowsPrefetchPfFiles, FlushDnsResolverCache, RequestSteamApp730IntegrityValidation,
        ResetWinsockCatalog, TrimSystemFileCacheWorkingSet,
    };
    match action {
        ClearCs2App730ShaderCache => shader(config, CleanupShaderCacheKind::Cs2),
        ClearNvidiaDxShaderCache => shader(config, CleanupShaderCacheKind::NvidiaDx),
        ClearNvidiaGlShaderCache => shader(config, CleanupShaderCacheKind::NvidiaGl),
        ClearDirectXShaderCache => shader(config, CleanupShaderCacheKind::DirectX),
        ClearAmdDxShaderCache => {
            if discover_hardware()
                .map_err(CleanupDisposition::Failed)?
                .gpu_branch
                != Some(GpuBranch::Amd)
            {
                Err(CleanupDisposition::Inapplicable(
                    "no exact AMD display adapter is active".into(),
                ))
            } else {
                shader(config, CleanupShaderCacheKind::AmdDx)
            }
        }
        ClearWindowsTemp => cleanup_paths::windows_temp().map_err(CleanupDisposition::Failed),
        ClearCurrentUserTemp => {
            cleanup_paths::current_user_temp().map_err(CleanupDisposition::Failed)
        }
        DeleteWindowsPrefetchPfFiles => {
            cleanup_paths::prefetch_pf().map_err(CleanupDisposition::Failed)
        }
        FlushDnsResolverCache => dns::flush().map_err(CleanupDisposition::Deferred),
        TrimSystemFileCacheWorkingSet => system_cache::trim().map_err(CleanupDisposition::Failed),
        ClearApplicationEventLog => {
            event_logs::clear("Application").map_err(CleanupDisposition::Failed)
        }
        ClearSystemEventLog => event_logs::clear("System").map_err(CleanupDisposition::Failed),
        ClearSetupEventLog => event_logs::clear("Setup").map_err(CleanupDisposition::Failed),
        RequestSteamApp730IntegrityValidation => copy_text_to_clipboard(STEAM_VALIDATE_730_URI)
            .map(|()| 0)
            .map_err(CleanupDisposition::Failed),
        ResetWinsockCatalog => reset_winsock_catalog().map_err(CleanupDisposition::Failed),
        CleanupAction::ArmDriverRefreshRuntimeHandoff => Err(CleanupDisposition::Failed(
            "driver action may only run in driver mode".into(),
        )),
    }
}

#[cfg(windows)]
fn reset_winsock_catalog() -> Result<usize, String> {
    // Microsoft exposes catalog reset through the inbox netsh command rather
    // than a documented Win32 reset API. Keep the exception a closed typed
    // vector: absolute System32 resolution, no shell, and no caller arguments.
    CommandVector::new(CommandName::Netsh, &["winsock", "reset"])?
        .run()
        .map(|_| 0)
}

#[cfg(windows)]
fn shader(
    config: &VerifiedConfig,
    kind: CleanupShaderCacheKind,
) -> Result<usize, CleanupDisposition> {
    if !crate::shader_cache_delete_qualified() {
        return Err(CleanupDisposition::Deferred(
            "handle-backed shader-cache deletion is disabled pending Windows VM qualification"
                .into(),
        ));
    }
    clear_cleanup_shader_cache(config.value(), kind).map_err(CleanupDisposition::Failed)
}

#[cfg(windows)]
mod cleanup_paths {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;

    fn wide_result(mut value: Vec<u16>, used: u32, label: &str) -> Result<PathBuf, String> {
        let used = usize::try_from(used).map_err(|_| format!("{label} length overflows"))?;
        if used == 0 || used >= value.len() {
            return Err(format!("{label} returned an incomplete path"));
        }
        value.truncate(used);
        let path = PathBuf::from(std::ffi::OsString::from_wide(&value));
        let text = path.to_string_lossy();
        if text.len() < 4 || text.starts_with(r"\\") || text.as_bytes().get(1) != Some(&b':') {
            return Err(format!("{label} did not resolve to a fixed local drive"));
        }
        Ok(path)
    }

    fn windows_root() -> Result<PathBuf, String> {
        let mut value = vec![0_u16; 32_768];
        let used = unsafe { GetWindowsDirectoryW(Some(&mut value)) };
        wide_result(value, used, "Windows directory")
    }

    fn user_temp_root() -> Result<PathBuf, String> {
        // GetTempPathW honors attacker-controlled environment variables and
        // therefore cannot authorize recursive deletion in an elevated
        // process. Bind the target to the OS-known LocalAppData identity.
        Ok(crate::known_folders()?.local_app_data().join("Temp"))
    }

    pub(super) fn windows_temp() -> Result<usize, String> {
        crate::shader_cache_handles::delete_fixed_roots(&[windows_root()?.join("Temp")])
    }

    pub(super) fn current_user_temp() -> Result<usize, String> {
        crate::shader_cache_handles::delete_fixed_roots(&[user_temp_root()?])
    }

    pub(super) fn prefetch_pf() -> Result<usize, String> {
        crate::shader_cache_handles::delete_prefetch_pf_files(&windows_root()?.join("Prefetch"))
    }
}

#[cfg(windows)]
mod system_cache {
    use std::mem::size_of;
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_NOT_ALL_ASSIGNED, GetLastError, HANDLE},
        Security::{
            AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
            SE_INCREASE_QUOTA_NAME, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
            TOKEN_PRIVILEGES, TOKEN_QUERY,
        },
        System::{
            Memory::SetSystemFileCacheSize,
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };

    struct QuotaPrivilege {
        token: HANDLE,
        previous: TOKEN_PRIVILEGES,
    }
    impl QuotaPrivilege {
        fn enable() -> Result<Self, String> {
            let mut token = HANDLE::default();
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                    &mut token,
                )
            }
            .map_err(|error| format!("open token for SeIncreaseQuotaPrivilege: {error}"))?;
            let mut luid = Default::default();
            unsafe { LookupPrivilegeValueW(None, SE_INCREASE_QUOTA_NAME, &mut luid) }
                .map_err(|error| format!("resolve SeIncreaseQuotaPrivilege: {error}"))?;
            let requested = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let mut previous = TOKEN_PRIVILEGES::default();
            let mut returned = 0;
            unsafe {
                AdjustTokenPrivileges(
                    token,
                    false,
                    Some(&requested),
                    size_of::<TOKEN_PRIVILEGES>() as u32,
                    Some(&mut previous),
                    Some(&mut returned),
                )
            }
            .map_err(|error| format!("enable SeIncreaseQuotaPrivilege: {error}"))?;
            if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED
                || returned != size_of::<TOKEN_PRIVILEGES>() as u32
            {
                unsafe {
                    let _ = CloseHandle(token);
                }
                return Err("SeIncreaseQuotaPrivilege is unavailable on the elevated token".into());
            }
            Ok(Self { token, previous })
        }
    }
    impl Drop for QuotaPrivilege {
        fn drop(&mut self) {
            unsafe {
                let _ =
                    AdjustTokenPrivileges(self.token, false, Some(&self.previous), 0, None, None);
                let _ = CloseHandle(self.token);
            }
        }
    }
    pub(super) fn trim() -> Result<usize, String> {
        let _privilege = QuotaPrivilege::enable()?;
        unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) }
            .map_err(|error| format!("SetSystemFileCacheSize: {error}"))?;
        Ok(0)
    }
}

#[cfg(windows)]
mod event_logs {
    use windows::{Win32::System::EventLog::EvtClearLog, core::PCWSTR};
    pub(super) fn clear(channel: &str) -> Result<usize, String> {
        let wide = channel.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe { EvtClearLog(None, PCWSTR(wide.as_ptr()), PCWSTR::null(), 0) }
            .map_err(|error| format!("EvtClearLog {channel}: {error}"))?;
        Ok(0)
    }
}

#[cfg(windows)]
mod dns {
    use std::mem::transmute;
    use windows::{
        Win32::{
            Foundation::FreeLibrary,
            System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
        },
        core::{PCSTR, w},
    };
    pub(super) fn flush() -> Result<usize, String> {
        let module =
            unsafe { LoadLibraryExW(w!("dnsapi.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(|error| format!("load fixed System32 dnsapi.dll: {error}"))?;
        let symbol =
            unsafe { GetProcAddress(module, PCSTR(c"DnsFlushResolverCache".as_ptr().cast())) }
                .ok_or("DnsFlushResolverCache is not dynamically exported by fixed dnsapi.dll")?;
        let flush: unsafe extern "system" fn() -> windows::core::BOOL =
            unsafe { transmute(symbol) };
        let result = unsafe { flush() }.as_bool();
        unsafe {
            let _ = FreeLibrary(module);
        }
        result
            .then_some(0)
            .ok_or("DnsFlushResolverCache returned false".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn verified_config() -> VerifiedConfig {
        let bytes = include_bytes!("../../../../frametime.toml").to_vec();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        VerifiedConfig::from_verified_bytes(bytes.clone(), bytes.len() as u64, &digest)
            .expect("verified fixture")
    }

    #[test]
    fn report_has_one_result_in_exact_core_order_even_when_every_host_action_is_deferred() {
        let report = run(
            CleanupMode::Full,
            Path::new("C:\\FRAMETIME_CFG"),
            &verified_config(),
        );
        assert_eq!(
            report.action_results.len(),
            cleanup_actions(CleanupMode::Full).len()
        );
        assert_eq!(
            report
                .action_results
                .iter()
                .map(|result| result.action)
                .collect::<Vec<_>>(),
            cleanup_actions(CleanupMode::Full)
                .iter()
                .map(|spec| spec.action)
                .collect::<Vec<_>>(),
        );
    }
    #[test]
    fn fixed_steam_validation_target_is_clipboard_only() {
        assert_eq!(STEAM_VALIDATE_730_URI, "steam://validate/730");
        assert!(!STEAM_VALIDATE_730_URI.contains(' '));
    }
}
