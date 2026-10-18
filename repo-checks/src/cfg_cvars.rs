//! CS2 console-name validation for the shipped cfg assets.
//!
//! Valve renames and removes CVars between builds (the September 2026
//! crosshair rework did both). The committed snapshot lists every name a player
//! cfg may use in the pinned build, so a stale asset line fails here instead of
//! being silently ignored by the game.

use std::collections::BTreeSet;

/// The lower-cased first token of every command line in `source`, with `//`
/// comments and blank lines skipped.
pub fn cfg_command_names(source: &str) -> Vec<String> {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The names in a snapshot file: one per line, `#` comments skipped.
pub fn snapshot_names(snapshot: &str) -> BTreeSet<String> {
    snapshot
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The quoted `"setting.*"` keys in a KeyValues-style video settings file,
/// lower-cased, with `//` comments skipped.
pub fn video_setting_keys(source: &str) -> Vec<String> {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default().trim())
        .filter_map(|line| line.strip_prefix('"'))
        .filter_map(|line| line.split('"').next())
        .filter(|key| key.starts_with("setting."))
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{cfg_command_names, snapshot_names, video_setting_keys};
    use crate::{read_utf8, repository_root};

    /// Every shipped CS2 cfg asset under `assets/cfgs`, sorted.
    fn cfg_assets() -> Vec<PathBuf> {
        let mut assets = fs::read_dir(repository_root().join("assets/cfgs"))
            .expect("read assets/cfgs")
            .map(|entry| entry.expect("read assets/cfgs entry").path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.ends_with(".cfg")
                                || name.ends_with(".cfg.example")
                                || name.ends_with(".cfg.template")
                        })
            })
            .collect::<Vec<_>>();
        assets.sort_unstable();
        assets
    }

    #[test]
    fn command_names_skip_comments_and_keep_first_tokens() {
        let source =
            "// header\nfps_max \"400\" // cap\n\n  Echo \"x // y\"\ncl_ticktiming print detail\n";
        assert_eq!(
            cfg_command_names(source),
            ["fps_max", "echo", "cl_ticktiming"]
        );
    }

    #[test]
    fn shipped_cfg_assets_use_only_current_player_settable_names() {
        let root = repository_root();
        let allowed = snapshot_names(&read_utf8(
            &root.join("repo-checks/data/cs2-console-names.txt"),
        ));
        assert!(
            allowed.len() > 1000,
            "console-name snapshot looks truncated"
        );
        let assets = cfg_assets();
        for path in &assets {
            let name = path.display();
            let unknown = cfg_command_names(&read_utf8(path))
                .into_iter()
                .filter(|command| !allowed.contains(command))
                .collect::<Vec<_>>();
            assert!(
                unknown.is_empty(),
                "{name} uses names that are unregistered, hidden, cheat, or development-only in the pinned CS2 build: {unknown:?}"
            );
        }
        assert!(
            assets.len() > 10,
            "expected the cfg asset set under assets/cfgs"
        );
    }

    #[test]
    fn video_settings_reference_uses_only_shipped_video_keys() {
        let root = repository_root();
        let allowed = snapshot_names(&read_utf8(
            &root.join("repo-checks/data/cs2-video-settings.txt"),
        ));
        let keys = video_setting_keys(&read_utf8(&root.join("assets/videosettings.txt")));
        assert!(
            keys.len() >= 10,
            "videosettings.txt lost its keys: {keys:?}"
        );
        let unknown = keys
            .iter()
            .filter(|key| !allowed.contains(*key))
            .collect::<Vec<_>>();
        assert!(
            unknown.is_empty(),
            "videosettings.txt uses keys absent from the pinned CS2 binaries: {unknown:?}"
        );
    }

    #[test]
    fn shipped_cfg_assets_leave_the_crosshair_to_the_crosshair_project() {
        // Crosshair CVars belong to small-indie-crosshair-company; setting them
        // here would override a converted crosshair on every exec.
        for path in cfg_assets() {
            let crosshair = cfg_command_names(&read_utf8(&path))
                .into_iter()
                .filter(|command| {
                    command.starts_with("cl_crosshair")
                        || command.starts_with("cl_grenadecrosshair")
                        || command == "cl_show_observer_crosshair"
                        || command == "cl_observed_bot_crosshair"
                })
                .collect::<Vec<_>>();
            assert!(
                crosshair.is_empty(),
                "{} sets crosshair CVars: {crosshair:?}",
                path.display()
            );
        }
    }
}
