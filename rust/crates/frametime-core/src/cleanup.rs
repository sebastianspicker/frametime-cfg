/// Operator-selected cleanup scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMode {
    Quick,
    Full,
    Driver,
}

/// A bounded, exact class of resource a cleanup action may affect.
///
/// Paths are deliberately resolved by a platform adapter.  The planner never
/// accepts arbitrary paths, recursive roots, or Steam library identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupTargetClass {
    Cs2App730ShaderCache,
    WindowsTemp,
    CurrentUserTemp,
    DnsResolverCache,
    SystemFileCacheWorkingSet,
    NvidiaDxShaderCache,
    NvidiaGlShaderCache,
    DirectXShaderCache,
    AmdDxShaderCache,
    WindowsPrefetchPfFiles,
    WinsockCatalog,
    ApplicationEventLog,
    SystemEventLog,
    SetupEventLog,
    SteamApp730IntegrityValidation,
    DriverRefreshRuntimeHandoff,
}

/// Target classes that are categorically outside every cleanup mode.
///
/// Keeping these in the core contract makes the exclusions reviewable by
/// native adapters and callers before they ever resolve a filesystem path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeniedCleanupTargetClass {
    SuiteLogs,
    SuiteBackupStateProgressOrRuntime,
    WindowsDriverStore,
    Cs2InstallOrContent,
    Non730SteamLibrary,
}

/// The recovery statement attached to one bounded action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupRecovery {
    /// The operating system, GPU driver, or CS2 rebuilds the cache as needed.
    RebuiltByOwner,
    /// The action resets the subsystem to its system default; custom providers
    /// may need manual reconfiguration after a restart.
    ResetToSystemDefault,
    /// Deleted records cannot be restored by this suite.
    NotRecoverable,
    /// No cleanup occurs until a separately verified runtime handoff exists.
    RequiresVerifiedRuntimeHandoff,
}

/// Declarative safety and recovery information for one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupActionSpec {
    pub action: CleanupAction,
    pub target: CleanupTargetClass,
    pub irreversible: bool,
    pub recovery: CleanupRecovery,
}

/// Named actions are typed so partial outcomes cannot be attributed to a
/// free-form display string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupAction {
    ClearCs2App730ShaderCache,
    ClearWindowsTemp,
    ClearCurrentUserTemp,
    FlushDnsResolverCache,
    TrimSystemFileCacheWorkingSet,
    ClearNvidiaDxShaderCache,
    ClearNvidiaGlShaderCache,
    ClearDirectXShaderCache,
    ClearAmdDxShaderCache,
    DeleteWindowsPrefetchPfFiles,
    ResetWinsockCatalog,
    ClearApplicationEventLog,
    ClearSystemEventLog,
    ClearSetupEventLog,
    RequestSteamApp730IntegrityValidation,
    ArmDriverRefreshRuntimeHandoff,
}

/// A structured result for one planned action.  Native adapters must report a
/// result for each attempted action instead of collapsing partial work into one
/// success message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupActionResult {
    pub action: CleanupAction,
    pub outcome: CleanupActionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupActionOutcome {
    Completed {
        affected_items: usize,
    },
    /// The exact target does not exist on this host or is not relevant to its
    /// installed hardware. This is a successful no-op, not a failure.
    Inapplicable {
        reason: String,
    },
    /// The operation has a deliberately retained safety or platform gate.
    /// Callers can distinguish it from both absence and an attempted failure.
    Deferred {
        reason: String,
    },
    Skipped {
        reason: String,
    },
    Failed {
        reason: String,
    },
}

impl CleanupActionResult {
    #[must_use]
    pub const fn failed(&self) -> bool {
        matches!(self.outcome, CleanupActionOutcome::Failed { .. })
    }
}

/// Aggregate result of one cleanup request.  A report with one failed action
/// is explicitly partial even when other actions completed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub action_results: Vec<CleanupActionResult>,
    pub restart_required: bool,
}

