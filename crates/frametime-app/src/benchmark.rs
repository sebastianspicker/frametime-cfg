use std::{fs::File, io::Read, path::PathBuf};

use frametime_domain::fps::{
    BenchmarkCapture, BenchmarkRunEvidence, FpsCapStrategy, MAX_VPROF_INPUT_BYTES,
    ValidatedBenchmarkCapture, measured_fps_cap, parse_vprof_bytes_detailed,
    parse_vprof_output_detailed,
};
use frametime_windows::{
    AuthenticatedPackage, copy_text_to_clipboard, persist_baseline_benchmark,
    persist_final_benchmark, persist_fps_capture, platform_is_supported, read_text_from_clipboard,
};

use crate::commands::{FpsRequest, FpsStrategyValue, ValidatedFpsRequest, VprofBenchmarkRequest};
use crate::{
    ApplicationError, BenchmarkOutcome, BenchmarkPersistence, FpsCapOutcome, FpsCapStrategyLabel,
    require_authenticated_package,
};

pub fn run_fps_cap(request: FpsRequest) -> Result<FpsCapOutcome, ApplicationError> {
    let FpsRequest {
        average,
        text,
        file,
        clipboard,
        strategy,
        measured_cap,
        refresh_hz,
        ceiling_margin_hz,
        label,
        copy,
        no_persist,
    } = request;
    let cap_strategy = select_fps_strategy(strategy, measured_cap, refresh_hz, ceiling_margin_hz)?;
    if let Some(average_fps) = average {
        if text.is_some() || file.is_some() || clipboard {
            return Err(fps_vprof_source_error());
        }
        if !average_fps.is_finite() || average_fps <= 0.0 {
            return Err(ApplicationError::Invalid(
                "AVERAGE_FPS must be finite and greater than zero".into(),
            ));
        }
        if !matches!(
            cap_strategy,
            FpsCapStrategy::RawLatency { measured_cap: None }
        ) {
            return Err(ApplicationError::Invalid(
                "aggregate-only AVERAGE_FPS cannot authorize a new nonzero cap; provide complete VProf run evidence".into(),
            ));
        }
        let capture = BenchmarkCapture {
            average_fps,
            p1_fps: 0.0,
            runs: 1,
        };
        let effects = FpsSideEffects {
            label,
            copy,
            no_persist,
        };
        let persistence = effects.apply(0, capture, None)?;
        return Ok(FpsCapOutcome {
            cap: 0,
            capture,
            run_evidence: None,
            strategy: FpsCapStrategyLabel::RawUncapped,
            copied_to_clipboard: copy,
            persistence,
        });
    }
    let source = select_vprof_source(text, file, clipboard, fps_vprof_source_error)?;
    let capture = read_vprof_capture(source)?;
    evaluate_fps_cap(ValidatedFpsRequest {
        capture,
        strategy,
        measured_cap,
        refresh_hz,
        ceiling_margin_hz,
        label,
        copy,
        no_persist,
    })
}

pub fn evaluate_fps_cap(request: ValidatedFpsRequest) -> Result<FpsCapOutcome, ApplicationError> {
    let cap_strategy = select_fps_strategy(
        request.strategy,
        request.measured_cap,
        request.refresh_hz,
        request.ceiling_margin_hz,
    )?;
    let cap = measured_fps_cap(cap_strategy, &request.capture).ok_or_else(|| {
        let selected_cap = requested_cap(cap_strategy).unwrap_or_default();
        ApplicationError::Invalid(format!(
            "selected cap {selected_cap} rejected: {} failing runs among {} valid runs, including {} invalid runs with P1 above Avg; at least 5 valid runs are required and every P1 must support the cap",
            request.capture.failing_runs(selected_cap),
            request.capture.observations().len(),
            request.capture.invalid_ordered_runs()
        ))
    })?;
    let effects = FpsSideEffects {
        label: request.label,
        copy: request.copy,
        no_persist: request.no_persist,
    };
    let aggregate = request.capture.aggregate();
    let run_evidence = request.capture.run_evidence().clone();
    let persistence = effects.apply(cap, aggregate, Some(&run_evidence))?;
    Ok(FpsCapOutcome {
        cap,
        capture: aggregate,
        run_evidence: Some(run_evidence),
        strategy: fps_strategy_label(request.strategy, cap),
        copied_to_clipboard: request.copy,
        persistence,
    })
}

const fn requested_cap(strategy: FpsCapStrategy) -> Option<u32> {
    match strategy {
        FpsCapStrategy::RawLatency { measured_cap } => measured_cap,
        FpsCapStrategy::Vrr {
            refresh_hz,
            ceiling_margin_hz,
        } => refresh_hz.checked_sub(ceiling_margin_hz),
    }
}

