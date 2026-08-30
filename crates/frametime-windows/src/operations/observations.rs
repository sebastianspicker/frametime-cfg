use crate::*;
pub(crate) const FPS_CAP_FINAL_STEP_MESSAGE: &str =
    "FPS cap will be calculated in the final step after all optimizations.";

pub(crate) fn inspect_config_state(
    config: Option<&Config>,
    state: &State,
) -> Result<Inspection, String> {
    let config = config.ok_or("validated frametime.toml is unavailable")?;
    config
        .validate()
        .map_err(|error| format!("invalid config: {error}"))?;
    state.validate().map_err(str::to_owned)?;
    if !state.work_dir.eq_ignore_ascii_case(WINDOWS_WORK_DIR) {
        return Err("state workDir must be C:\\FRAMETIME_CFG".into());
    }
    Ok(Inspection::Satisfied)
}

pub(crate) fn verify_config_state(config: Option<&Config>, state: &State) -> Result<(), String> {
    match inspect_config_state(config, state)? {
        Inspection::Satisfied => Ok(()),
        Inspection::Advisory { .. } => {
            Err("configuration observation is advisory and unverified".into())
        }
        _ => Err("configuration/state observation was not satisfied".into()),
    }
}

pub(crate) fn inspect_gpu_inventory(hardware: &HardwareInfo) -> Result<Inspection, String> {
    if hardware.display_adapters.is_empty() {
        Err("native display-adapter inventory is empty".into())
    } else {
        Ok(Inspection::Satisfied)
    }
}

pub(crate) fn verify_gpu_inventory(
    captured: &HardwareInfo,
    observed: &HardwareInfo,
) -> Result<(), String> {
    inspect_gpu_inventory(captured)?;
    inspect_gpu_inventory(observed)?;
    if observed == captured {
        Ok(())
    } else {
        Err("display-adapter inventory changed after inspection".into())
    }
}

pub(crate) fn inspect_fps_cap_info(
    config: Option<&Config>,
    state: &State,
) -> Result<Inspection, String> {
    if state.fps_cap == 0 && state.avg_fps == 0.0 {
        return Ok(Inspection::Satisfied);
    }
    if !state.avg_fps.is_finite() || state.avg_fps <= 0.0 {
        return Ok(Inspection::Unsupported);
    }
    let Some(config) = config else {
        return Ok(Inspection::Unsupported);
    };
    if config.validate().is_err() {
        return Ok(Inspection::Unsupported);
    }
    let capture = state.final_benchmark.as_ref().map_or(
        frametime_domain::fps::BenchmarkCapture {
            average_fps: state.avg_fps,
            p1_fps: state.p1_fps.unwrap_or_default(),
            runs: 1,
        },
        |receipt| frametime_domain::fps::BenchmarkCapture {
            average_fps: receipt.avg_fps,
            p1_fps: receipt.p1_fps,
            runs: receipt.runs,
        },
    );
    let expected = frametime_domain::fps::measured_fps_cap(config.fps_cap.strategy(), capture);
    Ok(if expected == Some(state.fps_cap) {
        Inspection::Satisfied
    } else {
        Inspection::Unsupported
    })
}

pub(crate) fn verify_fps_cap_info(config: Option<&Config>, state: &State) -> Result<(), String> {
    match inspect_fps_cap_info(config, state)? {
        Inspection::Satisfied => Ok(()),
        Inspection::Unsupported => Err("FPS-cap observation is incomplete or inconsistent".into()),
        Inspection::Advisory { .. } => Err("FPS-cap observation is advisory and unverified".into()),
        Inspection::NeedsApply | Inspection::Inapplicable => {
            Err("FPS-cap observation returned an invalid verification state".into())
        }
    }
}

pub(crate) fn fps_cap_info_messages(state: &State) -> Vec<String> {
    let mut messages = vec![FPS_CAP_FINAL_STEP_MESSAGE.into()];
    if state.fps_cap > 0 {
        messages.push(format!(
            "Already calculated cap: {} (avg {:.1})",
            state.fps_cap, state.avg_fps
        ));
    } else if state.avg_fps > 0.0 {
        messages.push("Selected FPS strategy: raw latency, uncapped (fps_max 0).".into());
    }
    messages
}
