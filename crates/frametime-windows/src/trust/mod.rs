//! Authentication, protected-root, retained-handle, and runtime-publication boundary.

mod config_authority;
mod executable;
#[cfg(any(test, windows))]
pub(crate) mod io_contract;
#[cfg(windows)]
pub(crate) mod io_windows;
#[cfg(any(test, windows))]
pub(crate) mod json_common;
#[cfg(windows)]
pub(crate) mod json_windows;
pub(crate) mod package_trust;
mod public;
mod runtime_publish;
mod runtime_trust;
mod windows;

pub use config_authority::*;
pub use executable::*;
pub use package_trust::*;
pub use public::*;
pub use runtime_publish::*;
pub use runtime_trust::*;
#[cfg(windows)]
pub(crate) use windows::trusted_work_dir;
