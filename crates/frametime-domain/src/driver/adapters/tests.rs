use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

use super::*;
use crate::driver::{
    AuthenticodeEvidence, AuthenticodeStatus, CanonicalPackageSet, GpuVendor,
    PackageRemovalDisposition, SafeModeState, Sha256Digest, generate_dry_run_plan,
};

const CAPTURED: &str = "2026-01-01T00:00:02Z";
const RESUMED: &str = "2026-01-01T00:20:00Z";
const NOW: &str = "2026-01-01T00:20:01Z";

struct FixedClock;

impl ExecutionClock for FixedClock {
    fn current_utc(&self) -> Result<String, AdapterFailure> {
        Ok(NOW.into())
    }
}

struct ConfirmedSafeMode;

impl SafeModeInspectionAdapter for ConfirmedSafeMode {
    fn observe_safe_mode(
        &self,
        target_gpu: &ExactGpuIdentity,
    ) -> Result<SafeModeObservation, AdapterFailure> {
        Ok(SafeModeObservation {
            target_gpu: target_gpu.clone(),
            state: SafeModeState::Confirmed,
            observed_at_utc: RESUMED.into(),
            boot_session_id: "resumed-safe-mode".into(),
        })
    }
}

struct MutableRemoval {
    packages: RefCell<Vec<PublishedDriverPackage>>,
    store_names: RefCell<BTreeSet<OemPublishedName>>,
    removal_calls: Cell<usize>,
}

impl MutableRemoval {
    fn new(packages: Vec<PublishedDriverPackage>) -> Self {
        let store_names = packages
            .iter()
            .map(|package| package.published_name.clone())
            .collect();
        Self {
            packages: RefCell::new(packages),
            store_names: RefCell::new(store_names),
            removal_calls: Cell::new(0),
        }
    }

    fn with_store_names(
        packages: Vec<PublishedDriverPackage>,
        store_names: BTreeSet<OemPublishedName>,
    ) -> Self {
        Self {
            packages: RefCell::new(packages),
            store_names: RefCell::new(store_names),
            removal_calls: Cell::new(0),
        }
    }
}

impl PackageExecutionAdapter for MutableRemoval {
    fn published_package_is_present(
        &self,
        _target_gpu: &ExactGpuIdentity,
        published_name: &OemPublishedName,
    ) -> Result<bool, AdapterFailure> {
        Ok(self.store_names.borrow().contains(published_name))
    }

    fn remove_published_package(
        &self,
        _target_gpu: &ExactGpuIdentity,
        expected: &PublishedDriverPackage,
    ) -> Result<PackageRemovalOutcome, AdapterFailure> {
        let published_name = &expected.published_name;
        self.removal_calls.set(self.removal_calls.get() + 1);
        let disposition = if self.store_names.borrow_mut().remove(published_name) {
            self.packages
                .borrow_mut()
                .retain(|package| package.published_name != *published_name);
            PackageRemovalDisposition::Removed
        } else {
            PackageRemovalDisposition::AlreadyAbsent
        };
        Ok(PackageRemovalOutcome {
            published_name: published_name.clone(),
            disposition,
            observed_at_utc: RESUMED.into(),
        })
    }

    fn inspect_published_packages(
        &self,
        _target_gpu: &ExactGpuIdentity,
    ) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
        Ok(self.packages.borrow().clone())
    }
}

fn package(target: &ExactGpuIdentity, name: &str) -> PublishedDriverPackage {
    PublishedDriverPackage {
        target_gpu: target.clone(),
        published_name: OemPublishedName::parse(name).expect("OEM name"),
        original_inf_name: "nv_dispi.inf".into(),
        provider_name: "NVIDIA".into(),
        driver_version: "1.0".into(),
        driver_store_package_sha256: Some(
            Sha256Digest::parse(if name == "oem1.inf" {
                "1".repeat(64)
            } else {
                "2".repeat(64)
            })
            .expect("store package digest"),
        ),
        extensions: BTreeMap::new(),
    }
}