struct FpsSideEffects {
    label: String,
    copy: bool,
    no_persist: bool,
}

impl FpsSideEffects {
    fn apply(
        self,
        cap: u32,
        capture: BenchmarkCapture,
        run_evidence: Option<&BenchmarkRunEvidence>,
    ) -> Result<BenchmarkPersistence, ApplicationError> {
        let package = if authenticated_package_is_required(self.copy, self.no_persist) {
            Some(require_authenticated_package()?)
        } else {
            None
        };
        if self.copy {
            copy_text_to_clipboard(&cap.to_string()).map_err(ApplicationError::failed)?;
        }
        persist_selected_fps_capture(
            package.as_ref(),
            cap,
            capture,
            run_evidence,
            self.label,
            self.no_persist,
        )
    }
}

fn select_fps_strategy(
    strategy: FpsStrategyValue,
    measured_cap: u32,
    refresh_hz: u32,
    ceiling_margin_hz: u32,
) -> Result<FpsCapStrategy, ApplicationError> {
    match strategy {
        FpsStrategyValue::Raw if measured_cap == 0 => {
            Ok(FpsCapStrategy::RawLatency { measured_cap: None })
        }
        FpsStrategyValue::Raw if (30..=1000).contains(&measured_cap) => {
            Ok(FpsCapStrategy::RawLatency {
                measured_cap: Some(measured_cap),
            })
        }
        FpsStrategyValue::Raw => Err(ApplicationError::Invalid(
            "--measured-cap must be zero (uncapped) or between 30 and 1000".into(),
        )),
        FpsStrategyValue::Vrr
            if (30..=1000).contains(&refresh_hz)
                && ceiling_margin_hz > 0
                && ceiling_margin_hz < refresh_hz =>
        {
            Ok(FpsCapStrategy::Vrr {
                refresh_hz,
                ceiling_margin_hz,
            })
        }
        FpsStrategyValue::Vrr => Err(ApplicationError::Invalid(
            "VRR requires --refresh-hz between 30 and 1000 and a positive --ceiling-margin-hz below refresh".into(),
        )),
    }
}

fn authenticated_package_is_required(copy: bool, no_persist: bool) -> bool {
    platform_is_supported() && (copy || !no_persist)
}

fn persist_selected_fps_capture(
    package: Option<&AuthenticatedPackage>,
    cap: u32,
    capture: BenchmarkCapture,
    run_evidence: Option<&BenchmarkRunEvidence>,
    label: String,
    no_persist: bool,
) -> Result<BenchmarkPersistence, ApplicationError> {
    if platform_is_supported() && !no_persist {
        persist_fps_capture(
            package.ok_or_else(|| {
                ApplicationError::failed("FPS persistence lost package authority")
            })?,
            cap,
            capture,
            run_evidence.cloned(),
            label,
        )
        .map_err(ApplicationError::failed)?;
        Ok(BenchmarkPersistence::Persisted)
    } else if no_persist {
        Ok(BenchmarkPersistence::Disabled)
    } else {
        Ok(BenchmarkPersistence::UnsupportedHost)
    }
}

fn fps_strategy_label(strategy: FpsStrategyValue, cap: u32) -> FpsCapStrategyLabel {
    if cap == 0 {
        FpsCapStrategyLabel::RawUncapped
    } else if matches!(strategy, FpsStrategyValue::Vrr) {
        FpsCapStrategyLabel::VrrCeiling
    } else {
        FpsCapStrategyLabel::RawMeasured
    }
}

fn fps_vprof_source_error() -> ApplicationError {
    ApplicationError::Invalid(
        "provide exactly one of AVERAGE_FPS, --vprof-text, --vprof-file, or --clipboard".into(),
    )
}

/// P1:17 is intentionally not a FPS-cap calculator: it accepts only VProf
/// sources and persists a complete baseline observation on Windows.
pub fn run_baseline_benchmark(
    request: VprofBenchmarkRequest,
) -> Result<BenchmarkOutcome, ApplicationError> {
    let capture = read_complete_vprof_capture(request, "baseline-benchmark")?;
    require_windows_benchmark_host("baseline-benchmark")?;
    let package = require_authenticated_package()?;
    persist_baseline_benchmark(&package, &capture).map_err(ApplicationError::failed)?;
    Ok(BenchmarkOutcome {
        capture: capture.aggregate(),
        run_evidence: Some(capture.run_evidence().clone()),
        receipt: None,
    })
}

/// P3:13 is committed only through the transaction-bound final receipt API.
/// It does not clear the retained same-user Phase 3 handoff.
pub(crate) fn read_final_benchmark_capture(
    request: VprofBenchmarkRequest,
) -> Result<ValidatedBenchmarkCapture, ApplicationError> {
    let capture = read_complete_vprof_capture(request, "final-benchmark")?;
    require_windows_benchmark_host("final-benchmark")?;
    Ok(capture)
}

