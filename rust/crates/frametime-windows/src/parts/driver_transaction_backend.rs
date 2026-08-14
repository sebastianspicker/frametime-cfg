#[cfg(windows)]
use crate::driver_capability::{authorization_expiry_after, validate_bounded_nvidia_authorization};
#[cfg(windows)]
use frametime_driver::{
    ArtifactAcquisitionAuthorization, ArtifactIdentity, ArtifactLocator, DriverPlanInput,
    CaptureFreshnessPolicy, ExecutionClock, GpuVendor as DriverGpuVendor, InstallationEvidence,
    InspectionAdapter, SCHEMA_VERSION, generate_dry_run_plan,
};

/// Prepare P1:18/P1:19 as one durable, readback-verified transaction. The
/// routing fields are deliberately narrower than a URL: the host, signer,
/// digest, executable path, and GPU identity are all derived or compiled.
#[cfg(windows)]
pub fn prepare_nvidia_driver(
    work_dir: &std::path::Path,
    artifact_id: String,
    artifact_file_name: String,
    server_path: String,
) -> Result<DriverTransaction, String> {
    let locator = ArtifactLocator {
        artifact_id,
        artifact_file_name,
        extensions: BTreeMap::new(),
    };
    locator.validate().map_err(|error| error.to_string())?;
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
        NvidiaArtifactLocation::Official {
            host: NvidiaDownloadHost::International,
            path: server_path,
        },
    );
    let (_capability, artifact) = acquirer
        .acquire_verified(&locator, &target_gpu)
        .map_err(|error| error.to_string())?;
    let plan = generate_dry_run_plan(&DriverPlanInput {
        target_gpu: target_gpu.clone(),
        installed_packages,
        artifact: artifact.clone(),
        extensions: BTreeMap::new(),
    })
    .map_err(|error| error.to_string())?;
    let package_set_sha256 = frametime_driver::CanonicalPackageSet::from_unsorted(
        target_gpu.clone(),
        plan.entries
            .get(1)
            .and_then(|entry| match &entry.action {
                frametime_driver::PlannedDriverAction::RecordExactPackages { packages } => {
                    Some(packages.clone())
                }
                _ => None,
            })
            .ok_or("prepared plan lacks exact package record")?,
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
        artifact: ArtifactIdentity::from_descriptor(&artifact).map_err(|error| error.to_string())?,
        authorized_at_utc: authorized_at_utc.clone(),
        // A driver artifact is high-impact authority. Its 24-hour expiry is
        // calculated from the same trusted UTC clock used by execution.
        expires_at_utc: authorization_expiry_after(&authorized_at_utc)
            .map_err(|error| error.to_string())?,
    };
    let transaction = DriverTransaction::prepared(plan, artifact, authorization)?;
    persist_driver_transaction(work_dir, &transaction)
}

#[cfg(windows)]
struct NativeDriverClock;

#[cfg(windows)]
impl ExecutionClock for NativeDriverClock {
    fn current_utc(&self) -> Result<String, frametime_driver::AdapterFailure> {
        Ok(trusted_current_utc())
    }
}

#[cfg(windows)]
pub fn remove_prepared_nvidia_driver(work_dir: &std::path::Path) -> Result<DriverTransaction, String> {
    let mut transaction = load_driver_transaction(work_dir)?
        .ok_or("P2:2 requires durable P1:18/P1:19 NVIDIA transaction evidence")?;
    if transaction.removal.is_some() {
        return Ok(transaction);
    }
    let inspection = WindowsDriverInspection::native();
    let removal = PnpUtilDriverRemoval::new(NativeSystem32ToolRunner, inspection);
    let capture = frametime_driver::capture_driver_execution(
        &transaction.plan,
        &WindowsSafeModeInspection,
        &removal,
        &NativeDriverClock,
        CaptureFreshnessPolicy { maximum_age_seconds: 900 },
    )
    .map_err(|error| error.to_string())?;
    validate_driver_authorization(&transaction, &capture, &trusted_current_utc())?;
    let evidence = frametime_driver::remove_captured_packages(
        &transaction.plan,
        capture.clone(),
        &removal,
        &NativeDriverClock,
        CaptureFreshnessPolicy { maximum_age_seconds: 900 },
    )
    .map_err(|error| error.to_string())?;
    transaction.capture = Some(capture);
    transaction.removal = Some(evidence);
    persist_driver_transaction(work_dir, &transaction)
}

