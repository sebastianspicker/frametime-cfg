use std::collections::BTreeMap;

use frametime_driver::{
    ArtifactAcquisitionAuthorization, ArtifactIdentity, ArtifactLocator, AuthenticodeEvidence,
    AuthenticodeStatus, CanonicalPackageSet, CaptureFreshnessPolicy, CaptureReceipt,
    DriverExecutionCapture, DriverPlanInput, DriverPlanStep, ExactGpuIdentity, GpuVendor,
    InstallationEvidence, InstalledArtifactObservation, OemPublishedName,
    PackageRemovalDisposition, PackageRemovalOutcome, PublishedDriverPackage,
    RemovalExecutionEvidence, SCHEMA_VERSION, SafeModeObservation, SafeModeState, Sha256Digest,
    SignedArtifactDescriptor, ValidationError, generate_dry_run_plan, validate_capture_binding,
};
use serde_json::{Value, json};

fn digest(character: char) -> Sha256Digest {
    Sha256Digest::parse(character.to_string().repeat(64)).expect("test digest")
}

fn gpu() -> ExactGpuIdentity {
    ExactGpuIdentity::new(GpuVendor::Nvidia, 0x2684, 0x1458, 0x40a7, 1)
}

fn input() -> DriverPlanInput {
    let target_gpu = gpu();
    DriverPlanInput {
        target_gpu: target_gpu.clone(),
        installed_packages: vec![PublishedDriverPackage {
            target_gpu: target_gpu.clone(),
            published_name: OemPublishedName::parse("oem12.inf").expect("OEM name"),
            original_inf_name: "nv_disp.inf".into(),
            provider_name: "NVIDIA Corporation".into(),
            driver_version: "580.1".into(),
            extensions: BTreeMap::new(),
        }],
        artifact: SignedArtifactDescriptor {
            locator: ArtifactLocator {
                artifact_id: "nvidia-580-1".into(),
                artifact_file_name: "driver-package.exe".into(),
                extensions: BTreeMap::new(),
            },
            target_gpu,
            payload_sha256: digest('a'),
            authenticode: AuthenticodeEvidence {
                status: AuthenticodeStatus::Valid,
                signer_subject: "CN=Example Signer".into(),
                signer_thumbprint_sha256: digest('b'),
                observed_at_utc: "2026-08-10T10:00:00Z".into(),
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        },
        extensions: BTreeMap::new(),
    }
}

fn package_set() -> CanonicalPackageSet {
    let request = input();
    CanonicalPackageSet::from_unsorted(request.target_gpu, request.installed_packages)
        .expect("canonical packages")
}

fn execution_capture() -> (frametime_driver::DryRunDriverPlan, DriverExecutionCapture) {
    let plan = generate_dry_run_plan(&input()).expect("plan");
    let packages = package_set();
    let capture = DriverExecutionCapture {
        schema_version: SCHEMA_VERSION,
        plan_sha256: plan.input_sha256.clone(),
        target_gpu: plan.target_gpu.clone(),
        safe_mode: SafeModeObservation {
            target_gpu: plan.target_gpu.clone(),
            state: SafeModeState::Confirmed,
            observed_at_utc: "2026-08-10T10:00:00Z".into(),
            boot_session_id: "boot-123".into(),
        },
        package_set_sha256: packages.fingerprint().expect("fingerprint"),
        installed_packages: packages,
        captured_at_utc: "2026-08-10T10:01:00Z".into(),
    };
    (plan, capture)
}

fn authorization(
    capture: &DriverExecutionCapture,
    request: &DriverPlanInput,
) -> ArtifactAcquisitionAuthorization {
    ArtifactAcquisitionAuthorization {
        schema_version: SCHEMA_VERSION,
        authorization_id: "operator-approval-1".into(),
        plan_sha256: capture.plan_sha256.clone(),
        target_gpu: capture.target_gpu.clone(),
        package_set_sha256: capture.package_set_sha256.clone(),
        artifact: ArtifactIdentity::from_descriptor(&request.artifact).expect("identity"),
        authorized_at_utc: "2026-08-10T10:01:01Z".into(),
        expires_at_utc: "2026-08-10T10:30:00Z".into(),
    }
}

#[test]
fn rejects_traversal_unc_and_noncanonical_oem_names() {
    for value in [
        "../oem1.inf",
        "oem1.inf/child",
        r"\\host\share\oem1.inf",
        r"C:\oem1.inf",
        "OEM1.INF",
        "oem.inf",
        "oem1.inf;other",
    ] {
        assert!(OemPublishedName::parse(value).is_err(), "accepted {value}");
    }
}

#[test]
fn artifact_locator_validate_enforces_leaf_boundaries() {
    let cases = [
        ("128-byte leaf", "a".repeat(128), Ok(())),
        ("128-byte multibyte leaf", "é".repeat(64), Ok(())),
        (
            "130-byte multibyte leaf",
            "é".repeat(65),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "129-byte leaf",
            "a".repeat(129),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "empty leaf",
            "".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "current-directory leaf",
            ".".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "parent-directory leaf",
            "..".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "forward slash",
            "driver/package.inf".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "backslash",
            r"driver\package.inf".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "colon",
            "C:package.inf".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "control character",
            "driver\u{0000}.inf".into(),
            Err(ValidationError::Invalid {
                field: "artifactFileName",
            }),
        ),
        (
            "representative artifact",
            "driver-package.exe".into(),
            Ok(()),
        ),
    ];

    for (label, artifact_file_name, expected) in cases {
        let locator = ArtifactLocator {
            artifact_id: "nvidia-580-1".into(),
            artifact_file_name,
            extensions: BTreeMap::new(),
        };
        assert_eq!(locator.validate(), expected, "{label}");
    }
}

#[test]
fn oem_published_name_parse_enforces_leaf_boundaries() {
    let cases = [
        (
            "128-byte leaf",
            format!("oem{}.inf", "1".repeat(121)),
            Ok(()),
        ),
        (
            "129-byte leaf",
            format!("oem{}.inf", "1".repeat(122)),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        (
            "current-directory leaf",
            ".".into(),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        (
            "parent-directory leaf",
            "..".into(),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        (
            "forward slash",
            "oem1.inf/child".into(),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        (
            "backslash",
            r"oem1.inf\child".into(),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        (
            "colon",
            "oem1:alt.inf".into(),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        (
            "control character",
            "oem1\u{0000}.inf".into(),
            Err(ValidationError::Invalid {
                field: "publishedName",
            }),
        ),
        ("representative OEM name", "oem12.inf".into(), Ok(())),
    ];

    for (label, value, expected) in cases {
        assert_eq!(
            OemPublishedName::parse(value).map(|_| ()),
            expected,
            "{label}"
        );
    }
}

#[test]
fn rejects_malformed_digest_and_vendor_spoofing_during_json_decode() {
    assert!(Sha256Digest::parse("A".repeat(64)).is_err());
    let mut forged = serde_json::to_value(input()).expect("input JSON");
    forged["targetGpu"]["pciVendorId"] = json!(0x1002_u16);
    let parsed: DriverPlanInput = serde_json::from_value(forged).expect("syntactic JSON");
    assert_eq!(parsed.validate(), Err(ValidationError::VendorMismatch));
}

#[test]
fn rejects_package_for_a_different_exact_device_even_with_same_vendor() {
    let mut request = input();
    request.installed_packages[0].target_gpu.pci_device_id = 0x2685;
    assert_eq!(request.validate(), Err(ValidationError::PackageGpuMismatch));
}

#[test]
fn rejects_non_valid_signature_evidence_and_artifact_paths() {
    let mut request = input();
    request.artifact.authenticode.status = AuthenticodeStatus::Indeterminate;
    assert_eq!(
        request.validate(),
        Err(ValidationError::InvalidSignatureEvidence)
    );
    request.artifact.authenticode.status = AuthenticodeStatus::Valid;
    request.artifact.locator.artifact_file_name = r"\\host\share\artifact.bin".into();
    assert!(request.validate().is_err());
}

#[test]
fn plan_is_deterministic_ordered_and_read_only() {
    let mut first = input();
    let mut second_package = first.installed_packages[0].clone();
    second_package.published_name = OemPublishedName::parse("oem2.inf").expect("OEM name");
    first.installed_packages.push(second_package.clone());
    let mut reversed = first.clone();
    reversed.installed_packages.reverse();
    let left = generate_dry_run_plan(&first).expect("first plan");
    let right = generate_dry_run_plan(&reversed).expect("reversed plan");
    assert_eq!(left, right);
    assert!(left.read_only);
    assert_eq!(
        left.entries
            .iter()
            .map(|entry| entry.step)
            .collect::<Vec<_>>(),
        vec![
            DriverPlanStep::P1_18,
            DriverPlanStep::P1_19,
            DriverPlanStep::P2_2,
            DriverPlanStep::P3_1,
        ]
    );
    assert_eq!(
        serde_json::to_value(&left).expect("plan JSON")["entries"]
            .as_array()
            .expect("entries")
            .len(),
        4
    );
}

#[test]
fn unknown_fields_round_trip_without_authorizing_them() {
    let mut value = serde_json::to_value(input()).expect("input JSON");
    value["futureInput"] = json!({"opaque": true});
    value["artifact"]["futureArtifact"] = json!("retained");
    let parsed: DriverPlanInput = serde_json::from_value(value).expect("input decode");
    parsed.validate().expect("known contract still valid");
    let encoded: Value = serde_json::to_value(parsed).expect("input encode");
    assert_eq!(encoded["futureInput"]["opaque"], json!(true));
    assert_eq!(encoded["artifact"]["futureArtifact"], json!("retained"));
}

#[test]
fn capture_binding_validation_requires_complete_same_plan_evidence() {
    let plan = generate_dry_run_plan(&input()).expect("plan");
    let valid = CaptureReceipt {
        schema_version: SCHEMA_VERSION,
        plan_sha256: plan.input_sha256.clone(),
        target_gpu: plan.target_gpu.clone(),
        complete: true,
        captured_at_utc: "2026-08-10T10:01:00Z".into(),
        extensions: BTreeMap::new(),
    };
    validate_capture_binding(&plan, &valid).expect("bound receipt");
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    assert_eq!(
        validate_capture_binding(&plan, &incomplete),
        Err(ValidationError::IncompleteReceipt)
    );
    let mut mismatched = valid;
    mismatched.plan_sha256 = digest('c');
    assert_eq!(
        validate_capture_binding(&plan, &mismatched),
        Err(ValidationError::ReceiptPlanMismatch)
    );
}

#[test]
fn execution_capture_requires_canonical_current_safe_mode_bound_inventory() {
    let (plan, capture) = execution_capture();
    let policy = CaptureFreshnessPolicy {
        maximum_age_seconds: 120,
    };
    capture
        .validate_for_plan_at(&plan, policy, "2026-08-10T10:02:00Z")
        .expect("fresh complete capture");

    let mut unordered = capture.clone();
    unordered
        .installed_packages
        .packages
        .push(unordered.installed_packages.packages[0].clone());
    assert!(
        unordered
            .validate_for_plan_at(&plan, policy, "2026-08-10T10:02:00Z")
            .is_err()
    );

    let mut indeterminate = capture.clone();
    indeterminate.safe_mode.state = SafeModeState::Indeterminate;
    assert_eq!(
        indeterminate.validate_for_plan_at(&plan, policy, "2026-08-10T10:02:00Z"),
        Err(ValidationError::SafeModeNotConfirmed)
    );
    assert_eq!(
        capture.validate_for_plan_at(&plan, policy, "2026-08-10T10:04:00Z"),
        Err(ValidationError::StaleCapture)
    );
    let mut stale_safe_mode = capture;
    stale_safe_mode.safe_mode.observed_at_utc = "2026-08-10T09:58:00Z".into();
    assert_eq!(
        stale_safe_mode.validate_for_plan_at(&plan, policy, "2026-08-10T10:02:00Z"),
        Err(ValidationError::StaleCapture)
    );
}

#[test]
fn authorization_rejects_identity_substitution_and_expiry() {
    let request = input();
    let (_, capture) = execution_capture();
    let authorization = authorization(&capture, &request);
    authorization
        .validate_for_capture_at(&capture, &request.artifact, "2026-08-10T10:02:00Z")
        .expect("bound authorization");

    let mut substituted = authorization.clone();
    substituted.artifact.payload_sha256 = digest('c');
    assert_eq!(
        substituted.validate_for_capture_at(&capture, &request.artifact, "2026-08-10T10:02:00Z"),
        Err(ValidationError::ArtifactIdentityMismatch)
    );
    assert_eq!(
        authorization.validate_for_capture_at(&capture, &request.artifact, "2026-08-10T10:31:00Z"),
        Err(ValidationError::AuthorizationExpired)
    );
}

#[test]
fn removal_evidence_rejects_partial_results_and_stale_inventory() {
    let (plan, capture) = execution_capture();
    let policy = CaptureFreshnessPolicy {
        maximum_age_seconds: 120,
    };
    let mut evidence = RemovalExecutionEvidence {
        capture,
        outcomes: Vec::new(),
        post_removal_packages: CanonicalPackageSet::from_unsorted(gpu(), Vec::new())
            .expect("empty post-removal inventory"),
        observed_at_utc: "2026-08-10T10:02:00Z".into(),
    };
    assert!(
        evidence
            .validate_for_plan_at(&plan, policy, "2026-08-10T10:02:01Z")
            .is_err()
    );
    evidence.outcomes.push(PackageRemovalOutcome {
        published_name: OemPublishedName::parse("oem12.inf").expect("OEM name"),
        disposition: PackageRemovalDisposition::Failed {
            reason: "access denied".into(),
        },
        observed_at_utc: "2026-08-10T10:01:30Z".into(),
    });
    assert_eq!(
        evidence.validate_for_plan_at(&plan, policy, "2026-08-10T10:02:01Z"),
        Err(ValidationError::InvalidRemovalEvidence)
    );
}

#[test]
fn installation_evidence_requires_fresh_identity_bound_post_install_inventory() {
    let request = input();
    let (plan, capture) = execution_capture();
    let authorization = authorization(&capture, &request);
    let packages = package_set();
    let mut evidence = InstallationEvidence {
        authorization,
        fresh_authenticode: AuthenticodeEvidence {
            status: AuthenticodeStatus::Valid,
            signer_subject: request.artifact.authenticode.signer_subject.clone(),
            signer_thumbprint_sha256: request
                .artifact
                .authenticode
                .signer_thumbprint_sha256
                .clone(),
            observed_at_utc: "2026-08-10T10:01:02Z".into(),
            extensions: BTreeMap::new(),
        },
        installed_artifact: InstalledArtifactObservation {
            artifact: ArtifactIdentity::from_descriptor(&request.artifact).expect("identity"),
            observed_at_utc: "2026-08-10T10:02:00Z".into(),
        },
        post_install_packages: packages,
        observed_at_utc: "2026-08-10T10:02:01Z".into(),
    };
    evidence
        .validate_for_plan_at(
            &plan,
            &capture,
            &request.artifact,
            CaptureFreshnessPolicy {
                maximum_age_seconds: 120,
            },
            "2026-08-10T10:02:02Z",
        )
        .expect("fresh post-install evidence");
    evidence.installed_artifact.artifact.payload_sha256 = digest('d');
    assert_eq!(
        evidence.validate_for_capture_at(&capture, &request.artifact, "2026-08-10T10:02:02Z"),
        Err(ValidationError::InvalidInstallationEvidence)
    );
}

#[test]
fn installation_evidence_requires_fresh_authorized_signature_binding() {
    let request = input();
    let (plan, capture) = execution_capture();
    let authorization = authorization(&capture, &request);
    let mut evidence = InstallationEvidence {
        authorization,
        fresh_authenticode: AuthenticodeEvidence {
            status: AuthenticodeStatus::Valid,
            signer_subject: request.artifact.authenticode.signer_subject.clone(),
            signer_thumbprint_sha256: request
                .artifact
                .authenticode
                .signer_thumbprint_sha256
                .clone(),
            observed_at_utc: "2026-08-10T10:01:02Z".into(),
            extensions: BTreeMap::new(),
        },
        installed_artifact: InstalledArtifactObservation {
            artifact: ArtifactIdentity::from_descriptor(&request.artifact).expect("identity"),
            observed_at_utc: "2026-08-10T10:02:00Z".into(),
        },
        post_install_packages: package_set(),
        observed_at_utc: "2026-08-10T10:02:01Z".into(),
    };
    let validate = |evidence: &InstallationEvidence| {
        evidence.validate_for_plan_at(
            &plan,
            &capture,
            &request.artifact,
            CaptureFreshnessPolicy {
                maximum_age_seconds: 120,
            },
            "2026-08-10T10:02:02Z",
        )
    };
    validate(&evidence).expect("fresh signature binds installation");

    evidence.fresh_authenticode.status = AuthenticodeStatus::Invalid;
    assert_eq!(
        validate(&evidence),
        Err(ValidationError::InvalidSignatureEvidence)
    );
    evidence.fresh_authenticode.status = AuthenticodeStatus::Valid;
    evidence.fresh_authenticode.signer_subject = "CN=Substituted".into();
    assert_eq!(
        validate(&evidence),
        Err(ValidationError::InvalidInstallationEvidence)
    );
    evidence.fresh_authenticode.signer_subject =
        request.artifact.authenticode.signer_subject.clone();
    evidence.fresh_authenticode.signer_thumbprint_sha256 = digest('d');
    assert_eq!(
        validate(&evidence),
        Err(ValidationError::InvalidInstallationEvidence)
    );
    evidence.fresh_authenticode.signer_thumbprint_sha256 = request
        .artifact
        .authenticode
        .signer_thumbprint_sha256
        .clone();
    evidence.fresh_authenticode.observed_at_utc = "2026-08-10T10:01:00Z".into();
    assert_eq!(
        validate(&evidence),
        Err(ValidationError::InvalidInstallationEvidence)
    );
    evidence.fresh_authenticode.observed_at_utc = "2026-08-10T10:02:01Z".into();
    assert_eq!(
        validate(&evidence),
        Err(ValidationError::InvalidInstallationEvidence)
    );
}

#[test]
fn installation_evidence_does_not_authorize_unknown_fresh_signature_fields() {
    let request = input();
    let (_, capture) = execution_capture();
    let authorization = authorization(&capture, &request);
    let value = json!({
        "authorization": authorization,
        "freshAuthenticode": {
            "futureStatus": "valid",
            "signerSubject": request.artifact.authenticode.signer_subject,
            "signerThumbprintSha256": request.artifact.authenticode.signer_thumbprint_sha256,
            "observedAtUtc": "2026-08-10T10:01:02Z"
        },
        "installedArtifact": {
            "artifact": ArtifactIdentity::from_descriptor(&request.artifact).expect("identity"),
            "observedAtUtc": "2026-08-10T10:02:00Z"
        },
        "postInstallPackages": package_set(),
        "observedAtUtc": "2026-08-10T10:02:01Z"
    });
    let decoded = serde_json::from_value::<InstallationEvidence>(value);
    assert!(
        decoded.is_err(),
        "unknown fields cannot replace required status"
    );
}
