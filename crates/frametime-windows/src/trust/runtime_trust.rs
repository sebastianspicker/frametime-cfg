//! Selected-runtime trust boundary.

#[cfg(windows)]
mod contract;
mod retained_runtime;
#[cfg(windows)]
mod windows_inspector;

#[cfg(windows)]
pub(crate) use contract::{
    MAX_RUNTIME_METADATA_BYTES, MAX_RUNTIME_PAYLOAD_BYTES, selected_generation,
    validate_runtime_contract,
};
pub(crate) use retained_runtime::inspect_selected_runtime_integrity;
pub use retained_runtime::{
    VerifiedSelectedRuntime, retain_selected_runtime, runtime_inventory_incomplete,
};

#[cfg(windows)]
pub(crate) use windows_inspector::{
    RetainedRuntimeNode, inspect as inspect_selected_runtime_integrity_windows,
};
