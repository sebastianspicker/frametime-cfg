//! Native-driver adapters consume typed SetupAPI observations and typed
//! argv vectors.  They never parse human-facing PnPUtil output or execute a
//! shell.  The artifact source and signature verifier are injected because
//! retaining a protected file handle is part of the host-specific authority.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use frametime_domain::driver::SafeModeState;
use frametime_domain::driver::{
    AdapterFailure, ArtifactIdentity, ArtifactLocator, AuthenticodeEvidence, ExactGpuIdentity,
    GpuVendor, InspectionAdapter, OemPublishedName, PublishedDriverPackage,
    SafeModeInspectionAdapter, SafeModeObservation, Sha256Digest, SignedArtifactDescriptor,
};

#[cfg(windows)]
use crate::WindowsSetupApiEnumerator;
use crate::{PciDeviceClass, PciDeviceEnumerator, enumerate_present_status_ok_pci};

const NVIDIA_SUBJECT: &str = "CN=NVIDIA Corporation";
pub(super) const MAX_NVIDIA_ARTIFACT_BYTES: usize = 2 * 1024 * 1024 * 1024;
pub(super) const DRIVER_ARTIFACTS_LEAF: &str = "driver-artifacts";
const NVIDIA_SIGNER_SHA256: &[&str] =
    &["28af76241322f210da473d9569eff6f27124c4ca9f43933da547e8d068b0a95d"];

#[cfg(windows)]
mod native;

fn adapter(operation: &'static str, reason: impl Into<String>) -> AdapterFailure {
    AdapterFailure {
        operation,
        reason: reason.into(),
    }
}

#[cfg(windows)]
pub(crate) fn verify_microsoft_signed_tool(path: &Path) -> Result<(), AdapterFailure> {
    native::verify_microsoft_signed_path(path)
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedNvidiaLaunchEvidence {
    pub outcome: ProcessOutcome,
    pub artifact: ArtifactIdentity,
    pub authenticode: AuthenticodeEvidence,
}

#[cfg(windows)]
pub(crate) fn launch_prepared_nvidia_package(
    expected: &SignedArtifactDescriptor,
    expected_package_sha256: &Sha256Digest,
    observed_at_utc: &str,
    spawn: bool,
) -> Result<PreparedNvidiaLaunchEvidence, AdapterFailure> {
    native::launch_prepared_nvidia_package(
        expected,
        expected_package_sha256,
        observed_at_utc,
        spawn,
    )
}

#[cfg(windows)]
pub(crate) fn verify_basic_display_fallback() -> Result<(), AdapterFailure> {
    let path = NativeSystem32ToolRunner
        .system32()?
        .join("drivers")
        .join("BasicDisplay.sys");
    if !path.is_file() {
        return Err(adapter(
            "verify basic display fallback",
            "System32 drivers contains no BasicDisplay.sys",
        ));
    }
    native::verify_microsoft_signed_path(&path)
}

fn exact_gpu(
    binding: &frametime_domain::binding::PciDeviceBinding,
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
    bindings: Vec<(PciDeviceClass, frametime_domain::binding::PciDeviceBinding)>,
    target: &ExactGpuIdentity,
) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
    let mut packages = BTreeMap::new();
    for (class, binding) in bindings {
        if class != PciDeviceClass::Display || exact_gpu(&binding)? != *target {
            continue;
        }
        // The inbox basic display driver (e.g. after package removal) is not a vendor package.
        if binding.published_inf.eq_ignore_ascii_case("display.inf") {
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
            driver_store_package_sha256: None,
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
    fn bindings(
        &self,
    ) -> Result<Vec<(PciDeviceClass, frametime_domain::binding::PciDeviceBinding)>, AdapterFailure>
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
            frametime_domain::binding::PciDeviceBinding,
            Vec<frametime_domain::binding::PciDeviceBinding>,
        ),
        AdapterFailure,
    >
    where
        E: PciDeviceEnumerator,
    {
        let bindings = self.bindings()?;
        let (target_gpu, installed_packages) =
            super::cleanup_observation::from_bindings(bindings.clone(), exact_gpu)?;
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
        let packages = packages_for(self.bindings()?, target)?;
        #[cfg(windows)]
        return native::bind_driver_store_identities(packages);
        #[cfg(not(windows))]
        Ok(packages)
    }
}
/// Native Safe Mode observation with a per-observation tick stamp. The stamp is
/// not an authorization secret and is not a boot identity: it changes on every
/// call and consumers only require it to be non-empty.
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
            crate::system::platform::system_directory_with(|failure| match failure {
                crate::system::platform::SystemDirectoryFailure::Read => {
                    adapter("resolve PnPUtil", "GetSystemDirectoryW failed")
                }
                crate::system::platform::SystemDirectoryFailure::TooLarge => {
                    adapter("resolve PnPUtil", "System32 path is too large")
                }
            })
        }
        #[cfg(not(windows))]
        {
            Err(adapter("resolve PnPUtil", "supported only on Windows"))
        }
    }

    fn run(&self, executable: &Path, argv: &[String]) -> Result<ProcessOutcome, AdapterFailure> {
        #[cfg(windows)]
        {
            native::run_microsoft_signed_system_tool(executable, argv)
        }
        #[cfg(not(windows))]
        {
            let _ = (executable, argv);
            Err(adapter("run PnPUtil", "supported only on Windows"))
        }
    }
}

mod removal;
pub use removal::PnpUtilDriverRemoval;

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
#[cfg(windows)]
pub(crate) use artifact::trusted_utc_timestamp;
pub use artifact::{
    DriverArtifactStore, NativeNvidiaArtifactStore, NativeNvidiaSignatureVerifier,
    NvidiaSignatureVerifier, VerifiedDriverArtifact,
};
pub(crate) use artifact::{
    parse_utc_seconds, validate_bounded_nvidia_authorization,
    validate_nvidia_authorization_structure,
};

mod download;
pub use download::NvidiaArtifactAcquirer;