fn fixture() -> (DryRunDriverPlan, DriverExecutionCapture) {
    let target = ExactGpuIdentity::new(GpuVendor::Nvidia, 0x2684, 0x10de, 1, 1);
    let packages = vec![package(&target, "oem1.inf"), package(&target, "oem2.inf")];
    let plan = generate_dry_run_plan(&DriverPlanInput {
        target_gpu: target.clone(),
        installed_packages: packages.clone(),
        artifact: SignedArtifactDescriptor {
            locator: ArtifactLocator {
                artifact_id: "nvidia-1".into(),
                artifact_file_name: "setup.exe".into(),
                extensions: BTreeMap::new(),
            },
            target_gpu: target.clone(),
            payload_sha256: Sha256Digest::parse("a".repeat(64)).expect("digest"),
            authenticode: AuthenticodeEvidence {
                status: AuthenticodeStatus::Valid,
                signer_subject: "NVIDIA".into(),
                signer_thumbprint_sha256: Sha256Digest::parse("b".repeat(64)).expect("signer"),
                observed_at_utc: "2026-01-01T00:00:00Z".into(),
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        },
        extensions: BTreeMap::new(),
    })
    .expect("driver plan");
    let installed_packages =
        CanonicalPackageSet::from_unsorted(target.clone(), packages).expect("package set");
    let capture = DriverExecutionCapture {
        schema_version: SCHEMA_VERSION,
        plan_sha256: plan.input_sha256.clone(),
        target_gpu: target.clone(),
        safe_mode: SafeModeObservation {
            target_gpu: target,
            state: SafeModeState::Confirmed,
            observed_at_utc: "2026-01-01T00:00:01Z".into(),
            boot_session_id: "original-safe-mode".into(),
        },
        package_set_sha256: installed_packages.fingerprint().expect("fingerprint"),
        installed_packages,
        captured_at_utc: CAPTURED.into(),
    };
    (plan, capture)
}

#[test]
fn armed_retry_accepts_absent_package_after_capture_window() {
    let (plan, capture) = fixture();
    let removal = MutableRemoval::new(vec![capture.installed_packages.packages[1].clone()]);
    let freshness = CaptureFreshnessPolicy {
        maximum_age_seconds: 900,
    };
    assert!(
        remove_captured_packages(&plan, capture.clone(), &removal, &FixedClock, freshness,)
            .is_err()
    );

    let evidence = resume_captured_package_removal(
        &plan,
        capture,
        &ConfirmedSafeMode,
        &removal,
        &FixedClock,
        freshness,
    )
    .expect("resume stale armed removal");
    assert!(evidence.resume_safe_mode.is_some());
    assert_eq!(
        evidence.outcomes[0].disposition,
        PackageRemovalDisposition::AlreadyAbsent
    );
    assert_eq!(removal.removal_calls.get(), 2);
}

#[test]
fn armed_retry_without_any_prior_removal_requires_live_authorization() {
    let (plan, capture) = fixture();
    let removal = MutableRemoval::new(capture.installed_packages.packages.clone());
    let error = resume_captured_package_removal(
        &plan,
        capture,
        &ConfirmedSafeMode,
        &removal,
        &FixedClock,
        CaptureFreshnessPolicy {
            maximum_age_seconds: 900,
        },
    )
    .expect_err("an unstarted cleanup must not use recovery authority");

    assert!(error.reason.contains("live authorization"));
    assert_eq!(removal.removal_calls.get(), 0);
}

#[test]
fn armed_retry_rejects_a_rebound_published_name_before_removal() {
    let (plan, capture) = fixture();
    let mut rebound = capture.installed_packages.packages[1].clone();
    rebound.driver_store_package_sha256 =
        Some(Sha256Digest::parse("3".repeat(64)).expect("replacement store digest"));
    let removal = MutableRemoval::new(vec![rebound]);
    let error = resume_captured_package_removal(
        &plan,
        capture,
        &ConfirmedSafeMode,
        &removal,
        &FixedClock,
        CaptureFreshnessPolicy {
            maximum_age_seconds: 900,
        },
    )
    .expect_err("a rebound OEM INF must fail closed");

    assert!(error.reason.contains("immutable capture"));
    assert_eq!(removal.removal_calls.get(), 0);
}

#[test]
fn armed_retry_rejects_store_presence_without_an_exact_target_record() {
    let (plan, capture) = fixture();
    let store_names = capture
        .installed_packages
        .packages
        .iter()
        .map(|package| package.published_name.clone())
        .collect();
    let removal = MutableRemoval::with_store_names(Vec::new(), store_names);
    let error = resume_captured_package_removal(
        &plan,
        capture,
        &ConfirmedSafeMode,
        &removal,
        &FixedClock,
        CaptureFreshnessPolicy {
            maximum_age_seconds: 900,
        },
    )
    .expect_err("Store presence without a rebound target record must fail closed");

    assert!(error.reason.contains("disagree"));
    assert_eq!(removal.removal_calls.get(), 0);
}
