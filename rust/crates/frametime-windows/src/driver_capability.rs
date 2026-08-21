//! Native-driver adapters consume typed SetupAPI observations and typed
//! argv vectors.  They never parse human-facing PnPUtil output or execute a
//! shell.  The artifact source and signature verifier are injected because
//! retaining a protected file handle is part of the host-specific authority.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use frametime_driver::SafeModeState;
use frametime_driver::{
    AdapterFailure, ArtifactAcquisitionAuthorization, ArtifactIdentity, ArtifactLocator,
    AuthenticodeEvidence, AuthenticodeStatus, ExactGpuIdentity, GpuVendor, InspectionAdapter,
    InstalledArtifactObservation, OemPublishedName, PackageExecutionAdapter,
    PackageRemovalDisposition, PackageRemovalOutcome, PublishedDriverPackage,
    SafeModeInspectionAdapter, SafeModeObservation, Sha256Digest, SignedArtifactDescriptor,
};

#[cfg(windows)]
use crate::WindowsSetupApiEnumerator;
use crate::{PciDeviceClass, PciDeviceEnumerator, enumerate_present_status_ok_pci};

const NVIDIA_SUBJECT: &str = "CN=NVIDIA Corporation";
const MAX_NVIDIA_ARTIFACT_BYTES: usize = 2 * 1024 * 1024 * 1024;
const DRIVER_ARTIFACTS_LEAF: &str = "driver-artifacts";

#[cfg(windows)]
mod native;

fn adapter(operation: &'static str, reason: impl Into<String>) -> AdapterFailure {
    AdapterFailure {
        operation,
        reason: reason.into(),
    }
}

fn exact_gpu(
    binding: &frametime_core::PciDeviceBinding,
) -> Result<ExactGpuIdentity, AdapterFailure> {
    let vendor = match binding.vendor_id {
        0x10de => GpuVendor::Nvidia,
        0x1002 => GpuVendor::Amd,
        0x8086 => GpuVendor::Intel,
        _ => {
            return Err(adapter(
                "inspect GPU",
                "display PnP identity has an unsupported PCI vendor",
            ));
        }
    };
    Ok(ExactGpuIdentity::new(
        vendor,
        binding.device_id,
        binding.subsystem_vendor_id,
        binding.subsystem_device_id,
        binding.revision_id,
    ))
}

fn packages_for(
    bindings: Vec<(PciDeviceClass, frametime_core::PciDeviceBinding)>,
    target: &ExactGpuIdentity,
) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
    let mut packages = BTreeMap::new();
    for (class, binding) in bindings {
        if class != PciDeviceClass::Display || exact_gpu(&binding)? != *target {
            continue;
        }
        let published_name = OemPublishedName::parse(binding.published_inf.to_ascii_lowercase())
            .map_err(|_| {
                adapter(
                    "inspect packages",
                    "display PnP binding has no canonical oem<N>.inf",
                )
            })?;
        let package = PublishedDriverPackage {
            target_gpu: target.clone(),
            published_name: published_name.clone(),
            // SetupAPI's driver-INF path is the installed package's native
            // identity.  The domain requires a leaf, so use that canonical
            // OEM INF rather than localized driver-store display metadata.
            original_inf_name: published_name.as_str().to_owned(),
            provider_name: binding.driver_provider,
            driver_version: binding.driver_version,
            extensions: BTreeMap::new(),
        };
        package
            .validate_for(target)
            .map_err(|e| adapter("inspect packages", e.to_string()))?;
        match packages.insert(published_name, package.clone()) {
            Some(existing) if existing != package => {
                return Err(adapter(
                    "inspect packages",
                    "conflicting PnP records bind one OEM INF",
                ));
            }
            _ => {}
        }
    }
    Ok(packages.into_values().collect())
}
/// SetupAPI-backed inspection.  A multi-GPU display system is rejected until
/// a controller supplies an explicit selected PnP identity; guessing would
/// permit a package to bind to the wrong adapter.
#[derive(Debug)]
pub struct WindowsDriverInspection<E> {
    enumerator: E,
}

