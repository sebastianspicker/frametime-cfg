use std::collections::BTreeMap;

pub const OPTIMIZATION_TEMPLATE: &str =
    include_str!("../../../assets/cfgs/optimization.cfg.template");

#[must_use]
pub fn optimization_values() -> BTreeMap<&'static str, &'static str> {
    OPTIMIZATION_TEMPLATE
        .lines()
        .filter_map(|line| line.split_once(' '))
        .collect()
}

#[must_use]
pub fn render_optimization_cfg_at(timestamp: &str) -> String {
    let mut output = format!(
        "// frametime.cfg - optimization.cfg\n\
         // Generated: {timestamp}\n\
         // Optional autoexec bootstrap; command order can vary with other configs.\n\
         // Prove the active cfg with a temporary sentinel before relying on it.\n\
         // Keep personal settings in your own autoexec or separate cfg.\n\
         //\n\
         // Optional standalone CFGs (also in game\\csgo\\cfg\\, use from console as needed):\n\
         //   exec net_stable     - buffering None / reset\n\
         //   exec net_highping   - buffering None; high RTT cannot be buffered away\n\
         //   exec net_unstable   - 1 Packet trial for measured jitter/misses\n\
         //   exec net_bad        - 2 Packets only when misses persist at 1\n\
         //   exec debug_hud      - temporary telemetry and network diagnostics\n\
         //   exec debug_hud_off  - restore normal conditional telemetry\n\
         //   exec frame_baseline_400 - restore the configured 400 FPS baseline\n\
         //   exec frame_raw_uncapped - raw-latency branch; run before connecting\n\
         //   exec audio_stable   - restore audio defaults (Natural EQ)\n\
         //   exec audio_eq_crisp - optional mid/high emphasis\n\
         //   exec audio_eq_smooth - optional reduced mid/high energy\n\
         //   exec audio_legacy_lr_isolation - legacy hard L/R trial\n\
         //   exec audio_lowlatency_025 - unproven larger-buffer experiment\n\
         //   exec audio_lowlatency_001 - restore mixer defaults\n\n"
    );
    output.push_str(OPTIMIZATION_TEMPLATE);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_source_has_one_explicit_generated_assignment() {
        assert_eq!(optimization_values().len(), 1);
        assert_eq!(optimization_values()["fps_max"], "400");
    }

    #[test]
    fn generated_header_matches_current_minimal_shape_and_order() {
        let output = render_optimization_cfg_at("2026-08-10 12:34");
        assert!(
            output.starts_with(
                "// frametime.cfg - optimization.cfg\n// Generated: 2026-08-10 12:34\n"
            )
        );
        assert!(output.ends_with("fps_max 400\n"));
        assert_eq!(optimization_values().len(), 1);
        assert!(!output.contains("cl_interp"));
        assert!(!output.contains("snd_mixahead"));
    }
}
