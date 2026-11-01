use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_VPROF_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_VPROF_OBSERVATIONS: usize = 1_000;
pub const MIN_CAP_AUTHORIZATION_RUNS: usize = 5;
pub const BENCHMARK_RUN_EVIDENCE_SCHEMA_VERSION: u8 = 1;

struct VprofResult<'a> {
    text: &'a str,
    average: &'a str,
    p1: &'a str,
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

fn split_token(text: &str, stop: impl Fn(char) -> bool) -> (&str, &str) {
    text.split_at(text.find(stop).unwrap_or(text.len()))
}

/// The text after `[VProf] FPS:` when `text` begins with that marker.
fn vprof_start_rest(text: &str) -> Option<&str> {
    let rest = strip_prefix_ignore_case(text, "[vprof]")?.trim_start();
    strip_prefix_ignore_case(rest, "fps:")
}

fn vprof_result_at(text: &str) -> Option<VprofResult<'_>> {
    let rest = strip_prefix_ignore_case(vprof_start_rest(text)?.trim_start(), "avg")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let (average, rest) = split_token(rest, |c| c.is_whitespace() || c == ',');
    let rest = rest.trim_start().strip_prefix(',')?.trim_start();
    let rest = strip_prefix_ignore_case(rest, "p1")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let (p1, tail) = split_token(rest, char::is_whitespace);
    (!average.is_empty() && !p1.is_empty()).then(|| VprofResult {
        text: &text[..text.len() - tail.len()],
        average,
        p1,
    })
}

/// Every `[VProf] FPS: Avg = <n>, P1 = <n>` result on one line.
fn vprof_results(line: &str) -> Vec<VprofResult<'_>> {
    let lower = line.to_ascii_lowercase();
    let mut results = Vec::new();
    let mut from = 0;
    while let Some(offset) = lower[from..].find("[vprof]") {
        let start = from + offset;
        from = start + 1;
        if let Some(result) = vprof_result_at(&line[start..]) {
            from = start + result.text.len();
            results.push(result);
        }
    }
    results
}

fn vprof_line_starts_run(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower
        .match_indices("[vprof]")
        .any(|(start, _)| vprof_start_rest(&line[start..]).is_some())
}

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
    #[error("VProf result line could not be parsed: {line}")]
    UnparsableRun { line: String },
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
    if let Some(line) = input
        .lines()
        .find(|line| vprof_line_starts_run(line) && vprof_results(line).is_empty())
    {
        return Err(VprofParseError::UnparsableRun { line: line.into() });
    }
    let mut observations = Vec::new();
    for result in input.lines().flat_map(vprof_results) {
        let unparsable = || VprofParseError::UnparsableRun {
            line: result.text.into(),
        };
        let Some(average_fps) = result
            .average
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
        else {
            return Err(unparsable());
        };
        let Some(p1_fps) = result
            .p1
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)
        else {
            return Err(unparsable());
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
    fn parser_accepts_mixed_case_and_extra_whitespace() {
        let capture = parse_vprof_output_detailed(
            "[vprof]   fps:\tAVG  =  300.5 ,   p1\t= 180.25\n[VPROF]FPS:avg=301,P1=181",
        )
        .expect("capture");
        assert_eq!(capture.observations().len(), 2);
        assert_eq!(capture.observations()[0].average_fps, 300.5);
        assert_eq!(capture.observations()[0].p1_fps, 180.25);
        assert_eq!(capture.observations()[1].p1_fps, 181.0);
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
    fn unparsable_result_lines_are_rejected_not_dropped() {
        for bad in [
            "[VProf] FPS: Avg=bad, P1=100",
            "[VProf] FPS: Avg=300.0, P1=-nan(ind)",
            "[vprof] fps: Avg=300.0",
        ] {
            let input = format!("noise\n[VProf] FPS: Avg=300.2, P1=150.0\n{bad}\n");
            assert!(matches!(
                parse_vprof_output_detailed(&input),
                Err(VprofParseError::UnparsableRun { .. })
            ));
        }
    }

    #[test]
    fn compatibility_parser_retains_legacy_aggregate() {
        let capture = parse_vprof_output(
            "noise\n[VProf] FPS: Avg=300.2, P1=150.0\n\
             [VProf] FPS: Avg=200.0, P1=0\n",
        )
        .expect("capture");
        assert_eq!(capture.runs, 2);
        assert_eq!(capture.average_fps, 250.1);
        assert_eq!(capture.p1_fps, 75.0);
    }
}
