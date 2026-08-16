use std::{
    fs,
    path::{Path, PathBuf},
};

use frametime_core::fps::{BenchmarkCapture, parse_vprof_output, recommended_cap};
use frametime_windows::{
    WINDOWS_WORK_DIR, copy_text_to_clipboard, persist_baseline_benchmark, persist_final_benchmark,
    persist_fps_capture, platform_is_supported, read_text_from_clipboard,
};

use crate::{
    cli::{FpsRequest, VprofBenchmarkRequest},
    error::AppError,
    package_auth::require_authenticated_package,
};

pub(crate) fn run_fps_cap(request: FpsRequest) -> Result<(), AppError> {
    if !(0.01..=0.50).contains(&request.reduction) {
        return Err(AppError::Invalid(
            "--reduction must be between 0.01 and 0.50".into(),
        ));
    }
    if !(30..=500).contains(&request.minimum) {
        return Err(AppError::Invalid(
            "--minimum must be between 30 and 500".into(),
        ));
    }
    let capture = read_fps_capture(
        request.average,
        request.text,
        request.file,
        request.clipboard,
    )?;
    let cap = recommended_cap(capture.average_fps, request.reduction, request.minimum);
    if cap == 0 {
        return Err(AppError::Invalid(
            "average_fps must be a finite positive number".into(),
        ));
    }
    println!("Recommended fps_max: {cap}");
    println!(
        "Formula: floor(avg - floor(avg * reduction)), minimum {}",
        request.minimum
    );
    println!(
        "Average FPS: {:.1}; P1 FPS: {:.1}; P1 ratio: {}; Runs: {}",
        capture.average_fps,
        capture.p1_fps,
        capture
            .p1_ratio()
            .map_or_else(|| "n/a".into(), |ratio| format!("{ratio:.3}")),
        capture.runs
    );
    let _package = if platform_is_supported() && (request.copy || !request.no_persist) {
        Some(require_authenticated_package()?)
    } else {
        None
    };
    if request.copy {
        copy_text_to_clipboard(&cap.to_string()).map_err(AppError::failed)?;
        println!("Copied fps_max to the native clipboard.");
    }
    if platform_is_supported() && !request.no_persist {
        persist_fps_capture(Path::new(WINDOWS_WORK_DIR), cap, capture, request.label)
            .map_err(AppError::failed)?;
        println!("Persisted benchmark state and history in {WINDOWS_WORK_DIR}.");
    } else if request.no_persist {
        println!("Persistence disabled by --no-persist.");
    } else {
        println!("Not persisted: this host is not Windows.");
    }
    Ok(())
}

fn read_fps_capture(
    average: Option<f64>,
    text: Option<String>,
    file: Option<PathBuf>,
    clipboard: bool,
) -> Result<BenchmarkCapture, AppError> {
    match (average, text, file, clipboard) {
        (Some(value), None, None, false) => Ok(BenchmarkCapture {
            average_fps: value,
            p1_fps: 0.0,
            runs: 1,
        }),
        (None, text, file, clipboard) => {
            let source = select_vprof_source(text, file, clipboard, fps_vprof_source_error)?;
            read_vprof_capture(source)
        }
        _ => Err(fps_vprof_source_error()),
    }
}

fn fps_vprof_source_error() -> AppError {
    AppError::Invalid(
        "provide exactly one of AVERAGE_FPS, --vprof-text, --vprof-file, or --clipboard".into(),
    )
}

/// P1:17 is intentionally not a FPS-cap calculator: it accepts only VProf
/// sources and persists a complete baseline observation on Windows.
pub(crate) fn run_baseline_benchmark(request: VprofBenchmarkRequest) -> Result<(), AppError> {
    let capture = read_complete_vprof_capture(request, "baseline-benchmark")?;
    require_windows_benchmark_host("baseline-benchmark")?;
    let _package = require_authenticated_package()?;
    persist_baseline_benchmark(Path::new(WINDOWS_WORK_DIR), capture).map_err(AppError::failed)?;
    println!(
        "Persisted Baseline (before optimizations): Avg {:.1}; P1 {:.1}; Runs: {}.",
        capture.average_fps, capture.p1_fps, capture.runs
    );
    Ok(())
}

