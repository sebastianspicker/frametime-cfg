use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::{AtomicU8, Ordering}},
};

use frametime_driver::{
    AdapterFailure, ArtifactAcquisitionAuthorization, ArtifactIdentity, ArtifactLocator,
    AuthenticodeEvidence, AuthenticodeStatus, ExactGpuIdentity, GpuVendor as DriverGpuVendor,
    InspectionAdapter, OemPublishedName, PackageExecutionAdapter, SCHEMA_VERSION, Sha256Digest,
    SignedArtifactDescriptor,
};

use crate::driver_capability::VerifiedDriverArtifact;
use crate::driver_capability::authorization_expiry_after;
use crate::{
    DriverArtifactStore, NvidiaArtifactAcquirer, NvidiaArtifactLocation, NvidiaDownloadHost,
    NvidiaInstaller, NvidiaInstallerRunner, NvidiaSignatureVerifier, PnpUtilDriverRemoval,
    ProcessOutcome, System32ToolRunner, WindowsDriverInspection,
};

#[derive(Clone)]
struct Devices(Vec<crate::PciDeviceObservation>);
impl crate::PciDeviceEnumerator for Devices {
    fn enumerate_pci_devices(
        &self,
    ) -> Result<Vec<crate::PciDeviceObservation>, crate::DeviceBindingError> {
        Ok(self.0.clone())
    }
}
fn gpu() -> ExactGpuIdentity {
    ExactGpuIdentity::new(DriverGpuVendor::Nvidia, 0x2684, 0x1043, 0x1234, 1)
}
fn display() -> crate::PciDeviceObservation {
    crate::PciDeviceObservation {
        present: true,
        status_ok: true,
        binding: frametime_core::PciDeviceBinding {
            schema_version: frametime_core::NATIVE_BINDING_SCHEMA_VERSION,
            instance_id: "PCI\\VEN_10DE&DEV_2684&SUBSYS_12341043&REV_01\\1".into(),
            container_id: "{00000000-0000-0000-0000-000000000001}".into(),
            class_guid: crate::PciDeviceClass::Display.class_guid().into(),
            vendor_id: 0x10de,
            device_id: 0x2684,
            subsystem_vendor_id: 0x1043,
            subsystem_device_id: 0x1234,
            revision_id: 1,
            driver_provider: "NVIDIA Corporation".into(),
            driver_version: "1.2.3.4".into(),
            published_inf: "oem42.inf".into(),
            observed_at_utc: "2026-08-13T10:00:00Z".into(),
            unknown: Default::default(),
        },
    }
}

#[test]
fn inspection_binds_only_the_setupapi_display_identity() {
    let inspection = WindowsDriverInspection::with_enumerator(Devices(vec![display()]));
    let target = inspection.inspect_exact_gpu().expect("target");
    assert_eq!(target, gpu());
    assert_eq!(
        inspection
            .inspect_published_packages(&target)
            .expect("packages")[0]
            .published_name
            .as_str(),
        "oem42.inf"
    );
}

#[test]
fn p1_18_captures_one_native_nvidia_binding_without_gpu_input() {
    let inspection = WindowsDriverInspection::with_enumerator(Devices(vec![display()]));
    let (target, packages) = inspection
        .inspect_driver_cleanup_preparation()
        .expect("P1:18 observation");

    assert_eq!(target.vendor_id, 0x10de);
    assert_eq!(packages, vec![target]);
}

#[test]
fn p1_18_refuses_multiple_pnp_bindings_with_the_same_gpu_identity() {
    let mut second = display();
    second.binding.instance_id = "PCI\\VEN_10DE&DEV_2684&SUBSYS_12341043&REV_01\\2".into();
    let inspection = WindowsDriverInspection::with_enumerator(Devices(vec![display(), second]));

    assert!(inspection.inspect_driver_cleanup_preparation().is_err());
}

