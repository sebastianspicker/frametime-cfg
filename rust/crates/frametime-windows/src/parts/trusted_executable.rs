// Fixed executable roles in the authenticated portable package.
//
// Retention and publisher authentication are intentionally inseparable and
// live in `package_trust`; no path-only executable capability is exposed.

pub const GUI_EXECUTABLE_NAME: &str = "frametime-gui.exe";
pub const CLI_EXECUTABLE_NAME: &str = "frametime.exe";
