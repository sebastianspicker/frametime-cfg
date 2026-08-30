//! Prepared-package tweak generation and optional deep INF surgery.

use std::fs;
use std::path::{Path, PathBuf};

use crate::pipeline::note;
use crate::{InstallError, InstallOptions};

pub(crate) fn apply_tweaks(
    prepared: &Path,
    opts: &InstallOptions,
    log: &mut Vec<String>,
) -> Result<(), InstallError> {
    if opts.live_registry_apply {
        return Err(InstallError::UntrustedInstaller(
            "live_registry_apply is disabled until installer signer authentication exists".into(),
        ));
    }
    let tweaks_dir = prepared.join("driver-foundry-post-install");
    fs::create_dir_all(&tweaks_dir)?;
    let tweaks_json = serde_json::json!({
        "clean_install": opts.clean_install,
        "unattended": opts.unattended,
        "disable_driver_telemetry": opts.disable_telemetry,
        "disable_installer_telemetry": opts.disable_installer_telemetry,
        "disable_nvcontainer": opts.disable_nvcontainer,
        "disable_nvcamera": opts.disable_nvcamera,
        "disable_hdcp": opts.disable_hdcp,
        "disable_mpo": opts.disable_mpo,
        "disable_hdaudio_sleep": opts.disable_hdaudio_sleep,
        "enable_msi": opts.enable_msi,
        "deep_inf": opts.deep_inf,
        "live_registry_apply": opts.live_registry_apply,
        "preset": opts.preset.to_ascii_lowercase(),
    });
    fs::write(
        prepared.join("driver-foundry-tweaks.json"),
        serde_json::to_string_pretty(&tweaks_json)?,
    )?;
    // Post-install registry markers (file always; live apply optional)
    let mut reg = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    if opts.disable_telemetry || opts.disable_installer_telemetry {
        reg.push_str(
            r#"[HKEY_LOCAL_MACHINE\SOFTWARE\NVIDIA Corporation\Global\Telemetry]
"Enable"=dword:00000000
"#,
        );
    }
    if opts.enable_msi {
        reg.push_str(
            r#"[HKEY_LOCAL_MACHINE\SOFTWARE\DriverFoundry]
"MsiModeRequested"=dword:00000001
"#,
        );
    }
    if opts.disable_nvcamera {
        reg.push_str(
            r#"[HKEY_LOCAL_MACHINE\SOFTWARE\NVIDIA Corporation\Global\NVTweak]
"NvCameraEnable"=dword:00000000
"#,
        );
    }
    if opts.disable_hdcp {
        reg.push_str(
            r#"[HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\nvlddmkm\Parameters]
"RMHdcpKeyglobZero"=dword:00000001
"#,
        );
    }
    if opts.disable_mpo {
        reg.push_str(
            r#"[HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\Dwm]
"OverlayTestMode"=dword:00000005
"#,
        );
    }
    fs::write(tweaks_dir.join("post-install-markers.reg"), &reg)?;

    let nvi2 = prepared.join("NVI2");
    fs::create_dir_all(&nvi2)?;
    let mut cfg = String::new();
    if opts.clean_install {
        cfg.push_str("clean=1\n");
    }
    if opts.unattended {
        cfg.push_str("unattended=1\n");
    }
    if opts.disable_telemetry || opts.disable_installer_telemetry {
        cfg.push_str("telemetry=0\n");
    }
    fs::write(nvi2.join("setup.cfg"), cfg)?;
    if opts.deep_inf {
        apply_deep_inf_markers(prepared, log)?;
    }
    note(
        log,
        "S3-Tweaks",
        "Tweaks applied (prepared tree): clean-install, telemetry, nvcamera/hdcp/mpo/msi markers; setupArgs silent",
    );
    Ok(())
}

/// Substrings (case-insensitive) whose non-comment INF lines get deep-stripped.
const DEEP_INF_STRIP_NEEDLES: &[&str] = &[
    "telemetry",
    "nvtelemetry",
    "gfexperience",
    "nvcontainer",
    "appx",
    "nvapp",
];

