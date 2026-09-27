//! Native Windows boundary for the frametime configuration transaction.
//!
//! This crate deliberately has no PowerShell, shell, or command-line-string
//! execution path. Registry state is handled through Win32 and the six
//! exceptional OS tools are represented as typed, separately-quoted argument
//! vectors.  The planner stays portable; live operations are Windows-only.
//!
//! On other hosts the native code paths are fail-closed stubs, so code reached
//! only from them is legitimately unused there. The Windows target build is the
//! authority for dead code (CI Windows lane and `scripts\verify.cmd`).
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

#[cfg(not(windows))]
use std::fs;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use frametime_domain::binding::NetworkAdapterBinding as CoreNetworkAdapterBinding;
use frametime_domain::{
    backup::{BackupEntry, BackupFile, CS2_OPTIONAL_CONFIG_TRANSACTION_STEP},
    benchmark::{
        BenchmarkRecord, FINAL_BENCHMARK_LABEL, MAX_BENCHMARK_HISTORY,
        prepare_baseline_benchmark_commit_with_evidence, prepare_final_benchmark_commit,
        prepare_final_benchmark_commit_with_evidence, prepare_final_benchmark_legacy_retry,
        validate_benchmark_run_evidence, validate_persisted_baseline_benchmark,
        validate_persisted_final_benchmark,
    },
    benchmark::{FinalBenchmarkCommit, FinalBenchmarkReceipt},
    catalog::{ActionIntent, GpuApplicability, OperationKind, OrchestrationRole, Step, StepId},
    cleanup::CleanupReport,
    config::Config,
    cs2_config::{Cs2ConfigController, Cs2ConfigRequest, OptionalCfgAsset},
    engine::{Backend, Inspection, Operation},
    evidence::{EvidenceRequirement, ObservationReceipt, ObservationSubject},
    fps::{
        BenchmarkCapture, BenchmarkRunEvidence, MIN_CAP_AUTHORIZATION_RUNS,
        ValidatedBenchmarkCapture,
    },
    handoff::{RebootStage, TransactionId},
    operations::GpuBranch,
    policy::Profile,
    state::{Progress, State},
    steam::Cs2Install,
    verification::{VerificationItem, VerificationReport, VerificationStatus},
    video::VideoRow,
};
use frametime_domain::{binding::NetworkAdapterBinding, operations::plan_for_step};
use frametime_domain::{binding::PciDeviceBinding as CorePciDeviceBinding, handoff::RuntimeRecord};
use serde_json::Value;

/// The only location a live backend may read or persist transaction state.
pub const WINDOWS_WORK_DIR: &str = r"C:\FRAMETIME_CFG";
const BACKUP_FILE: &str = "backup.json";
const AUDIT_FILE: &str = "audit.json";
const EVIDENCE_FILE: &str = "evidence.json";
const PROGRESS_FILE: &str = "progress.json";
const STATE_FILE: &str = "state.json";
const LOCK_FILE: &str = "backup.lock";
const PHASE2_HANDOFF: &str = "*!FRAMETIME_Phase2";
const PHASE3_HANDOFF: &str = "FRAMETIME_CFG_FRAMETIME_Phase3";
#[cfg(windows)]
const TRUSTED_WORK_DIR_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

pub(crate) const fn shader_cache_delete_qualified() -> bool {
    cfg!(feature = "qualified-shader-cache-delete")
}

mod backend;
mod diagnostics;
mod driver;
mod operations;
mod reboot;
mod storage;
mod system;
mod trust;

// Crate-internal flat namespace, identical on every target.
pub(crate) use self::backend::*;
pub(crate) use self::diagnostics::*;
pub(crate) use self::driver::load_driver_transaction_at;
pub(crate) use self::driver::*;
pub(crate) use self::reboot::*;
pub(crate) use self::storage::*;
pub(crate) use self::storage::{load_progress_at, load_state_at};
pub(crate) use self::trust::*;
pub(crate) use operations::*;
pub(crate) use system::platform::{boot_mode, clipboard};
pub(crate) use system::services::{native_services, native_task_scheduler};
#[cfg(windows)]
pub(crate) use system::windows_setupapi;
#[cfg(windows)]
pub(crate) use system::wmi::native as wmi;
pub(crate) use system::*;
pub(crate) use system::{cleanup_native, platform};
pub(crate) use system::{discover_cs2_install, discover_video_txt, read_trusted_video_document};
#[cfg(windows)]
pub(crate) use trust::trusted_work_dir;

// The public API is the same on every target. Native operations compile on
// non-Windows hosts as fail-closed stubs so the app and frontends can be
// built, tested, and previewed there.
pub use self::backend::{
    BackupSummary, BackupSummaryEntry, LiveBackend, PlannerBackend, clear_backup,
    deploy_optional_cs2_cfgs, export_backup, read_backup_summary, read_log,
    read_presentable_step_messages, reset_progress, restore_all_from_package,
    restore_all_from_runtime, restore_selected_from_package, run_network_stack_transaction,
};
pub use self::diagnostics::{WindowsHardwareDiagnostics, preview_video};
pub use self::driver::{
    DriverArtifactStore, DriverLifecycleStatus, DriverTransaction, NativeNvidiaArtifactStore,
    NativeNvidiaInstallerRunner, NativeNvidiaSignatureVerifier, NativeSystem32ToolRunner,
    NvidiaArtifactAcquirer, NvidiaArtifactLocation, NvidiaDownloadHost, NvidiaInstaller,
    NvidiaInstallerRunner, NvidiaInstallerSource, NvidiaLabBuildManifest, NvidiaLabBuildRequest,
    NvidiaPreparationRequest, NvidiaSignatureVerifier, PnpUtilDriverRemoval,
    PreparedNvidiaPackageManifest, ProcessOutcome, System32ToolRunner, VerifiedDriverArtifact,
    WindowsDriverInspection, WindowsSafeModeInspection, build_nvidia_lab, inspect_driver_status,
    prepare_nvidia_driver, prepare_nvidia_driver_with_options, reconcile_nvidia_profiles,
};
pub use self::reboot::{
    BootMode, BootModeEvidence, HandoffEvidence, RebootHandoffState, SafebootEvidence,
    arm_phase_three_handoff, arm_safe_mode_handoff, clear_phase_three_handoff,
    complete_phase_two_safe_boot_clear, current_boot_mode, inspect_reboot_handoff_state,
    platform_is_supported, relaunch_phase_three_handoff,
};
pub use self::storage::{
    FinalBenchmarkStatus, arm_driver_cleanup, cleanup_full, cleanup_quick, configure_profile,
    copy_text_to_clipboard, final_benchmark_status, load_backup, load_benchmark_history,
    load_progress, load_state, persist_baseline_benchmark, persist_final_benchmark,
    persist_fps_capture, read_text_from_clipboard, verify_settings,
};
pub use self::system::platform::harden_process_dll_search;
pub use self::trust::{
    AuthenticatedPackage, VerifiedSelectedRuntime, authenticate_current_package,
    launch_published_safe_mode_handoff, publish_current_packaged_runtime, retain_selected_runtime,
    runtime_inventory_incomplete,
};
