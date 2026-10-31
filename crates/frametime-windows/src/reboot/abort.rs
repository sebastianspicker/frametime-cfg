use super::{ArmingHost, NativeArmingHost};
use crate::*;
// Abort of a stuck reboot handoff (F-28). Only the boot-time values the active
// transaction armed are removed, each by an exact, idempotent match; the
// decision logic sits behind a small host boundary so it is testable without
// BCD or the registry.

use frametime_domain::{
    benchmark::FinalBenchmarkReceipt,
    handoff::{RebootStage, RebootTransaction},
    runtime::RUNTIME_GENERATIONS_DIR,
};
use serde::{Deserialize, Serialize};

const ABORTED_TRANSACTIONS_FILE: &str = "aborted-reboot-transactions.json";
const ABORTED_TRANSACTIONS_SCHEMA: u8 = 1;
const PHASE2_LABEL: &str = "HKLM RunOnce *!FRAMETIME_Phase2";
const PHASE3_LABEL: &str = "HKCU Run FRAMETIME_CFG_FRAMETIME_Phase3";
const SAFE_BOOT_LABEL: &str = "Safe Boot (bcdedit safeboot)";

/// What `abort-handoff` found for each boot-time item the workflow arms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandoffAbortReport {
    pub removed: Vec<String>,
    pub already_absent: Vec<String>,
    pub remaining: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffAbort {
    /// No active reboot transaction exists; nothing was changed.
    NoActiveTransaction,
    /// Every armed item is gone; the transaction was archived and cleared.
    Cleared(HandoffAbortReport),
    /// At least one item had to be left; the transaction is retained.
    Incomplete(HandoffAbortReport),
}

pub(crate) trait AbortHost: ArmingHost {
    fn phase_three_handoff(&self) -> Result<Option<String>, String>;
    fn delete_phase_three_handoff(&self) -> Result<(), String>;
    fn current_user_sid(&self) -> Result<String, String>;
}