#[test]
fn p1_18_rejects_a_noncanonical_installed_package_binding() {
    let mut invalid = display();
    invalid.binding.published_inf = "nvlddmkm.inf".into();
    let inspection = WindowsDriverInspection::with_enumerator(Devices(vec![invalid]));

    assert!(inspection.inspect_driver_cleanup_preparation().is_err());
}

#[test]
fn p1_18_reobservation_ignores_timestamp_but_rejects_package_substitution() {
    let (_, packages) = WindowsDriverInspection::with_enumerator(Devices(vec![display()]))
        .inspect_driver_cleanup_preparation()
        .expect("P1:18 observation");
    let target = packages[0].clone();
    let mut reobserved = target.clone();
    reobserved.observed_at_utc = "2026-08-13T10:01:00Z".into();
    assert!(same_driver_cleanup_preparation(
        &target,
        &packages,
        &reobserved,
        &[reobserved.clone()],
    ));

    reobserved.published_inf = "oem43.inf".into();
    assert!(!same_driver_cleanup_preparation(
        &target,
        &packages,
        &reobserved,
        &[reobserved.clone()],
    ));
}

#[test]
fn p1_18_native_non_nvidia_identity_is_inapplicable() {
    let mut target = display().binding;
    target.vendor_id = 0x1002;

    assert_eq!(
        driver_cleanup_preparation_inspection(&target),
        Inspection::Inapplicable
    );
}

#[derive(Default)]
struct Runner {
    argv: Mutex<Vec<String>>,
}
impl System32ToolRunner for Runner {
    fn system32(&self) -> Result<PathBuf, AdapterFailure> {
        Ok(PathBuf::from("/Windows/System32"))
    }
    fn run(&self, executable: &Path, argv: &[String]) -> Result<ProcessOutcome, AdapterFailure> {
        assert_eq!(executable, Path::new("/Windows/System32/pnputil.exe"));
        *self.argv.lock().expect("lock") = argv.to_vec();
        Ok(ProcessOutcome { exit_code: Some(0) })
    }
}
#[test]
fn pnputil_uses_fixed_argv_and_never_parses_output() {
    let removal = PnpUtilDriverRemoval::new(
        Runner::default(),
        WindowsDriverInspection::with_enumerator(Devices(vec![display()])),
    );
    let outcome = removal
        .remove_published_package(&gpu(), &OemPublishedName::parse("oem42.inf").expect("name"))
        .expect("removal");
    assert!(matches!(
        outcome.disposition,
        frametime_driver::PackageRemovalDisposition::Removed
    ));
}

