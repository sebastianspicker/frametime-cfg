use crate::{
    catalog::Phase,
    state::{Progress, State},
};

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
#[expect(
    clippy::struct_excessive_bools,
    reason = "the public cross-crate migration observation layout must retain its four named compatibility fields"
)]
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
    if let Some(handoff) = armed_inventory_handoff(inventory) {
        return MigrationDecision::Refuse(handoff);
    }
    if no_migration_state_exists(state, progress) {
        return MigrationDecision::NotNeeded;
    }
    if has_incomplete_runtime_state(state, progress) {
        return MigrationDecision::Refuse(LegacyHandoff::IncompleteRuntime);
    }
    let Some(progress) = progress else {
        return MigrationDecision::ConfirmIdle;
    };
    let (completed, skipped) = phase_one_progress_counts(progress);
    if completed + skipped == 0 {
        MigrationDecision::ConfirmIdle
    } else {
        MigrationDecision::ConfirmPartialPhaseOne { completed, skipped }
    }
}

fn armed_inventory_handoff(inventory: MigrationInventory) -> Option<LegacyHandoff> {
    if inventory.phase_two_run_once_armed {
        return Some(LegacyHandoff::PhaseTwoArmed);
    }
    if inventory.phase_three_run_armed {
        return Some(LegacyHandoff::PhaseThreeArmed);
    }
    if inventory.safe_boot_armed {
        return Some(LegacyHandoff::SafeBootArmed);
    }
    inventory
        .incomplete_runtime
        .then_some(LegacyHandoff::IncompleteRuntime)
}

fn no_migration_state_exists(state: Option<&State>, progress: Option<&Progress>) -> bool {
    state.is_none() && progress.is_none()
}

fn has_incomplete_runtime_state(state: Option<&State>, progress: Option<&Progress>) -> bool {
    let has_later_phase_progress = progress.is_some_and(has_resolved_later_phase_progress);
    let has_armed_phase_one_state = state.is_some_and(state_has_armed_phase_one_handoff);
    has_later_phase_progress || has_armed_phase_one_state
}

fn has_resolved_later_phase_progress(progress: &Progress) -> bool {
    let phase_two_is_resolved = progress.has_resolved_in_phase(Phase::Two);
    let phase_three_is_resolved = progress.has_resolved_in_phase(Phase::Three);
    phase_two_is_resolved || phase_three_is_resolved
}

fn state_has_armed_phase_one_handoff(state: &State) -> bool {
    let safe_mode_is_ready = state.phase1_safe_mode_ready;
    let has_typed_transaction = state.active_reboot_transaction.is_some();
    let has_tolerated_legacy_transaction = state.unknown.contains_key("activeRebootTransaction");
    safe_mode_is_ready || has_typed_transaction || has_tolerated_legacy_transaction
}

fn phase_one_progress_counts(progress: &Progress) -> (usize, usize) {
    let completed = crate::catalog::step_catalog()
        .iter()
        .filter(|step| step.id.phase == Phase::One && progress.is_completed(step.id))
        .count();
    let skipped = crate::catalog::step_catalog()
        .iter()
        .filter(|step| step.id.phase == Phase::One && progress.is_skipped(step.id))
        .count();
    (completed, skipped)
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
    fn inventory_preserves_armed_handoff_refusal_precedence() {
        let inventory = MigrationInventory {
            phase_two_run_once_armed: true,
            phase_three_run_armed: true,
            safe_boot_armed: true,
            incomplete_runtime: true,
        };
        assert_eq!(
            assess_inventory(
                Some(&State::default()),
                Some(&Progress::default()),
                inventory
            ),
            MigrationDecision::Refuse(LegacyHandoff::PhaseTwoArmed)
        );
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
            active_reboot_transaction: Some(crate::handoff::RebootTransaction::default()),
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
