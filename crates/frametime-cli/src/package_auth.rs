use crate::error::AppError;

pub(crate) fn run_authentication_smoke() -> Result<(), AppError> {
    // The application layer already prefixes the error.
    let package = frametime_app::authenticate_package()
        .map_err(|error| AppError::failed(error.to_string()))?;
    println!(
        "PACKAGE AUTH OK: version {}; {} payload files",
        package.manifest().version(),
        package.manifest().files().len()
    );
    Ok(())
}