fn apply_deep_inf_markers(prepared: &Path, log: &mut Vec<String>) -> Result<(), InstallError> {
    fs::write(
        prepared.join("driver-foundry-deep-inf.flag"),
        "deep-inf enabled: text-level INF surgery on Display.Driver/*.inf\n",
    )?;

    let infs = collect_display_driver_infs(prepared);
    let mut edited = 0usize;
    for inf_path in &infs {
        let marker = inf_path.with_extension("inf.driver-foundry-deep");
        fs::write(
            &marker,
            format!(
                "deep-inf target: {}\ntext-level strip of Telemetry/GFExperience/NvContainer/Appx/NvApp lines\n",
                inf_path.display()
            ),
        )?;
        let changed = rewrite_inf_deep_strip(inf_path)?;
        edited += 1;
        note(
            log,
            "S3-Tweaks",
            &format!(
                "Deep-INF surgery: {} (lines_stripped={changed})",
                inf_path.display()
            ),
        );
    }

    if infs.is_empty() {
        note(
            log,
            "S3-Tweaks",
            "Deep-INF option path: flag written; zero INFs edited (no Display.Driver/*.inf)",
        );
    } else {
        note(
            log,
            "S3-Tweaks",
            &format!(
                "Deep-INF option path: {edited}/{} INF file(s) rewritten under Display.Driver*",
                infs.len()
            ),
        );
    }
    Ok(())
}

/// Collect `*.inf` under prepared/Display.Driver and other top-level Display.* dirs.
pub(crate) fn collect_display_driver_infs(prepared: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = Vec::new();
    let primary = prepared.join("Display.Driver");
    if primary.is_dir() {
        dirs.push(primary);
    }
    if let Ok(rd) = fs::read_dir(prepared) {
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("Display.Driver") {
                continue;
            }
            if is_display_driver_like_dir(&name) {
                dirs.push(p);
            }
        }
    }
    for dir in dirs {
        if let Ok(rd) = fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("inf"))
                        .unwrap_or(false)
                {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

fn is_display_driver_like_dir(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "display.driver" || n.starts_with("display.")
}

/// Rewrite INF in place: comment out telemetry/GFE/container/appx lines; append marker comment.
/// Returns number of lines that received the strip prefix.
pub(crate) fn rewrite_inf_deep_strip(inf_path: &Path) -> Result<usize, InstallError> {
    let original = fs::read_to_string(inf_path)?;
    let mut stripped = 0usize;
    let mut out_lines: Vec<String> = Vec::with_capacity(original.lines().count() + 2);
    for line in original.lines() {
        let trimmed = line.trim_start();
        // Already-commented lines stay as-is (valid-ish INF).
        if trimmed.starts_with(';') {
            out_lines.push(line.to_string());
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let hit = DEEP_INF_STRIP_NEEDLES.iter().any(|n| lower.contains(n));
        if hit {
            out_lines.push(format!("; driver-foundry-deep-strip {line}"));
            stripped += 1;
        } else {
            out_lines.push(line.to_string());
        }
    }
    // Trailing applied marker (once).
    let joined = out_lines.join("\n");
    let mut body = if joined.ends_with('\n') || joined.is_empty() {
        joined
    } else {
        format!("{joined}\n")
    };
    if !body.contains("driver-foundry deep-inf applied") {
        body.push_str("; driver-foundry deep-inf applied\n");
    }
    fs::write(inf_path, body)?;
    Ok(stripped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn deep_inf_marker_write_failure_stops_the_stage() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dfoundry-deep-inf-failure-{unique}"));
        fs::create_dir_all(root.join("driver-foundry-deep-inf.flag")).unwrap();

        let mut log = Vec::new();
        assert!(apply_deep_inf_markers(&root, &mut log).is_err());
        assert!(
            log.is_empty(),
            "failed required marker write must stop before success logs"
        );

        let _ = fs::remove_dir_all(root);
    }
}
