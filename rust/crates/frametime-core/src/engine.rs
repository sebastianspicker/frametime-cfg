use crate::{
    audit::{IrreversibleAudit, RebuildableAudit, RecoveryRequirement},
    backup::BackupEntry,
    catalog::Step,
    evidence::{EvidenceRequirement, ObservationReceipt},
    policy::{Decision, Profile},
    state::Progress,
};
use thiserror::Error;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inspection {
    Satisfied,
    NeedsApply,
    Inapplicable,
    /// A check-only step whose state cannot be authoritatively observed.
    Advisory {
        reason: &'static str,
    },
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Inspect(String),
    CaptureBackup(String),
    PersistBackup(String),
    CaptureAudit(String),
    PersistAudit(String),
    CaptureEvidence(String),
    PersistEvidence(String),
    VerifyEvidence(String),
    Apply(String),
    Verify(String),
    FinalizeAudit(String),
    FailAudit(String),
    Complete(String),
    Skip(String),
    Advisory { key: String, reason: String },
    Plan(String),
}
#[derive(Debug, Clone, Copy)]
pub struct Operation {
    pub step: Step,
}
#[derive(Debug, Default)]
pub struct RunReport {
    pub events: Vec<Event>,
    pub completed: usize,
    pub skipped: usize,
    pub advisories: usize,
    pub failed: usize,
}
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("step {key} inspection failed: {message}")]
    Inspect { key: String, message: String },
    #[error("step {key} is unsupported by the active backend")]
    Unsupported { key: String },
    #[error("workflow cancellation was requested before step {key}; no operation in that step ran")]
    Cancelled { key: String },
    #[error("step {key} backup capture failed: {message}; mutation was blocked")]
    Backup { key: String, message: String },
    #[error("step {key} backup persistence failed: {message}; mutation was blocked")]
    Persist { key: String, message: String },
    #[error("step {key} recovery audit capture failed: {message}; mutation was blocked")]
    AuditCapture { key: String, message: String },
    #[error("step {key} recovery audit persistence failed: {message}; mutation was blocked")]
    AuditPersist { key: String, message: String },
    #[error("step {key} prerequisite evidence capture failed: {message}; completion was blocked")]
    EvidenceCapture { key: String, message: String },
    #[error(
        "step {key} prerequisite evidence persistence failed: {message}; completion was blocked"
    )]
    EvidencePersist { key: String, message: String },
    #[error(
        "step {key} prerequisite evidence verification failed: {message}; completion was blocked"
    )]
    EvidenceVerify { key: String, message: String },
    #[error("step {key} apply failed: {message}")]
    Apply { key: String, message: String },
    #[error("step {key} verification failed: {message}; progress was not completed")]
    Verify { key: String, message: String },
    #[error("step {key} recovery audit finalization failed: {message}; progress was not completed")]
    AuditFinalize { key: String, message: String },
    #[error(
        "step {key} failed recovery audit persistence failed: {message}; progress was not completed"
    )]
    AuditFailurePersist { key: String, message: String },
    #[error("step {key} progress persistence failed: {message}")]
    Progress { key: String, message: String },
}
pub trait Backend {
    fn is_dry_run(&self) -> bool;
    fn inspect(&mut self, operation: Operation) -> Result<Inspection, String>;
    fn plan(&mut self, operation: Operation) -> Result<Vec<String>, String>;
    /// Capture every recovery identity required before mutation.
    fn capture_backups(&mut self, operation: Operation) -> Result<Vec<BackupEntry>, String>;
    /// Persist one complete batch atomically; partial recovery is forbidden.
    fn persist_backups(&mut self, entries: &[BackupEntry]) -> Result<(), String>;
    /// Defaults to byte-restorable recovery.
    fn recovery_requirement(&self, _: Operation) -> RecoveryRequirement {
        RecoveryRequirement::LosslessBackup
    }
    fn capture_pending_audit(&mut self, _: Operation) -> Result<RebuildableAudit, String> {
        Err("backend did not implement rebuildable audit capture".into())
    }
    fn persist_pending_audit(&mut self, _: &RebuildableAudit) -> Result<(), String> {
        Err("backend did not implement rebuildable audit persistence".into())
    }
    fn finalize_audit(&mut self, _: &RebuildableAudit) -> Result<(), String> {
        Err("backend did not implement rebuildable audit finalization".into())
    }
    fn capture_pending_irreversible_audit(
        &mut self,
        _: Operation,
    ) -> Result<IrreversibleAudit, String> {
        Err("backend did not implement irreversible audit capture".into())
    }
    fn persist_pending_irreversible_audit(&mut self, _: &IrreversibleAudit) -> Result<(), String> {
        Err("backend did not implement irreversible audit persistence".into())
    }
    fn finalize_irreversible_audit(&mut self, _: &IrreversibleAudit) -> Result<(), String> {
        Err("backend did not implement irreversible audit finalization".into())
    }
    fn fail_irreversible_audit(&mut self, _: &IrreversibleAudit) -> Result<(), String> {
        Err("backend did not implement failed irreversible audit persistence".into())
    }
    /// Opt in when later phases require a durable observation receipt.
    fn evidence_requirement(&self, _: Operation) -> EvidenceRequirement {
        EvidenceRequirement::None
    }
    fn capture_evidence(&mut self, _: Operation) -> Result<ObservationReceipt, String> {
        Err("backend did not implement prerequisite evidence capture".into())
    }
    fn persist_evidence(&mut self, _: &ObservationReceipt) -> Result<(), String> {
        Err("backend did not implement prerequisite evidence persistence".into())
    }
    /// Re-observe after persistence; bytes alone never authorize mutation.
    fn verify_evidence(&mut self, _: Operation, _: &ObservationReceipt) -> Result<(), String> {
        Err("backend did not implement prerequisite evidence verification".into())
    }
    fn apply(&mut self, operation: Operation) -> Result<(), String>;
    fn verify(&mut self, operation: Operation) -> Result<(), String>;
    fn persist_progress(&mut self, progress: &Progress) -> Result<(), String>;
    fn timestamp(&self) -> String;
}
pub struct Engine<B> {
    backend: B,
    progress: Progress,
}

