#[cfg(windows)]
use super::capability::{
    authorization_expiry_after, validate_bounded_nvidia_authorization,
    validate_nvidia_authorization_structure,
};
#[cfg(windows)]
use super::transaction::{load_profile_backup_at, persist_driver_transaction};
use crate::*;
#[cfg(windows)]
use frametime_domain::driver::{
    ArtifactAcquisitionAuthorization, ArtifactIdentity, ArtifactLocator, CaptureFreshnessPolicy,
    DriverPlanInput, DriverTransactionV2Stage, ExecutionClock, GpuVendor as DriverGpuVendor,
    InspectionAdapter, InstallationEvidence, NvidiaComponentPreset, SCHEMA_VERSION,
    generate_dry_run_plan,
};
use std::path::PathBuf;

#[cfg(windows)]
mod source;
#[cfg(windows)]
use source::resolve_source;
#[cfg(any(test, windows))]
mod profile_restore;
#[cfg(windows)]
use profile_restore::complete_profile_restore;
#[cfg(windows)]
pub use profile_restore::reconcile_nvidia_profiles;
#[cfg(windows)]
mod preparation;
#[cfg(windows)]
use preparation::{
    clear_stale_preparation_outputs, finish_preparation, resolve_component_selection,
    resume_preparation,
};
#[cfg(windows)]
mod package_binding;
#[cfg(windows)]
use package_binding::{verified_prepared_package_digest, verify_transaction_prepared_package};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvidiaInstallerSource {
    OfficialUrl(String),
    LocalInstaller(PathBuf),
    LegacyServerPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaPreparationRequest {
    pub source: NvidiaInstallerSource,
    pub preset: frametime_domain::driver::NvidiaComponentPreset,
    pub select: Vec<String>,
    pub deselect: Vec<String>,
    pub artifact_id: Option<String>,
    pub artifact_file_name: Option<String>,
}

/// Prepare P1:18/P1:19 as one durable, readback-verified transaction. The
/// routing fields are deliberately narrower than a URL: the host, signer,
/// digest, executable path, and GPU identity are all derived or compiled.
#[cfg(windows)]
pub fn prepare_nvidia_driver(
    _package: &AuthenticatedPackage,
    artifact_id: String,
    artifact_file_name: String,
    server_path: String,
) -> Result<DriverTransaction, String> {
    prepare_nvidia_driver_with_options(
        _package,
        NvidiaPreparationRequest {
            source: NvidiaInstallerSource::LegacyServerPath(server_path),
            preset: NvidiaComponentPreset::Recommended,
            select: Vec::new(),
            deselect: Vec::new(),
            artifact_id: Some(artifact_id),
            artifact_file_name: Some(artifact_file_name),
        },
    )
}

#[cfg(windows)]
pub fn prepare_nvidia_driver_with_options(
    _package: &AuthenticatedPackage,
    request: NvidiaPreparationRequest,
) -> Result<DriverTransaction, String> {
    let (artifact_id, artifact_file_name, location) = resolve_source(&request)?;
    let locator = ArtifactLocator {
        artifact_id,
        artifact_file_name,
        extensions: BTreeMap::new(),
    };
    locator.validate().map_err(|error| error.to_string())?;
    let trusted = TrustedWorkDir::acquire_fixed()?;
    if let Some(transaction) = resume_preparation(&trusted, &locator, &request)? {
        return Ok(transaction);
    }
    clear_stale_preparation_outputs(&trusted)?;
    if let NvidiaInstallerSource::LocalInstaller(source) = &request.source {
        source::stage_local_installer(source, &trusted, &locator.artifact_file_name)?;
    }
    let inspection = WindowsDriverInspection::native();
    let target_gpu = inspection
        .inspect_exact_gpu()
        .map_err(|error| error.to_string())?;
    if target_gpu.vendor != DriverGpuVendor::Nvidia {
        return Err("prepare-nvidia requires one exact active NVIDIA GPU".into());
    }
    let installed_packages = inspection
        .inspect_published_packages(&target_gpu)
        .map_err(|error| error.to_string())?;
    if installed_packages.is_empty() {
        return Err("prepare-nvidia requires exact existing NVIDIA package evidence".into());
    }
    let acquirer = NvidiaArtifactAcquirer::new(
        NativeNvidiaArtifactStore::acquire_fixed_root().map_err(|error| error.to_string())?,
        NativeNvidiaSignatureVerifier,
        location,
    );
    let (capability, artifact) = acquirer
        .acquire_verified(&locator, &target_gpu)
        .map_err(|error| error.to_string())?;
    let extracted = trusted.path().join("driver-extracted");
    let package_output = trusted.path().join("driver-package");
    let installer_path = trusted
        .path()
        .join("driver-artifacts")
        .join(&locator.artifact_file_name);
    super::extract_nvidia_sfx(&installer_path, &extracted)?;
    capability.revalidate().map_err(|error| error.to_string())?;
    let source_directories = source_component_directories(&extracted)?;
    let selection = resolve_component_selection(&request, &source_directories)?;
    super::build_nvidia_package(&extracted, &package_output, &selection)?;
    let prepared_package_sha256 = verified_prepared_package_digest(&trusted, &selection)?;
    capability.revalidate().map_err(|error| error.to_string())?;
    let transaction = prepare_durable_transaction(
        &trusted,
        locator,
        target_gpu,
        installed_packages,
        artifact,
        selection,
        prepared_package_sha256,
    )?;
    finish_preparation(&trusted, transaction)
}

#[cfg(windows)]
fn prepare_durable_transaction(
    trusted: &TrustedWorkDir,
    locator: ArtifactLocator,
    target_gpu: frametime_domain::driver::ExactGpuIdentity,
    installed_packages: Vec<frametime_domain::driver::PublishedDriverPackage>,
    artifact: frametime_domain::driver::SignedArtifactDescriptor,
    selection: frametime_domain::driver::NvidiaComponentSelection,
    prepared_package_sha256: frametime_domain::driver::Sha256Digest,
) -> Result<DriverTransaction, String> {
    let plan = generate_dry_run_plan(&DriverPlanInput {
        target_gpu: target_gpu.clone(),
        installed_packages,
        artifact: artifact.clone(),
        extensions: BTreeMap::new(),
    })
    .map_err(|error| error.to_string())?;
    let package_set_sha256 = frametime_domain::driver::CanonicalPackageSet::from_unsorted(
        target_gpu.clone(),
        exact_packages_from_plan(&plan)?,
    )
    .map_err(|error| error.to_string())?
    .fingerprint()
    .map_err(|error| error.to_string())?;
    let authorized_at_utc = trusted_current_utc();
    let authorization = ArtifactAcquisitionAuthorization {
        schema_version: SCHEMA_VERSION,
        authorization_id: format!("nvidia-{}", locator.artifact_id),
        plan_sha256: plan.input_sha256.clone(),
        target_gpu,
        package_set_sha256,
        artifact: ArtifactIdentity::from_descriptor(&artifact)
            .map_err(|error| error.to_string())?,
        authorized_at_utc: authorized_at_utc.clone(),
        // Driver-artifact authority expires using the same trusted clock as execution.
        expires_at_utc: authorization_expiry_after(&authorized_at_utc)
            .map_err(|error| error.to_string())?,
    };
    let transaction = DriverTransaction::prepared_v2(
        plan,
        artifact,
        authorization,
        selection,
        prepared_package_sha256,
    )?;
    persist_driver_transaction(trusted.path(), &transaction)
}

#[cfg(windows)]
fn exact_packages_from_plan(
    plan: &frametime_domain::driver::DryRunDriverPlan,
) -> Result<Vec<frametime_domain::driver::PublishedDriverPackage>, String> {
    plan.entries
        .get(1)
        .and_then(|entry| match &entry.action {
            frametime_domain::driver::PlannedDriverAction::RecordExactPackages { packages } => {
                Some(packages.clone())
            }
            _ => None,
        })
        .ok_or_else(|| "prepared plan lacks exact package record".into())
}

#[cfg(windows)]
fn source_component_directories(root: &std::path::Path) -> Result<Vec<String>, String> {
    let mut directories = Vec::new();
    for entry in std::fs::read_dir(root)
        .map_err(|error| format!("read extracted NVIDIA package: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read NVIDIA package entry: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("inspect NVIDIA package entry: {error}"))?
            .is_dir()
        {
            directories.push(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "NVIDIA package component directory is not Unicode")?,
            );
        }
    }
    directories.sort();
    Ok(directories)
}

#[cfg(windows)]
pub(crate) struct NativeDriverClock;

#[cfg(windows)]
impl ExecutionClock for NativeDriverClock {
    fn current_utc(&self) -> Result<String, frametime_domain::driver::AdapterFailure> {
        Ok(trusted_current_utc())
    }
}

#[cfg(windows)]
pub fn remove_prepared_nvidia_driver(
    _runtime: &VerifiedSelectedRuntime,
) -> Result<DriverTransaction, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let mut transaction = load_driver_transaction_at(trusted.path())?
        .ok_or("P2:2 requires durable P1:18/P1:19 NVIDIA transaction evidence")?;
    let lifecycle = transaction
        .lifecycle
        .clone()
        .ok_or("P2:2 refuses a v1 transaction; prepare a new v2 NVIDIA package")?;
    let armed_removal = lifecycle.stage == DriverTransactionV2Stage::SafeModeHandoffArmed;
    if transaction.removal.is_some() {
        return Ok(transaction);
    }
    verify_transaction_prepared_package(&trusted, &transaction)?;
    let profile_backup = lifecycle
        .profile_backup
        .as_ref()
        .ok_or("P2:2 requires a durable NVIDIA profile backup")?;
    let _ = load_profile_backup_at(trusted.path(), profile_backup)?;
    super::capability::verify_basic_display_fallback().map_err(|error| error.to_string())?;
    let inspection = WindowsDriverInspection::native();
    let removal = PnpUtilDriverRemoval::new(NativeSystem32ToolRunner, inspection);
    let capture = match lifecycle.stage {
        DriverTransactionV2Stage::ProfileBackupPersisted => {
            let capture = frametime_domain::driver::capture_driver_execution(
                &transaction.plan,
                &WindowsSafeModeInspection,
                &removal,
                &NativeDriverClock,
                CaptureFreshnessPolicy {
                    maximum_age_seconds: 900,
                },
            )
            .map_err(|error| error.to_string())?;
            transaction.capture = Some(capture.clone());
            transaction.lifecycle = Some(
                lifecycle
                    .transition(DriverTransactionV2Stage::StateCapturePersisted, None)
                    .map_err(|error| format!("advance driver capture stage: {error:?}"))?,
            );
            transaction = persist_driver_transaction(trusted.path(), &transaction)?;
            capture
        }
        DriverTransactionV2Stage::StateCapturePersisted
        | DriverTransactionV2Stage::SafeModeHandoffArmed => transaction
            .capture
            .clone()
            .ok_or("v2 capture stage lacks persisted capture evidence")?,
        _ => return Err("P2:2 driver lifecycle is not at a resumable Safe Mode boundary".into()),
    };
    let removal_freshness = CaptureFreshnessPolicy {
        maximum_age_seconds: 900,
    };
    let resuming_partial_removal = armed_removal
        && frametime_domain::driver::captured_package_removal_started(&capture, &removal)
            .map_err(|error| error.to_string())?;
    if resuming_partial_removal {
        validate_driver_resume_authorization(&transaction, &capture, removal_freshness)?;
    } else {
        validate_driver_authorization(
            &transaction,
            &capture,
            removal_freshness,
            &trusted_current_utc(),
        )?;
    }
    if transaction
        .lifecycle
        .as_ref()
        .is_some_and(|value| value.stage == DriverTransactionV2Stage::StateCapturePersisted)
    {
        let lifecycle = transaction
            .lifecycle
            .as_ref()
            .expect("checked lifecycle")
            .transition(DriverTransactionV2Stage::SafeModeHandoffArmed, None)
            .map_err(|error| format!("advance Safe Mode handoff stage: {error:?}"))?;
        transaction.lifecycle = Some(lifecycle);
        transaction = persist_driver_transaction(trusted.path(), &transaction)?;
    }
    let evidence = execute_package_removal(
        &transaction,
        capture.clone(),
        &removal,
        resuming_partial_removal,
        removal_freshness,
    )?;
    transaction.removal = Some(evidence);
    transaction.lifecycle = Some(
        transaction
            .lifecycle
            .as_ref()
            .ok_or("v2 cleanup stage lacks lifecycle state")?
            .transition(DriverTransactionV2Stage::CleanupComplete, None)
            .map_err(|error| format!("advance cleanup stage: {error:?}"))?,
    );
    persist_driver_transaction(trusted.path(), &transaction)
}

