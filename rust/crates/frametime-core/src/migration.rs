use crate::{Progress, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyHandoff {
    None,
    PhaseTwoArmed,
    PhaseThreeArmed,
    SafeBootArmed,
    IncompleteRuntime,
}

/// Read-only inventory collected before migration writes anything.  The
/// platform layer must inspect the fixed Run/RunOnce values and BCD directly;
/// an unavailable query is represented as `incomplete_runtime` by callers
/// that cannot establish a safe clean start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MigrationInventory {
    pub phase_two_run_once_armed: bool,
    pub phase_three_run_armed: bool,
    pub safe_boot_armed: bool,
    pub incomplete_runtime: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MigrationDecision {
    NotNeeded,
    ConfirmIdle,
    ConfirmPartialPhaseOne { completed: usize, skipped: usize },
    Refuse(LegacyHandoff),
}

pub fn assess(
    state: Option<&State>,
    progress: Option<&Progress>,
    handoff: LegacyHandoff,
) -> MigrationDecision {
    let inventory = match handoff {
        LegacyHandoff::None => MigrationInventory::default(),
        LegacyHandoff::PhaseTwoArmed => MigrationInventory {
            phase_two_run_once_armed: true,
            ..MigrationInventory::default()
        },
        LegacyHandoff::PhaseThreeArmed => MigrationInventory {
            phase_three_run_armed: true,
            ..MigrationInventory::default()
        },
        LegacyHandoff::SafeBootArmed => MigrationInventory {
            safe_boot_armed: true,
            ..MigrationInventory::default()
        },
        LegacyHandoff::IncompleteRuntime => MigrationInventory {
            incomplete_runtime: true,
            ..MigrationInventory::default()
        },
    };
    assess_inventory(state, progress, inventory)
}

/// Migration is never allowed to discard an armed reboot mechanism or a
/// runtime that cannot be proved complete.  Idle and P1-only history still
/// requires an explicit caller confirmation through the returned decision.
pub fn assess_inventory(
    state: Option<&State>,
    progress: Option<&Progress>,
    inventory: MigrationInventory,
) -> MigrationDecision {
    if inventory.phase_two_run_once_armed {
        return MigrationDecision::Refuse(LegacyHandoff::PhaseTwoArmed);
    }
    if inventory.phase_three_run_armed {
        return MigrationDecision::Refuse(LegacyHandoff::PhaseThreeArmed);
    }
    if inventory.safe_boot_armed {
        return MigrationDecision::Refuse(LegacyHandoff::SafeBootArmed);
    }
    if inventory.incomplete_runtime {
        return MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime);
    }
    if state.is_none() && progress.is_none() {
        return MigrationDecision::NotNeeded;
    }
    let p2_or_p3 = progress.is_some_and(|progress| {
        progress
            .completed_steps
            .iter()
            .chain(&progress.skipped_steps)
            .any(|key| key.starts_with("P2:") || key.starts_with("P3:"))
    });
    let phase_one_armed = state.is_some_and(|state| {
        state.phase1_safe_mode_ready
            || state.active_reboot_transaction.is_some()
            || state.unknown.contains_key("activeRebootTransaction")
    });
    if p2_or_p3 || phase_one_armed {
        return MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime);
    }
    let Some(progress) = progress else {
        return MigrationDecision::ConfirmIdle;
    };
    let completed = progress
        .completed_steps
        .iter()
        .filter(|key| key.starts_with("P1:"))
        .count();
    let skipped = progress
        .skipped_steps
        .iter()
        .filter(|key| key.starts_with("P1:"))
        .count();
    if completed + skipped == 0 {
        MigrationDecision::ConfirmIdle
    } else {
        MigrationDecision::ConfirmPartialPhaseOne { completed, skipped }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_armed_handoff() {
        assert_eq!(
            assess(
                Some(&State::default()),
                Some(&Progress::default()),
                LegacyHandoff::PhaseTwoArmed
            ),
            MigrationDecision::Refuse(LegacyHandoff::PhaseTwoArmed)
        );
    }

    #[test]
    fn orphan_partial_progress_still_requires_migration_confirmation() {
        let mut progress = Progress::default();
        progress.complete(1, 1, "now".into());
        assert_eq!(
            assess(None, Some(&progress), LegacyHandoff::None),
            MigrationDecision::ConfirmPartialPhaseOne {
                completed: 1,
                skipped: 0
            }
        );
    }

    #[test]
    fn idle_and_skipped_phase_one_history_require_confirmation() {
        assert_eq!(
            assess_inventory(
                Some(&State::default()),
                Some(&Progress::default()),
                MigrationInventory::default()
            ),
            MigrationDecision::ConfirmIdle
        );
        let mut progress = Progress::default();
        progress.skip(1, 7);
        assert_eq!(
            assess_inventory(
                Some(&State::default()),
                Some(&progress),
                MigrationInventory::default()
            ),
            MigrationDecision::ConfirmPartialPhaseOne {
                completed: 0,
                skipped: 1
            }
        );
    }

    #[test]
    fn orphan_phase_two_progress_fails_closed() {
        let mut progress = Progress::default();
        progress.complete(2, 1, "now".into());
        assert_eq!(
            assess(None, Some(&progress), LegacyHandoff::None),
            MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime)
        );
    }

    #[test]
    fn inventory_refuses_every_armed_reboot_artifact() {
        for (inventory, expected) in [
            (
                MigrationInventory {
                    phase_two_run_once_armed: true,
                    ..MigrationInventory::default()
                },
                LegacyHandoff::PhaseTwoArmed,
            ),
            (
                MigrationInventory {
                    phase_three_run_armed: true,
                    ..MigrationInventory::default()
                },
                LegacyHandoff::PhaseThreeArmed,
            ),
            (
                MigrationInventory {
                    safe_boot_armed: true,
                    ..MigrationInventory::default()
                },
                LegacyHandoff::SafeBootArmed,
            ),
            (
                MigrationInventory {
                    incomplete_runtime: true,
                    ..MigrationInventory::default()
                },
                LegacyHandoff::IncompleteRuntime,
            ),
        ] {
            assert_eq!(
                assess_inventory(
                    Some(&State::default()),
                    Some(&Progress::default()),
                    inventory
                ),
                MigrationDecision::Refuse(expected)
            );
        }
    }

    #[test]
    fn phase_one_readiness_flag_refuses_migration() {
        let state = State {
            phase1_safe_mode_ready: true,
            ..State::default()
        };
        assert_eq!(
            assess_inventory(
                Some(&state),
                Some(&Progress::default()),
                MigrationInventory::default()
            ),
            MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime)
        );
    }

    #[test]
    fn typed_reboot_transaction_refuses_migration() {
        let state = State {
            active_reboot_transaction: Some(crate::RebootTransaction::default()),
            ..State::default()
        };
        assert_eq!(
            assess_inventory(
                Some(&state),
                Some(&Progress::default()),
                MigrationInventory::default()
            ),
            MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime)
        );
    }

    #[test]
    fn malformed_preserved_reboot_transaction_refuses_migration() {
        let state: State =
            serde_json::from_str(r#"{"activeRebootTransaction":false}"#).expect("tolerant state");
        assert!(state.active_reboot_transaction.is_none());
        assert!(state.unknown.contains_key("activeRebootTransaction"));
        assert_eq!(
            assess_inventory(
                Some(&state),
                Some(&Progress::default()),
                MigrationInventory::default()
            ),
            MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime)
        );
    }
}
