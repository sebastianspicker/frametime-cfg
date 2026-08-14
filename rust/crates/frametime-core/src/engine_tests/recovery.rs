use super::*;

fn run_irreversible(
    mock: Mock,
    phase: u8,
    number: u8,
) -> (Result<RunReport, EngineError>, Mock, Progress) {
    let step = *STEPS
        .iter()
        .find(|step| step.phase as u8 == phase && step.number == number)
        .expect("catalog step");
    let mut engine = Engine::new(mock, Progress::default());
    let result = engine.run(&[step], Profile::Yolo);
    let (backend, progress) = engine.into_parts();
    (result, backend, progress)
}

#[test]
fn manual_recovery_audit_is_durable_before_mutation_and_finalized_after_verify() {
    let (result, backend, progress) = run_irreversible(
        Mock {
            manual: true,
            ..Mock::default()
        },
        1,
        13,
    );
    result.expect("manual recovery run");
    assert_eq!(
        backend.calls,
        [
            "inspect",
            "irreversible_audit_capture",
            "irreversible_audit_persist",
            "apply",
            "verify",
            "irreversible_audit_finalize",
            "progress"
        ]
    );
    assert!(progress.completed_steps.contains("P1:13"));
}

#[test]
fn mixed_recovery_persists_lossless_backup_and_manual_audit_before_mutation() {
    let (result, backend, progress) = run_irreversible(
        Mock {
            mixed: true,
            ..Mock::default()
        },
        3,
        1,
    );
    result.expect("mixed recovery run");
    assert_eq!(
        backend.calls,
        [
            "inspect",
            "backup",
            "persist",
            "irreversible_audit_capture",
            "irreversible_audit_persist",
            "apply",
            "verify",
            "irreversible_audit_finalize",
            "progress"
        ]
    );
    assert!(progress.completed_steps.contains("P3:1"));
}

#[test]
fn manual_and_mixed_recovery_fault_prefixes_block_or_retain_audit_state() {
    assert_pre_mutation_failures();
    assert_post_mutation_failures();
}

fn assert_pre_mutation_failures() {
    for (manual, mixed, phase, number, failure, expected_calls) in [
        (
            true,
            false,
            1,
            13,
            "irreversible_audit_capture",
            vec!["inspect", "irreversible_audit_capture"],
        ),
        (
            true,
            false,
            1,
            13,
            "irreversible_audit_persist",
            vec![
                "inspect",
                "irreversible_audit_capture",
                "irreversible_audit_persist",
            ],
        ),
        (false, true, 3, 1, "backup", vec!["inspect", "backup"]),
        (
            false,
            true,
            3,
            1,
            "persist",
            vec!["inspect", "backup", "persist"],
        ),
        (
            false,
            true,
            3,
            1,
            "irreversible_audit_capture",
            vec!["inspect", "backup", "persist", "irreversible_audit_capture"],
        ),
        (
            false,
            true,
            3,
            1,
            "irreversible_audit_persist",
            vec![
                "inspect",
                "backup",
                "persist",
                "irreversible_audit_capture",
                "irreversible_audit_persist",
            ],
        ),
    ] {
        let (result, backend, progress) = run_irreversible(
            Mock {
                manual,
                mixed,
                failure: Some(failure),
                ..Mock::default()
            },
            phase,
            number,
        );
        assert!(result.is_err(), "{failure}");
        assert_eq!(backend.calls, expected_calls, "{failure}");
        assert!(!backend.calls.contains(&"apply"), "{failure}");
        assert!(progress.completed_steps.is_empty(), "{failure}");
    }
}

fn assert_post_mutation_failures() {
    for (failure, fail_apply, expected_calls) in [
        (
            "apply",
            false,
            vec![
                "inspect",
                "irreversible_audit_capture",
                "irreversible_audit_persist",
                "apply",
                "irreversible_audit_fail",
            ],
        ),
        (
            "verify",
            false,
            vec![
                "inspect",
                "irreversible_audit_capture",
                "irreversible_audit_persist",
                "apply",
                "verify",
                "irreversible_audit_fail",
            ],
        ),
        (
            "irreversible_audit_fail",
            true,
            vec![
                "inspect",
                "irreversible_audit_capture",
                "irreversible_audit_persist",
                "apply",
                "irreversible_audit_fail",
            ],
        ),
        (
            "irreversible_audit_finalize",
            false,
            vec![
                "inspect",
                "irreversible_audit_capture",
                "irreversible_audit_persist",
                "apply",
                "verify",
                "irreversible_audit_finalize",
            ],
        ),
    ] {
        let (result, backend, progress) = run_irreversible(
            Mock {
                manual: true,
                fail_apply,
                failure: Some(failure),
                ..Mock::default()
            },
            1,
            13,
        );
        assert!(result.is_err(), "{failure}");
        assert_eq!(backend.calls, expected_calls, "{failure}");
        assert!(!backend.calls.contains(&"progress"), "{failure}");
        assert!(progress.completed_steps.is_empty(), "{failure}");
    }
}
