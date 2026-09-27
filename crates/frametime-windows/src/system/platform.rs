//! Small Windows-platform adapters kept separate by trust boundary.

#[path = "platform/clipboard.rs"]
mod clipboard_impl;
mod os;
mod sid;
#[cfg(test)]
mod tests;
mod tools;
mod work_lock;

pub(crate) use clipboard_impl::clipboard;
#[cfg(windows)]
pub(crate) use os::build_number;
pub use os::harden_process_dll_search;
pub(crate) use os::{boot_mode, is_supported};
pub(crate) use sid::*;
pub(crate) use tools::*;
pub(crate) use work_lock::*;
