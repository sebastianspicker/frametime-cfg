//! Native Windows boundary for the frametime configuration transaction.
//!
//! This crate deliberately has no PowerShell, shell, or command-line-string
//! execution path. Registry state is handled through Win32 and the six
//! exceptional OS tools are represented as typed, separately-quoted argument
//! vectors.  The planner stays portable; live operations are Windows-only.

#[cfg(any(test, not(windows)))]
use std::fs;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use frametime_core::{
    Backend, BackupEntry, BackupFile, CleanupReport, Config, Cs2ConfigController, Cs2ConfigRequest,
    Cs2Install, EvidenceRequirement, FinalBenchmarkCommit, FinalBenchmarkReceipt, GpuBranch,
    GpuVendor, Inspection, NativeCs2ConfigFs, ObservationReceipt, ObservationSubject, Operation,
    OptionalCfgAsset, Profile, Progress, RebootStage, State, TransactionId, VerificationItem,
    VerificationReport, VerificationStatus, VideoDocument, VideoRow, VideoTier,
    benchmark::{
        BenchmarkRecord, FINAL_BENCHMARK_LABEL, MAX_BENCHMARK_HISTORY,
        prepare_baseline_benchmark_commit, prepare_final_benchmark_commit,
        validate_persisted_baseline_benchmark, validate_persisted_final_benchmark,
    },
    discover_cs2_install, discover_video_txt,
    fps::BenchmarkCapture,
    plan_for_step, read_trusted_video_document, resolve_video_tier,
};
#[cfg(any(test, windows))]
use frametime_core::{VideoFilePlatform, VideoWriteReport, write_trusted_video_config};
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
#[cfg(any(test, windows))]
// A protected DACL alone is insufficient: the object's owner retains the
// ability to rewrite its DACL. Keep newly created trusted objects owned by
// the local Administrators group as well as limiting their DACL to BA/SYSTEM.
const TRUSTED_WORK_DIR_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

include!("parts/video.rs");
include!("parts/reboot_hardware.rs");
include!("parts/persistence.rs");
include!("parts/config_authority.rs");
#[path = "parts/cleanup_native.rs"]
mod cleanup_native;
include!("parts/persistence_final_support.rs");
include!("parts/chipset.rs");
include!("parts/chipset_windows.rs");
include!("parts/smbios.rs");
include!("parts/backend_public.rs");
include!("parts/backend.rs");
include!("parts/backend_capture_helpers.rs");
include!("parts/native_drs_backend.rs");
include!("parts/native_drs_observation.rs");
include!("parts/backend_construction.rs");
include!("parts/backend_transaction.rs");
include!("parts/action_registry_builders.rs");
include!("parts/action_catalog.rs");
include!("parts/hags.rs");
include!("parts/hags_backend.rs");
include!("parts/action_descriptor.rs");
include!("parts/vbs.rs");
include!("parts/planner_backend.rs");
include!("parts/action_runtime.rs");
include!("parts/observations.rs");
include!("parts/runtime_trust_contract.rs");
include!("parts/runtime_trust.rs");
include!("parts/runtime_publish_contract.rs");
include!("parts/runtime_publish.rs");
include!("parts/reboot_handoff.rs");
include!("parts/trusted_executable.rs");
include!("parts/package_trust.rs");
include!("parts/shader_cache.rs");
#[cfg(windows)]
#[path = "parts/cleanup_shader.rs"]
mod cleanup_shader;
include!("parts/shader_cache_backend.rs");
include!("parts/irreversible_audit_backend.rs");
include!("parts/evidence_store.rs");
include!("parts/evidence_backend.rs");
#[cfg(windows)]
mod shader_cache_handle_validation;
#[cfg(windows)]
mod shader_cache_handles;
include!("parts/trusted_public.rs");
include!("parts/trusted_windows.rs");
#[cfg(any(test, windows))]
mod trusted_io_contract {
    include!("parts/trusted_io_contract.rs");
}
#[cfg(windows)]
mod trusted_io_windows {
    include!("parts/trusted_io_windows.rs");
}
#[cfg(any(test, windows))]
mod trusted_json_common;
#[cfg(windows)]
mod trusted_json_windows;
include!("parts/platform.rs");
include!("parts/network.rs");
include!("parts/network_stack.rs");
include!("parts/dns.rs");
include!("parts/cs2_registry.rs");
include!("parts/cs2_config.rs");
include!("parts/registry.rs");
include!("parts/autostart.rs");
include!("parts/power_plan.rs");
include!("parts/wmi.rs");
include!("parts/pagefile.rs");
include!("parts/pagefile_native.rs");
include!("parts/recovery.rs");
include!("parts/native_drs_recovery.rs");
include!("parts/debloat.rs");
include!("parts/debloat_validation.rs");
include!("parts/debloat_windows.rs");
include!("parts/debloat_backend.rs");
include!("parts/services.rs");
include!("parts/device_bindings.rs");
include!("parts/device_binding_resolution.rs");
include!("parts/network_adapter_bindings.rs");
include!("parts/processor_topology.rs");
include!("parts/interrupts.rs");
include!("parts/driver_transaction_backend.rs");
mod driver_capability;
mod driver_cleanup_observation;
mod driver_transaction;
pub use driver_capability::{
    DriverArtifactStore, NativeNvidiaArtifactStore, NativeNvidiaInstallerRunner,
    NativeNvidiaSignatureVerifier, NativeSystem32ToolRunner, NvidiaArtifactAcquirer,
    NvidiaArtifactLocation, NvidiaArtifactPolicy, NvidiaDownloadHost, NvidiaInstaller,
    NvidiaInstallerRunner, NvidiaSignatureVerifier, PnpUtilDriverRemoval, ProcessOutcome,
    System32ToolRunner, VerifiedDriverArtifact, WindowsDriverInspection, WindowsSafeModeInspection,
};
pub use driver_transaction::{
    DriverTransaction, load_driver_transaction, persist_driver_transaction,
};
include!("parts/interrupt_registry.rs");
include!("parts/interrupt_registry_windows.rs");
include!("parts/interrupt_backend.rs");
include!("parts/verification_backend.rs");
#[path = "parts/native_drs.rs"]
mod drs_transaction;
#[cfg(windows)]
#[path = "parts/native_drs_windows.rs"]
mod drs_windows_adapter;
#[cfg(any(test, windows))]
#[path = "parts/native_drs_abi.rs"]
mod native_drs_abi;
pub use drs_transaction::{
    CS2_PROFILE_NAME, CS2_SETTINGS, DrsApplicationOriginal, DrsApplyReport, DrsBackup, DrsError,
    DrsOriginalSetting, DrsPreparation, DrsTargetSetting, NvapiDrs, apply_cs2_profile,
    capture_cs2_backup, prepare_cs2_profile, restore_cs2_profile, verify_cs2_profile,
};
#[cfg(windows)]
pub use drs_windows_adapter::NativeNvapiDrs;

#[cfg(test)]
mod tests {
    include!("tests/tests_a.rs");
    include!("tests/tests_b.rs");
    include!("tests/device_interrupts.rs");
    include!("tests/driver_capability.rs");
    include!("tests/irreversible_audit.rs");
    include!("tests/debloat.rs");
    include!("tests/runtime_publish.rs");
    include!("tests/network_stack.rs");
}
