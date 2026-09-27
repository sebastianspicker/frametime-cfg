//! Driver acquisition, cleanup, and NVIDIA DRS transactions.

mod capability;
mod cleanup_observation;
#[cfg(test)]
mod cleanup_v2;
#[cfg_attr(not(windows), allow(dead_code))]
mod drs;
#[cfg(any(test, windows))]
mod drs_abi;
mod drs_backend;
mod drs_observation;
#[cfg(windows)]
mod drs_windows;
mod lab_builder;
mod package_builder;
mod transaction;
mod transaction_backend;

pub use capability::{
    DriverArtifactStore, NativeNvidiaArtifactStore, NativeNvidiaInstallerRunner,
    NativeNvidiaSignatureVerifier, NativeSystem32ToolRunner, NvidiaArtifactAcquirer,
    NvidiaArtifactLocation, NvidiaDownloadHost, NvidiaInstaller, NvidiaInstallerRunner,
    NvidiaSignatureVerifier, PnpUtilDriverRemoval, ProcessOutcome, System32ToolRunner,
    VerifiedDriverArtifact, WindowsDriverInspection, WindowsSafeModeInspection,
};
pub(crate) use transaction::load_driver_transaction_at;
pub use transaction::{DriverLifecycleStatus, DriverTransaction, inspect_driver_status};

pub use drs::{
    CS2_SETTINGS, DrsApplicationOriginal, DrsBackup, DrsError, DrsOriginalSetting, DrsPreparation,
    NvapiDrs,
};
#[cfg(windows)]
pub(crate) use drs::{
    apply_cs2_profile, capture_cs2_backup, prepare_cs2_profile, restore_cs2_profile,
    verify_cs2_profile,
};
pub(crate) use drs_observation::*;
#[cfg(windows)]
pub use drs_windows::NativeNvapiDrs;
pub use lab_builder::{NvidiaLabBuildManifest, NvidiaLabBuildRequest, build_nvidia_lab};
pub use package_builder::{
    NvidiaPackageFile, PreparedNvidiaPackageManifest, build_nvidia_package, extract_nvidia_sfx,
};
pub(crate) use transaction_backend::*;
pub use transaction_backend::{
    NvidiaInstallerSource, NvidiaPreparationRequest, prepare_nvidia_driver,
    prepare_nvidia_driver_with_options, reconcile_nvidia_profiles,
};
