use super::*;
use crate::{
    PciDeviceBinding,
    audit::{RebuildableAudit, RebuildableTarget},
    catalog::STEPS,
    evidence::{EvidenceRequirement, ObservationReceipt, ObservationSubject},
    operations::{GpuBranch, plan_for_step},
};

#[derive(Default)]
struct Mock {
    calls: Vec<&'static str>,
    dry: bool,
    rebuildable: bool,
    manual: bool,
    mixed: bool,
    fail_apply: bool,
    evidence: bool,
    failure: Option<&'static str>,
    inspection: Option<Inspection>,
    audit_step: Option<&'static str>,
    audit_targets: Option<Vec<RebuildableTarget>>,
}

impl Mock {
    fn call(&mut self, call: &'static str) -> Result<(), String> {
        self.calls.push(call);
        if self.failure == Some(call) {
            Err("injected failure".into())
        } else {
            Ok(())
        }
    }
}

#[path = "engine_tests/backend.rs"]
mod backend;
#[path = "engine_tests/recovery.rs"]
mod recovery;

#[test]
fn rebuildable_audit_must_be_pending_and_bound_to_the_current_step() {
    let (result, backend, progress) = run(Mock {
        rebuildable: true,
        audit_step: Some("P1:4"),
        ..Mock::default()
    });
    assert!(matches!(result, Err(EngineError::AuditCapture { .. })));
    assert_eq!(backend.calls, ["inspect", "audit_capture"]);
    assert!(progress.completed_steps.is_empty());
}

#[test]
fn p1_3_rejects_an_incomplete_audit_target_set_before_persistence_or_mutation() {
    let (result, backend, progress) = run(Mock {
        rebuildable: true,
        audit_targets: Some(vec![RebuildableTarget::Cs2ShaderCache]),
        ..Mock::default()
    });
    assert!(matches!(result, Err(EngineError::AuditCapture { .. })));
    assert_eq!(backend.calls, ["inspect", "audit_capture"]);
    assert!(progress.completed_steps.is_empty());
}

fn run(mock: Mock) -> (Result<RunReport, EngineError>, Mock, Progress) {
    let mut engine = Engine::new(mock, Progress::default());
    let result = engine.run(&STEPS[2..3], Profile::Yolo);
    let (backend, progress) = engine.into_parts();
    (result, backend, progress)
}

fn test_device_binding() -> PciDeviceBinding {
    PciDeviceBinding {
        schema_version: 1,
        instance_id: r"PCI\VEN_10DE&DEV_2684&SUBSYS_47101462&REV_A1\4&abc&0&0008".into(),
        container_id: "{01234567-89ab-cdef-0123-456789abcdef}".into(),
        class_guid: "{4d36e968-e325-11ce-bfc1-08002be10318}".into(),
        vendor_id: 0x10de,
        device_id: 0x2684,
        subsystem_vendor_id: 0x1462,
        subsystem_device_id: 0x4710,
        revision_id: 0xa1,
        driver_provider: "NVIDIA".into(),
        driver_version: "32.0.15.1234".into(),
        published_inf: "oem42.inf".into(),
        observed_at_utc: "2026-08-13T12:00:00Z".into(),
        unknown: std::collections::BTreeMap::new(),
    }
}

fn run_evidence(mock: Mock) -> (Result<RunReport, EngineError>, Mock, Progress) {
    let step = STEPS
        .iter()
        .find(|step| step.phase as u8 == 1 && step.number == 21)
        .copied()
        .expect("P1:21");
    let mut engine = Engine::new(mock, Progress::default());
    let result = engine.run_with_consent(&[step], Profile::Custom, |_| true);
    let (backend, progress) = engine.into_parts();
    (result, backend, progress)
}

#[test]
fn durable_observation_is_persisted_and_reobserved_before_progress() {
    let (result, backend, progress) = run_evidence(Mock {
        evidence: true,
        inspection: Some(Inspection::Satisfied),
        ..Mock::default()
    });
    result.expect("evidence run");
    assert_eq!(
        backend.calls,
        [
            "inspect",
            "evidence_capture",
            "evidence_persist",
            "evidence_verify",
            "verify",
            "progress",
        ]
    );
    assert!(progress.completed_steps.contains("P1:21"));
}

#[test]
fn durable_observation_failures_never_complete_progress() {
    for failure in ["evidence_capture", "evidence_persist", "evidence_verify"] {
        let (result, backend, progress) = run_evidence(Mock {
            evidence: true,
            inspection: Some(Inspection::Satisfied),
            failure: Some(failure),
            ..Mock::default()
        });
        assert!(result.is_err(), "{failure}");
        assert!(!backend.calls.contains(&"progress"), "{failure}");
        assert!(progress.completed_steps.is_empty(), "{failure}");
    }
}

#[test]
fn lossless_backup_order_is_unchanged_by_default() {
    let (result, backend, _) = run(Mock::default());
    result.expect("run");
    assert_eq!(
        backend.calls,
        [
            "inspect", "backup", "persist", "apply", "verify", "progress"
        ]
    );
}

