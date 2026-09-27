use frametime_app::{
    ApplicationError, BenchmarkPersistence, FpsCapOutcome, FpsStrategyValue, ValidatedFpsRequest,
    evaluate_fps_cap,
};
use frametime_domain::fps::ValidatedBenchmarkCapture;
#[cfg(test)]
use frametime_domain::fps::parse_vprof_output_detailed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsStage {
    Import,
    Evaluate,
    Review,
    Save,
    Saved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsSettings {
    Raw {
        measured_cap: u32,
    },
    Vrr {
        refresh_hz: u32,
        ceiling_margin_hz: u32,
    },
}

impl FpsSettings {
    #[must_use]
    pub const fn raw(measured_cap: u32) -> Self {
        Self::Raw { measured_cap }
    }

    #[must_use]
    pub const fn vrr(refresh_hz: u32, ceiling_margin_hz: u32) -> Self {
        Self::Vrr {
            refresh_hz,
            ceiling_margin_hz,
        }
    }

    pub fn from_text(
        strategy: FpsStrategyValue,
        measured_cap: &str,
        refresh_hz: &str,
        ceiling_margin_hz: &str,
    ) -> Result<Self, String> {
        match strategy {
            FpsStrategyValue::Raw => measured_cap
                .trim()
                .parse()
                .map(Self::raw)
                .map_err(|_| "Measured cap must be a whole number.".into()),
            FpsStrategyValue::Vrr => Ok(Self::vrr(
                parse_number(refresh_hz, "Refresh rate")?,
                parse_number(ceiling_margin_hz, "Ceiling margin")?,
            )),
        }
    }

    const fn request_values(self) -> (FpsStrategyValue, u32, u32, u32) {
        match self {
            Self::Raw { measured_cap } => (FpsStrategyValue::Raw, measured_cap, 0, 0),
            Self::Vrr {
                refresh_hz,
                ceiling_margin_hz,
            } => (FpsStrategyValue::Vrr, 0, refresh_hz, ceiling_margin_hz),
        }
    }
}

fn parse_number(value: &str, label: &str) -> Result<u32, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("{label} must be a whole number."))
}

#[derive(Debug)]
pub struct FpsSession {
    stage: FpsStage,
    capture: Option<ValidatedBenchmarkCapture>,
    settings: Option<FpsSettings>,
    outcome: Option<FpsCapOutcome>,
    failure: Option<String>,
    save_pending: bool,
}

impl Default for FpsSession {
    fn default() -> Self {
        Self {
            stage: FpsStage::Import,
            capture: None,
            settings: None,
            outcome: None,
            failure: None,
            save_pending: false,
        }
    }
}

impl FpsSession {
    #[must_use]
    pub const fn stage(&self) -> FpsStage {
        self.stage
    }

    #[must_use]
    pub const fn capture(&self) -> Option<&ValidatedBenchmarkCapture> {
        self.capture.as_ref()
    }

    #[must_use]
    pub const fn outcome(&self) -> Option<&FpsCapOutcome> {
        self.outcome.as_ref()
    }

    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    #[cfg(test)]
    fn import_text(&mut self, text: &str) -> Result<(), String> {
        match parse_vprof_output_detailed(text) {
            Ok(capture) => {
                self.set_capture(capture);
                Ok(())
            }
            Err(error) => {
                self.invalidate_source();
                self.set_failure(error.to_string())
            }
        }
    }

    pub fn set_capture(&mut self, capture: ValidatedBenchmarkCapture) {
        self.reset();
        self.capture = Some(capture);
    }

    pub fn evaluate_text(
        &mut self,
        strategy: FpsStrategyValue,
        measured_cap: &str,
        refresh_hz: &str,
        ceiling_margin_hz: &str,
    ) -> Result<(), String> {
        let settings =
            match FpsSettings::from_text(strategy, measured_cap, refresh_hz, ceiling_margin_hz) {
                Ok(settings) => settings,
                Err(error) => {
                    self.invalidate();
                    return self.set_failure(error);
                }
            };
        self.evaluate(settings)
    }

    pub fn evaluate(&mut self, settings: FpsSettings) -> Result<(), String> {
        self.invalidate();
        let Some(capture) = self.capture.clone() else {
            return self.set_failure("Import complete VProf Avg/P1 evidence first.");
        };
        let request = fps_request(capture, settings, String::new(), true);
        match evaluate_fps_cap(request) {
            Ok(outcome) => {
                self.settings = Some(settings);
                self.outcome = Some(outcome);
                Ok(())
            }
            Err(error) => self.set_failure(error.to_string()),
        }
    }

