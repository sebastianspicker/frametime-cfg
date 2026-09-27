use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_VPROF_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_VPROF_OBSERVATIONS: usize = 1_000;
pub const MIN_CAP_AUTHORIZATION_RUNS: usize = 5;
pub const BENCHMARK_RUN_EVIDENCE_SCHEMA_VERSION: u8 = 1;

static VPROF_RESULT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[VProf\]\s*FPS:\s*Avg\s*=\s*([^\s,]+)\s*,\s*P1\s*=\s*(\S+)")
        .expect("static VProf expression")
});

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BenchmarkCapture {
    pub average_fps: f64,
    pub p1_fps: f64,
    pub runs: u32,
}

impl BenchmarkCapture {
    #[must_use]
    pub fn p1_ratio(self) -> Option<f64> {
        (self.average_fps > 0.0 && self.p1_fps > 0.0).then_some(self.p1_fps / self.average_fps)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkObservation {
    pub average_fps: f64,
    pub p1_fps: f64,
}

impl BenchmarkObservation {
    fn is_valid(self) -> bool {
        self.average_fps.is_finite()
            && self.average_fps > 0.0
            && self.p1_fps.is_finite()
            && self.p1_fps >= 0.0
    }
}

/// Versioned, ordered per-run evidence. Values are retained exactly as parsed;
/// only the compatibility aggregate is rounded to one decimal place.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRunEvidence {
    pub schema_version: u8,
    pub observations: Vec<BenchmarkObservation>,
}

impl BenchmarkRunEvidence {
    pub fn new(observations: Vec<BenchmarkObservation>) -> Result<Self, &'static str> {
        let evidence = Self {
            schema_version: BENCHMARK_RUN_EVIDENCE_SCHEMA_VERSION,
            observations,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != BENCHMARK_RUN_EVIDENCE_SCHEMA_VERSION {
            return Err("unsupported benchmark run evidence schema");
        }
        if self.observations.is_empty() {
            return Err("benchmark run evidence is empty");
        }
        if self.observations.len() > MAX_VPROF_OBSERVATIONS {
            return Err("benchmark run evidence exceeds its observation limit");
        }
        if self
            .observations
            .iter()
            .any(|observation| !observation.is_valid())
        {
            return Err("benchmark run evidence contains an invalid observation");
        }
        Ok(())
    }

    pub fn validate_against(&self, capture: BenchmarkCapture) -> Result<(), &'static str> {
        self.validate()?;
        if self.aggregate() != capture {
            return Err("benchmark run evidence does not match its aggregate");
        }
        Ok(())
    }

    #[must_use]
    pub fn aggregate(&self) -> BenchmarkCapture {
        let (average_total, p1_total) =
            self.observations
                .iter()
                .fold((0.0, 0.0), |(average, p1), observation| {
                    (average + observation.average_fps, p1 + observation.p1_fps)
                });
        let runs = u32::try_from(self.observations.len()).expect("observation limit fits u32");
        BenchmarkCapture {
            average_fps: round_one_decimal(average_total / f64::from(runs)),
            p1_fps: round_one_decimal(p1_total / f64::from(runs)),
            runs,
        }
    }

