//! Windows configuration operations for operating-system and game state.

pub(crate) mod cleanup_native;
#[cfg(windows)]
pub(crate) mod cleanup_shader;
mod cs2;
mod device_binding_resolution;
pub(crate) mod device_bindings;
mod dns;
mod interrupt_backend;
mod interrupt_registry;
pub(crate) mod interrupt_registry_windows;
mod interrupts;
mod network;
pub(crate) mod network_adapter_bindings;
mod network_stack;
mod pagefile;
mod pagefile_native;
pub(crate) mod platform;
mod registry;
pub(crate) mod services;
#[cfg(windows)]
pub(crate) mod shader_cache;
mod steam_discovery;
pub(crate) mod wmi;

pub(crate) use cs2::*;
pub use device_binding_resolution::*;
#[cfg(windows)]
pub(crate) use device_bindings::windows_setupapi;
pub use device_bindings::*;
pub(crate) use dns::*;
pub(crate) use interrupt_registry::*;
#[cfg(windows)]
pub(crate) use interrupt_registry_windows::WindowsInterruptRegistry;
pub use interrupts::*;
pub(crate) use network::*;
#[cfg(windows)]
pub(crate) use network_adapter_bindings::WindowsIpHelperNetworkAdapterEnumerator;
pub(crate) use network_stack::*;
pub(crate) use pagefile::*;
pub(crate) use pagefile_native::*;
pub(crate) use platform::*;
pub(crate) use registry::*;
#[cfg(windows)]
pub(crate) use shader_cache::*;
pub(crate) use steam_discovery::{
    discover_cs2_install, discover_video_txt, read_trusted_video_document,
};