    pub fn begin_review(&mut self) -> Result<(), String> {
        if self.stage != FpsStage::Evaluate || self.outcome.is_none() || self.settings.is_none() {
            return self.set_failure("Evaluate the current evidence and settings before review.");
        }
        self.failure = None;
        self.stage = FpsStage::Review;
        Ok(())
    }

    pub fn persistence_request(
        &mut self,
        label: impl Into<String>,
    ) -> Result<ValidatedFpsRequest, String> {
        if self.stage == FpsStage::Save && !self.save_pending {
            return Err(self
                .failure
                .clone()
                .unwrap_or_else(|| "The previous save has already finished.".into()));
        }
        if self.stage != FpsStage::Review || self.save_pending {
            return self.set_failure("Review the current evaluation before saving.");
        }
        let Some(capture) = self.capture.clone() else {
            return self.set_failure("Imported benchmark evidence is no longer current.");
        };
        let Some(settings) = self.settings else {
            return self.set_failure("Evaluated FPS settings are no longer current.");
        };
        self.failure = None;
        self.stage = FpsStage::Save;
        self.save_pending = true;
        Ok(fps_request(capture, settings, label.into(), false))
    }

    pub fn finish_save(
        &mut self,
        result: Result<FpsCapOutcome, ApplicationError>,
    ) -> Result<(), String> {
        if self.stage == FpsStage::Save && !self.save_pending {
            return Err(self
                .failure
                .clone()
                .unwrap_or_else(|| "The save result has already been handled.".into()));
        }
        if self.stage != FpsStage::Save || !self.save_pending {
            return self.set_failure("No reviewed FPS save is awaiting a result.");
        }
        self.save_pending = false;
        match result {
            Ok(outcome) if outcome.persistence == BenchmarkPersistence::Persisted => {
                self.outcome = Some(outcome);
                self.failure = None;
                self.stage = FpsStage::Saved;
                Ok(())
            }
            Ok(outcome) => {
                let failure = match outcome.persistence {
                    BenchmarkPersistence::Disabled => "FPS persistence was disabled; no saved completion was recorded.",
                    BenchmarkPersistence::UnsupportedHost => "FPS persistence is unsupported on this host; no saved completion was recorded.",
                    BenchmarkPersistence::Persisted => unreachable!(),
                };
                self.outcome = Some(outcome);
                self.set_failure(failure)
            }
            Err(error) => self.set_failure(format!(
                "FPS save failed and may be partial: {error}. Verify benchmark history before starting another save."
            )),
        }
    }

    /// Drop a result after settings change while retaining the imported capture.
    pub fn invalidate(&mut self) {
        self.settings = None;
        self.outcome = None;
        self.failure = None;
        self.save_pending = false;
        self.stage = if self.capture.is_some() {
            FpsStage::Evaluate
        } else {
            FpsStage::Import
        };
    }

    /// Drop both the imported capture and any result after source text changes.
    pub fn invalidate_source(&mut self) {
        self.capture = None;
        self.invalidate();
    }