#[cfg(windows)]
impl WindowsDriverInspection<WindowsSetupApiEnumerator> {
    #[must_use]
    pub fn native() -> Self {
        Self {
            enumerator: WindowsSetupApiEnumerator,
        }
    }
}

impl<E> WindowsDriverInspection<E> {
    #[must_use]
    pub fn with_enumerator(enumerator: E) -> Self {
        Self { enumerator }
    }

    fn bindings(
        &self,
    ) -> Result<Vec<(PciDeviceClass, frametime_core::PciDeviceBinding)>, AdapterFailure>
    where
        E: PciDeviceEnumerator,
    {
        enumerate_present_status_ok_pci(&self.enumerator)
            .map_err(|e| adapter("inspect PnP devices", e.to_string()))
    }

    pub fn inspect_driver_cleanup_preparation(
        &self,
    ) -> Result<
        (
            frametime_core::PciDeviceBinding,
            Vec<frametime_core::PciDeviceBinding>,
        ),
        AdapterFailure,
    >
    where
        E: PciDeviceEnumerator,
    {
        let bindings = self.bindings()?;
        let (target_gpu, installed_packages) =
            crate::driver_cleanup_observation::from_bindings(bindings.clone(), exact_gpu)?;
        packages_for(bindings, &exact_gpu(&target_gpu)?)?;
        Ok((target_gpu, installed_packages))
    }
}

impl<E: PciDeviceEnumerator> InspectionAdapter for WindowsDriverInspection<E> {
    fn inspect_exact_gpu(&self) -> Result<ExactGpuIdentity, AdapterFailure> {
        let mut candidates = Vec::new();
        for (class, binding) in self.bindings()? {
            if class == PciDeviceClass::Display {
                let gpu = exact_gpu(&binding)?;
                if !candidates.contains(&gpu) {
                    candidates.push(gpu);
                }
            }
        }
        match candidates.len() {
            1 => Ok(candidates.pop().expect("one GPU")),
            0 => Err(adapter(
                "inspect GPU",
                "no active status-OK PCI display GPU was found",
            )),
            _ => Err(adapter(
                "inspect GPU",
                "multiple display GPUs require an explicit PnP selection",
            )),
        }
    }

    fn inspect_published_packages(
        &self,
        target: &ExactGpuIdentity,
    ) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
        packages_for(self.bindings()?, target)
    }
}
/// Native Safe Mode observation with a boot-session token.  The token is not
/// an authorization secret; it prevents evidence from one boot being reused
/// after a reboot.
#[derive(Debug, Default)]
pub struct WindowsSafeModeInspection;

impl SafeModeInspectionAdapter for WindowsSafeModeInspection {
    fn observe_safe_mode(
        &self,
        target_gpu: &ExactGpuIdentity,
    ) -> Result<SafeModeObservation, AdapterFailure> {
        #[cfg(windows)]
        {
            use windows::Win32::{
                System::SystemInformation::GetTickCount64,
                UI::WindowsAndMessaging::{GetSystemMetrics, SM_CLEANBOOT},
            };
            let state = if unsafe { GetSystemMetrics(SM_CLEANBOOT) } == 0 {
                SafeModeState::NotDetected
            } else {
                SafeModeState::Confirmed
            };
            Ok(SafeModeObservation {
                target_gpu: target_gpu.clone(),
                state,
                observed_at_utc: crate::timestamp(),
                boot_session_id: format!("boot-ticks-{}", unsafe { GetTickCount64() }),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = target_gpu;
            Err(adapter("observe Safe Mode", "supported only on Windows"))
        }
    }
}

/// Result of an argv-only child process. Output is deliberately not retained:
/// it is localized display text and may not be used as mutation evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessOutcome {
    pub exit_code: Option<i32>,
}

pub trait System32ToolRunner {
    fn system32(&self) -> Result<PathBuf, AdapterFailure>;
    fn run(&self, executable: &Path, argv: &[String]) -> Result<ProcessOutcome, AdapterFailure>;
}

#[derive(Debug, Default)]
pub struct NativeSystem32ToolRunner;

impl System32ToolRunner for NativeSystem32ToolRunner {
    fn system32(&self) -> Result<PathBuf, AdapterFailure> {
        #[cfg(windows)]
        {
            use std::{ffi::OsString, os::windows::ffi::OsStringExt};
            use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
            let mut buffer = vec![0_u16; 260];
            loop {
                let copied = unsafe { GetSystemDirectoryW(Some(&mut buffer)) };
                if copied == 0 {
                    return Err(adapter("resolve PnPUtil", "GetSystemDirectoryW failed"));
                }
                let copied = usize::try_from(copied)
                    .map_err(|_| adapter("resolve PnPUtil", "System32 path is too large"))?;
                if copied < buffer.len() {
                    return Ok(PathBuf::from(OsString::from_wide(&buffer[..copied])));
                }
                buffer.resize(
                    copied
                        .checked_add(1)
                        .ok_or_else(|| adapter("resolve PnPUtil", "System32 path is too large"))?,
                    0,
                );
            }
        }
        #[cfg(not(windows))]
        {
            Err(adapter("resolve PnPUtil", "supported only on Windows"))
        }
    }

