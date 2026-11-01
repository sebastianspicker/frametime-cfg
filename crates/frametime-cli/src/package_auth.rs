use frametime_app::{AuthenticatedPackage, authenticate_package};

use crate::error::AppError;

/// Mint the non-cloneable capability required for any packaged Windows write
/// or external launch. The capability is intentionally retained by the caller
/// for the complete operation lifetime.
pub(crate) fn require_authenticated_package() -> Result<AuthenticatedPackage, AppError> {
    // The application layer already prefixes the error.
    authenticate_package().map_err(|error| AppError::failed(error.to_string()))
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
