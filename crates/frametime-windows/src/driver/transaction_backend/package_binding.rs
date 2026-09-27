use frametime_domain::driver::{NvidiaComponentSelection, Sha256Digest};

use crate::TrustedWorkDir;
use crate::driver::DriverTransaction;
use crate::driver::package_builder::{
    prepared_nvidia_package_digest, verify_prepared_nvidia_package,
};

pub(super) fn verified_prepared_package_digest(
    trusted: &TrustedWorkDir,
    selection: &NvidiaComponentSelection,
) -> Result<Sha256Digest, String> {
    let package = trusted.path().join("driver-package");
    let manifest = verify_prepared_nvidia_package(&package)?;
    let selected = selection.selected.iter().cloned().collect::<Vec<_>>();
    let required_unclassified = selection
        .required_unclassified
        .iter()
        .map(|component| component.directory.clone())
        .collect::<Vec<_>>();
    if manifest.selected_components != selected
        || manifest.required_unclassified != required_unclassified
    {
        return Err("prepared NVIDIA package does not match its component selection".into());
    }
    prepared_nvidia_package_digest(&manifest)
}

pub(super) fn verify_transaction_prepared_package(
    trusted: &TrustedWorkDir,
    transaction: &DriverTransaction,
) -> Result<(), String> {
    let selection = transaction
        .component_selection
        .as_ref()
        .ok_or("driver transaction lacks its component selection")?;
    let expected = transaction
        .prepared_package_sha256
        .as_ref()
        .ok_or("driver transaction lacks its prepared-package binding")?;
    let actual = verified_prepared_package_digest(trusted, selection)?;
    if &actual == expected {
        Ok(())
    } else {
        Err("prepared NVIDIA package does not match the durable transaction".into())
    }
}
