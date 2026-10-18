//! Safe-mode and post-reboot handoff operations.

mod arming_rollback;
mod handoff;
mod hardware;

use arming_rollback::{NativeArmingHost, roll_back_partial_arming};
pub use handoff::*;
pub use hardware::*;
