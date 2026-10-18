use crate::*;
// Rollback of a partially armed P1:38 handoff, behind a small host boundary so
// the decision logic is testable without BCD or the registry.

pub(crate) trait ArmingHost {
    fn safe_boot(&self) -> SafebootEvidence;
    fn clear_safe_boot(&self) -> Result<(), String>;
    fn phase_two_handoff(&self) -> Result<Option<String>, String>;
    fn delete_phase_two_handoff(&self) -> Result<(), String>;
}

pub(crate) struct NativeArmingHost;

impl ArmingHost for NativeArmingHost {
    fn safe_boot(&self) -> SafebootEvidence {
        safeboot_evidence()
    }
    fn clear_safe_boot(&self) -> Result<(), String> {
        clear_safeboot()
    }
    fn phase_two_handoff(&self) -> Result<Option<String>, String> {
        read_handoff_value(Hive::LocalMachine, RUNONCE_KEY, PHASE2_HANDOFF)
    }
    fn delete_phase_two_handoff(&self) -> Result<(), String> {
        registry_delete(Hive::LocalMachine, RUNONCE_KEY, PHASE2_HANDOFF)
    }
}

/// Undo only values this call created and that still hold what it wrote, so a
/// value planted or changed by someone else is never removed.
pub(crate) fn roll_back_partial_arming(
    host: &impl ArmingHost,
    error: String,
    phase2: &str,
    wrote_handoff: bool,
    set_safe_boot: bool,
) -> String {
    let mut failures = Vec::new();
    if set_safe_boot {
        match host.safe_boot() {
            SafebootEvidence::Configured(value) if value == "minimal" => {
                if let Err(rollback) = host.clear_safe_boot() {
                    failures.push(rollback);
                }
            }
            SafebootEvidence::Absent => {}
            SafebootEvidence::Configured(value) => {
                failures.push(format!(
                    "safeboot {value} was set by someone else; left in place"
                ));
            }
            SafebootEvidence::Unavailable => failures.push("safeboot state is unreadable".into()),
        }
    }
    if wrote_handoff {
        match host.phase_two_handoff() {
            Ok(Some(value)) if value == phase2 => {
                if let Err(rollback) = host.delete_phase_two_handoff() {
                    failures.push(rollback);
                }
            }
            Ok(None) => {}
            Ok(Some(_)) => {
                failures.push("RunOnce handoff was changed by someone else; left in place".into())
            }
            Err(rollback) => failures.push(rollback),
        }
    }
    if failures.is_empty() {
        format!(
            "{error}; partial P1:38 arming was rolled back; rerun optimize, and if it refuses because of an armed handoff or active reboot transaction, follow the stuck-handoff procedure in recovery.md"
        )
    } else {
        format!(
            "{error}; P1:38 rollback incomplete ({}); follow the stuck-handoff procedure in recovery.md",
            failures.join("; ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const PHASE2: &str = "\"C:\\FRAMETIME_CFG\\frametime.exe\" phase2 --yes";

    struct FakeHost {
        safe_boot: SafebootEvidence,
        handoff: Option<String>,
        delete_fails: bool,
        clear_fails: bool,
        read_fails: bool,
        calls: RefCell<Vec<&'static str>>,
    }

    impl FakeHost {
        fn new(safe_boot: SafebootEvidence, handoff: Option<&str>) -> Self {
            Self {
                safe_boot,
                handoff: handoff.map(str::to_owned),
                delete_fails: false,
                clear_fails: false,
                read_fails: false,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl ArmingHost for FakeHost {
        fn safe_boot(&self) -> SafebootEvidence {
            self.safe_boot.clone()
        }
        fn clear_safe_boot(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("clear safe boot");
            if self.clear_fails {
                Err("clear failed".into())
            } else {
                Ok(())
            }
        }
        fn phase_two_handoff(&self) -> Result<Option<String>, String> {
            if self.read_fails {
                Err("read failed".into())
            } else {
                Ok(self.handoff.clone())
            }
        }
        fn delete_phase_two_handoff(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("delete handoff");
            if self.delete_fails {
                Err("delete failed".into())
            } else {
                Ok(())
            }
        }
    }

    fn minimal() -> SafebootEvidence {
        SafebootEvidence::Configured("minimal".into())
    }

    #[test]
    fn rolls_back_both_values_this_call_wrote() {
        let host = FakeHost::new(minimal(), Some(PHASE2));
        let message = roll_back_partial_arming(&host, "persist".into(), PHASE2, true, true);
        assert_eq!(*host.calls.borrow(), ["clear safe boot", "delete handoff"]);
        assert!(message.starts_with("persist; partial P1:38 arming was rolled back"));
    }

    #[test]
    fn never_touches_values_that_existed_before_the_call() {
        let host = FakeHost::new(minimal(), Some(PHASE2));
        roll_back_partial_arming(&host, "persist".into(), PHASE2, false, false);
        assert!(host.calls.borrow().is_empty());
    }

    #[test]
    fn skips_steps_that_never_ran() {
        let host = FakeHost::new(SafebootEvidence::Absent, None);
        let message = roll_back_partial_arming(&host, "differs".into(), PHASE2, true, true);
        assert!(host.calls.borrow().is_empty());
        assert!(message.contains("was rolled back"));
    }

    #[test]
    fn leaves_values_changed_by_someone_else_and_says_so() {
        let host = FakeHost::new(
            SafebootEvidence::Configured("network".into()),
            Some("planted phase2"),
        );
        let message = roll_back_partial_arming(&host, "differs".into(), PHASE2, true, true);
        assert!(host.calls.borrow().is_empty());
        assert!(message.contains("rollback incomplete"));
        assert!(message.contains("safeboot network was set by someone else"));
        assert!(message.contains("RunOnce handoff was changed by someone else"));
    }

    #[test]
    fn unreadable_state_is_an_incomplete_rollback() {
        let mut host = FakeHost::new(SafebootEvidence::Unavailable, Some(PHASE2));
        host.read_fails = true;
        let message = roll_back_partial_arming(&host, "persist".into(), PHASE2, true, true);
        assert!(host.calls.borrow().is_empty());
        assert!(message.contains("safeboot state is unreadable"));
        assert!(message.contains("read failed"));
    }

    #[test]
    fn rolls_back_only_safe_boot_when_the_handoff_pre_existed() {
        let mut host = FakeHost::new(minimal(), Some(PHASE2));
        host.clear_fails = true;
        let message = roll_back_partial_arming(&host, "persist".into(), PHASE2, false, true);
        assert_eq!(*host.calls.borrow(), ["clear safe boot"]);
        assert!(message.contains("rollback incomplete (clear failed)"));
    }

    #[test]
    fn reports_an_incomplete_rollback() {
        let mut host = FakeHost::new(SafebootEvidence::Absent, Some(PHASE2));
        host.delete_fails = true;
        let message = roll_back_partial_arming(&host, "persist".into(), PHASE2, true, true);
        assert_eq!(*host.calls.borrow(), ["delete handoff"]);
        assert!(message.contains("rollback incomplete (delete failed)"));
    }
}
