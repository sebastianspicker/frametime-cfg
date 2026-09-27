//! Platform-neutral presentation model for the native desktop front end.
//!
//! This façade deliberately contains no Win32 types. Keeping navigation,
//! package authority, catalog filtering, video guidance, network reporting,
//! and operation state in small modules keeps safety-critical UI states
//! testable on every host, including CI hosts that cannot create a window.

mod catalog_filter;
mod navigation;
mod network_reporting;
mod operation_state;
mod package_auth;
mod video_preview;

#[cfg(windows)]
pub use catalog_filter::*;
pub use navigation::*;
#[cfg(windows)]
pub use network_reporting::*;
#[cfg(windows)]
pub use operation_state::*;
#[cfg(windows)]
pub use package_auth::*;
#[cfg(windows)]
pub use video_preview::*;

mod presentation_reads;
pub mod snapshots;
#[cfg(windows)]
pub use presentation_reads::read_presentation;

mod input_text;
#[cfg(windows)]
pub use input_text::{VprofInputState, decode_bounded_control_text};

pub mod fps_session;