    #[must_use]
    pub fn p1_range(&self) -> (f64, f64) {
        self.observations
            .iter()
            .map(|observation| observation.p1_fps)
            .fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            )
    }

    #[must_use]
    pub fn supporting_runs(&self, cap: u32) -> usize {
        let cap = f64::from(cap);
        self.observations
            .iter()
            .filter(|observation| {
                observation.p1_fps <= observation.average_fps && observation.p1_fps >= cap
            })
            .count()
    }

    #[must_use]
    pub fn failing_runs(&self, cap: u32) -> usize {
        self.observations.len() - self.supporting_runs(cap)
    }

    #[must_use]
    pub fn invalid_ordered_runs(&self) -> usize {
        self.observations
            .iter()
            .filter(|observation| observation.p1_fps > observation.average_fps)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedBenchmarkCapture {
    aggregate: BenchmarkCapture,
    run_evidence: BenchmarkRunEvidence,
}

impl ValidatedBenchmarkCapture {
    pub fn new(run_evidence: BenchmarkRunEvidence) -> Result<Self, &'static str> {
        run_evidence.validate()?;
        let aggregate = run_evidence.aggregate();
        if !aggregate.average_fps.is_finite()
            || aggregate.average_fps <= 0.0
            || !aggregate.p1_fps.is_finite()
            || aggregate.p1_fps < 0.0
        {
            return Err("benchmark run evidence aggregate is invalid");
        }
        Ok(Self {
            aggregate,
            run_evidence,
        })
    }

    #[must_use]
    pub const fn aggregate(&self) -> BenchmarkCapture {
        self.aggregate
    }

    #[must_use]
    pub const fn run_evidence(&self) -> &BenchmarkRunEvidence {
        &self.run_evidence
    }

    #[must_use]
    pub fn observations(&self) -> &[BenchmarkObservation] {
        &self.run_evidence.observations
    }

    #[must_use]
    pub fn p1_range(&self) -> (f64, f64) {
        self.run_evidence.p1_range()
    }

    #[must_use]
    pub fn supporting_runs(&self, cap: u32) -> usize {
        self.run_evidence.supporting_runs(cap)
    }

    #[must_use]
    pub fn failing_runs(&self, cap: u32) -> usize {
        self.run_evidence.failing_runs(cap)
    }

    #[must_use]
    pub fn invalid_ordered_runs(&self) -> usize {
        self.run_evidence.invalid_ordered_runs()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VprofParseError {
    #[error("VProf input exceeds the {max_bytes}-byte limit ({actual_bytes} bytes)")]
    InputTooLarge {
        actual_bytes: usize,
        max_bytes: usize,
    },
    #[error("VProf input is not complete UTF-8")]
    InvalidUtf8,
    #[error("VProf input contains no valid Avg/P1 observations")]
    NoValidObservations,
    #[error("VProf input exceeds the limit of {max_observations} valid observations")]
    TooManyObservations { max_observations: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsCapStrategy {
    RawLatency {
        measured_cap: Option<u32>,
    },
    Vrr {
        refresh_hz: u32,
        ceiling_margin_hz: u32,
    },
}

/// Authorizes a new cap only from ordered per-run evidence. Every one of at
/// least five runs must sustain a nonzero raw or VRR ceiling.
#[must_use]
pub fn measured_fps_cap(
    strategy: FpsCapStrategy,
    capture: &ValidatedBenchmarkCapture,
) -> Option<u32> {
    match strategy {
        FpsCapStrategy::RawLatency { measured_cap: None } => Some(0),
        FpsCapStrategy::RawLatency {
            measured_cap: Some(cap),
        } => sustainable_cap(cap, capture),
        FpsCapStrategy::Vrr {
            refresh_hz,
            ceiling_margin_hz,
        } => refresh_hz
            .checked_sub(ceiling_margin_hz)
            .filter(|cap| *cap > 0)
            .and_then(|cap| sustainable_cap(cap, capture)),
    }
}

/// Compatibility check for already-persisted aggregate-only state. Callers
/// must not use this function to authorize a new nonzero cap.
#[must_use]
pub fn legacy_aggregate_fps_cap(
    strategy: FpsCapStrategy,
    capture: BenchmarkCapture,
) -> Option<u32> {
    match strategy {
        FpsCapStrategy::RawLatency { measured_cap: None } => Some(0),
        FpsCapStrategy::RawLatency {
            measured_cap: Some(cap),
        } => legacy_sustainable_cap(cap, capture),
        FpsCapStrategy::Vrr {
            refresh_hz,
            ceiling_margin_hz,
        } => refresh_hz
            .checked_sub(ceiling_margin_hz)
            .filter(|cap| *cap > 0)
            .and_then(|cap| legacy_sustainable_cap(cap, capture)),
    }
}

fn sustainable_cap(cap: u32, capture: &ValidatedBenchmarkCapture) -> Option<u32> {
    let aggregate = capture.aggregate();
    (cap > 0
        && aggregate.average_fps.is_finite()
        && aggregate.average_fps > 0.0
        && capture.observations().len() >= MIN_CAP_AUTHORIZATION_RUNS
        && capture.failing_runs(cap) == 0)
        .then_some(cap)
}

fn legacy_sustainable_cap(cap: u32, capture: BenchmarkCapture) -> Option<u32> {
    (cap > 0
        && capture.average_fps.is_finite()
        && capture.average_fps > 0.0
        && capture.p1_fps.is_finite()
        && capture.p1_fps >= f64::from(cap)
        && capture.runs >= u32::try_from(MIN_CAP_AUTHORIZATION_RUNS).expect("minimum fits u32"))
    .then_some(cap)
}

pub fn parse_vprof_bytes_detailed(
    input: &[u8],
) -> Result<ValidatedBenchmarkCapture, VprofParseError> {
    if input.len() > MAX_VPROF_INPUT_BYTES {
        return Err(VprofParseError::InputTooLarge {
            actual_bytes: input.len(),
            max_bytes: MAX_VPROF_INPUT_BYTES,
        });
    }
    let text = std::str::from_utf8(input).map_err(|_| VprofParseError::InvalidUtf8)?;
    parse_vprof_output_detailed(text)
}

pub fn parse_vprof_output_detailed(
    input: &str,
) -> Result<ValidatedBenchmarkCapture, VprofParseError> {
    if input.len() > MAX_VPROF_INPUT_BYTES {
        return Err(VprofParseError::InputTooLarge {
            actual_bytes: input.len(),
            max_bytes: MAX_VPROF_INPUT_BYTES,
        });
    }
    let mut observations = Vec::new();
    for captures in VPROF_RESULT.captures_iter(input) {
        let Some(average_fps) = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
        else {
            continue;
        };
        let Some(p1_fps) = captures
            .get(2)
            .and_then(|value| value.as_str().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
        else {
            continue;
        };
        if observations.len() == MAX_VPROF_OBSERVATIONS {
            return Err(VprofParseError::TooManyObservations {
                max_observations: MAX_VPROF_OBSERVATIONS,
            });
        }
        observations.push(BenchmarkObservation {
            average_fps,
            p1_fps,
        });
    }
    if observations.is_empty() {
        return Err(VprofParseError::NoValidObservations);
    }
    ValidatedBenchmarkCapture::new(
        BenchmarkRunEvidence::new(observations).expect("parser creates valid observations"),
    )
    .map_err(|_| VprofParseError::NoValidObservations)
}

/// Compatibility parser that returns the legacy rounded aggregate view.
#[must_use]
pub fn parse_vprof_output(input: &str) -> Option<BenchmarkCapture> {
    parse_vprof_output_detailed(input)
        .ok()
        .map(|capture| capture.aggregate())
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(p1_values: &[f64]) -> ValidatedBenchmarkCapture {
        ValidatedBenchmarkCapture::new(
            BenchmarkRunEvidence::new(
                p1_values
                    .iter()
                    .map(|p1_fps| BenchmarkObservation {
                        average_fps: 600.0,
                        p1_fps: *p1_fps,
                    })
                    .collect(),
            )
            .expect("run evidence"),
        )
        .expect("capture")
    }

    #[test]
    fn every_run_must_support_a_new_nonzero_cap() {
        let evidence = capture(&[100.0, 100.0, 100.0, 100.0, 600.0]);
        assert_eq!(evidence.aggregate().p1_fps, 200.0);
        assert_eq!(evidence.supporting_runs(180), 1);
        assert_eq!(evidence.failing_runs(180), 4);
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(180)
                },
                &evidence
            ),
            None
        );
    }

    #[test]
    fn chooses_raw_and_vrr_caps_from_five_supporting_runs() {
        let evidence = capture(&[243.2; 5]);
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(240)
                },
                &evidence
            ),
            Some(240)
        );
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::Vrr {
                    refresh_hz: 240,
                    ceiling_margin_hz: 3
                },
                &evidence
            ),
            Some(237)
        );
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::Vrr {
                    refresh_hz: 144,
                    ceiling_margin_hz: 144
                },
                &evidence
            ),
            None
        );
    }

    #[test]
    fn finite_observations_cannot_overflow_into_an_invalid_aggregate() {
        let evidence = BenchmarkRunEvidence::new(vec![
            BenchmarkObservation {
                average_fps: f64::MAX,
                p1_fps: 1.0,
            },
            BenchmarkObservation {
                average_fps: f64::MAX,
                p1_fps: 1.0,
            },
        ])
        .expect("individually finite evidence");
        assert_eq!(
            ValidatedBenchmarkCapture::new(evidence),
            Err("benchmark run evidence aggregate is invalid")
        );
    }

    #[test]
    fn thresholds_use_unrounded_observations() {
        let evidence = capture(&[179.96, 180.01, 180.01, 180.01, 180.01]);
        assert_eq!(evidence.aggregate().p1_fps, 180.0);
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(180)
                },
                &evidence
            ),
            None
        );
    }

    #[test]
    fn p1_above_average_is_parsed_but_cannot_support_a_cap() {
        let capture =
            parse_vprof_output_detailed("[VProf] FPS: Avg=100, P1=180\n".repeat(5).as_str())
                .expect("legacy-compatible parsed observations");
        assert_eq!(capture.invalid_ordered_runs(), 5);
        assert_eq!(capture.supporting_runs(90), 0);
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(90)
                },
                &capture
            ),
            None
        );
    }

    #[test]
    fn uncapped_raw_does_not_require_five_runs() {
        let evidence = capture(&[0.0]);
        assert_eq!(
            measured_fps_cap(FpsCapStrategy::RawLatency { measured_cap: None }, &evidence),
            Some(0)
        );
    }

    #[test]
    fn detailed_parser_retains_order_and_unrounded_values() {
        let capture = parse_vprof_output_detailed(
            "[VProf] FPS: Avg=300.04, P1=179.96\n[VProf] FPS: Avg=301.06, P1=180.04",
        )
        .expect("capture");
        assert_eq!(capture.aggregate().average_fps, 300.6);
        assert_eq!(capture.aggregate().p1_fps, 180.0);
        assert_eq!(capture.observations()[0].p1_fps, 179.96);
        assert_eq!(capture.observations()[1].average_fps, 301.06);
    }

    #[test]
    fn exact_input_and_observation_limits_are_enforced() {
        let line = "[VProf] FPS: Avg=300, P1=200\n";
        let mut exact = line.to_owned();
        exact.push_str(&"x".repeat(MAX_VPROF_INPUT_BYTES - exact.len()));
        assert!(parse_vprof_output_detailed(&exact).is_ok());
        exact.push('x');
        assert!(matches!(
            parse_vprof_output_detailed(&exact),
            Err(VprofParseError::InputTooLarge { .. })
        ));

        let thousand = line.repeat(MAX_VPROF_OBSERVATIONS);
        assert_eq!(
            parse_vprof_output_detailed(&thousand)
                .expect("maximum observations")
                .observations()
                .len(),
            MAX_VPROF_OBSERVATIONS
        );
        assert!(matches!(
            parse_vprof_output_detailed(&(thousand + line)),
            Err(VprofParseError::TooManyObservations { .. })
        ));
    }

    #[test]
    fn byte_parser_requires_complete_utf8() {
        assert_eq!(
            parse_vprof_bytes_detailed(b"[VProf] FPS: Avg=300, P1=200\xff"),
            Err(VprofParseError::InvalidUtf8)
        );
    }

    #[test]
    fn compatibility_parser_retains_legacy_aggregate() {
        let capture = parse_vprof_output(
            "noise\n[VProf] FPS: Avg=300.2, P1=150.0\n\
             [VProf] FPS: Avg=bad, P1=100\n\
             [VProf] FPS: Avg=200.0, P1=0\n",
        )
        .expect("capture");
        assert_eq!(capture.runs, 2);
        assert_eq!(capture.average_fps, 250.1);
        assert_eq!(capture.p1_fps, 75.0);
    }
}
