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
pub fn render_optimization_cfg() -> String {
    let timestamp = crate::logging::legacy_timestamp_now();
    render_optimization_cfg_at(&timestamp[..16])
}

#[must_use]
pub fn render_optimization_cfg_at(timestamp: &str) -> String {
    let mut output = format!(
        "// frametime.cfg - optimization.cfg\n\
         // Generated: {timestamp}\n\
         // exec'd from the end of autoexec.cfg - overrides earlier same-named CVars.\n\
         // To revert one setting: remove or comment its line here.\n\
         // To revert all:         remove 'exec optimization.cfg' from autoexec.cfg.\n\
         //\n\
         // Optional standalone CFGs (also in game\\csgo\\cfg\\, use from console as needed):\n\
         //   exec net_stable     - baseline / reset (stable wired/fiber)\n\
         //   exec net_highping   - 60ms+ ping, stable route\n\
         //   exec net_unstable   - jitter + loss, ping OK (Wi-Fi / 4G)\n\
         //   exec net_bad        - high ping + jitter/loss (satellite / mobile)\n\
         //   exec debug_hud      - temporary telemetry and network diagnostics\n\
         //   exec debug_hud_off  - reset diagnostic telemetry to quiet defaults\n\
         //   exec audio_stable   - suite audio buffer default / reset\n\
         //   exec audio_lowlatency_025 - experimental lower audio buffer\n\
         //   exec audio_lowlatency_001 - aggressive lower audio buffer\n\n"
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
            "// Your CS2 autoexec - add personal CVars above the exec line.\n\n{AUTOEXEC_LINE}\n"
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
    fn current_legacy_source_has_73_generated_assignments() {
        assert_eq!(optimization_values().len(), 73);
        assert_eq!(optimization_values()["fps_max"], "0");
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
            "// Your CS2 autoexec - add personal CVars above the exec line.\n\nexec optimization.cfg\n"
        );
    }

    #[test]
    fn generated_header_matches_legacy_shape_and_order() {
        let output = render_optimization_cfg_at("2026-08-10 12:34");
        assert!(
            output.starts_with(
                "// frametime.cfg - optimization.cfg\n// Generated: 2026-08-10 12:34\n"
            )
        );
        assert!(output.contains("\ncl_interp_ratio 1\ncl_interp 0\n"));
        assert!(output.ends_with("mat_monitorgamma_tv_enabled 0\n"));
    }
}