struct Store {
    leaf: String,
    bytes: Vec<u8>,
}
impl DriverArtifactStore for Store {
    fn acquire(
        &self,
        _: &NvidiaArtifactLocation,
        _: &str,
        _: usize,
    ) -> Result<VerifiedDriverArtifact, AdapterFailure> {
        Ok(VerifiedDriverArtifact::for_test(
            &self.leaf,
            self.bytes.clone(),
        ))
    }
}
struct FailingStore(&'static str);
impl DriverArtifactStore for FailingStore {
    fn acquire(
        &self,
        _: &NvidiaArtifactLocation,
        _: &str,
        _: usize,
    ) -> Result<VerifiedDriverArtifact, AdapterFailure> {
        Err(AdapterFailure {
            operation: "download NVIDIA artifact",
            reason: self.0.into(),
        })
    }
}
struct Signer {
    subject: &'static str,
    thumbprint: String,
}
impl NvidiaSignatureVerifier for Signer {
    fn verify_nvidia(
        &self,
        _: &VerifiedDriverArtifact,
    ) -> Result<(String, String), AdapterFailure> {
        Ok((self.subject.into(), self.thumbprint.clone()))
    }
}
fn location() -> NvidiaArtifactLocation {
    NvidiaArtifactLocation::Official {
        host: NvidiaDownloadHost::International,
        path: "/drivers/setup.exe".into(),
    }
}
fn locator() -> ArtifactLocator {
    ArtifactLocator {
        artifact_id: "nvidia-1".into(),
        artifact_file_name: "setup.exe".into(),
        extensions: Default::default(),
    }
}
fn trusted_acquirer(
    store: impl DriverArtifactStore,
) -> NvidiaArtifactAcquirer<impl DriverArtifactStore, Signer> {
    NvidiaArtifactAcquirer::new(
        store,
        Signer {
            subject: "CN=NVIDIA Corporation",
            thumbprint: "ab".repeat(32),
        },
        location(),
    )
}

#[test]
fn acquisition_rejects_substitution_signer_mismatch_and_download_failures() {
    let wrong_leaf = trusted_acquirer(Store {
        leaf: "driver-artifacts/other.exe".into(),
        bytes: b"signed".to_vec(),
    });
    assert!(wrong_leaf.acquire_verified(&locator(), &gpu()).is_err());
    let wrong_signer = NvidiaArtifactAcquirer::new(
        Store {
            leaf: "driver-artifacts/setup.exe".into(),
            bytes: b"signed".to_vec(),
        },
        Signer {
            subject: "CN=Not NVIDIA",
            thumbprint: "ab".repeat(32),
        },
        location(),
    );
    assert!(wrong_signer.acquire_verified(&locator(), &gpu()).is_err());
    for reason in [
        "redirect response is forbidden",
        "response exceeds bounded policy",
    ] {
        let acquirer = trusted_acquirer(FailingStore(reason));
        assert!(acquirer
            .acquire_verified(&locator(), &gpu())
            .unwrap_err()
            .reason
            .contains(reason));
    }
}

struct Installer {
    launches: Arc<AtomicU8>,
}
impl NvidiaInstallerRunner for Installer {
    fn launch_verified(
        &self,
        artifact: &VerifiedDriverArtifact,
        argv: &[String],
    ) -> Result<ProcessOutcome, AdapterFailure> {
        artifact.revalidate()?;
        assert_eq!(argv, ["-s", "-noreboot"]);
        self.launches.fetch_add(1, Ordering::SeqCst);
        Ok(ProcessOutcome { exit_code: Some(0) })
    }
}
fn descriptor(capability: &VerifiedDriverArtifact) -> SignedArtifactDescriptor {
    SignedArtifactDescriptor {
        locator: locator(),
        target_gpu: gpu(),
        payload_sha256: capability.payload_sha256().clone(),
        authenticode: AuthenticodeEvidence {
            status: AuthenticodeStatus::Valid,
            signer_subject: "CN=NVIDIA Corporation".into(),
            signer_thumbprint_sha256: Sha256Digest::parse("ab".repeat(32)).expect("signer"),
            observed_at_utc: "2026-08-13T10:00:00Z".into(),
            extensions: Default::default(),
        },
        extensions: Default::default(),
    }
}

fn authorization(
    descriptor: &SignedArtifactDescriptor,
    expires_at_utc: &str,
) -> ArtifactAcquisitionAuthorization {
    ArtifactAcquisitionAuthorization {
        schema_version: SCHEMA_VERSION,
        authorization_id: "nvidia-test".into(),
        plan_sha256: Sha256Digest::parse("cd".repeat(32)).expect("plan"),
        target_gpu: gpu(),
        package_set_sha256: Sha256Digest::parse("ef".repeat(32)).expect("packages"),
        artifact: ArtifactIdentity::from_descriptor(descriptor).expect("artifact identity"),
        authorized_at_utc: "2026-08-13T10:00:00Z".into(),
        expires_at_utc: expires_at_utc.into(),
    }
}

fn installer_signer() -> Signer {
    Signer {
        subject: "CN=NVIDIA Corporation",
        thumbprint: "ab".repeat(32),
    }
}

#[test]
fn installer_rejects_stale_digest_before_launch() {
    let mut artifact =
        VerifiedDriverArtifact::for_test("driver-artifacts/setup.exe", b"signed".to_vec());
    let descriptor = descriptor(&artifact);
    artifact.replace_test_bytes(b"substituted".to_vec());
    let installer = NvidiaInstaller::new(
        Installer {
            launches: Arc::new(AtomicU8::new(0)),
        },
        WindowsDriverInspection::with_enumerator(Devices(vec![display()])),
        installer_signer(),
    );
    assert!(installer
        .install_and_reinspect(
            artifact,
            &descriptor,
            &authorization(&descriptor, "2026-08-13T10:24:00Z"),
            "2026-08-13T10:01:00Z",
        )
        .is_err());
}

#[test]
fn installer_consumes_matching_capability_and_reinspects() {
    let artifact =
        VerifiedDriverArtifact::for_test("driver-artifacts/setup.exe", b"signed".to_vec());
    let descriptor = descriptor(&artifact);
    let installer = NvidiaInstaller::new(
        Installer {
            launches: Arc::new(AtomicU8::new(0)),
        },
        WindowsDriverInspection::with_enumerator(Devices(vec![display()])),
        installer_signer(),
    );
    let (_, _, fresh_authenticode) = installer
        .install_and_reinspect(
            artifact,
            &descriptor,
            &authorization(&descriptor, "2026-08-13T10:24:00Z"),
            "2026-08-13T10:01:00Z",
        )
        .expect("installer evidence");
    assert_eq!(fresh_authenticode.status, AuthenticodeStatus::Valid);
    assert_eq!(fresh_authenticode.signer_subject, descriptor.authenticode.signer_subject);
    assert_eq!(
        fresh_authenticode.signer_thumbprint_sha256,
        descriptor.authenticode.signer_thumbprint_sha256
    );
    assert_eq!(fresh_authenticode.observed_at_utc, "2026-08-13T10:01:00Z");
}

#[test]
fn installer_rejects_expired_or_indefinite_authorization_before_runner() {
    for expiry in ["9999-12-31T23:59:59Z", "2026-08-13T10:00:00Z"] {
        let artifact =
            VerifiedDriverArtifact::for_test("driver-artifacts/setup.exe", b"signed".to_vec());
        let descriptor = descriptor(&artifact);
        let launches = Arc::new(AtomicU8::new(0));
        let installer = NvidiaInstaller::new(
            Installer {
                launches: Arc::clone(&launches),
            },
            WindowsDriverInspection::with_enumerator(Devices(vec![display()])),
            installer_signer(),
        );

        assert!(installer
            .install_and_reinspect(
                artifact,
                &descriptor,
                &authorization(&descriptor, expiry),
                "2026-08-13T10:01:00Z",
            )
            .is_err());
        assert_eq!(launches.load(Ordering::SeqCst), 0, "runner must not launch for {expiry}");
    }
}

#[test]
fn authorization_expiry_is_exactly_bounded_to_24_hours() {
    assert_eq!(
        authorization_expiry_after("2026-08-13T10:00:00Z").expect("expiry"),
        "2026-08-14T10:00:00Z"
    );
}

#[test]
fn installer_rechecks_the_compiled_signer_policy_before_runner() {
    let artifact =
        VerifiedDriverArtifact::for_test("driver-artifacts/setup.exe", b"signed".to_vec());
    let descriptor = descriptor(&artifact);
    let launches = Arc::new(AtomicU8::new(0));
    let installer = NvidiaInstaller::new(
        Installer {
            launches: Arc::clone(&launches),
        },
        WindowsDriverInspection::with_enumerator(Devices(vec![display()])),
        Signer {
            subject: "CN=Not NVIDIA",
            thumbprint: "ab".repeat(32),
        },
    );

    assert!(installer
        .install_and_reinspect(
            artifact,
            &descriptor,
            &authorization(&descriptor, "2026-08-13T10:24:00Z"),
            "2026-08-13T10:01:00Z",
        )
        .is_err());
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}
