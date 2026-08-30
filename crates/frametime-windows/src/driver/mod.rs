//! Driver acquisition, cleanup, and NVIDIA DRS transactions.

mod capability;
mod cleanup_observation;
#[cfg_attr(not(windows), allow(dead_code))]
mod drs;
#[cfg(any(test, windows))]
mod drs_abi;
mod drs_backend;
mod drs_observation;
#[cfg(windows)]
mod drs_windows;
mod transaction;
mod transaction_backend;

pub use capability::{
    DriverArtifactStore, NativeNvidiaArtifactStore, NativeNvidiaInstallerRunner,
    NativeNvidiaSignatureVerifier, NativeSystem32ToolRunner, NvidiaArtifactAcquirer,
    NvidiaArtifactLocation, NvidiaDownloadHost, NvidiaInstaller, PnpUtilDriverRemoval,
    System32ToolRunner, WindowsDriverInspection, WindowsSafeModeInspection,
};
pub use transaction::DriverTransaction;
pub(crate) use transaction::load_driver_transaction_at;

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
pub use transaction_backend::prepare_nvidia_driver;
pub(crate) use transaction_backend::*;
