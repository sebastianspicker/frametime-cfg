use frametime_windows::{AuthenticatedPackage, authenticate_current_package};

use crate::error::AppError;

/// Mint the non-cloneable capability required for any packaged Windows write
/// or external launch. The capability is intentionally retained by the caller
/// for the complete operation lifetime.
pub(crate) fn require_authenticated_package() -> Result<AuthenticatedPackage, AppError> {
    authenticate_current_package().map_err(|error| {
        AppError::failed(format!(
            "authenticated release package is required: {error}"
        ))
    })
}

pub(crate) fn run_authentication_smoke() -> Result<(), AppError> {
    let package = require_authenticated_package()?;
    println!(
        "PACKAGE AUTH OK: version {}; {} payload files",
        package.manifest().version(),
        package.manifest().files().len()
    );
    Ok(())
}
