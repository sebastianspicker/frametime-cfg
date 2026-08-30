use regex::Regex;

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

/// Selects the latency goal before choosing a cap from measured benchmark data.
///
/// Raw latency intentionally permits an uncapped path. A nonzero raw cap and a
/// VRR cap are accepted only when at least five recorded runs show P1 FPS can
/// sustain that ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsCapStrategy {
    /// Permit tearing for the lowest-latency path. `None` means `fps_max 0`.
    RawLatency { measured_cap: Option<u32> },
    /// Stay below the selected display refresh for a smooth VRR path.
    Vrr {
        refresh_hz: u32,
        ceiling_margin_hz: u32,
    },
}

/// Returns the `fps_max` value supported by the selected strategy and capture.
///
/// `Some(0)` is the explicit raw, uncapped choice. `None` means the capture's
/// P1 FPS does not sustain the requested ceiling, so no cap is recommended.
#[must_use]
pub fn measured_fps_cap(strategy: FpsCapStrategy, capture: BenchmarkCapture) -> Option<u32> {
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

fn sustainable_cap(cap: u32, capture: BenchmarkCapture) -> Option<u32> {
    (cap > 0
        && capture.average_fps.is_finite()
        && capture.average_fps > 0.0
        && capture.p1_fps.is_finite()
        && capture.p1_fps >= f64::from(cap)
        && capture.runs >= 5)
        .then_some(cap)
}

/// Parses all valid `VProf` result lines and returns their one-decimal averages.
/// Invalid runs are ignored exactly as the legacy workflow does.
pub fn parse_vprof_output(input: &str) -> Option<BenchmarkCapture> {
    let pattern = Regex::new(r"(?i)\[VProf\]\s*FPS:\s*Avg\s*=\s*([^\s,]+)\s*,\s*P1\s*=\s*(\S+)")
        .expect("static VProf expression");
    let mut avg_total = 0.0;
    let mut p1_total = 0.0;
    let mut runs = 0_u32;
    for captures in pattern.captures_iter(input) {
        let Some(average) = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
        else {
            continue;
        };
        let Some(p1) = captures
            .get(2)
            .and_then(|value| value.as_str().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
        else {
            continue;
        };
        avg_total += average;
        p1_total += p1;
        runs += 1;
    }
    (runs > 0).then(|| BenchmarkCapture {
        average_fps: round_one_decimal(avg_total / f64::from(runs)),
        p1_fps: round_one_decimal(p1_total / f64::from(runs)),
        runs,
    })
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chooses_explicit_raw_and_vrr_strategies_from_measured_headroom() {
        let sustained = BenchmarkCapture {
            average_fps: 306.4,
            p1_fps: 243.2,
            runs: 5,
        };
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency { measured_cap: None },
                BenchmarkCapture {
                    average_fps: f64::NAN,
                    p1_fps: f64::NAN,
                    runs: 0,
                },
            ),
            Some(0)
        );
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(240),
                },
                sustained,
            ),
            Some(240)
        );
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::Vrr {
                    refresh_hz: 240,
                    ceiling_margin_hz: 3,
                },
                sustained,
            ),
            Some(237)
        );
    }

    #[test]
    fn rejects_caps_without_p1_headroom_or_a_valid_vrr_ceiling() {
        let insufficient = BenchmarkCapture {
            average_fps: 300.0,
            p1_fps: 180.0,
            runs: 5,
        };
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(240),
                },
                insufficient,
            ),
            None
        );
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::RawLatency {
                    measured_cap: Some(180),
                },
                BenchmarkCapture {
                    average_fps: 300.0,
                    p1_fps: 200.0,
                    runs: 4,
                },
            ),
            None
        );
        assert_eq!(
            measured_fps_cap(
                FpsCapStrategy::Vrr {
                    refresh_hz: 144,
                    ceiling_margin_hz: 144,
                },
                insufficient,
            ),
            None
        );
    }

    #[test]
    fn parses_and_averages_only_valid_vprof_runs() {
        let capture = parse_vprof_output(
            "noise\n[VProf] FPS: Avg=300.2, P1=150.0\n\
             [VProf] FPS: Avg=bad, P1=100\n\
             [VProf] FPS: Avg=200.0, P1=0\n",
        )
        .expect("capture");
        assert_eq!(capture.runs, 2);
        assert_eq!(capture.average_fps, 250.1);
        assert_eq!(capture.p1_fps, 75.0);
        assert_eq!(capture.p1_ratio(), Some(75.0 / 250.1));
        assert!(parse_vprof_output("no benchmark data").is_none());
    }
}
