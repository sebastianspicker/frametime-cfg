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
        capture.aggregate(),
        BenchmarkCapture {
            average_fps: 250.0,
            p1_fps: 125.0,
            runs: 2,
        }
    );
}

#[test]
fn fps_cap_keeps_manual_average_as_its_only_non_vprof_source() {
    let manual = run_fps_cap(FpsRequest {
        average: Some(240.0),
        text: None,
        file: None,
        clipboard: false,
        strategy: FpsStrategyValue::Raw,
        measured_cap: 0,
        refresh_hz: 0,
        ceiling_margin_hz: 3,
        label: "test".into(),
        copy: false,
        no_persist: true,
    })
    .expect("manual average");
    assert_eq!(
        manual.capture,
        BenchmarkCapture {
            average_fps: 240.0,
            p1_fps: 0.0,
            runs: 1,
        }
    );

    let ambiguous = run_fps_cap(FpsRequest {
        average: Some(240.0),
        text: Some("[VProf] FPS: Avg=300.0, P1=150.0".into()),
        file: None,
        clipboard: false,
        strategy: FpsStrategyValue::Raw,
        measured_cap: 0,
        refresh_hz: 0,
        ceiling_margin_hz: 3,
        label: "test".into(),
        copy: false,
        no_persist: true,
    })
    .expect_err("manual average plus VProf source");
    assert_eq!(
        ambiguous.to_string(),
        "provide exactly one of AVERAGE_FPS, --vprof-text, --vprof-file, or --clipboard"
    );
}

#[test]
fn fps_strategy_validation_preserves_raw_and_vrr_boundaries() {
    assert_eq!(
        select_fps_strategy(FpsStrategyValue::Raw, 0, 0, 0).expect("uncapped raw"),
        FpsCapStrategy::RawLatency { measured_cap: None }
    );
    assert_eq!(
        select_fps_strategy(FpsStrategyValue::Raw, 30, 0, 0).expect("minimum raw cap"),
        FpsCapStrategy::RawLatency {
            measured_cap: Some(30),
        }
    );
    assert_eq!(
        select_fps_strategy(FpsStrategyValue::Vrr, 0, 240, 10).expect("valid VRR cap"),
        FpsCapStrategy::Vrr {
            refresh_hz: 240,
            ceiling_margin_hz: 10,
        }
    );

    assert_eq!(
        select_fps_strategy(FpsStrategyValue::Raw, 29, 0, 0)
            .expect_err("raw cap below minimum")
            .to_string(),
        "--measured-cap must be zero (uncapped) or between 30 and 1000"
    );
    assert_eq!(
        select_fps_strategy(FpsStrategyValue::Vrr, 0, 240, 240)
            .expect_err("VRR margin equals refresh")
            .to_string(),
        "VRR requires --refresh-hz between 30 and 1000 and a positive --ceiling-margin-hz below refresh"
    );
}

#[test]
fn fps_strategy_labels_distinguish_uncapped_raw_measured_and_vrr() {
    assert_eq!(
        fps_strategy_label(FpsStrategyValue::Raw, 0),
        FpsCapStrategyLabel::RawUncapped
    );
    assert_eq!(
        fps_strategy_label(FpsStrategyValue::Raw, 120),
        FpsCapStrategyLabel::RawMeasured
    );
    assert_eq!(
        fps_strategy_label(FpsStrategyValue::Vrr, 237),
        FpsCapStrategyLabel::VrrCeiling
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
        "--vprof-text: VProf input contains no valid Avg/P1 observations"
    );
}

#[test]
fn incomplete_capture_is_rejected_by_validation_without_reading_a_source() {
    let capture = parse_vprof_output_detailed("[VProf] FPS: Avg=300.0, P1=0.0")
        .expect("syntactically valid capture");
    let error = validate_complete_vprof_capture(capture, "final-benchmark")
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

#[test]
fn advisory_source_reader_keeps_weak_runs_without_authorizing_a_cap() {
    let capture = read_fps_capture(vprof_request(
        Some("[VProf] FPS: Avg=356, P1=248\n[VProf] FPS: Avg=348, P1=230"),
        None,
        false,
    ))
    .expect("advisory import accepts evidence independently of cap selection");
    assert_eq!(capture.observations().len(), 2);
    assert_eq!(capture.failing_runs(237), 1);
    assert!(read_fps_capture(vprof_request(None, None, false)).is_err());
}

#[test]
fn advisory_source_reader_rejects_an_oversized_file_without_truncating_evidence() {
    let path =
        std::env::temp_dir().join(format!("frametime-vprof-limit-{}.txt", std::process::id()));
    let bytes = vec![b'x'; MAX_VPROF_INPUT_BYTES + 1];
    std::fs::write(&path, bytes).expect("write isolated input");
    let result = read_fps_capture(vprof_request(None, Some(path.clone()), false));
    std::fs::remove_file(path).expect("remove test-owned input");
    assert!(
        result
            .expect_err("input must not be truncated")
            .to_string()
            .contains("exceeds")
    );
}
