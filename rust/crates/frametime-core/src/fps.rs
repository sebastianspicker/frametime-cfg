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

#[must_use]
pub fn recommended_cap(average_fps: f64, reduction: f64, minimum: u32) -> u32 {
    if !average_fps.is_finite()
        || average_fps <= 0.0
        || !reduction.is_finite()
        || !(0.0..1.0).contains(&reduction)
    {
        return 0;
    }
    let reduction_amount = (average_fps * reduction).floor();
    (average_fps - reduction_amount)
        .floor()
        .max(f64::from(minimum)) as u32
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
    fn calculates_floor_with_minimum() {
        assert_eq!(recommended_cap(312.0, 0.09, 60), 284);
        assert_eq!(recommended_cap(101.0, 0.09, 60), 92);
        assert_eq!(recommended_cap(50.0, 0.09, 60), 60);
        assert_eq!(recommended_cap(f64::NAN, 0.09, 60), 0);
        assert_eq!(recommended_cap(100.0, f64::NAN, 60), 0);
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