impl CleanupReport {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.action_results.iter().any(|result| {
            matches!(
                result.outcome,
                CleanupActionOutcome::Deferred { .. } | CleanupActionOutcome::Failed { .. }
            )
        })
    }

    #[must_use]
    pub fn affected_items(&self) -> usize {
        self.action_results
            .iter()
            .map(|result| match result.outcome {
                CleanupActionOutcome::Completed { affected_items } => affected_items,
                CleanupActionOutcome::Inapplicable { .. }
                | CleanupActionOutcome::Deferred { .. }
                | CleanupActionOutcome::Skipped { .. }
                | CleanupActionOutcome::Failed { .. } => 0,
            })
            .sum()
    }
}

const QUICK_ACTIONS: &[CleanupActionSpec] = &[
    CleanupActionSpec {
        action: CleanupAction::ClearCs2App730ShaderCache,
        target: CleanupTargetClass::Cs2App730ShaderCache,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearWindowsTemp,
        target: CleanupTargetClass::WindowsTemp,
        irreversible: true,
        recovery: CleanupRecovery::NotRecoverable,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearCurrentUserTemp,
        target: CleanupTargetClass::CurrentUserTemp,
        irreversible: true,
        recovery: CleanupRecovery::NotRecoverable,
    },
    CleanupActionSpec {
        action: CleanupAction::FlushDnsResolverCache,
        target: CleanupTargetClass::DnsResolverCache,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
    CleanupActionSpec {
        action: CleanupAction::TrimSystemFileCacheWorkingSet,
        target: CleanupTargetClass::SystemFileCacheWorkingSet,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
];

const FULL_ACTIONS: &[CleanupActionSpec] = &[
    CleanupActionSpec {
        action: CleanupAction::ClearNvidiaDxShaderCache,
        target: CleanupTargetClass::NvidiaDxShaderCache,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearNvidiaGlShaderCache,
        target: CleanupTargetClass::NvidiaGlShaderCache,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearDirectXShaderCache,
        target: CleanupTargetClass::DirectXShaderCache,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearAmdDxShaderCache,
        target: CleanupTargetClass::AmdDxShaderCache,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
    CleanupActionSpec {
        action: CleanupAction::DeleteWindowsPrefetchPfFiles,
        target: CleanupTargetClass::WindowsPrefetchPfFiles,
        irreversible: true,
        recovery: CleanupRecovery::NotRecoverable,
    },
    CleanupActionSpec {
        action: CleanupAction::ResetWinsockCatalog,
        target: CleanupTargetClass::WinsockCatalog,
        irreversible: true,
        recovery: CleanupRecovery::ResetToSystemDefault,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearApplicationEventLog,
        target: CleanupTargetClass::ApplicationEventLog,
        irreversible: true,
        recovery: CleanupRecovery::NotRecoverable,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearSystemEventLog,
        target: CleanupTargetClass::SystemEventLog,
        irreversible: true,
        recovery: CleanupRecovery::NotRecoverable,
    },
    CleanupActionSpec {
        action: CleanupAction::ClearSetupEventLog,
        target: CleanupTargetClass::SetupEventLog,
        irreversible: true,
        recovery: CleanupRecovery::NotRecoverable,
    },
    CleanupActionSpec {
        action: CleanupAction::RequestSteamApp730IntegrityValidation,
        target: CleanupTargetClass::SteamApp730IntegrityValidation,
        irreversible: false,
        recovery: CleanupRecovery::RebuiltByOwner,
    },
];

const DRIVER_ACTIONS: &[CleanupActionSpec] = &[CleanupActionSpec {
    action: CleanupAction::ArmDriverRefreshRuntimeHandoff,
    target: CleanupTargetClass::DriverRefreshRuntimeHandoff,
    irreversible: false,
    recovery: CleanupRecovery::RequiresVerifiedRuntimeHandoff,
}];

const DENIED_TARGETS: &[DeniedCleanupTargetClass] = &[
    DeniedCleanupTargetClass::SuiteLogs,
    DeniedCleanupTargetClass::SuiteBackupStateProgressOrRuntime,
    DeniedCleanupTargetClass::WindowsDriverStore,
    DeniedCleanupTargetClass::Cs2InstallOrContent,
    DeniedCleanupTargetClass::Non730SteamLibrary,
];

/// The exact action specifications for a mode.  Full includes the quick
/// baseline.  Driver performs no cleanup actions until its runtime handoff is
/// available; it does not silently fall through to the Full action set.
#[must_use]
pub fn cleanup_actions(mode: CleanupMode) -> Vec<CleanupActionSpec> {
    match mode {
        CleanupMode::Quick => QUICK_ACTIONS.to_vec(),
        CleanupMode::Full => QUICK_ACTIONS.iter().chain(FULL_ACTIONS).copied().collect(),
        CleanupMode::Driver => DRIVER_ACTIONS.to_vec(),
    }
}

/// Classes that native cleanup adapters must refuse, regardless of mode.
#[must_use]
pub const fn denied_cleanup_targets() -> &'static [DeniedCleanupTargetClass] {
    DENIED_TARGETS
}

/// Full cleanup has destructive system-reset and record-deletion effects and
/// therefore needs an acknowledgement in addition to ordinary confirmation.
#[must_use]
pub const fn requires_irreversible_acknowledgement(mode: CleanupMode) -> bool {
    matches!(mode, CleanupMode::Full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_contract_composes_quick_and_full_actions_with_recovery_declarations() {
        let actions = cleanup_actions(CleanupMode::Full);
        assert!(actions.iter().any(|spec| {
            spec.action == CleanupAction::ClearCs2App730ShaderCache
                && spec.target == CleanupTargetClass::Cs2App730ShaderCache
                && spec.recovery == CleanupRecovery::RebuiltByOwner
        }));
        assert!(actions.iter().any(|spec| {
            spec.action == CleanupAction::ResetWinsockCatalog
                && spec.irreversible
                && spec.recovery == CleanupRecovery::ResetToSystemDefault
        }));
        assert!(requires_irreversible_acknowledgement(CleanupMode::Full));
    }

    #[test]
    fn driver_contract_is_handoff_only_and_hard_denials_are_complete() {
        assert_eq!(
            cleanup_actions(CleanupMode::Driver),
            DRIVER_ACTIONS.to_vec(),
            "driver cleanup must not execute Full actions before a handoff"
        );
        assert_eq!(
            denied_cleanup_targets(),
            [
                DeniedCleanupTargetClass::SuiteLogs,
                DeniedCleanupTargetClass::SuiteBackupStateProgressOrRuntime,
                DeniedCleanupTargetClass::WindowsDriverStore,
                DeniedCleanupTargetClass::Cs2InstallOrContent,
                DeniedCleanupTargetClass::Non730SteamLibrary,
            ]
        );
    }

    #[test]
    fn failed_action_makes_cleanup_partial_without_losing_completed_counts() {
        let report = CleanupReport {
            action_results: vec![
                CleanupActionResult {
                    action: CleanupAction::ClearWindowsTemp,
                    outcome: CleanupActionOutcome::Completed { affected_items: 3 },
                },
                CleanupActionResult {
                    action: CleanupAction::FlushDnsResolverCache,
                    outcome: CleanupActionOutcome::Failed {
                        reason: "access denied".into(),
                    },
                },
            ],
            ..CleanupReport::default()
        };
        assert_eq!(report.affected_items(), 3);
        assert!(!report.is_complete());
    }

    #[test]
    fn deferred_outcomes_keep_cleanup_partial_without_inflating_counts() {
        let report = CleanupReport {
            action_results: vec![
                CleanupActionResult {
                    action: CleanupAction::ClearAmdDxShaderCache,
                    outcome: CleanupActionOutcome::Inapplicable {
                        reason: "no AMD display adapter".into(),
                    },
                },
                CleanupActionResult {
                    action: CleanupAction::ResetWinsockCatalog,
                    outcome: CleanupActionOutcome::Deferred {
                        reason: "no shellless documented API".into(),
                    },
                },
            ],
            ..CleanupReport::default()
        };
        assert!(!report.is_complete());
        assert_eq!(report.affected_items(), 0);
    }
}