#[cfg(windows)]
fn execute_package_removal(
    transaction: &DriverTransaction,
    capture: frametime_domain::driver::DriverExecutionCapture,
    removal: &dyn frametime_domain::driver::PackageExecutionAdapter,
    resuming: bool,
    freshness: CaptureFreshnessPolicy,
) -> Result<frametime_domain::driver::RemovalExecutionEvidence, String> {
    let result = if resuming {
        frametime_domain::driver::resume_captured_package_removal(
            &transaction.plan,
            capture,
            &WindowsSafeModeInspection,
            removal,
            &NativeDriverClock,
            freshness,
        )
    } else {
        frametime_domain::driver::remove_captured_packages(
            &transaction.plan,
            capture,
            removal,
            &NativeDriverClock,
            freshness,
        )
    };
    result.map_err(|error| error.to_string())
}

#[cfg(windows)]
pub fn install_prepared_nvidia_driver(
    _runtime: &VerifiedSelectedRuntime,
) -> Result<DriverTransaction, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let mut transaction = load_driver_transaction_at(trusted.path())?
        .ok_or("P3:1 requires durable P1:18/P1:19 NVIDIA transaction evidence")?;
    if transaction.lifecycle.is_none() {
        return Err("P3:1 refuses a v1 transaction; prepare a new v2 NVIDIA package".into());
    }
    if transaction.installation.is_some() {
        return complete_profile_restore(transaction, trusted.path());
    }
    let capture = transaction
        .capture
        .clone()
        .ok_or("P3:1 requires retained P2:2 capture")?;
    if !transaction.removal_complete() {
        return Err("P3:1 requires coherent P2:2 removal evidence".into());
    }
    verify_transaction_prepared_package(&trusted, &transaction)?;
    validate_driver_resume_authorization(
        &transaction,
        &capture,
        CaptureFreshnessPolicy {
            maximum_age_seconds: 86_400,
        },
    )?;
    let pre_launch_utc = trusted_current_utc();
    let (installed_artifact, post_install_packages, fresh_authenticode) =
        install_prepared_artifact(&transaction, &pre_launch_utc)?;
    let installation = InstallationEvidence {
        authorization: transaction.authorization.clone(),
        fresh_authenticode,
        installed_artifact,
        post_install_packages: frametime_domain::driver::CanonicalPackageSet::from_unsorted(
            transaction.plan.target_gpu.clone(),
            post_install_packages,
        )
        .map_err(|error| error.to_string())?,
        observed_at_utc: trusted_current_utc(),
    };
    installation
        .validate_for_recovery(&capture, &transaction.artifact)
        .map_err(|error| error.to_string())?;
    transaction.installation = Some(installation);
    transaction.lifecycle = Some(
        transaction
            .lifecycle
            .as_ref()
            .ok_or("P3:1 refuses a v1 transaction; prepare a new v2 NVIDIA package")?
            .transition(DriverTransactionV2Stage::InstallationComplete, None)
            .map_err(|error| format!("advance installation stage: {error:?}"))?,
    );
    let transaction = persist_driver_transaction(trusted.path(), &transaction)?;
    complete_profile_restore(transaction, trusted.path())
}