/// P3:13 is committed only through the transaction-bound final receipt API.
/// It does not clear the retained same-user Phase 3 handoff.
pub(crate) fn read_final_benchmark_capture(
    request: VprofBenchmarkRequest,
) -> Result<BenchmarkCapture, AppError> {
    let capture = read_complete_vprof_capture(request, "final-benchmark")?;
    require_windows_benchmark_host("final-benchmark")?;
    Ok(capture)
}

pub(crate) fn persist_final_benchmark_capture(
    capture: BenchmarkCapture,
    config: &frametime_windows::VerifiedConfig,
) -> Result<(), AppError> {
    let receipt = persist_final_benchmark(Path::new(WINDOWS_WORK_DIR), capture, config)
        .map_err(AppError::failed)?;
    println!(
        "Persisted After all optimizations: Avg {:.1}; P1 {:.1}; Runs: {}; fps_max {}.",
        receipt.avg_fps, receipt.p1_fps, receipt.runs, receipt.fps_cap
    );
    println!("Final benchmark receipt: {}.", receipt.receipt_id);
    Ok(())
}

fn read_complete_vprof_capture(
    request: VprofBenchmarkRequest,
    command: &str,
) -> Result<BenchmarkCapture, AppError> {
    if request.clipboard && !platform_is_supported() {
        return Err(AppError::Failed(format!(
            "{command} is only supported on Windows; no artifacts were written"
        )));
    }
    let source = select_vprof_source(request.text, request.file, request.clipboard, || {
        AppError::Invalid(format!(
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
    invalid_source: impl FnOnce() -> AppError,
) -> Result<VprofSource, AppError> {
    match (text, file, clipboard) {
        (Some(value), None, false) => Ok(VprofSource::Text(value)),
        (None, Some(path), false) => Ok(VprofSource::File(path)),
        (None, None, true) => Ok(VprofSource::Clipboard),
        _ => Err(invalid_source()),
    }
}

fn read_vprof_capture(source: VprofSource) -> Result<BenchmarkCapture, AppError> {
    let invalid_result = match &source {
        VprofSource::Text(_) => "--vprof-text contains no valid VProf result",
        VprofSource::File(_) => "--vprof-file contains no valid VProf result",
        VprofSource::Clipboard => "clipboard contains no valid VProf result",
    };
    let value = match source {
        VprofSource::Text(value) => value,
        VprofSource::File(path) => fs::read_to_string(path)
            .map_err(|error| AppError::failed(format!("read VProf file: {error}")))?,
        VprofSource::Clipboard => read_text_from_clipboard().map_err(AppError::failed)?,
    };
    parse_vprof_output(&value).ok_or_else(|| AppError::Invalid(invalid_result.into()))
}

fn validate_complete_vprof_capture(
    capture: BenchmarkCapture,
    command: &str,
) -> Result<BenchmarkCapture, AppError> {
    let average_is_valid = capture.average_fps.is_finite() && capture.average_fps > 0.0;
    let p1_is_valid =
        capture.p1_fps.is_finite() && capture.p1_fps > 0.0 && capture.p1_fps <= capture.average_fps;
    let runs_are_valid = capture.runs > 0;
    if !(average_is_valid && p1_is_valid && runs_are_valid) {
        return Err(AppError::Invalid(format!(
            "{command} requires complete VProf Avg > 0, P1 > 0, and runs > 0"
        )));
    }
    Ok(capture)
}

fn require_windows_benchmark_host(command: &str) -> Result<(), AppError> {
    if !platform_is_supported() {
        return Err(AppError::Failed(format!(
            "{command} is only supported on Windows; no artifacts were written"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vprof_request(
        text: Option<&str>,
        file: Option<PathBuf>,
        clipboard: bool,
    ) -> VprofBenchmarkRequest {
        VprofBenchmarkRequest {
            text: text.map(str::to_owned),
            file,
            clipboard,
        }
    }

    #[test]
    fn complete_vprof_text_capture_is_read_and_validated() {
        let capture = read_complete_vprof_capture(
            vprof_request(
                Some("[VProf] FPS: Avg=300.0, P1=150.0\n[VProf] FPS: Avg=200.0, P1=100.0"),
                None,
                false,
            ),
            "baseline-benchmark",
        )
        .expect("complete capture");

        assert_eq!(
            capture,
            BenchmarkCapture {
                average_fps: 250.0,
                p1_fps: 125.0,
                runs: 2,
            }
        );
    }

    #[test]
    fn fps_cap_keeps_manual_average_as_its_only_non_vprof_source() {
        let manual = read_fps_capture(Some(240.0), None, None, false).expect("manual average");
        assert_eq!(
            manual,
            BenchmarkCapture {
                average_fps: 240.0,
                p1_fps: 0.0,
                runs: 1,
            }
        );

        let ambiguous = read_fps_capture(
            Some(240.0),
            Some("[VProf] FPS: Avg=300.0, P1=150.0".into()),
            None,
            false,
        )
        .expect_err("manual average plus VProf source");
        assert_eq!(
            ambiguous.to_string(),
            "provide exactly one of AVERAGE_FPS, --vprof-text, --vprof-file, or --clipboard"
        );
    }

    #[test]
    fn vprof_source_selection_rejects_missing_or_ambiguous_sources_before_file_reads() {
        let missing =
            read_complete_vprof_capture(vprof_request(None, None, false), "baseline-benchmark")
                .expect_err("missing source");
        assert_eq!(
            missing.to_string(),
            "baseline-benchmark: provide exactly one of --vprof-text, --vprof-file, or --clipboard"
        );

        let ambiguous = read_complete_vprof_capture(
            vprof_request(
                Some("[VProf] FPS: Avg=300.0, P1=150.0"),
                Some(PathBuf::from("source-must-not-be-read.vprof")),
                false,
            ),
            "baseline-benchmark",
        )
        .expect_err("ambiguous source");
        assert_eq!(
            ambiguous.to_string(),
            "baseline-benchmark: provide exactly one of --vprof-text, --vprof-file, or --clipboard"
        );
    }

    #[test]
    fn invalid_vprof_source_preserves_its_input_specific_error() {
        let error = read_complete_vprof_capture(
            vprof_request(Some("no VProf result"), None, false),
            "baseline-benchmark",
        )
        .expect_err("invalid source");

        assert_eq!(
            error.to_string(),
            "--vprof-text contains no valid VProf result"
        );
    }

    #[test]
    fn incomplete_capture_is_rejected_by_validation_without_reading_a_source() {
        let error = validate_complete_vprof_capture(
            BenchmarkCapture {
                average_fps: 300.0,
                p1_fps: 0.0,
                runs: 1,
            },
            "final-benchmark",
        )
        .expect_err("incomplete capture");

        assert_eq!(
            error.to_string(),
            "final-benchmark requires complete VProf Avg > 0, P1 > 0, and runs > 0"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn clipboard_benchmark_rejection_happens_before_source_selection() {
        let error = read_complete_vprof_capture(
            vprof_request(
                Some("[VProf] FPS: Avg=300.0, P1=150.0"),
                Some(PathBuf::from("clipboard-and-file-must-not-be-read.vprof")),
                true,
            ),
            "final-benchmark",
        )
        .expect_err("unsupported clipboard benchmark");

        assert_eq!(
            error.to_string(),
            "final-benchmark is only supported on Windows; no artifacts were written"
        );
    }
}