impl AbortHost for NativeArmingHost {
    fn phase_three_handoff(&self) -> Result<Option<String>, String> {
        read_handoff_value(Hive::CurrentUser, RUN_KEY, PHASE3_HANDOFF)
    }
    fn delete_phase_three_handoff(&self) -> Result<(), String> {
        registry_delete(Hive::CurrentUser, RUN_KEY, PHASE3_HANDOFF)
    }
    fn current_user_sid(&self) -> Result<String, String> {
        current_token_user_sid()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AbortedRebootTransactions {
    schema_version: u8,
    aborted: Vec<AbortedRebootTransaction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AbortedRebootTransaction {
    aborted_utc: String,
    removed: Vec<String>,
    already_absent: Vec<String>,
    transaction: RebootTransaction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    final_benchmark: Option<FinalBenchmarkReceipt>,
}

/// Undo exactly what the active reboot transaction armed. The transaction is
/// archived and cleared only when every armed item is gone; otherwise state
/// is left untouched as evidence for the manual recovery procedure.
pub fn abort_reboot_handoff(_package: &AuthenticatedPackage) -> Result<HandoffAbort, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    require_elevation()?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let (state, _) = read_handoff_session(work_dir)?;
    let outcome = abort_handoff_with(&NativeArmingHost, &state, work_dir);
    if let (HandoffAbort::Cleared(report), Some(transaction)) =
        (&outcome, &state.active_reboot_transaction)
    {
        archive_aborted_transaction(
            &trusted,
            transaction,
            state.final_benchmark.as_ref(),
            report,
        )?;
        let next_state = cleared_state(state.clone());
        next_state
            .validate()
            .map_err(|error| format!("abort-handoff cleared state is invalid: {error}"))?;
        persist_state_readback(work_dir, &next_state, "abort-handoff")?;
    }
    Ok(outcome)
}

/// Decide and perform the exact removals for `state`'s active transaction.
/// `Cleared` means every armed item is gone and the caller may clear state.
pub(crate) fn abort_handoff_with(
    host: &impl AbortHost,
    state: &State,
    work_dir: &Path,
) -> HandoffAbort {
    let Some(transaction) = &state.active_reboot_transaction else {
        return HandoffAbort::NoActiveTransaction;
    };
    let expected = expected_handoff_commands(work_dir, transaction);
    let mut report = HandoffAbortReport::default();
    abort_registry_value(
        PHASE2_LABEL,
        expected.as_ref().map(|(phase2, _)| phase2.as_str()),
        || host.phase_two_handoff(),
        || host.delete_phase_two_handoff(),
        &mut report,
    );
    match host.current_user_sid() {
        Ok(sid) if transaction.initiator_user_sid.as_deref() == Some(sid.as_str()) => {
            abort_registry_value(
                PHASE3_LABEL,
                expected.as_ref().map(|(_, phase3)| phase3.as_str()),
                || host.phase_three_handoff(),
                || host.delete_phase_three_handoff(),
                &mut report,
            );
        }
        Ok(_) => report.remaining.push(format!(
            "{PHASE3_LABEL}: not inspected because the current user is not the initiating user {}; that user must run abort-handoff --yes",
            transaction.initiator_user_sid.as_deref().unwrap_or("(unrecorded)")
        )),
        Err(error) => report.remaining.push(format!(
            "{PHASE3_LABEL}: not inspected because the current TokenUser is unreadable ({error})"
        )),
    }
    abort_safe_boot(host, &transaction.stage, &mut report);
    if report.remaining.is_empty() {
        HandoffAbort::Cleared(report)
    } else {
        HandoffAbort::Incomplete(report)
    }
}

/// The exact commands the transaction's own runtime armed. An absent or
/// incomplete runtime record makes them unknown, so no registry value is removed.
pub(crate) fn expected_handoff_commands(
    work_dir: &Path,
    transaction: &RebootTransaction,
) -> Option<(String, String)> {
    let runtime = transaction
        .runtime
        .as_ref()
        .filter(|runtime| runtime.is_complete())?;
    let executable = work_dir
        .join(RUNTIME_GENERATIONS_DIR)
        .join(&runtime.generation)
        .join(&runtime.executable_path);
    Some((
        handoff_command(&executable, "phase2 --yes").ok()?,
        handoff_command(&executable, "phase3-handoff").ok()?,
    ))
}

/// State after a complete abort: no transaction, no Safe Mode readiness, and
/// no transaction-bound final benchmark (which state validation forbids
/// without a transaction; the receipt is archived with the transaction).
pub(crate) fn cleared_state(mut state: State) -> State {
    state.active_reboot_transaction = None;
    state.phase1_safe_mode_ready = false;
    state.final_benchmark = None;
    state
}

fn abort_registry_value(
    label: &str,
    expected: Option<&str>,
    read: impl Fn() -> Result<Option<String>, String>,
    delete: impl FnOnce() -> Result<(), String>,
    report: &mut HandoffAbortReport,
) {
    match read() {
        Ok(None) => report.already_absent.push(label.to_owned()),
        Ok(Some(value)) if expected == Some(value.as_str()) => {
            match delete().and_then(|()| read()) {
                Ok(None) => report.removed.push(label.to_owned()),
                Ok(Some(_)) => report
                    .remaining
                    .push(format!("{label}: delete readback still shows the value")),
                Err(error) => report.remaining.push(format!("{label}: {error}")),
            }
        }
        Ok(Some(_)) if expected.is_none() => report.remaining.push(format!(
            "{label}: the transaction records no usable runtime, so the expected command is unknown; left in place"
        )),
        Ok(Some(_)) => report.remaining.push(format!(
            "{label}: value differs from the transaction's expected command; left in place"
        )),
        Err(error) => report
            .remaining
            .push(format!("{label}: unreadable ({error}); left in place")),
    }
}

/// The workflow's own `safeboot minimal` exists only while the transaction is
/// at `PhaseOneSafeModeArmed`: P2:1 clears it and moves to `PhaseTwoSafeMode`.
/// At any later stage a `minimal` value was not armed by this workflow.
fn abort_safe_boot(host: &impl AbortHost, stage: &RebootStage, report: &mut HandoffAbortReport) {
    match host.safe_boot() {
        SafebootEvidence::Absent => report.already_absent.push(SAFE_BOOT_LABEL.to_owned()),
        SafebootEvidence::Configured(value)
            if value == "minimal" && *stage != RebootStage::PhaseOneSafeModeArmed =>
        {
            report.remaining.push(format!(
                "{SAFE_BOOT_LABEL}: safeboot minimal is set, but the transaction stage {} means this workflow already cleared its Safe Boot; left in place",
                stage.as_str()
            ));
        }
        SafebootEvidence::Configured(value) if value == "minimal" => match host.clear_safe_boot() {
            Ok(()) => report.removed.push(SAFE_BOOT_LABEL.to_owned()),
            Err(error) => report.remaining.push(format!("{SAFE_BOOT_LABEL}: {error}")),
        },
        SafebootEvidence::Configured(value) => report.remaining.push(format!(
            "{SAFE_BOOT_LABEL}: safeboot {value} was not set by this workflow; left in place"
        )),
        SafebootEvidence::Unavailable => report.remaining.push(format!(
            "{SAFE_BOOT_LABEL}: the safeboot setting is unreadable; left in place"
        )),
    }
}

fn archive_aborted_transaction(
    trusted: &TrustedWorkDir,
    transaction: &RebootTransaction,
    final_benchmark: Option<&FinalBenchmarkReceipt>,
    report: &HandoffAbortReport,
) -> Result<(), String> {
    let mut archive = if trusted.path().join(ABORTED_TRANSACTIONS_FILE).exists() {
        let existing: AbortedRebootTransactions =
            read_json_trusted(trusted, ABORTED_TRANSACTIONS_FILE).map_err(|error| {
                format!("abort-handoff read aborted transaction evidence: {error}")
            })?;
        if existing.schema_version != ABORTED_TRANSACTIONS_SCHEMA {
            return Err("abort-handoff aborted transaction evidence has an unknown schema".into());
        }
        existing
    } else {
        AbortedRebootTransactions {
            schema_version: ABORTED_TRANSACTIONS_SCHEMA,
            aborted: Vec::new(),
        }
    };
    archive.aborted.push(AbortedRebootTransaction {
        aborted_utc: timestamp(),
        removed: report.removed.clone(),
        already_absent: report.already_absent.clone(),
        transaction: transaction.clone(),
        final_benchmark: final_benchmark.cloned(),
    });
    write_json_atomic_trusted(trusted, ABORTED_TRANSACTIONS_FILE, &archive)
        .map_err(|error| format!("abort-handoff persist aborted transaction evidence: {error}"))?;
    let persisted: AbortedRebootTransactions =
        read_json_trusted(trusted, ABORTED_TRANSACTIONS_FILE).map_err(|error| {
            format!("abort-handoff read back aborted transaction evidence: {error}")
        })?;
    (persisted == archive).then_some(()).ok_or_else(|| {
        "abort-handoff aborted transaction evidence readback differs; retry".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use frametime_domain::handoff::{RebootStage, RuntimeRecord, TransactionId};
    use std::cell::RefCell;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const GENERATION: &str = "0123456789abcdef0123456789abcdef";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct FakeHost {
        safe_boot: RefCell<SafebootEvidence>,
        phase2: RefCell<Option<String>>,
        phase3: RefCell<Option<String>>,
        sid: String,
        calls: RefCell<Vec<&'static str>>,
    }

    impl FakeHost {
        fn new(
            safe_boot: SafebootEvidence,
            phase2: Option<String>,
            phase3: Option<String>,
        ) -> Self {
            Self {
                safe_boot: RefCell::new(safe_boot),
                phase2: RefCell::new(phase2),
                phase3: RefCell::new(phase3),
                sid: SID.into(),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl ArmingHost for FakeHost {
        fn safe_boot(&self) -> SafebootEvidence {
            self.safe_boot.borrow().clone()
        }
        fn clear_safe_boot(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("clear safe boot");
            *self.safe_boot.borrow_mut() = SafebootEvidence::Absent;
            Ok(())
        }
        fn phase_two_handoff(&self) -> Result<Option<String>, String> {
            Ok(self.phase2.borrow().clone())
        }
        fn delete_phase_two_handoff(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("delete phase2");
            *self.phase2.borrow_mut() = None;
            Ok(())
        }
    }

    impl AbortHost for FakeHost {
        fn phase_three_handoff(&self) -> Result<Option<String>, String> {
            Ok(self.phase3.borrow().clone())
        }
        fn delete_phase_three_handoff(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("delete phase3");
            *self.phase3.borrow_mut() = None;
            Ok(())
        }
        fn current_user_sid(&self) -> Result<String, String> {
            Ok(self.sid.clone())
        }
    }

    // A string-only path; nothing below ever touches the filesystem.
    fn work_dir() -> &'static Path {
        Path::new(WINDOWS_WORK_DIR)
    }

    fn state_at(stage: RebootStage) -> State {
        let transaction = RebootTransaction {
            schema_version: 1,
            transaction_id: TransactionId::parse("fedcba9876543210fedcba9876543210").ok(),
            initiator_user_sid: Some(SID.into()),
            stage,
            runtime: Some(RuntimeRecord {
                generation: GENERATION.into(),
                manifest_sha256: HASH.into(),
                payload_contract_hash: HASH.into(),
                executable_path: "frametime.exe".into(),
                executable_sha256: HASH.into(),
                unknown: Default::default(),
            }),
            ..RebootTransaction::default()
        };
        State {
            phase1_safe_mode_ready: true,
            active_reboot_transaction: Some(transaction),
            ..State::default()
        }
    }

    fn commands(state: &State) -> (String, String) {
        let transaction = state.active_reboot_transaction.as_ref().unwrap();
        expected_handoff_commands(work_dir(), transaction).unwrap()
    }

    fn minimal() -> SafebootEvidence {
        SafebootEvidence::Configured("minimal".into())
    }

    #[test]
    fn no_transaction_changes_nothing() {
        let host = FakeHost::new(minimal(), Some("phase2".into()), Some("phase3".into()));
        let outcome = abort_handoff_with(&host, &State::default(), work_dir());
        assert_eq!(outcome, HandoffAbort::NoActiveTransaction);
        assert!(host.calls.borrow().is_empty());
    }

    #[test]
    fn expected_commands_come_from_the_transaction_runtime() {
        let state = state_at(RebootStage::PhaseThreeArmed);
        let (phase2, phase3) = commands(&state);
        assert!(phase2.contains(GENERATION) && phase2.ends_with("\" phase2 --yes"));
        assert!(phase3.contains(GENERATION) && phase3.ends_with("\" phase3-handoff"));
        let mut missing = state.active_reboot_transaction.unwrap();
        missing.runtime = None;
        assert_eq!(expected_handoff_commands(work_dir(), &missing), None);
    }

    #[test]
    fn all_three_armed_exactly_are_removed_and_the_transaction_cleared() {
        let state = state_at(RebootStage::PhaseOneSafeModeArmed);
        let (phase2, phase3) = commands(&state);
        let host = FakeHost::new(minimal(), Some(phase2), Some(phase3));
        let HandoffAbort::Cleared(report) = abort_handoff_with(&host, &state, work_dir()) else {
            panic!("expected a cleared abort");
        };
        assert_eq!(
            *host.calls.borrow(),
            ["delete phase2", "delete phase3", "clear safe boot"]
        );
        assert_eq!(report.removed.len(), 3);
        assert!(report.already_absent.is_empty() && report.remaining.is_empty());
        let cleared = cleared_state(state);
        assert!(cleared.active_reboot_transaction.is_none());
        assert!(!cleared.phase1_safe_mode_ready);
        assert!(cleared.validate().is_ok());
    }

    #[test]
    fn a_differing_runonce_value_is_kept_and_the_transaction_retained() {
        let state = state_at(RebootStage::PhaseOneSafeModeArmed);
        let host = FakeHost::new(
            minimal(),
            Some("\"C:\\other.exe\" phase2 --yes".into()),
            None,
        );
        let HandoffAbort::Incomplete(report) = abort_handoff_with(&host, &state, work_dir()) else {
            panic!("expected an incomplete abort");
        };
        assert_eq!(*host.calls.borrow(), ["clear safe boot"]);
        assert!(host.phase2.borrow().is_some());
        assert_eq!(report.remaining.len(), 1);
        assert!(report.remaining[0].starts_with(PHASE2_LABEL));
        assert!(report.remaining[0].contains("differs"));
    }

    #[test]
    fn a_different_user_skips_hkcu_and_reports_the_initiating_user() {
        let state = state_at(RebootStage::PhaseThreeArmed);
        let (phase2, phase3) = commands(&state);
        let mut host = FakeHost::new(SafebootEvidence::Absent, Some(phase2), Some(phase3));
        host.sid = "S-1-5-21-1-2-3-1002".into();
        let HandoffAbort::Incomplete(report) = abort_handoff_with(&host, &state, work_dir()) else {
            panic!("expected an incomplete abort");
        };
        assert_eq!(*host.calls.borrow(), ["delete phase2"]);
        assert!(host.phase3.borrow().is_some());
        assert_eq!(report.remaining.len(), 1);
        assert!(report.remaining[0].starts_with(PHASE3_LABEL));
        assert!(report.remaining[0].contains(SID));
    }

    #[test]
    fn a_non_minimal_safe_boot_is_kept() {
        let state = state_at(RebootStage::PhaseOneSafeModeArmed);
        let host = FakeHost::new(SafebootEvidence::Configured("network".into()), None, None);
        let HandoffAbort::Incomplete(report) = abort_handoff_with(&host, &state, work_dir()) else {
            panic!("expected an incomplete abort");
        };
        assert!(host.calls.borrow().is_empty());
        assert_eq!(report.remaining.len(), 1);
        assert!(report.remaining[0].contains("safeboot network"));
    }

    #[test]
    fn minimal_safe_boot_after_the_workflow_cleared_its_own_is_kept() {
        for stage in [
            RebootStage::PhaseTwoSafeMode,
            RebootStage::PhaseThreeArmed,
            RebootStage::PhaseThreeComplete,
            RebootStage::RecoveryRequired,
        ] {
            let state = state_at(stage.clone());
            let host = FakeHost::new(minimal(), None, None);
            let HandoffAbort::Incomplete(report) = abort_handoff_with(&host, &state, work_dir())
            else {
                panic!("expected an incomplete abort");
            };
            assert!(host.calls.borrow().is_empty());
            assert_eq!(report.remaining.len(), 1);
            assert!(report.remaining[0].contains(stage.as_str()));
            assert!(report.remaining[0].contains("already cleared its Safe Boot"));
        }
    }

    #[test]
    fn already_absent_values_after_a_stranded_p2_1_clear_the_transaction() {
        let state = state_at(RebootStage::PhaseOneSafeModeArmed);
        let host = FakeHost::new(SafebootEvidence::Absent, None, None);
        let HandoffAbort::Cleared(report) = abort_handoff_with(&host, &state, work_dir()) else {
            panic!("expected a cleared abort");
        };
        assert!(host.calls.borrow().is_empty());
        assert!(report.removed.is_empty());
        assert_eq!(report.already_absent.len(), 3);
    }

    #[test]
    fn an_unknown_runtime_removes_no_registry_value() {
        let mut state = state_at(RebootStage::PhaseOneSafeModeArmed);
        if let Some(transaction) = state.active_reboot_transaction.as_mut() {
            transaction.runtime = None;
        }
        let host = FakeHost::new(SafebootEvidence::Absent, Some("phase2".into()), None);
        let HandoffAbort::Incomplete(report) = abort_handoff_with(&host, &state, work_dir()) else {
            panic!("expected an incomplete abort");
        };
        assert!(host.calls.borrow().is_empty());
        assert!(report.remaining[0].contains("expected command is unknown"));
    }
}
