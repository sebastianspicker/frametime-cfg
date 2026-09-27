#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Overview,
    Assess,
    SetupVerify,
    Benchmark,
    Network,
    Video,
    Cs2Cfg,
    Drivers,
    Recovery,
}

impl Area {
    pub const ALL: [Self; 9] = [
        Self::Overview,
        Self::Assess,
        Self::SetupVerify,
        Self::Benchmark,
        Self::Network,
        Self::Video,
        Self::Cs2Cfg,
        Self::Drivers,
        Self::Recovery,
    ];
    pub const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Assess => "Assess",
            Self::SetupVerify => "Setup / Verify",
            Self::Benchmark => "Benchmark",
            Self::Network => "Network",
            Self::Video => "Video",
            Self::Cs2Cfg => "CS2 CFG",
            Self::Drivers => "Drivers",
            Self::Recovery => "Recovery",
        }
    }
    pub const fn action(self) -> Action {
        match self {
            Self::Overview => Action::Refresh,
            Self::Assess => Action::HardwareDoctor,
            Self::SetupVerify => Action::PhaseChoice,
            Self::Benchmark => Action::CalculateFpsCap,
            Self::Network => Action::NetworkApply,
            Self::Video => Action::VideoRefresh,
            Self::Cs2Cfg => Action::Cs2CfgApply,
            Self::Drivers => Action::DriverInspect,
            Self::Recovery => Action::ExportBackup,
        }
    }
    pub const fn action_label(self) -> &'static str {
        match self.action() {
            Action::Refresh => "Refresh work directory",
            Action::HardwareDoctor => "Hardware doctor",
            Action::CalculateFpsCap => "Calculate FPS cap",
            Action::NetworkApply => "Enable Ethernet RSS",
            Action::PhaseChoice => "Configure selected profile",
            Action::VideoRefresh => "Refresh video preview",
            Action::Cs2CfgApply => "Install selected CS2 CFG",
            Action::DriverInspect => "Inspect driver lifecycle",
            Action::ExportBackup => "Export backup",
        }
    }
    pub const fn description(self) -> &'static str {
        match self {
            Self::Overview => {
                "Read the native runtime state and choose a task area. No PowerShell is used."
            }
            Self::Assess => {
                "Run versioned, read-only native hardware diagnostics. Results remain visible here and never advance workflow progress."
            }
            Self::SetupVerify => {
                "Use the authenticated package to configure a profile, start or resume Phase 1, or verify the protected workflow state."
            }
            Self::Benchmark => {
                "Paste at least five complete VProf Avg/P1 runs, then evaluate an explicit raw cap. No run count is inferred."
            }
            Self::Network => {
                "Enable only the supported Ethernet RSS master state in-process. Queue, processor, offload, QoS, and vendor-specific values remain unchanged."
            }
            Self::Video => {
                "Discover one trusted CS2 video document and present read-only UI-level guidance. The GUI never changes CS2 video files."
            }
            Self::Cs2Cfg => {
                "Select one packaged optional CS2 CFG. The authenticated native CLI installs only that fixed asset after administrator approval."
            }
            Self::Drivers => {
                "Inspect the consolidated NVIDIA inventory, package preparation, transaction, profile backup, reconciliation, verification, and recovery state. Safe Mode execution remains CLI-only."
            }
            Self::Recovery => {
                "Inspect retained recovery records, export a byte-verified backup, or run a confirmed native restore."
            }
        }
    }
    pub const fn table_rows(self) -> &'static [(&'static str, &'static str, &'static str)] {
        match self {
            Self::Overview => &[
                ("Runtime", "C:\\FRAMETIME_CFG", "Read-only inspection"),
                ("Safety", "Normal Windows session", "Safe Mode prohibited"),
                ("Terminal", "frametime.exe", "Native CLI only"),
            ],
            Self::Assess => &[
                (
                    "Hardware diagnostics",
                    "Read only",
                    "Versioned typed native results",
                ),
                ("Unavailable adapters", "Fail closed", "No shell fallback"),
                ("Result", "Partial failures shown", "No workflow progress"),
            ],
            Self::SetupVerify => &[
                (
                    "Configuration",
                    "Authenticated package",
                    "Writes the selected profile and dry-run mode",
                ),
                (
                    "Phase 1",
                    "Native CLI",
                    "Publishes and arms the protected reboot handoff",
                ),
                ("Verification", "Read only", "Reports exact workflow state"),
                ("Phase 2", "Safe Mode", "GUI intentionally prohibited"),
            ],
            Self::Benchmark => &[
                ("Input", "VProf Avg/P1 lines", "At least five parsed runs"),
                (
                    "Raw strategy",
                    "fps_max 0 or measured cap",
                    "No percentage formula",
                ),
                (
                    "Persistence",
                    "Authenticated CLI",
                    "Adds a validated VProf capture to history",
                ),
            ],
            Self::Network => &[
                (
                    "Ethernet RSS baseline",
                    "P1:16",
                    "Explicit confirmation required",
                ),
                (
                    "Execution",
                    "In process",
                    "Authenticated GUI elevation; no CLI fallback",
                ),
                ("Result", "Engine report", "Counts and final event shown"),
            ],
            Self::Video => &[
                (
                    "Steam discovery",
                    "Read only",
                    "One numeric userdata/cs2_video.txt only",
                ),
                (
                    "Display goal",
                    "Manual UI guidance",
                    "Raw, NVIDIA VRR, AMD FreeSync, or GPU-constrained",
                ),
                ("Apply", "Disabled", "No video-file mutation surface"),
            ],
            Self::Cs2Cfg => &[
                (
                    "Asset selection",
                    "One fixed packaged CFG",
                    "Closed list; arbitrary paths are not accepted",
                ),
                (
                    "Execution",
                    "Authenticated native CLI",
                    "Administrator approval is required",
                ),
                (
                    "Result",
                    "Verified deployment",
                    "Recovery evidence is retained",
                ),
            ],
            Self::Drivers => &DRIVER_ROWS,
            Self::Recovery => &[
                ("Backup", "backup.json", "Native recovery grid"),
                (
                    "Restore",
                    "Confirmed native API",
                    "Partial records are retained",
                ),
                (
                    "Export",
                    "Byte verified",
                    "Choose destination with native dialog",
                ),
            ],
        }
    }
}