impl<B: Backend> Engine<B> {
    pub fn new(backend: B, progress: Progress) -> Self {
        Self { backend, progress }
    }
    pub fn into_parts(self) -> (B, Progress) {
        (self.backend, self.progress)
    }
    pub fn run(&mut self, steps: &[Step], profile: Profile) -> Result<RunReport, EngineError> {
        self.run_with_consent(steps, profile, |_| false)
    }
    pub fn run_with_consent(
        &mut self,
        steps: &[Step],
        profile: Profile,
        consent: impl FnMut(&Step) -> bool,
    ) -> Result<RunReport, EngineError> {
        self.run_with_control(steps, profile, consent, || false)
    }
    pub fn run_with_control(
        &mut self,
        steps: &[Step],
        profile: Profile,
        mut consent: impl FnMut(&Step) -> bool,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<RunReport, EngineError> {
        let mut report = RunReport::default();
        for step in steps {
            let operation = Operation { step: *step };
            let key = Progress::key(step.phase as u8, step.number);
            if self.skip_recorded_or_cancelled(&key, &mut cancelled)? {
                continue;
            }

            if self.should_skip_for_policy(step, &profile, &mut consent) {
                self.record_skip(step, &key, !self.backend.is_dry_run(), &mut report)?;
                continue;
            }

            report.events.push(Event::Inspect(key.clone()));
            let inspection =
                self.backend
                    .inspect(operation)
                    .map_err(|message| EngineError::Inspect {
                        key: key.clone(),
                        message,
                    })?;
            if self.backend.is_dry_run() {
                self.report_dry_run(operation, &key, inspection, &mut report)?;
                continue;
            }

            self.run_live_step(operation, &key, inspection, &mut report)?;
        }
        Ok(report)
    }
    fn skip_recorded_or_cancelled(
        &mut self,
        key: &str,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<bool, EngineError> {
        if self.progress.completed_steps.contains(key)
            || self.progress.skipped_steps.contains(key)
            || self.progress.advisories.contains_key(key)
        {
            return Ok(true);
        }
        if !self.backend.is_dry_run() && cancelled() {
            return Err(EngineError::Cancelled { key: key.into() });
        }
        Ok(false)
    }
    fn should_skip_for_policy(
        &mut self,
        step: &Step,
        profile: &Profile,
        consent: &mut impl FnMut(&Step) -> bool,
    ) -> bool {
        match profile.decision(step.tier, step.risk) {
            Decision::Skip => true,
            Decision::Prompt => !self.backend.is_dry_run() && !consent(step),
            Decision::Auto => false,
        }
    }

    fn report_dry_run(
        &mut self,
        operation: Operation,
        key: &str,
        inspection: Inspection,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        for line in self
            .backend
            .plan(operation)
            .map_err(|message| EngineError::Inspect {
                key: key.into(),
                message,
            })?
        {
            report.events.push(Event::Plan(line));
        }
        match inspection {
            Inspection::Inapplicable => {
                report.events.push(Event::Skip(key.into()));
                report.skipped += 1;
            }
            Inspection::Unsupported => report.failed += 1,
            Inspection::Advisory { reason } => {
                report.events.push(Event::Advisory {
                    key: key.into(),
                    reason: reason.into(),
                });
                report.advisories += 1;
            }
            Inspection::Satisfied | Inspection::NeedsApply => report.completed += 1,
        }
        Ok(())
    }

    fn run_live_step(
        &mut self,
        operation: Operation,
        key: &str,
        inspection: Inspection,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        let step = operation.step;
        match inspection {
            Inspection::Unsupported => Err(EngineError::Unsupported { key: key.into() }),
            Inspection::Inapplicable => self.record_skip(&step, key, true, report),
            Inspection::Advisory { reason } => self.record_advisory(&step, key, reason, report),
            Inspection::Satisfied | Inspection::NeedsApply => {
                if inspection == Inspection::Satisfied || step.check_only {
                    if self.backend.evidence_requirement(operation)
                        == EvidenceRequirement::DurableReceipt
                    {
                        self.run_evidence_operation(operation, key, report)?;
                    } else {
                        self.backend
                            .verify(operation)
                            .map_err(|message| EngineError::Verify {
                                key: key.into(),
                                message,
                            })?;
                        report.events.push(Event::Verify(key.into()));
                    }
                } else {
                    match self.backend.recovery_requirement(operation) {
                        RecoveryRequirement::RebuildableAudit => {
                            self.run_rebuildable_operation(operation, key, report)?;
                        }
                        RecoveryRequirement::ManualRecoveryAudit | RecoveryRequirement::Mixed => {
                            self.run_irreversible_operation(operation, key, report)?;
                        }
                        RecoveryRequirement::LosslessBackup => {
                            self.run_lossless_operation(operation, key, report)?;
                        }
                    }
                }
                self.complete_live_step(&step, key, report)
            }
        }
    }

    fn record_advisory(
        &mut self,
        step: &Step,
        key: &str,
        reason: &'static str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        let mut progress = self.progress.clone();
        progress.acknowledge_advisory(step.phase as u8, step.number, reason.into());
        self.backend
            .persist_progress(&progress)
            .map_err(|message| EngineError::Progress {
                key: key.into(),
                message,
            })?;
        self.progress = progress;
        report.events.push(Event::Advisory {
            key: key.into(),
            reason: reason.into(),
        });
        report.advisories += 1;
        Ok(())
    }

    fn record_skip(
        &mut self,
        step: &Step,
        key: &str,
        persist_progress: bool,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        report.events.push(Event::Skip(key.into()));
        if persist_progress {
            let mut progress = self.progress.clone();
            progress.skip(step.phase as u8, step.number);
            self.backend
                .persist_progress(&progress)
                .map_err(|message| EngineError::Progress {
                    key: key.into(),
                    message,
                })?;
            self.progress = progress;
        }
        report.skipped += 1;
        Ok(())
    }

    fn complete_live_step(
        &mut self,
        step: &Step,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        let mut progress = self.progress.clone();
        progress.complete(step.phase as u8, step.number, self.backend.timestamp());
        self.backend
            .persist_progress(&progress)
            .map_err(|message| EngineError::Progress {
                key: key.into(),
                message,
            })?;
        self.progress = progress;
        report.events.push(Event::Complete(key.into()));
        report.completed += 1;
        Ok(())
    }

    fn run_lossless_operation(
        &mut self,
        operation: Operation,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        self.capture_lossless_backups(operation, key, report)?;
        self.apply_and_verify(operation, key, report)
    }

    fn capture_lossless_backups(
        &mut self,
        operation: Operation,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        report.events.push(Event::CaptureBackup(key.into()));
        let backups =
            self.backend
                .capture_backups(operation)
                .map_err(|message| EngineError::Backup {
                    key: key.into(),
                    message,
                })?;
        if backups.is_empty() {
            return Err(EngineError::Backup {
                key: key.into(),
                message: "mutating operation captured no recovery entries".into(),
            });
        }
        self.backend
            .persist_backups(&backups)
            .map_err(|message| EngineError::Persist {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::PersistBackup(key.into()));
        Ok(())
    }

    fn run_evidence_operation(
        &mut self,
        operation: Operation,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        report.events.push(Event::CaptureEvidence(key.into()));
        let receipt = self
            .backend
            .capture_evidence(operation)
            .map_err(|message| EngineError::EvidenceCapture {
                key: key.into(),
                message,
            })?;
        receipt
            .validate_for(key)
            .map_err(|error| EngineError::EvidenceCapture {
                key: key.into(),
                message: error.to_string(),
            })?;
        self.backend.persist_evidence(&receipt).map_err(|message| {
            EngineError::EvidencePersist {
                key: key.into(),
                message,
            }
        })?;
        report.events.push(Event::PersistEvidence(key.into()));
        self.backend
            .verify_evidence(operation, &receipt)
            .map_err(|message| EngineError::EvidenceVerify {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::VerifyEvidence(key.into()));
        self.backend
            .verify(operation)
            .map_err(|message| EngineError::Verify {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::Verify(key.into()));
        Ok(())
    }

    fn run_rebuildable_operation(
        &mut self,
        operation: Operation,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        report.events.push(Event::CaptureAudit(key.into()));
        let audit = self
            .backend
            .capture_pending_audit(operation)
            .map_err(|message| EngineError::AuditCapture {
                key: key.into(),
                message,
            })?;
        if !audit.is_valid_pending_for(key) {
            return Err(EngineError::AuditCapture {
                key: key.into(),
                message: "rebuildable mutation must capture a complete pending audit record bound to the current step".into(),
            });
        }
        self.backend
            .persist_pending_audit(&audit)
            .map_err(|message| EngineError::AuditPersist {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::PersistAudit(key.into()));
        self.apply_and_verify(operation, key, report)?;
        let finalized = audit.finalized(self.backend.timestamp());
        self.backend
            .finalize_audit(&finalized)
            .map_err(|message| EngineError::AuditFinalize {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::FinalizeAudit(key.into()));
        Ok(())
    }

    fn run_irreversible_operation(
        &mut self,
        operation: Operation,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        let requirement = self.backend.recovery_requirement(operation);
        if requirement == RecoveryRequirement::Mixed {
            self.capture_lossless_backups(operation, key, report)?;
        }
        self.run_pending_irreversible_audit(operation, key, requirement, report)
    }

    fn run_pending_irreversible_audit(
        &mut self,
        operation: Operation,
        key: &str,
        requirement: RecoveryRequirement,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        report.events.push(Event::CaptureAudit(key.into()));
        let audit = self
            .backend
            .capture_pending_irreversible_audit(operation)
            .map_err(|message| EngineError::AuditCapture {
                key: key.into(),
                message,
            })?;
        if !audit.is_valid_pending_for(requirement, key) {
            return Err(EngineError::AuditCapture {
                key: key.into(),
                message: "irreversible mutation must capture a pending fixed-subject audit for the current recovery requirement".into(),
            });
        }
        self.backend
            .persist_pending_irreversible_audit(&audit)
            .map_err(|message| EngineError::AuditPersist {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::PersistAudit(key.into()));
        if let Err(error) = self.apply_and_verify(operation, key, report) {
            let failed = audit.failed(self.backend.timestamp());
            self.backend
                .fail_irreversible_audit(&failed)
                .map_err(|message| EngineError::AuditFailurePersist {
                    key: key.into(),
                    message,
                })?;
            report.events.push(Event::FailAudit(key.into()));
            return Err(error);
        }
        let finalized = audit.finalized(self.backend.timestamp());
        self.backend
            .finalize_irreversible_audit(&finalized)
            .map_err(|message| EngineError::AuditFinalize {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::FinalizeAudit(key.into()));
        Ok(())
    }

    fn apply_and_verify(
        &mut self,
        operation: Operation,
        key: &str,
        report: &mut RunReport,
    ) -> Result<(), EngineError> {
        self.backend
            .apply(operation)
            .map_err(|message| EngineError::Apply {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::Apply(key.into()));
        self.backend
            .verify(operation)
            .map_err(|message| EngineError::Verify {
                key: key.into(),
                message,
            })?;
        report.events.push(Event::Verify(key.into()));
        Ok(())
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
