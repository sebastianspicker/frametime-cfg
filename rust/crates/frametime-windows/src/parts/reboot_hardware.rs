/// Returns whether this build can execute native Windows operations.
#[must_use]
pub fn platform_is_supported() -> bool {
    platform::is_supported()
}
#[must_use]
pub const fn platform_is_windows_target() -> bool {
    cfg!(windows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Normal,
    SafeMode,
}

/// Read-only evidence for a reboot handoff prerequisite.  `Unavailable` is
/// deliberate: orchestration must not infer an armed or absent state from an
/// incomplete registry/BCD/runtime observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffEvidence {
    Verified,
    Absent,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafebootEvidence {
    Configured(String),
    Absent,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootModeEvidence {
    Normal,
    SafeMode,
    Unavailable,
}

/// Evidence consumed by phase orchestration.  This API never modifies BCD,
/// Run/RunOnce, runtime selectors, state, progress, or backup data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebootHandoffState {
    pub boot_mode: BootModeEvidence,
    pub safeboot: SafebootEvidence,
    pub phase2_runonce_armed: HandoffEvidence,
    pub phase3_run_armed: HandoffEvidence,
    pub phase3_handoff_same_user: HandoffEvidence,
    pub selected_runtime_binding: HandoffEvidence,
    pub token_user_sid: Option<String>,
}

/// Read the native Safe Mode indicator without changing BCD, registry, or
/// phase state. `SM_CLEANBOOT` is zero only during a normal Windows boot.
pub fn current_boot_mode() -> Result<BootMode, String> {
    boot_mode::current()
}

/// Inspect the fixed BCD and registry identities used by reboot phases and
/// bind the current process to the selected handle-walked runtime generation.
pub fn inspect_reboot_handoff_state(work_dir: &Path) -> Result<RebootHandoffState, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let selected_runtime_binding = match inspect_selected_runtime_integrity(&trusted) {
        Ok(_) => HandoffEvidence::Verified,
        Err(_) => HandoffEvidence::Unavailable,
    };
    let boot_mode = match current_boot_mode() {
        Ok(BootMode::Normal) => BootModeEvidence::Normal,
        Ok(BootMode::SafeMode) => BootModeEvidence::SafeMode,
        Err(_) => BootModeEvidence::Unavailable,
    };
    let safeboot = safeboot_evidence();
    let phase2 = run_value_evidence(
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
        PHASE2_HANDOFF,
    );
    let phase3 = run_value_evidence(
        Hive::CurrentUser,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
        PHASE3_HANDOFF,
    );
    let token_user_sid = current_token_user_sid().ok();
    let initiator_user_sid = read_reboot_initiator_sid(&trusted);
    let phase3_handoff_same_user = same_user_handoff_evidence(
        phase3.clone(),
        token_user_sid.as_deref(),
        initiator_user_sid.as_deref(),
    );
    Ok(RebootHandoffState {
        boot_mode,
        safeboot,
        phase2_runonce_armed: phase2,
        phase3_run_armed: phase3,
        phase3_handoff_same_user,
        selected_runtime_binding,
        token_user_sid,
    })
}

fn read_reboot_initiator_sid(trusted: &TrustedWorkDir) -> Option<String> {
    let state: State = read_json_trusted(trusted, STATE_FILE).ok()?;
    state
        .active_reboot_transaction
        .and_then(|transaction| transaction.initiator_user_sid)
}

fn same_user_handoff_evidence(
    phase3: HandoffEvidence,
    token_user_sid: Option<&str>,
    initiator_user_sid: Option<&str>,
) -> HandoffEvidence {
    match phase3 {
        HandoffEvidence::Absent => HandoffEvidence::Absent,
        HandoffEvidence::Unavailable => HandoffEvidence::Unavailable,
        HandoffEvidence::Verified => match (token_user_sid, initiator_user_sid) {
            (Some(current), Some(initiator))
                if valid_canonical_sid(current)
                    && valid_canonical_sid(initiator)
                    && current == initiator =>
            {
                HandoffEvidence::Verified
            }
            (Some(current), Some(initiator))
                if valid_canonical_sid(current) && valid_canonical_sid(initiator) =>
            {
                HandoffEvidence::Absent
            }
            _ => HandoffEvidence::Unavailable,
        },
    }
}

fn valid_canonical_sid(value: &str) -> bool {
    let mut parts = value.split('-');
    if parts.next() != Some("S") || parts.next() != Some("1") {
        return false;
    }
    let Some(authority) = parts.next() else {
        return false;
    };
    if !canonical_decimal(authority) || authority.parse::<u64>().is_err() {
        return false;
    }
    let subauthorities = parts.collect::<Vec<_>>();
    !subauthorities.is_empty()
        && subauthorities.len() <= 15
        && subauthorities.iter().all(|part| {
            canonical_decimal(part)
                && part
                    .parse::<u64>()
                    .is_ok_and(|number| u32::try_from(number).is_ok())
        })
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn safeboot_evidence() -> SafebootEvidence {
    let result = CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}"])
        .and_then(|command| command.run());
    let Ok(text) = result else {
        return SafebootEvidence::Unavailable;
    };
    safeboot_evidence_from_bcd(&text)
}

fn safeboot_evidence_from_bcd(text: &str) -> SafebootEvidence {
    match text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("safeboot")
            .map(|value| value.trim().to_owned())
    }) {
        Some(value) if matches!(value.as_str(), "minimal" | "network" | "dsrepair") => {
            SafebootEvidence::Configured(value)
        }
        Some(_) => SafebootEvidence::Unavailable,
        None => SafebootEvidence::Absent,
    }
}

fn run_value_evidence(hive: Hive, key: &'static str, name: &'static str) -> HandoffEvidence {
    let change = RegistryChange {
        hive,
        key,
        name,
        value: RegValue::String(""),
    };
    match registry_read(&change) {
        Ok(Some(RegValue::String(value))) if !value.is_empty() => HandoffEvidence::Verified,
        Ok(None) => HandoffEvidence::Absent,
        _ => HandoffEvidence::Unavailable,
    }
}
