use std::collections::BTreeMap;

pub const OPTIMIZATION_TEMPLATE: &str =
    include_str!("../../../assets/cfgs/optimization.cfg.template");
pub const AUTOEXEC_LINE: &str = "exec optimization.cfg";

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
         //   exec audio_stable   - restore Natural EQ and automatic audio\n\
         //   exec audio_eq_crisp - optional mid/high emphasis\n\
         //   exec audio_eq_smooth - optional reduced mid/high energy\n\
         //   exec audio_legacy_lr_isolation - legacy hard L/R trial\n\
         //   exec audio_lowlatency_025 - manual larger-buffer compatibility test\n\
         //   exec audio_lowlatency_001 - compatibility alias for current defaults\n\n"
    );
    output.push_str(OPTIMIZATION_TEMPLATE);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[must_use]
pub fn ensure_autoexec_line(existing: &str) -> String {
    if existing.lines().any(is_optimization_exec) {
        return existing.to_owned();
    }
    if existing.is_empty() {
        return format!(
            "// Optional frametime.cfg bootstrap. Keep personal CVars in this file.\n// Add a temporary sentinel to prove CS2 executed the line; this does not guarantee precedence.\n\n{AUTOEXEC_LINE}\n"
        );
    }
    let has_blank_final_line = existing.ends_with("\n\n")
        || existing.ends_with("\r\n\r\n")
        || existing
            .trim_end_matches(['\r', '\n'])
            .rsplit(['\r', '\n'])
            .next()
            .is_some_and(|line| line.trim().is_empty());
    let mut output = existing.to_owned();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    if !has_blank_final_line {
        output.push('\n');
    }
    output.push_str(AUTOEXEC_LINE);
    output.push('\n');
    output
}

fn is_optimization_exec(line: &str) -> bool {
    line.split_once("//")
        .map_or(line, |(command, _)| command)
        .trim()
        .eq_ignore_ascii_case(AUTOEXEC_LINE)
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
    fn autoexec_bootstrap_is_idempotent_and_preserves_content() {
        let once = ensure_autoexec_line("bind x y\n");
        let twice = ensure_autoexec_line(&once);
        assert_eq!(once, twice);
        assert!(once.starts_with("bind x y"));
        assert!(once.lines().any(|line| line == AUTOEXEC_LINE));
    }

    #[test]
    fn autoexec_matches_legacy_comment_blank_line_and_new_file_contract() {
        let commented = "bind x y\nEXEC optimization.cfg // suite\n";
        assert_eq!(ensure_autoexec_line(commented), commented);
        assert_eq!(
            ensure_autoexec_line("bind x y"),
            "bind x y\n\nexec optimization.cfg\n"
        );
        assert_eq!(
            ensure_autoexec_line(""),
            "// Optional frametime.cfg bootstrap. Keep personal CVars in this file.\n// Add a temporary sentinel to prove CS2 executed the line; this does not guarantee precedence.\n\nexec optimization.cfg\n"
        );
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