#[test]
fn rebuildable_audit_is_durable_before_mutation_and_finalized_before_progress() {
    let (result, backend, progress) = run(Mock {
        rebuildable: true,
        ..Mock::default()
    });
    result.expect("run");
    assert_eq!(
        backend.calls,
        [
            "inspect",
            "audit_capture",
            "audit_persist",
            "apply",
            "verify",
            "audit_finalize",
            "progress"
        ]
    );
    assert!(progress.completed_steps.contains("P1:3"));
}

#[test]
fn dry_run_inspects_and_plans_without_recovery_persistence_or_mutation() {
    let (result, backend, progress) = run(Mock {
        dry: true,
        rebuildable: true,
        ..Mock::default()
    });
    result.expect("preview");
    assert_eq!(backend.calls, ["inspect", "plan"]);
    assert!(progress.completed_steps.is_empty());
    assert!(progress.skipped_steps.is_empty());
}

#[test]
fn p3_6_is_check_only_and_never_enters_the_mutation_path() {
    let step = *STEPS
        .iter()
        .find(|step| step.phase as u8 == 3 && step.number == 6)
        .expect("P3:6 catalog entry");
    assert!(step.check_only);
    assert!(!plan_for_step(&step, GpuBranch::Nvidia).mutating);

    let mut live_engine = Engine::new(Mock::default(), Progress::default());
    live_engine.run(&[step], Profile::Yolo).expect("live guide");
    let (live_backend, live_progress) = live_engine.into_parts();
    assert_eq!(live_backend.calls, ["inspect", "verify", "progress"]);
    assert!(live_progress.completed_steps.contains("P3:6"));

    let mut preview_engine = Engine::new(
        Mock {
            dry: true,
            ..Mock::default()
        },
        Progress::default(),
    );
    preview_engine
        .run(&[step], Profile::Yolo)
        .expect("preview guide");
    let (preview_backend, preview_progress) = preview_engine.into_parts();
    assert_eq!(preview_backend.calls, ["inspect", "plan"]);
    assert!(preview_progress.completed_steps.is_empty());
}

#[test]
fn every_rebuildable_audit_failure_blocks_later_stages_and_progress() {
    for (failure, expected_error, expected_calls) in [
        ("audit_capture", "capture", vec!["inspect", "audit_capture"]),
        (
            "audit_persist",
            "persist",
            vec!["inspect", "audit_capture", "audit_persist"],
        ),
        (
            "audit_finalize",
            "finalize",
            vec![
                "inspect",
                "audit_capture",
                "audit_persist",
                "apply",
                "verify",
                "audit_finalize",
            ],
        ),
    ] {
        let (result, backend, progress) = run(Mock {
            rebuildable: true,
            failure: Some(failure),
            ..Mock::default()
        });
        let error = result.expect_err(expected_error);
        assert!(matches!(
            (failure, error),
            ("audit_capture", EngineError::AuditCapture { .. })
                | ("audit_persist", EngineError::AuditPersist { .. })
                | ("audit_finalize", EngineError::AuditFinalize { .. })
        ));
        assert_eq!(backend.calls, expected_calls, "{failure}");
        assert!(progress.completed_steps.is_empty(), "{failure}");
    }
}

#[test]
fn cancellation_at_the_step_boundary_prevents_rebuildable_audit_and_mutation() {
    let mut engine = Engine::new(
        Mock {
            rebuildable: true,
            ..Mock::default()
        },
        Progress::default(),
    );
    let error = engine
        .run_with_control(&STEPS[2..3], Profile::Yolo, |_| true, || true)
        .expect_err("cancelled");
    assert!(matches!(error, EngineError::Cancelled { .. }));
    let (backend, progress) = engine.into_parts();
    assert!(backend.calls.is_empty());
    assert!(progress.completed_steps.is_empty());
}

#[test]
fn recorded_steps_bypass_consent_and_cancellation_at_the_control_boundary() {
    for skipped in [false, true] {
        let mut progress = Progress::default();
        if skipped {
            progress.skip(1, 3);
        } else {
            progress.complete(1, 3, "already completed".into());
        }
        let mut consent_checks = 0;
        let mut cancellation_checks = 0;
        let mut engine = Engine::new(Mock::default(), progress);

        let report = engine
            .run_with_control(
                &STEPS[2..3],
                Profile::Yolo,
                |_| {
                    consent_checks += 1;
                    true
                },
                || {
                    cancellation_checks += 1;
                    true
                },
            )
            .expect("recorded step is ignored");

        assert!(report.events.is_empty());
        assert_eq!(report.completed, 0);
        assert_eq!(report.skipped, 0);
        assert_eq!(consent_checks, 0);
        assert_eq!(cancellation_checks, 0);
        let (backend, _) = engine.into_parts();
        assert!(backend.calls.is_empty());
    }
}