#[cfg(windows)]
fn install_prepared_artifact(
    transaction: &DriverTransaction,
    pre_launch_utc: &str,
) -> Result<
    (
        frametime_domain::driver::InstalledArtifactObservation,
        Vec<frametime_domain::driver::PublishedDriverPackage>,
        frametime_domain::driver::AuthenticodeEvidence,
    ),
    String,
> {
    let package_sha256 = transaction
        .prepared_package_sha256
        .as_ref()
        .ok_or("driver transaction lacks its prepared-package binding")?;
    let launch = super::capability::launch_prepared_nvidia_package(
        &transaction.artifact,
        package_sha256,
        pre_launch_utc,
    )
    .map_err(|error| error.to_string())?;
    if launch.outcome.exit_code != Some(0) {
        return Err(
            "prepared NVIDIA installer returned a nonzero or unavailable exit status".into(),
        );
    }
    let packages = WindowsDriverInspection::native()
        .inspect_published_packages(&transaction.plan.target_gpu)
        .map_err(|error| error.to_string())?;
    if packages.is_empty() {
        return Err("prepared NVIDIA install produced no target-GPU package".into());
    }
    Ok((
        frametime_domain::driver::InstalledArtifactObservation {
            artifact: launch.artifact,
            observed_at_utc: trusted_current_utc(),
        },
        packages,
        launch.authenticode,
    ))
}