const DRIVER_ROWS: [(&str, &str, &str); 8] = [
    (
        "Inventory",
        "Read only",
        "Exact GPU, package, and component status",
    ),
    (
        "Source / signature",
        "Fail closed",
        "Fixed NVIDIA HTTPS authority or copied local installer",
    ),
    (
        "Package",
        "Six presets",
        "35 typed components; unknown directories retained",
    ),
    (
        "Transaction",
        "Version 2",
        "Ten durable, resumable lifecycle stages",
    ),
    (
        "Profile backup",
        "Digest bound",
        "All customized public DRS records",
    ),
    (
        "Reconciliation",
        "Explicit acceptance",
        "Rejected records remain auditable",
    ),
    (
        "Verification",
        "Identity bound",
        "Package, service, policy, DRS, and recovery evidence",
    ),
    ("Safe Mode", "CLI only", "No graphical mutation surface"),
];

pub const PROFILE_PREFERENCES: [&str; 5] = ["safe", "recommended", "competitive", "custom", "yolo"];
#[cfg(test)]
pub const SETUP_PHASE_ONE_ARGUMENTS: [&str; 2] = ["optimize", "--yes"];
#[cfg(test)]
pub const SETUP_VERIFY_ARGUMENTS: [&str; 1] = ["verify"];
#[cfg(test)]
pub const fn cs2_cfg_arguments(
    asset: frametime_domain::cs2_config::OptionalCfgAsset,
) -> [&'static str; 4] {
    ["cs2-cfg", "--asset", asset.cli_token(), "--yes"]
}
/// Phase 2 is a Safe Mode CLI handoff; the GUI never runs it from a Safe Mode boot.
pub const fn gui_allows_phase_2_in_safe_mode() -> bool {
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Refresh,
    HardwareDoctor,
    CalculateFpsCap,
    NetworkApply,
    PhaseChoice,
    VideoRefresh,
    Cs2CfgApply,
    DriverInspect,
    ExportBackup,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destinations_and_routes_are_stable() {
        assert_eq!(Area::ALL.len(), 9);
        assert_eq!(Area::ALL[2].title(), "Setup / Verify");
        assert_eq!(Area::ALL[7].title(), "Drivers");
        assert_eq!(Area::ALL[8].title(), "Recovery");
        assert!(matches!(Area::Assess.action(), Action::HardwareDoctor));
        assert_eq!(Area::Assess.action_label(), "Hardware doctor");
        assert!(Area::Assess.description().contains("workflow progress"));
        assert!(matches!(Area::Network.action(), Action::NetworkApply));
        assert_eq!(Area::Network.action_label(), "Enable Ethernet RSS");
        assert!(Area::Network.table_rows()[1].2.contains("no CLI"));
    }
    #[test]
    fn setup_and_cs2_cfg_contracts_are_fixed() {
        use frametime_domain::cs2_config::OptionalCfgAsset;
        assert!(matches!(Area::SetupVerify.action(), Action::PhaseChoice));
        assert_eq!(SETUP_PHASE_ONE_ARGUMENTS, ["optimize", "--yes"]);
        assert_eq!(SETUP_VERIFY_ARGUMENTS, ["verify"]);
        assert_eq!(OptionalCfgAsset::ALL.len(), 15);
        assert_eq!(
            OptionalCfgAsset::NetStable.display_label(),
            "Network stable"
        );
        assert_eq!(
            cs2_cfg_arguments(OptionalCfgAsset::AudioEqCrisp),
            ["cs2-cfg", "--asset", "audio-eq-crisp", "--yes"]
        );
        assert!(matches!(Area::Cs2Cfg.action(), Action::Cs2CfgApply));
        assert!(Area::Cs2Cfg.table_rows()[0].2.contains("arbitrary paths"));
    }
    #[test]
    fn profiles_and_safe_mode_remain_explicit() {
        assert_eq!(PROFILE_PREFERENCES.len(), 5);
        assert!(
            PROFILE_PREFERENCES
                .iter()
                .all(|profile| super::super::catalog_filter::profile_preference_is_valid(profile))
        );
        assert!(!gui_allows_phase_2_in_safe_mode());
        assert!(
            Area::SetupVerify
                .table_rows()
                .iter()
                .any(|row| row.2.contains("prohibited"))
        );
    }
}
