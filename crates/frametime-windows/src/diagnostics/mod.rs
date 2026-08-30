//! Hardware, firmware, and benchmark observation adapters.

mod chipset;
pub(crate) mod chipset_windows;
pub mod hardware;
pub(crate) mod processor_topology;
mod smbios;
mod video;

pub(crate) use chipset::*;
#[cfg(windows)]
pub(crate) use chipset_windows::*;
pub use hardware::WindowsHardwareDiagnostics;
#[cfg(windows)]
pub(crate) use processor_topology::*;
pub(crate) use smbios::*;
pub use video::*;