#[cfg(windows)]
pub(crate) fn validate_driver_authorization(
    transaction: &DriverTransaction,
    capture: &frametime_domain::driver::DriverExecutionCapture,
    freshness: CaptureFreshnessPolicy,
    now_utc: &str,
) -> Result<(), String> {
    capture
        .validate_for_plan_at(&transaction.plan, freshness, now_utc)
        .map_err(|error| error.to_string())?;
    transaction
        .authorization
        .validate_for_capture_at(capture, &transaction.artifact, now_utc)
        .map_err(|error| error.to_string())?;
    validate_bounded_nvidia_authorization(
        &transaction.authorization,
        &transaction.artifact,
        now_utc,
    )
    .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn validate_driver_resume_authorization(
    transaction: &DriverTransaction,
    capture: &frametime_domain::driver::DriverExecutionCapture,
    freshness: CaptureFreshnessPolicy,
) -> Result<(), String> {
    capture
        .validate_for_plan(&transaction.plan, freshness)
        .map_err(|error| error.to_string())?;
    transaction
        .authorization
        .validate_for_capture(capture, &transaction.artifact)
        .map_err(|error| error.to_string())?;
    validate_nvidia_authorization_structure(&transaction.authorization, &transaction.artifact)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(crate) fn trusted_current_utc() -> String {
    crate::driver::capability::trusted_utc_timestamp()
}

#[cfg(not(windows))]
pub fn remove_prepared_nvidia_driver(
    _runtime: &VerifiedSelectedRuntime,
) -> Result<DriverTransaction, String> {
    Err("P2:2 driver removal is supported only on Windows Safe Mode".into())
}

#[cfg(not(windows))]
pub fn install_prepared_nvidia_driver(
    _runtime: &VerifiedSelectedRuntime,
) -> Result<DriverTransaction, String> {
    Err("P3:1 driver installation is supported only on Windows".into())
}

#[cfg(not(windows))]
pub fn prepare_nvidia_driver(
    _package: &AuthenticatedPackage,
    _artifact_id: String,
    _artifact_file_name: String,
    _server_path: String,
) -> Result<DriverTransaction, String> {
    Err("prepare-nvidia is supported only on Windows".into())
}

#[cfg(not(windows))]
pub fn prepare_nvidia_driver_with_options(
    _package: &AuthenticatedPackage,
    request: NvidiaPreparationRequest,
) -> Result<DriverTransaction, String> {
    match &request.source {
        NvidiaInstallerSource::OfficialUrl(url)
            if !url.starts_with("https://international.download.nvidia.com/") =>
        {
            Err("official NVIDIA URL must use the fixed HTTPS download authority".into())
        }
        _ => Err("prepare-nvidia is supported only on Windows".into()),
    }
}

#[cfg(not(windows))]
pub fn reconcile_nvidia_profiles(
    _accept_driver_incompatible_profile_items: bool,
    _yes: bool,
) -> Result<DriverTransaction, String> {
    Err("NVIDIA profile reconciliation is supported only on Windows".into())
}
