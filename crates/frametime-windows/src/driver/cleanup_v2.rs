//! Typed, evidence-first NVIDIA cleanup coordinator.
//!
//! Native SetupAPI, Configuration Manager, SCM, Registry, Deployment/AppX,
//! and filesystem adapters implement this contract. The coordinator never
//! accepts commands, scripts, wildcard targets, or caller-selected roots.

use std::collections::BTreeSet;

use frametime_domain::driver::NvidiaCleanupScope;

const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NvidiaCleanupProof {
    ExactIdentityMatches,
    ReplacementAuthenticated,
    SafeModeReady,
    RecoveryPersisted,
    BasicDisplayFallback,
}

const REQUIRED_CLEANUP_PROOFS: [NvidiaCleanupProof; 5] = [
    NvidiaCleanupProof::ExactIdentityMatches,
    NvidiaCleanupProof::ReplacementAuthenticated,
    NvidiaCleanupProof::SafeModeReady,
    NvidiaCleanupProof::RecoveryPersisted,
    NvidiaCleanupProof::BasicDisplayFallback,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaCleanupPreconditions {
    verified: BTreeSet<NvidiaCleanupProof>,
}

impl NvidiaCleanupPreconditions {
    fn is_complete(&self) -> bool {
        REQUIRED_CLEANUP_PROOFS
            .iter()
            .all(|proof| self.verified.contains(proof))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvidiaCleanupSubsystem {
    Device,
    DriverStore,
    Service,
    Registry,
    Filesystem,
    Cache,
    Appx,
    Monitor,
    DriverSearchPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaCleanupTarget {
    pub subsystem: NvidiaCleanupSubsystem,
    pub identity: String,
    pub scope: NvidiaCleanupScope,
    pub reversible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaCleanupReport {
    pub completed: Vec<NvidiaCleanupTarget>,
    pub skipped_optional: Vec<NvidiaCleanupTarget>,
    pub recovered: Vec<NvidiaCleanupTarget>,
}

pub trait NvidiaCleanupAdapter {
    type Capture;

    fn capture(&mut self, target: &NvidiaCleanupTarget) -> Result<Self::Capture, String>;
    fn serialized_capture_bytes(&self, capture: &Self::Capture) -> Result<usize, String>;
    fn persist_capture(
        &mut self,
        target: &NvidiaCleanupTarget,
        capture: &Self::Capture,
    ) -> Result<(), String>;
    fn mutate(&mut self, target: &NvidiaCleanupTarget) -> Result<(), String>;
    fn verify(&mut self, target: &NvidiaCleanupTarget) -> Result<(), String>;
    fn recover(
        &mut self,
        target: &NvidiaCleanupTarget,
        capture: &Self::Capture,
    ) -> Result<(), String>;
}

pub fn execute_nvidia_cleanup<A: NvidiaCleanupAdapter>(
    preconditions: &NvidiaCleanupPreconditions,
    selected_optional_scopes: &BTreeSet<NvidiaCleanupScope>,
    targets: &[NvidiaCleanupTarget],
    adapter: &mut A,
) -> Result<NvidiaCleanupReport, String> {
    if !preconditions.is_complete() {
        return Err("NVIDIA cleanup preconditions are incomplete".into());
    }
    let mut report = NvidiaCleanupReport {
        completed: Vec::new(),
        skipped_optional: Vec::new(),
        recovered: Vec::new(),
    };
    let mut captures: Vec<(NvidiaCleanupTarget, A::Capture)> = Vec::new();
    for target in targets {
        validate_target(target)?;
        if target.scope != NvidiaCleanupScope::Default
            && !selected_optional_scopes.contains(&target.scope)
        {
            report.skipped_optional.push(target.clone());
            continue;
        }
        let capture = adapter.capture(target)?;
        if adapter.serialized_capture_bytes(&capture)? > MAX_CAPTURE_BYTES {
            return Err("NVIDIA cleanup capture exceeds the per-target bound".into());
        }
        adapter.persist_capture(target, &capture)?;
        if let Err(reason) = adapter.mutate(target).and_then(|()| adapter.verify(target)) {
            recover_target(adapter, target, &capture, &mut report)?;
            recover_applied(adapter, &captures, &mut report)?;
            return Err(format!(
                "NVIDIA cleanup stopped after {}: {reason}",
                target.identity
            ));
        }
        report.completed.push(target.clone());
        captures.push((target.clone(), capture));
    }
    Ok(report)
}

fn validate_target(target: &NvidiaCleanupTarget) -> Result<(), String> {
    if target.identity.is_empty() || target.identity.chars().any(char::is_control) {
        Err("NVIDIA cleanup target identity is invalid".into())
    } else {
        Ok(())
    }
}

fn recover_target<A: NvidiaCleanupAdapter>(
    adapter: &mut A,
    target: &NvidiaCleanupTarget,
    capture: &A::Capture,
    report: &mut NvidiaCleanupReport,
) -> Result<(), String> {
    if target.reversible {
        adapter.recover(target, capture)?;
        report.recovered.push(target.clone());
    }
    Ok(())
}

fn recover_applied<A: NvidiaCleanupAdapter>(
    adapter: &mut A,
    captures: &[(NvidiaCleanupTarget, A::Capture)],
    report: &mut NvidiaCleanupReport,
) -> Result<(), String> {
    for (target, capture) in captures.iter().rev() {
        recover_target(adapter, target, capture, report)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingAdapter {
        events: Vec<String>,
        fail: Option<String>,
    }

    impl NvidiaCleanupAdapter for RecordingAdapter {
        type Capture = String;

        fn capture(&mut self, target: &NvidiaCleanupTarget) -> Result<Self::Capture, String> {
            self.events.push(format!("capture:{}", target.identity));
            Ok(format!("state:{}", target.identity))
        }

        fn serialized_capture_bytes(&self, capture: &Self::Capture) -> Result<usize, String> {
            Ok(capture.len())
        }

        fn persist_capture(
            &mut self,
            target: &NvidiaCleanupTarget,
            _capture: &Self::Capture,
        ) -> Result<(), String> {
            self.events.push(format!("persist:{}", target.identity));
            Ok(())
        }

        fn mutate(&mut self, target: &NvidiaCleanupTarget) -> Result<(), String> {
            self.events.push(format!("mutate:{}", target.identity));
            if self.fail.as_deref() == Some(&target.identity) {
                Err("injected failure".into())
            } else {
                Ok(())
            }
        }

        fn verify(&mut self, target: &NvidiaCleanupTarget) -> Result<(), String> {
            self.events.push(format!("verify:{}", target.identity));
            Ok(())
        }

        fn recover(
            &mut self,
            target: &NvidiaCleanupTarget,
            _capture: &Self::Capture,
        ) -> Result<(), String> {
            self.events.push(format!("recover:{}", target.identity));
            Ok(())
        }
    }

    fn ready() -> NvidiaCleanupPreconditions {
        NvidiaCleanupPreconditions {
            verified: REQUIRED_CLEANUP_PROOFS.into_iter().collect(),
        }
    }

    fn target(
        subsystem: NvidiaCleanupSubsystem,
        identity: &str,
        scope: NvidiaCleanupScope,
    ) -> NvidiaCleanupTarget {
        NvidiaCleanupTarget {
            subsystem,
            identity: identity.into(),
            scope,
            reversible: true,
        }
    }

    #[test]
    fn default_preserves_every_optional_scope() {
        let targets = [
            target(
                NvidiaCleanupSubsystem::Device,
                "gpu",
                NvidiaCleanupScope::Default,
            ),
            target(
                NvidiaCleanupSubsystem::Cache,
                "driver-cache",
                NvidiaCleanupScope::Default,
            ),
            target(
                NvidiaCleanupSubsystem::Appx,
                "nvcpl",
                NvidiaCleanupScope::ControlPanel,
            ),
            target(
                NvidiaCleanupSubsystem::Monitor,
                "monitor",
                NvidiaCleanupScope::Monitors,
            ),
            target(
                NvidiaCleanupSubsystem::DriverSearchPolicy,
                "search-policy",
                NvidiaCleanupScope::DriverSearchPolicy,
            ),
        ];
        let mut adapter = RecordingAdapter::default();
        let report = execute_nvidia_cleanup(&ready(), &BTreeSet::new(), &targets, &mut adapter)
            .expect("default cleanup");
        assert_eq!(report.completed, targets[..2]);
        assert_eq!(report.skipped_optional, targets[2..]);
        assert_eq!(
            adapter.events,
            [
                "capture:gpu",
                "persist:gpu",
                "mutate:gpu",
                "verify:gpu",
                "capture:driver-cache",
                "persist:driver-cache",
                "mutate:driver-cache",
                "verify:driver-cache",
            ]
        );
    }

    #[test]
    fn service_registry_appx_file_and_policy_failures_recover_in_reverse_order() {
        let targets = [
            target(
                NvidiaCleanupSubsystem::Service,
                "service",
                NvidiaCleanupScope::Default,
            ),
            target(
                NvidiaCleanupSubsystem::Registry,
                "registry",
                NvidiaCleanupScope::Default,
            ),
            target(
                NvidiaCleanupSubsystem::Filesystem,
                "file",
                NvidiaCleanupScope::Default,
            ),
            target(
                NvidiaCleanupSubsystem::Appx,
                "appx",
                NvidiaCleanupScope::ControlPanel,
            ),
            target(
                NvidiaCleanupSubsystem::DriverSearchPolicy,
                "policy",
                NvidiaCleanupScope::DriverSearchPolicy,
            ),
        ];
        let scopes = [
            NvidiaCleanupScope::ControlPanel,
            NvidiaCleanupScope::DriverSearchPolicy,
        ]
        .into_iter()
        .collect();
        let mut adapter = RecordingAdapter {
            fail: Some("policy".into()),
            ..Default::default()
        };
        assert!(execute_nvidia_cleanup(&ready(), &scopes, &targets, &mut adapter).is_err());
        assert!(adapter.events.ends_with(&[
            "recover:policy".into(),
            "recover:appx".into(),
            "recover:file".into(),
            "recover:registry".into(),
            "recover:service".into(),
        ]));
    }

    #[test]
    fn missing_fallback_or_recovery_evidence_blocks_every_mutation() {
        let mut preconditions = ready();
        preconditions
            .verified
            .remove(&NvidiaCleanupProof::BasicDisplayFallback);
        let mut adapter = RecordingAdapter::default();
        assert!(
            execute_nvidia_cleanup(
                &preconditions,
                &BTreeSet::new(),
                &[target(
                    NvidiaCleanupSubsystem::DriverStore,
                    "oem42.inf",
                    NvidiaCleanupScope::Default,
                )],
                &mut adapter,
            )
            .is_err()
        );
        assert!(adapter.events.is_empty());
    }
}