    fn run(&self, executable: &Path, argv: &[String]) -> Result<ProcessOutcome, AdapterFailure> {
        #[cfg(windows)]
        {
            let status = std::process::Command::new(executable)
                .args(argv)
                .status()
                .map_err(|e| adapter("run PnPUtil", e.to_string()))?;
            Ok(ProcessOutcome {
                exit_code: status.code(),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (executable, argv);
            Err(adapter("run PnPUtil", "supported only on Windows"))
        }
    }
}

fn pnputil_path(system32: &Path) -> Result<PathBuf, AdapterFailure> {
    if !system32.is_absolute()
        || system32
            .file_name()
            .is_none_or(|n| !n.eq_ignore_ascii_case("System32"))
    {
        return Err(adapter(
            "resolve PnPUtil",
            "trusted System32 path is not absolute",
        ));
    }
    Ok(system32.join("pnputil.exe"))
}

/// Typed PnPUtil removal with the only accepted argv vector.
pub struct PnpUtilDriverRemoval<R, E> {
    runner: R,
    inspection: WindowsDriverInspection<E>,
}

impl<R, E> PnpUtilDriverRemoval<R, E> {
    #[must_use]
    pub fn new(runner: R, inspection: WindowsDriverInspection<E>) -> Self {
        Self { runner, inspection }
    }
}

impl<R: System32ToolRunner, E: PciDeviceEnumerator> PackageExecutionAdapter
    for PnpUtilDriverRemoval<R, E>
{
    fn remove_published_package(
        &self,
        _target: &ExactGpuIdentity,
        name: &OemPublishedName,
    ) -> Result<PackageRemovalOutcome, AdapterFailure> {
        let executable = pnputil_path(&self.runner.system32()?)?;
        let argv = vec![
            "/delete-driver".into(),
            name.as_str().into(),
            "/uninstall".into(),
            "/force".into(),
        ];
        let result = self.runner.run(&executable, &argv)?;
        Ok(PackageRemovalOutcome {
            published_name: name.clone(),
            disposition: match result.exit_code {
                Some(0) => PackageRemovalDisposition::Removed,
                Some(_) | None => PackageRemovalDisposition::Failed {
                    reason: "PnPUtil returned a nonzero or unavailable exit status".into(),
                },
            },
            observed_at_utc: crate::timestamp(),
        })
    }

    fn inspect_published_packages(
        &self,
        target: &ExactGpuIdentity,
    ) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
        self.inspection.inspect_published_packages(target)
    }
}

/// Only NVIDIA's HTTPS download CDN is admitted.  A path is an opaque,
/// slash-normalized server path, never a caller-provided generic URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvidiaDownloadHost {
    International,
}
impl NvidiaDownloadHost {
    const fn authority(&self) -> &'static str {
        "international.download.nvidia.com"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvidiaArtifactLocation {
    Official {
        host: NvidiaDownloadHost,
        path: String,
    },
    LocalLeaf(String),
}

impl NvidiaArtifactLocation {
    fn validate(&self, locator: &ArtifactLocator) -> Result<(), AdapterFailure> {
        match self {
            Self::Official { host, path }
                if host.authority() == "international.download.nvidia.com"
                    && path.starts_with('/')
                    && !path.contains(['\\', '?', '#'])
                    && path.ends_with(&locator.artifact_file_name) =>
            {
                Ok(())
            }
            Self::LocalLeaf(leaf)
                if leaf == &locator.artifact_file_name
                    && Path::new(leaf).components().count() == 1 =>
            {
                Ok(())
            }
            _ => Err(adapter(
                "acquire NVIDIA artifact",
                "artifact location violates the NVIDIA source policy",
            )),
        }
    }
}

mod artifact;
#[cfg(windows)]
pub(crate) use artifact::authorization_expiry_after;
pub(crate) use artifact::validate_bounded_nvidia_authorization;
pub use artifact::{
    DriverArtifactStore, NativeNvidiaArtifactStore, NativeNvidiaInstallerRunner,
    NativeNvidiaSignatureVerifier, NvidiaInstallerRunner, NvidiaSignatureVerifier,
    VerifiedDriverArtifact,
};

/// Authenticated NVIDIA installer capability. It is intentionally not a
/// generic executable launcher: the fixed argument vector is the complete
/// supported silent-install surface.
pub struct NvidiaInstaller<R, E, V> {
    runner: R,
    inspection: WindowsDriverInspection<E>,
    verifier: V,
}

impl<R, E, V> NvidiaInstaller<R, E, V> {
    #[must_use]
    pub fn new(runner: R, inspection: WindowsDriverInspection<E>, verifier: V) -> Self {
        Self {
            runner,
            inspection,
            verifier,
        }
    }