#[cfg(windows)]
pub fn install_prepared_nvidia_driver(work_dir: &std::path::Path) -> Result<DriverTransaction, String> {
    let mut transaction = load_driver_transaction(work_dir)?
        .ok_or("P3:1 requires durable P1:18/P1:19 NVIDIA transaction evidence")?;
    if transaction.installation.is_some() {
        return Ok(transaction);
    }
    let capture = transaction.capture.clone().ok_or("P3:1 requires retained P2:2 capture")?;
    if !transaction.removal_complete() {
        return Err("P3:1 requires coherent P2:2 removal evidence".into());
    }
    let now_utc = trusted_current_utc();
    validate_driver_authorization(&transaction, &capture, &now_utc)?;
    let store = NativeNvidiaArtifactStore::acquire_fixed_root().map_err(|error| error.to_string())?;
    let capability = store.acquire(
        &NvidiaArtifactLocation::LocalLeaf(transaction.artifact.locator.artifact_file_name.clone()),
        &format!("driver-artifacts/{}", transaction.artifact.locator.artifact_file_name),
        2 * 1024 * 1024 * 1024,
    )
    .map_err(|error| error.to_string())?;
    let installer = NvidiaInstaller::new(
        NativeNvidiaInstallerRunner,
        WindowsDriverInspection::native(),
        NativeNvidiaSignatureVerifier,
    );
    // The verifier records this clock value immediately before it launches the
    // retained capability, rather than reusing the earlier authorization check.
    let pre_launch_utc = trusted_current_utc();
    let (installed_artifact, post_install_packages, fresh_authenticode) = installer
        .install_and_reinspect(
            capability,
            &transaction.artifact,
            &transaction.authorization,
            &pre_launch_utc,
        )
        .map_err(|error| error.to_string())?;
    let installation = InstallationEvidence {
        authorization: transaction.authorization.clone(),
        fresh_authenticode,
        installed_artifact,
        post_install_packages: frametime_driver::CanonicalPackageSet::from_unsorted(
            transaction.plan.target_gpu.clone(), post_install_packages,
        )
        .map_err(|error| error.to_string())?,
        observed_at_utc: trusted_current_utc(),
    };
    installation
        .validate_for_plan_at(
            &transaction.plan,
            &capture,
            &transaction.artifact,
            CaptureFreshnessPolicy { maximum_age_seconds: 86_400 },
            &trusted_current_utc(),
        )
        .map_err(|error| error.to_string())?;
    transaction.installation = Some(installation);
    persist_driver_transaction(work_dir, &transaction)
}

#[cfg(windows)]
fn validate_driver_authorization(
    transaction: &DriverTransaction,
    capture: &frametime_driver::DriverExecutionCapture,
    now_utc: &str,
) -> Result<(), String> {
    transaction
        .authorization
        .validate_for_capture_at(capture, &transaction.artifact, now_utc)
        .map_err(|error| error.to_string())?;
    validate_bounded_nvidia_authorization(&transaction.authorization, &transaction.artifact, now_utc)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn trusted_current_utc() -> String {
    use windows::Win32::System::SystemInformation::GetSystemTime;

    let now = unsafe { GetSystemTime() };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}

#[cfg(not(windows))]
pub fn remove_prepared_nvidia_driver(
    _work_dir: &std::path::Path,
) -> Result<DriverTransaction, String> {
    Err("P2:2 driver removal is supported only on Windows Safe Mode".into())
}

#[cfg(not(windows))]
pub fn install_prepared_nvidia_driver(
    _work_dir: &std::path::Path,
) -> Result<DriverTransaction, String> {
    Err("P3:1 driver installation is supported only on Windows".into())
}

#[cfg(not(windows))]
pub fn prepare_nvidia_driver(
    _work_dir: &std::path::Path,
    _artifact_id: String,
    _artifact_file_name: String,
    _server_path: String,
) -> Result<DriverTransaction, String> {
    Err("prepare-nvidia is supported only on Windows".into())
}