pub(crate) fn persist_final_benchmark_capture(
    capture: ValidatedBenchmarkCapture,
    runtime: &frametime_windows::VerifiedSelectedRuntime,
) -> Result<BenchmarkOutcome, ApplicationError> {
    let receipt = persist_final_benchmark(runtime, &capture).map_err(ApplicationError::failed)?;
    Ok(BenchmarkOutcome {
        capture: capture.aggregate(),
        run_evidence: Some(capture.run_evidence().clone()),
        receipt: Some(receipt),
    })
}

/// Read one bounded VProf source for advisory GUI analysis without persistence.
pub fn read_fps_capture(
    request: VprofBenchmarkRequest,
) -> Result<ValidatedBenchmarkCapture, ApplicationError> {
    let source = select_vprof_source(request.text, request.file, request.clipboard, || {
        ApplicationError::Invalid("Choose exactly one VProf source".into())
    })?;
    read_vprof_capture(source)
}

fn read_complete_vprof_capture(
    request: VprofBenchmarkRequest,
    command: &str,
) -> Result<ValidatedBenchmarkCapture, ApplicationError> {
    if request.clipboard && !platform_is_supported() {
        return Err(ApplicationError::Failed(format!(
            "{command} is only supported on Windows; no artifacts were written"
        )));
    }
    let source = select_vprof_source(request.text, request.file, request.clipboard, || {
        ApplicationError::Invalid(format!(
            "{command}: provide exactly one of --vprof-text, --vprof-file, or --clipboard"
        ))
    })?;
    let capture = read_vprof_capture(source)?;
    validate_complete_vprof_capture(capture, command)
}

enum VprofSource {
    Text(String),
    File(PathBuf),
    Clipboard,
}

fn select_vprof_source(
    text: Option<String>,
    file: Option<PathBuf>,
    clipboard: bool,
    invalid_source: impl FnOnce() -> ApplicationError,
) -> Result<VprofSource, ApplicationError> {
    match (text, file, clipboard) {
        (Some(value), None, false) => Ok(VprofSource::Text(value)),
        (None, Some(path), false) => Ok(VprofSource::File(path)),
        (None, None, true) => Ok(VprofSource::Clipboard),
        _ => Err(invalid_source()),
    }
}

fn read_vprof_capture(source: VprofSource) -> Result<ValidatedBenchmarkCapture, ApplicationError> {
    let source_name = match &source {
        VprofSource::Text(_) => "--vprof-text",
        VprofSource::File(_) => "--vprof-file",
        VprofSource::Clipboard => "clipboard",
    };
    match source {
        VprofSource::Text(value) => parse_vprof_output_detailed(&value),
        VprofSource::File(path) => {
            let bytes = read_bounded_vprof_file(&path)?;
            parse_vprof_bytes_detailed(&bytes)
        }
        VprofSource::Clipboard => {
            let value = read_text_from_clipboard().map_err(ApplicationError::failed)?;
            parse_vprof_output_detailed(&value)
        }
    }
    .map_err(|error| ApplicationError::Invalid(format!("{source_name}: {error}")))
}

fn read_bounded_vprof_file(path: &PathBuf) -> Result<Vec<u8>, ApplicationError> {
    let file = File::open(path)
        .map_err(|error| ApplicationError::failed(format!("open VProf file: {error}")))?;
    let mut bytes = Vec::new();
    file.take((MAX_VPROF_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| ApplicationError::failed(format!("read VProf file: {error}")))?;
    Ok(bytes)
}

fn validate_complete_vprof_capture(
    capture: ValidatedBenchmarkCapture,
    command: &str,
) -> Result<ValidatedBenchmarkCapture, ApplicationError> {
    let aggregate = capture.aggregate();
    let average_is_valid = aggregate.average_fps.is_finite() && aggregate.average_fps > 0.0;
    let p1_is_valid = aggregate.p1_fps.is_finite()
        && aggregate.p1_fps > 0.0
        && aggregate.p1_fps <= aggregate.average_fps;
    let runs_are_valid = aggregate.runs > 0;
    if !(average_is_valid && p1_is_valid && runs_are_valid) {
        return Err(ApplicationError::Invalid(format!(
            "{command} requires complete VProf Avg > 0, P1 > 0, and runs > 0"
        )));
    }
    Ok(capture)
}

fn require_windows_benchmark_host(command: &str) -> Result<(), ApplicationError> {
    if !platform_is_supported() {
        return Err(ApplicationError::Failed(format!(
            "{command} is only supported on Windows; no artifacts were written"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