    pub fn return_to_import(&mut self) -> Result<(), String> {
        if self.stage == FpsStage::Save {
            return Err("Finish or reset the current save before returning to import.".into());
        }
        self.settings = None;
        self.outcome = None;
        self.failure = None;
        self.save_pending = false;
        self.stage = FpsStage::Import;
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn set_failure<T>(&mut self, failure: impl Into<String>) -> Result<T, String> {
        let failure = failure.into();
        self.failure = Some(failure.clone());
        Err(failure)
    }
}

fn fps_request(
    capture: ValidatedBenchmarkCapture,
    settings: FpsSettings,
    label: String,
    no_persist: bool,
) -> ValidatedFpsRequest {
    let (strategy, measured_cap, refresh_hz, ceiling_margin_hz) = settings.request_values();
    ValidatedFpsRequest {
        capture,
        strategy,
        measured_cap,
        refresh_hz,
        ceiling_margin_hz,
        label,
        copy: false,
        no_persist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vprof(p1_values: &[f64]) -> String {
        p1_values
            .iter()
            .map(|p1| format!("[VProf] FPS: Avg=600, P1={p1}\n"))
            .collect()
    }

    fn session(p1_values: &[f64]) -> FpsSession {
        let mut session = FpsSession::default();
        session
            .import_text(&vprof(p1_values))
            .expect("valid import");
        session
    }

    #[test]
    fn review_and_save_require_a_current_evaluation() {
        let mut session = session(&[245.0; 5]);
        assert_eq!(session.stage(), FpsStage::Import);
        assert!(session.begin_review().is_err());
        session.evaluate(FpsSettings::raw(240)).expect("evaluation");
        assert_eq!(session.stage(), FpsStage::Evaluate);
        session.begin_review().expect("review");
        assert_eq!(session.stage(), FpsStage::Review);

        session.invalidate();
        assert_eq!(session.stage(), FpsStage::Evaluate);
        assert!(session.outcome().is_none());
        assert!(session.persistence_request("stale").is_err());

        session.return_to_import().unwrap();
        assert_eq!(session.stage(), FpsStage::Import);
        assert!(session.capture().is_some());

        session.invalidate_source();
        assert_eq!(session.stage(), FpsStage::Import);
        assert!(session.capture().is_none());
        assert!(session.evaluate(FpsSettings::raw(240)).is_err());
    }

    #[test]
    fn application_policy_rejects_weak_and_four_run_evidence() {
        let mut weak = session(&[239.0, 245.0, 245.0, 245.0, 245.0]);
        assert!(weak.evaluate(FpsSettings::raw(240)).is_err());
        assert!(
            weak.failure()
                .is_some_and(|value| value.contains("1 failing runs"))
        );

        let mut four = session(&[245.0; 4]);
        assert!(four.evaluate(FpsSettings::raw(240)).is_err());
        assert!(
            four.failure()
                .is_some_and(|value| value.contains("at least 5"))
        );
    }

    #[test]
    fn raw_uncapped_measured_and_vrr_use_application_evaluation() {
        let mut uncapped = session(&[1.0]);
        uncapped.evaluate(FpsSettings::raw(0)).unwrap();
        assert_eq!(uncapped.outcome().unwrap().cap, 0);

        let mut measured = session(&[245.0; 5]);
        measured.evaluate(FpsSettings::raw(240)).unwrap();
        assert_eq!(measured.outcome().unwrap().cap, 240);

        let mut vrr = session(&[240.0; 5]);
        vrr.evaluate(FpsSettings::vrr(240, 3)).unwrap();
        assert_eq!(vrr.outcome().unwrap().cap, 237);
        assert!(
            vrr.evaluate_text(FpsStrategyValue::Vrr, "ignored", "240", "240")
                .is_err()
        );
    }

    #[test]
    fn only_persisted_save_reaches_saved_and_failed_attempt_is_not_reused() {
        let mut session = session(&[245.0; 5]);
        session.evaluate(FpsSettings::raw(240)).unwrap();
        let validated = session.outcome().unwrap().clone();
        session.begin_review().unwrap();
        let request = session.persistence_request("native wizard").unwrap();
        assert!(!request.no_persist);
        assert!(!request.copy);
        assert_eq!(session.stage(), FpsStage::Save);

        let mut unsupported = validated.clone();
        unsupported.persistence = BenchmarkPersistence::UnsupportedHost;
        assert!(session.finish_save(Ok(unsupported)).is_err());
        assert_ne!(session.stage(), FpsStage::Saved);
        let failure = session.failure().unwrap().to_owned();
        assert!(session.return_to_import().is_err());
        assert_eq!(session.failure(), Some(failure.as_str()));
        assert!(session.finish_save(Ok(validated.clone())).is_err());
        assert!(session.persistence_request("retry").is_err());
        assert_eq!(session.failure(), Some(failure.as_str()));

        session.reset();
        assert_eq!(session.stage(), FpsStage::Import);
        assert!(session.failure().is_none());
    }

    #[test]
    fn ambiguous_error_is_retained_and_persisted_result_completes() {
        let mut failed = session(&[245.0; 5]);
        failed.evaluate(FpsSettings::raw(240)).unwrap();
        failed.begin_review().unwrap();
        failed.persistence_request("failure").unwrap();
        assert!(
            failed
                .finish_save(Err(ApplicationError::failed("readback failed")))
                .is_err()
        );
        assert!(
            failed
                .failure()
                .is_some_and(|value| value.contains("may be partial"))
        );
        assert_eq!(failed.stage(), FpsStage::Save);

        let mut saved = session(&[245.0; 5]);
        saved.evaluate(FpsSettings::raw(240)).unwrap();
        let mut outcome = saved.outcome().unwrap().clone();
        saved.begin_review().unwrap();
        saved.persistence_request("success").unwrap();
        outcome.persistence = BenchmarkPersistence::Persisted;
        saved.finish_save(Ok(outcome)).unwrap();
        assert_eq!(saved.stage(), FpsStage::Saved);
    }
}
