const FPS_CAP_FINAL_STEP_MESSAGE: &str =
    "FPS cap will be calculated in the final step after all optimizations.";

fn inspect_config_state(config: Option<&Config>, state: &State) -> Result<Inspection, String> {
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

fn verify_config_state(config: Option<&Config>, state: &State) -> Result<(), String> {
    match inspect_config_state(config, state)? {
        Inspection::Satisfied => Ok(()),
        Inspection::Advisory { .. } => {
            Err("configuration observation is advisory and unverified".into())
        }
        _ => Err("configuration/state observation was not satisfied".into()),
    }
}

fn inspect_gpu_inventory(hardware: &HardwareInfo) -> Result<Inspection, String> {
    if hardware.display_adapters.is_empty() {
        Err("native display-adapter inventory is empty".into())
    } else {
        Ok(Inspection::Satisfied)
    }
}

fn verify_gpu_inventory(captured: &HardwareInfo, observed: &HardwareInfo) -> Result<(), String> {
    inspect_gpu_inventory(captured)?;
    inspect_gpu_inventory(observed)?;
    if observed == captured {
        Ok(())
    } else {
        Err("display-adapter inventory changed after inspection".into())
    }
}

fn inspect_fps_cap_info(config: Option<&Config>, state: &State) -> Result<Inspection, String> {
    if state.fps_cap == 0 && state.avg_fps == 0.0 {
        return Ok(Inspection::Satisfied);
    }
    if state.fps_cap == 0 || !state.avg_fps.is_finite() || state.avg_fps <= 0.0 {
        return Ok(Inspection::Unsupported);
    }
    let Some(config) = config else {
        return Ok(Inspection::Unsupported);
    };
    if config.validate().is_err() {
        return Ok(Inspection::Unsupported);
    }
    let expected = frametime_core::fps::recommended_cap(
        state.avg_fps,
        config.fps_cap.percent,
        config.fps_cap.minimum,
    );
    Ok(if state.fps_cap == expected {
        Inspection::Satisfied
    } else {
        Inspection::Unsupported
    })
}

fn verify_fps_cap_info(config: Option<&Config>, state: &State) -> Result<(), String> {
    match inspect_fps_cap_info(config, state)? {
        Inspection::Satisfied => Ok(()),
        Inspection::Unsupported => Err("FPS-cap observation is incomplete or inconsistent".into()),
        Inspection::Advisory { .. } => Err("FPS-cap observation is advisory and unverified".into()),
        Inspection::NeedsApply | Inspection::Inapplicable => {
            Err("FPS-cap observation returned an invalid verification state".into())
        }
    }
}

fn report_fps_cap_info(state: &State) {
    println!("{FPS_CAP_FINAL_STEP_MESSAGE}");
    if state.fps_cap > 0 {
        println!(
            "Already calculated cap: {} (avg {:.1})",
            state.fps_cap, state.avg_fps
        );
    }
}
