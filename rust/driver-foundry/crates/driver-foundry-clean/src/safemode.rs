//! Safe Mode prepare/clear hooks.
//!
//! **Safety:** never reboots or shuts down the host. Power actions are journal-only.
//! BCD mutation is unavailable until an authenticated, private capability exists.

use driver_foundry_common::ActionJournal;

#[derive(Debug, Clone)]
pub(crate) struct SafeModeResult {
    pub messages: Vec<String>,
}

/// Plan or apply Safe Mode boot configuration.
/// BCD mutation is deliberately refused for live requests. Never schedules a host reboot.
pub(crate) fn prepare_safeboot(
    network: bool,
    live: bool,
    journal: &mut ActionJournal,
) -> SafeModeResult {
    let mut messages = Vec::new();
    let mode = if network { "network" } else { "minimal" };
    messages.push(format!(
        "safemode: prepare safeboot mode={mode} live={live}"
    ));

    journal.plan(
        "SafeMode",
        "bcdedit_safeboot",
        format!("{{current}} safeboot {mode}"),
    );
    // Journal intent only — never auto-restart
    journal.plan_detail(
        "SafeMode",
        "schedule_restart",
        "prepare-safeboot",
        "journal-only: host restart never auto-issued",
    );

    if live {
        messages.push(
            "safemode: live BCD mutation refused until a private authenticated capability is shipped; restart never issued"
                .into(),
        );
    } else {
        messages.push("safemode: dry-run — would set bcdedit safeboot (no auto-restart)".into());
    }

    SafeModeResult { messages }
}

/// Clear Safe Mode BCD flag.
/// Live BCD mutation is deliberately unavailable.
pub(crate) fn clear_safeboot(live: bool, journal: &mut ActionJournal) -> SafeModeResult {
    let mut messages = Vec::new();
    messages.push(format!("safemode: clear safeboot live={live}"));
    journal.plan("SafeMode", "bcdedit_delete_safeboot", "{current}");

    if live {
        messages.push(
            "safemode: live BCD mutation refused until a private authenticated capability is shipped"
                .into(),
        );
    } else {
        messages.push("safemode: dry-run — would delete bcdedit safeboot value".into());
    }

    SafeModeResult { messages }
}

/// Request restart / shutdown — **journal only**.
///
/// Never invokes `shutdown.exe`. Host reboot/shutdown is never performed by this process.
pub(crate) fn request_power(
    restart: bool,
    shutdown: bool,
    live: bool,
    journal: &mut ActionJournal,
) {
    let _ = live;
    if restart {
        journal.plan_detail(
            "Power",
            "restart",
            "post-clean",
            "journal-only: never auto-restarts the host",
        );
    }
    if shutdown {
        journal.plan_detail(
            "Power",
            "shutdown",
            "post-clean",
            "journal-only: never auto-shuts-down the host",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_dry_run_plans() {
        let mut j = ActionJournal::default();
        let r = prepare_safeboot(false, false, &mut j);
        assert!(j.count_planned() >= 2);
        assert_eq!(j.count_executed(), 0);
        assert!(r.messages.iter().any(|m| m.contains("dry-run")));
    }

    #[test]
    fn clear_dry_run_plans() {
        let mut j = ActionJournal::default();
        clear_safeboot(false, &mut j);
        assert!(j.count_planned() >= 1);
        assert_eq!(j.count_executed(), 0);
    }

    #[test]
    fn request_power_never_executes() {
        let mut j = ActionJournal::default();
        // Even with live=true, must only journal
        request_power(true, true, true, &mut j);
        assert_eq!(j.count_executed(), 0);
        assert!(j.count_planned() >= 2);
        assert!(j.entries.iter().all(|e| e.detail.contains("journal-only")));
    }

    #[test]
    fn source_has_no_live_system_tool_or_env_bypass() {
        let src = include_str!("safemode.rs");
        let process_constructor = ["Command", "::new"].concat();
        assert!(
            !src.contains(&process_constructor),
            "safemode must never invoke a system tool"
        );
        let legacy_bypass = ["DFOUNDRY_ALLOW", "_BCDEDIT"].concat();
        assert!(!src.contains(&legacy_bypass));
    }
}