#[test]
fn skip_progress_persistence_failure_does_not_record_the_step_before_retry() {
    let initial_progress = Progress::default();
    let mut engine = Engine::new(
        Mock {
            failure: Some("progress"),
            inspection: Some(Inspection::Inapplicable),
            ..Mock::default()
        },
        initial_progress.clone(),
    );

    let error = engine
        .run(&STEPS[2..3], Profile::Yolo)
        .expect_err("progress persistence fails");
    assert!(matches!(error, EngineError::Progress { .. }));
    assert_eq!(engine.progress, initial_progress);
    assert_eq!(engine.backend.calls, ["inspect", "progress"]);

    engine.backend.failure = None;
    let report = engine
        .run(&STEPS[2..3], Profile::Yolo)
        .expect("retry runs the skipped step again");
    assert_eq!(
        report.events,
        [Event::Inspect("P1:3".into()), Event::Skip("P1:3".into()),]
    );
    assert_eq!(report.skipped, 1);
    assert_eq!(
        engine.backend.calls,
        ["inspect", "progress", "inspect", "progress"]
    );
    assert!(engine.progress.skipped_steps.contains("P1:3"));
}

#[test]
fn completion_progress_persistence_failure_does_not_record_the_step_before_retry() {
    let initial_progress = Progress::default();
    let mut engine = Engine::new(
        Mock {
            failure: Some("progress"),
            ..Mock::default()
        },
        initial_progress.clone(),
    );

    let error = engine
        .run(&STEPS[2..3], Profile::Yolo)
        .expect_err("progress persistence fails");
    assert!(matches!(error, EngineError::Progress { .. }));
    assert_eq!(engine.progress, initial_progress);
    assert_eq!(
        engine.backend.calls,
        [
            "inspect", "backup", "persist", "apply", "verify", "progress"
        ]
    );

    engine.backend.failure = None;
    let report = engine
        .run(&STEPS[2..3], Profile::Yolo)
        .expect("retry executes the complete operation again");
    assert_eq!(
        report.events,
        [
            Event::Inspect("P1:3".into()),
            Event::CaptureBackup("P1:3".into()),
            Event::PersistBackup("P1:3".into()),
            Event::Apply("P1:3".into()),
            Event::Verify("P1:3".into()),
            Event::Complete("P1:3".into()),
        ]
    );
    assert_eq!(report.completed, 1);
    assert_eq!(
        engine.backend.calls,
        [
            "inspect", "backup", "persist", "apply", "verify", "progress", "inspect", "backup",
            "persist", "apply", "verify", "progress",
        ]
    );
    assert!(engine.progress.completed_steps.contains("P1:3"));
}

#[test]
fn unsupported_and_inapplicable_steps_do_not_capture_recovery_records() {
    let (unsupported, backend, progress) = run(Mock {
        inspection: Some(Inspection::Unsupported),
        rebuildable: true,
        ..Mock::default()
    });
    assert!(matches!(unsupported, Err(EngineError::Unsupported { .. })));
    assert_eq!(backend.calls, ["inspect"]);
    assert!(progress.completed_steps.is_empty());

    let (inapplicable, backend, progress) = run(Mock {
        inspection: Some(Inspection::Inapplicable),
        rebuildable: true,
        ..Mock::default()
    });
    inapplicable.expect("skip");
    assert_eq!(backend.calls, ["inspect", "progress"]);
    assert!(progress.skipped_steps.contains("P1:3"));
}

#[test]
fn advisory_acknowledgement_never_captures_applies_verifies_or_completes() {
    let (result, backend, progress) = run(Mock {
        inspection: Some(Inspection::Advisory {
            reason: "authoritative firmware evidence is unavailable",
        }),
        ..Mock::default()
    });
    let report = result.expect("advisory is acknowledged");
    assert_eq!(backend.calls, ["inspect", "progress"]);
    assert_eq!(report.completed, 0);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.advisories, 1);
    assert!(matches!(
        report.events.as_slice(),
        [Event::Inspect(_), Event::Advisory { key, reason }]
            if key == "P1:3" && reason == "authoritative firmware evidence is unavailable"
    ));
    assert!(progress.completed_steps.is_empty());
    assert!(progress.skipped_steps.is_empty());
    assert_eq!(
        progress.advisories["P1:3"].reason,
        "authoritative firmware evidence is unavailable"
    );
}

#[test]
fn dry_run_reports_advisory_without_persistence_or_success() {
    let (result, backend, progress) = run(Mock {
        dry: true,
        inspection: Some(Inspection::Advisory {
            reason: "authoritative firmware evidence is unavailable",
        }),
        ..Mock::default()
    });
    let report = result.expect("preview");
    assert_eq!(backend.calls, ["inspect", "plan"]);
    assert_eq!(report.completed, 0);
    assert_eq!(report.failed, 0);
    assert_eq!(report.advisories, 1);
    assert!(progress.completed_steps.is_empty());
    assert!(progress.skipped_steps.is_empty());
    assert!(progress.advisories.is_empty());
}