    /// Installs and immediately returns fresh PnP package evidence. The
    /// controller binds it to authorization/capture using InstallationEvidence.
    pub fn install_and_reinspect(
        &self,
        artifact_capability: VerifiedDriverArtifact,
        artifact: &SignedArtifactDescriptor,
        authorization: &ArtifactAcquisitionAuthorization,
        now_utc: &str,
    ) -> Result<
        (
            InstalledArtifactObservation,
            Vec<PublishedDriverPackage>,
            AuthenticodeEvidence,
        ),
        AdapterFailure,
    >
    where
        R: NvidiaInstallerRunner,
        E: PciDeviceEnumerator,
        V: NvidiaSignatureVerifier,
    {
        let (installed, fresh_authenticode) =
            self.install_descriptor(artifact_capability, artifact, authorization, now_utc)?;
        let packages = self
            .inspection
            .inspect_published_packages(&artifact.target_gpu)?;
        if packages.is_empty() {
            return Err(adapter(
                "install NVIDIA artifact",
                "post-install SetupAPI inventory contains no target-GPU package",
            ));
        }
        Ok((installed, packages, fresh_authenticode))
    }

    fn install_descriptor(
        &self,
        artifact_capability: VerifiedDriverArtifact,
        artifact: &SignedArtifactDescriptor,
        authorization: &ArtifactAcquisitionAuthorization,
        now_utc: &str,
    ) -> Result<(InstalledArtifactObservation, AuthenticodeEvidence), AdapterFailure>
    where
        R: NvidiaInstallerRunner,
        V: NvidiaSignatureVerifier,
    {
        artifact
            .validate_for(&artifact.target_gpu)
            .map_err(|e| adapter("install NVIDIA artifact", e.to_string()))?;
        if artifact.target_gpu.vendor != GpuVendor::Nvidia
            || artifact.authenticode.signer_subject != NVIDIA_SUBJECT
        {
            return Err(adapter(
                "install driver",
                "AMD and Intel installation are unsupported without a signed artifact policy",
            ));
        }
        validate_bounded_nvidia_authorization(authorization, artifact, now_utc)?;
        artifact_capability.validate_descriptor(artifact)?;
        let argv = vec!["-s".into(), "-noreboot".into()];
        let fresh_authenticode = artifact::verify_nvidia_signature_against_policy(
            &self.verifier,
            &artifact_capability,
            artifact,
            now_utc,
        )?;
        if self
            .runner
            .launch_verified(&artifact_capability, &argv)?
            .exit_code
            != Some(0)
        {
            return Err(adapter(
                "install NVIDIA artifact",
                "NVIDIA installer returned a nonzero or unavailable exit status",
            ));
        }
        Ok((
            InstalledArtifactObservation {
                artifact: ArtifactIdentity::from_descriptor(artifact)
                    .map_err(|e| adapter("install NVIDIA artifact", e.to_string()))?,
                observed_at_utc: now_utc.into(),
            },
            fresh_authenticode,
        ))
    }
}

const NVIDIA_SIGNER_SHA256: &[&str] =
    &["28af76241322f210da473d9569eff6f27124c4ca9f43933da547e8d068b0a95d"];

/// The signer pin set is compiled into this crate. This zero-sized marker is
/// retained as an API name but deliberately has no caller-supplied constructor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NvidiaArtifactPolicy;
impl NvidiaArtifactPolicy {
    #[must_use]
    pub const fn compiled() -> Self {
        Self
    }
}
pub struct NvidiaArtifactAcquirer<S, V> {
    store: S,
    verifier: V,
    location: NvidiaArtifactLocation,
}
impl<S, V> NvidiaArtifactAcquirer<S, V> {
    #[must_use]
    pub fn new(store: S, verifier: V, location: NvidiaArtifactLocation) -> Self {
        Self {
            store,
            verifier,
            location,
        }
    }
}

impl<S: DriverArtifactStore, V: NvidiaSignatureVerifier> NvidiaArtifactAcquirer<S, V> {
    pub fn acquire_verified(
        &self,
        locator: &ArtifactLocator,
        target: &ExactGpuIdentity,
    ) -> Result<(VerifiedDriverArtifact, SignedArtifactDescriptor), AdapterFailure> {
        locator
            .validate()
            .map_err(|e| adapter("acquire NVIDIA artifact", e.to_string()))?;
        if target.vendor != GpuVendor::Nvidia {
            return Err(adapter(
                "acquire driver",
                "AMD and Intel installation are unsupported without a signed artifact policy",
            ));
        }
        self.location.validate(locator)?;
        let protected_leaf = format!("{DRIVER_ARTIFACTS_LEAF}/{}", locator.artifact_file_name);
        let artifact =
            self.store
                .acquire(&self.location, &protected_leaf, MAX_NVIDIA_ARTIFACT_BYTES)?;
        if artifact.protected_leaf != protected_leaf
            || artifact.length == 0
            || artifact.length as usize > MAX_NVIDIA_ARTIFACT_BYTES
        {
            return Err(adapter(
                "acquire NVIDIA artifact",
                "artifact length is outside the bounded policy",
            ));
        }
        let (subject, thumbprint) = self.verifier.verify_nvidia(&artifact)?;
        if !artifact::accepts_compiled_nvidia_policy(&subject, &thumbprint) {
            return Err(adapter(
                "acquire NVIDIA artifact",
                "WinVerifyTrust signer violates exact NVIDIA policy",
            ));
        }
        let signer = Sha256Digest::parse(thumbprint.to_ascii_lowercase())
            .map_err(|e| adapter("verify NVIDIA artifact", e.to_string()))?;
        let descriptor = SignedArtifactDescriptor {
            locator: locator.clone(),
            target_gpu: target.clone(),
            payload_sha256: artifact.payload_sha256.clone(),
            authenticode: AuthenticodeEvidence {
                status: AuthenticodeStatus::Valid,
                signer_subject: subject,
                signer_thumbprint_sha256: signer,
                observed_at_utc: artifact::trusted_utc_timestamp(),
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        };
        artifact.revalidate()?;
        Ok((artifact, descriptor))
    }
}
