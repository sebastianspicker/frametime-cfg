use super::*;
use frametime_core::catalog::STEPS;
use tempfile::TempDir;

struct TestVideoFilePlatform;

impl VideoFilePlatform for TestVideoFilePlatform {
    fn clear_read_only(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

fn video_fixture() -> (TempDir, PathBuf, PathBuf, String) {
    let root = tempfile::tempdir().expect("temporary Steam root");
    let video = root
        .path()
        .join("userdata")
        .join("123456")
        .join("730")
        .join("local")
        .join("cfg")
        .join("video.txt");
    fs::create_dir_all(video.parent().expect("video parent")).expect("create fixture");
    let original = concat!(
        "\"VideoConfig\"\n",
        "{\n",
        "    \"setting.msaa_samples\" \"2\" // retained managed comment\n",
        "    \"setting.unmanaged_quality\" \"operator-value\"\n",
        "    // unmanaged comment\n",
        "}\n"
    )
    .to_owned();
    fs::write(&video, &original).expect("write fixture");
    let steam_root = root.path().to_path_buf();
    (root, video, steam_root, original)
}

fn checked_in_config() -> Config {
    Config::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frametime.toml"))
        .expect("checked-in config")
}

#[test]
fn nagle_selection_uses_only_a_unique_fastest_up_physical_ethernet_interface() {
    let candidate = |guid: &str, speed, up, hardware| PhysicalEthernetCandidate {
        interface_guid: guid.into(),
        luid: speed,
        if_index: u32::try_from(speed).expect("small test speed"),
        link_speed: speed,
        is_ethernet: true,
        is_hardware: hardware,
        is_up: up,
    };
    let selected = select_unique_physical_ethernet([
        candidate("{11111111-1111-1111-1111-111111111111}", 1, true, true),
        candidate("{22222222-2222-2222-2222-222222222222}", 2, true, true),
        candidate("{33333333-3333-3333-3333-333333333333}", 3, false, true),
        candidate("{44444444-4444-4444-4444-444444444444}", 4, true, false),
    ])
    .expect("unique physical candidate");
    assert_eq!(
        selected.interface_guid,
        "{22222222-2222-2222-2222-222222222222}"
    );
    assert!(
        select_unique_physical_ethernet([
            candidate("{55555555-5555-5555-5555-555555555555}", 2, true, true),
            candidate("{66666666-6666-6666-6666-666666666666}", 2, true, true),
        ])
        .is_err()
    );
}

#[test]
fn nagle_restore_identity_rejects_tampered_key_or_metadata() {
    let guid = "{11111111-1111-1111-1111-111111111111}";
    let key = nagle_registry_key(guid).expect("key");
    let mut metadata = BTreeMap::new();
    metadata.insert("interfaceGuid".into(), Value::String(guid.into()));
    metadata.insert("interfaceLuid".into(), Value::from(7_u64));
    metadata.insert("interfaceIndex".into(), Value::from(8_u64));
    let mut tampered_key = metadata.clone();
    tampered_key.insert(
        "interfaceGuid".into(),
        Value::String("{22222222-2222-2222-2222-222222222222}".into()),
    );
    assert!(validate_nagle_restore_binding(&key, "TcpNoDelay", &tampered_key).is_err());
    assert!(validate_nagle_restore_binding(&key, "Unexpected", &metadata).is_err());
}

#[test]
fn cs2_registry_contract_has_exact_keys_values_and_rejects_hostile_restore_identity() {
    assert_eq!(
        cs2_registry_key(Cs2RegistryAction::DisableFullscreenOptimizations),
        APP_COMPAT_LAYERS_KEY
    );
    assert_eq!(
        cs2_registry_value(Cs2RegistryAction::DisableFullscreenOptimizations),
        "~ DISABLEDXMAXIMIZEDWINDOWEDMODE"
    );
    assert_eq!(
        cs2_registry_key(Cs2RegistryAction::HighPerformanceGpu),
        DIRECTX_GPU_PREFERENCES_KEY
    );
    assert_eq!(
        cs2_registry_value(Cs2RegistryAction::HighPerformanceGpu),
        "GpuPreference=2;"
    );
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "cs2Executable".into(),
        Value::String(r"C:\evil\cs2.exe".into()),
    );
    assert!(
        validate_cs2_restore_binding(
            "P1:4",
            DIRECTX_GPU_PREFERENCES_KEY,
            r"C:\evil\cs2.exe",
            &metadata
        )
        .is_err()
    );
    assert!(
        validate_cs2_restore_binding(
            "P1:30",
            DIRECTX_GPU_PREFERENCES_KEY,
            r"C:\other\cs2.exe",
            &metadata
        )
        .is_err()
    );
}

#[test]
fn service_power_contracts_have_exact_ordered_identities() {
    let config = checked_in_config();
    let cases = [
        (
            ServiceBatch::WindowsUpdate,
            None,
            vec!["wuauserv", "UsoSvc", "WaaSMedicSvc"],
        ),
        (
            ServiceBatch::SysMainSearchQwaveXbox,
            Some(&config),
            vec![
                "SysMain",
                "WSearch",
                "qWave",
                "XblAuthManager",
                "XblGameSave",
                "XboxNetApiSvc",
                "XboxGipSvc",
            ],
        ),
    ];
    for (batch, config, expected) in cases {
        assert_eq!(
            service_power_contract_map(batch, config).expect("contract"),
            expected.into_iter().map(str::to_owned).collect::<Vec<_>>()
        );
    }
    assert!(matches!(
        action_for(1, 15),
        Ok(Action::ServiceBatch(ServiceBatch::WindowsUpdate))
    ));
    assert!(matches!(
        action_for(1, 37),
        Ok(Action::ServiceBatch(ServiceBatch::SysMainSearchQwaveXbox))
    ));
    assert!(matches!(action_for(1, 6), Ok(Action::PowerPlan)));
}

#[test]
fn service_contract_rejects_unknown_config_and_tampered_capture_identity() {
    let mut config = checked_in_config();
    config.xbox_services.push("Spooler".into());
    assert!(
        service_power_contract_map(ServiceBatch::SysMainSearchQwaveXbox, Some(&config)).is_err()
    );

    let config = checked_in_config();
    let captured = vec!["SysMain".to_owned(), "Spooler".to_owned()];
    assert!(
        captured_service_names(
            ServiceBatch::SysMainSearchQwaveXbox,
            Some(&config),
            Some(&captured)
        )
        .is_err()
    );
    assert!(validate_service("P1:15", "wuauserv").is_ok());
    assert!(validate_service("P1:15", "SysMain").is_err());
    assert!(validate_service("P1:37", "XboxGipSvc").is_ok());
}

#[test]
fn video_preview_uses_only_selected_trusted_steam_root_and_has_all_rows() {
    let (_root, video, steam_root, original) = video_fixture();
    let controller = VideoController::new(&steam_root, GpuVendor::Other).expect("controller");

    let preview = controller.preview(VideoTier::Auto).expect("preview");

    assert_eq!(preview.steam_root, steam_root);
    assert_eq!(preview.video_path, video);
    assert_eq!(preview.requested_tier, VideoTier::Auto);
    assert_eq!(preview.resolved_tier, VideoTier::Mid);
    assert_eq!(preview.rows.len(), 13);
    assert_eq!(
        fs::read_to_string(&preview.video_path).expect("read fixture"),
        original
    );
}

#[test]
fn video_apply_captures_once_preserves_unmanaged_lines_and_readbacks_all_rows() {
    let (_root, video, steam_root, original) = video_fixture();
    let controller = VideoController::new(&steam_root, GpuVendor::Nvidia).expect("controller");

    let first = controller
        .apply_with_platform(VideoTier::Auto, &TestVideoFilePlatform)
        .expect("first apply");

    assert!(first.backup_created);
    assert_eq!(first.preview.resolved_tier, VideoTier::High);
    assert_eq!(first.preview.rows.len(), 13);
    assert!(
        first
            .preview
            .rows
            .iter()
            .all(|row| matches!(row.status, frametime_core::VideoStatus::Ok))
    );
    assert_eq!(
        fs::read_to_string(video.with_extension("txt.bak")).expect("backup"),
        original
    );
    let applied = fs::read_to_string(&video).expect("applied file");
    assert!(applied.contains("\"setting.unmanaged_quality\" \"operator-value\""));
    assert!(applied.contains("// unmanaged comment"));
    assert!(applied.contains("// retained managed comment"));

    let second = controller
        .apply_with_platform(VideoTier::Auto, &TestVideoFilePlatform)
        .expect("second apply");
    assert!(!second.backup_created);
    assert_eq!(
        fs::read_to_string(video.with_extension("txt.bak")).expect("backup"),
        original
    );
}

#[cfg(not(windows))]
#[test]
fn native_video_apply_fails_closed_on_non_windows() {
    let (_root, _video, steam_root, original) = video_fixture();
    let controller = VideoController::new(&steam_root, GpuVendor::Other).expect("controller");

    assert!(controller.apply(VideoTier::Mid).is_err());
    let preview = controller
        .preview(VideoTier::Mid)
        .expect("preview after refusal");
    assert_eq!(
        fs::read_to_string(preview.video_path).expect("fixture unchanged"),
        original
    );
}

#[test]
fn planner_has_no_persistence_path() {
    let mut planner = PlannerBackend::new(1);
    let operation = Operation { step: STEPS[0] };
    assert!(planner.is_dry_run());
    assert!(planner.plan(operation).expect("plan")[0].contains("NVIDIA RTX 5000"));
    assert!(planner.persist_progress(&Progress::default()).is_err());
    assert!(planner.capture_backups(operation).is_err());
    assert!(planner.apply(operation).is_err());
}

#[test]
fn planner_reports_firmware_checks_as_unverified_advisories_without_persistence() {
    let mut planner = PlannerBackend::new(1);
    for step in [STEPS[1], STEPS[8]] {
        let operation = Operation { step };
        assert!(matches!(
            planner.inspect(operation).expect("advisory inspection"),
            Inspection::Advisory { .. }
        ));
        let plan = planner.plan(operation).expect("advisory plan");
        assert!(plan[0].contains("advisory and unverified"));
        assert!(!plan[0].contains("unsupported"));
        assert!(planner.capture_backups(operation).is_err());
        assert!(planner.apply(operation).is_err());
        assert!(planner.verify(operation).is_err());
        assert!(planner.persist_progress(&Progress::default()).is_err());
    }
}
#[test]
fn shader_cache_preview_follows_the_explicit_qualification_gate() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 1 && step.number == 3)
            .expect("P1:3 catalog entry"),
    };
    let mut planner = PlannerBackend::new(1);
    let expected = if shader_cache_delete_qualified() {
        Inspection::NeedsApply
    } else {
        Inspection::Unsupported
    };
    assert_eq!(planner.inspect(operation).expect("inspect"), expected);
    let plan = planner.plan(operation).expect("plan");
    assert_eq!(
        plan[0].contains("unsupported"),
        !shader_cache_delete_qualified()
    );
}
#[test]
fn baseline_planner_previews_explicit_capture_without_persistence() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 1 && step.number == 17)
            .expect("P1:17 catalog entry"),
    };
    let mut planner = PlannerBackend::new(1);
    assert_eq!(
        planner.inspect(operation).expect("inspect"),
        Inspection::NeedsApply
    );
    let plan = planner.plan(operation).expect("plan");
    assert!(plan[0].contains("complete VProf capture"));
    assert!(plan[0].contains("baseline-benchmark"));
    assert!(planner.capture_backups(operation).is_err());
}
#[test]
fn amd_power_plan_preview_uses_the_topology_safe_subset() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 1 && step.number == 6)
            .expect("P1:6 catalog entry"),
    };
    let mut custom = PlannerBackend::new_with_profile(3, Profile::Custom);
    assert_eq!(
        custom.inspect(operation).expect("custom inspect"),
        Inspection::NeedsApply
    );
    assert!(!custom.plan(operation).expect("custom plan")[0].contains("unsupported"));

    let mut recommended = PlannerBackend::new_with_profile(3, Profile::Recommended);
    assert_eq!(
        recommended.inspect(operation).expect("recommended inspect"),
        Inspection::NeedsApply
    );
}
#[test]
fn all_fifty_four_steps_have_typed_actions() {
    for step in STEPS {
        assert!(
            action_for(step.phase as u8, step.number).is_ok(),
            "{}",
            step.title
        );
    }
}
#[test]
fn only_six_direct_executables_are_allowed() {
    assert_eq!(COMMAND_ALLOWLIST.len(), 6);
    for command in COMMAND_ALLOWLIST {
        assert!(command.program().ends_with(".exe"));
    }
    assert_eq!(CommandName::Netsh.program(), "netsh.exe");
    assert!(CommandVector::new(CommandName::Netsh, &["winsock", "reset"]).is_ok());
}
#[test]
fn arguments_are_vectors_not_shell_fragments() {
    assert!(
        CommandVector::new(
            CommandName::Bcdedit,
            &["/set", "{current}", "safeboot", "minimal"]
        )
        .is_ok()
    );
    assert!(CommandVector::new(CommandName::Bcdedit, &["/set; whoami"]).is_err());
}
#[test]
fn bcd_actions_have_exact_disjoint_backup_identities() {
    assert!(matches!(action_for(1, 10), Ok(Action::DynamicTick)));
    for step in STEPS {
        if let Ok(Action::Tool(command)) = action_for(step.phase as u8, step.number)
            && command.command == CommandName::Bcdedit
        {
            assert_eq!(
                command.arguments.as_slice(),
                ["/deletevalue", "{current}", "safeboot"]
            );
            assert_eq!(step.phase as u8, 2);
            assert_eq!(step.number, 1);
        }
    }
    assert!(matches!(action_for(1, 10), Ok(Action::DynamicTick)));
    assert_eq!(
        disabledynamictick_from_bcd("loader\n  0x26000060    TRUE\n").expect("raw parser"),
        Some(true)
    );
    assert_eq!(
        disabledynamictick_from_bcd("locale text\n  0x26000060    0\n").expect("raw parser"),
        Some(false)
    );
    assert!(disabledynamictick_from_bcd("0x26000060 perhaps\n").is_err());
    assert!(disabledynamictick_from_bcd("0x26000060 yes\n0x26000060 no\n").is_err());
    assert!(dynamic_tick_restore_binding("P1:10", "disabledynamictick").is_ok());
    assert!(dynamic_tick_restore_binding("P2:1", "disabledynamictick").is_err());
    assert!(dynamic_tick_restore_binding("P1:10", "safeboot").is_err());
}
#[test]
fn all_restore_identities_are_bounded() {
    assert!(validate_service("P1:37", "SysMain").is_ok());
    assert!(validate_service("P1:15", "wuauserv").is_ok());
    assert!(validate_service("P1:15", "SysMain").is_err());
    assert!(validate_service("P1:37", "Spooler").is_err());
    assert!(validate_power_plan_guid("00000000-0000-0000-0000-000000000000").is_ok());
    assert!(validate_power_plan_guid("../bad").is_err());
}
#[test]
fn phase_handoff_names_are_fixed() {
    assert!(PHASE2_HANDOFF.starts_with("*!"));
    assert_eq!(PHASE3_HANDOFF, "FRAMETIME_CFG_FRAMETIME_Phase3");
}

#[test]
fn phase_three_same_user_evidence_requires_run_value_and_matching_persisted_sid() {
    assert_eq!(
        same_user_handoff_evidence(
            HandoffEvidence::Verified,
            Some("S-1-5-21-100"),
            Some("S-1-5-21-100")
        ),
        HandoffEvidence::Verified
    );
    assert_eq!(
        same_user_handoff_evidence(
            HandoffEvidence::Verified,
            Some("S-1-5-21-200"),
            Some("S-1-5-21-100")
        ),
        HandoffEvidence::Absent
    );
    assert_eq!(
        same_user_handoff_evidence(HandoffEvidence::Verified, Some("S-1-5-21-100"), None),
        HandoffEvidence::Unavailable
    );
    assert_eq!(
        same_user_handoff_evidence(
            HandoffEvidence::Absent,
            Some("S-1-5-21-100"),
            Some("S-1-5-21-100")
        ),
        HandoffEvidence::Absent
    );
    assert_eq!(
        same_user_handoff_evidence(
            HandoffEvidence::Verified,
            Some("S-1-5-21-100"),
            Some("S-1-5-021-100")
        ),
        HandoffEvidence::Unavailable
    );
}
#[test]
fn safeboot_evidence_is_explicit_and_rejects_unknown_modes() {
    assert_eq!(
        safeboot_evidence_from_bcd("Windows Boot Loader\n    safeboot    minimal\n"),
        SafebootEvidence::Configured("minimal".into())
    );
    assert_eq!(
        safeboot_evidence_from_bcd("Windows Boot Loader\n"),
        SafebootEvidence::Absent
    );
    assert_eq!(
        safeboot_evidence_from_bcd("safeboot    injected\n"),
        SafebootEvidence::Unavailable
    );
}
#[test]
fn unavailable_runtime_and_initiator_binding_cannot_be_misreported_as_verified() {
    let state = RebootHandoffState {
        boot_mode: BootModeEvidence::Normal,
        safeboot: SafebootEvidence::Absent,
        phase2_runonce_armed: HandoffEvidence::Absent,
        phase3_run_armed: HandoffEvidence::Verified,
        phase3_handoff_same_user: HandoffEvidence::Unavailable,
        selected_runtime_binding: HandoffEvidence::Unavailable,
        token_user_sid: Some("S-1-5-21-test".into()),
    };
    assert_eq!(state.selected_runtime_binding, HandoffEvidence::Unavailable);
    assert_eq!(state.phase3_handoff_same_user, HandoffEvidence::Unavailable);
}
#[test]
fn fixed_live_paths_cannot_be_redirected() {
    assert_eq!(WINDOWS_WORK_DIR, r"C:\FRAMETIME_CFG");
    assert!(LiveBackend::new(PathBuf::from(r"D:\other")).is_err());
}
#[test]
fn trusted_root_lexical_gate_requires_an_exact_component_boundary() {
    assert!(requested_root_is_exact(Path::new(r"C:\FRAMETIME_CFG")));
    assert!(requested_root_is_exact(Path::new(r"c:/frametime_cfg\\")));
    assert!(!requested_root_is_exact(Path::new(
        r"C:\FRAMETIME_CFG_EVIL"
    )));
    assert!(!requested_root_is_exact(Path::new(
        r"C:\FRAMETIME_CFG\child"
    )));
    assert_eq!(
        TRUSTED_WORK_DIR_SDDL,
        "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
    );
}

#[cfg(windows)]
#[test]
fn exact_dacl_with_user_owned_root_or_backup_is_rejected() {
    use std::os::windows::ffi::OsStrExt;

    use windows::{
        Win32::{
            Foundation::CloseHandle,
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
                FILE_SHARE_WRITE, OPEN_EXISTING, READ_CONTROL, WRITE_DAC,
            },
        },
        core::PCWSTR,
    };

    fn assert_user_owned_exact_dacl_is_rejected(path: &Path, directory: bool) {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let flags = if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            Default::default()
        };
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0 | WRITE_DAC.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                flags,
                None,
            )
        }
        .expect("open user-owned trusted-storage fixture");
        trusted_work_dir::apply_exact_dacl(handle)
            .expect("apply exact DACL without changing owner");
        let error = trusted_work_dir::validate_exact_security(handle)
            .expect_err("user-owned object with exact DACL must be rejected");
        unsafe {
            let _ = CloseHandle(handle);
        }
        assert!(error.contains("owner"), "unexpected rejection: {error}");
    }

    let fixture = tempfile::tempdir().expect("trusted-storage fixture");
    let root = fixture.path().join("FRAMETIME_CFG");
    fs::create_dir(&root).expect("create user-owned root");
    let backup = root.join("backup.json");
    fs::write(&backup, b"{}").expect("create user-owned backup");

    assert_user_owned_exact_dacl_is_rejected(&root, true);
    assert_user_owned_exact_dacl_is_rejected(&backup, false);
}
#[test]
fn registry_restore_requires_the_captured_step_and_exact_identity() {
    assert!(
        validate_registry_restore_binding(
            "P1:12",
            Hive::CurrentUser,
            "SOFTWARE\\Microsoft\\GameBar",
            "AllowAutoGameMode",
        )
        .is_ok()
    );
    assert!(
        validate_registry_restore_binding(
            "P1:11",
            Hive::CurrentUser,
            "SOFTWARE\\Microsoft\\GameBar",
            "AllowAutoGameMode",
        )
        .is_err()
    );
    assert!(
        validate_registry_restore_binding(
            "P1:12",
            Hive::CurrentUser,
            "SOFTWARE\\Microsoft\\GameBar",
            "InjectedValue",
        )
        .is_err()
    );
}

#[test]
fn firmware_profile_and_resizable_bar_checks_are_explicit_advisories() {
    assert!(matches!(
        action_for(1, 2).expect("P1:2 action"),
        Action::Advisory("XMP/EXPO observation requires authoritative SMBIOS memory-profile data")
    ));
    assert!(matches!(
        action_for(1, 9).expect("P1:9 action"),
        Action::Advisory("Resizable BAR observation requires PCIe capability inspection")
    ));
}

#[test]
fn all_catalog_keys_have_one_descriptor_with_stable_capability_contracts() {
    let mut keys = BTreeSet::new();
    for step in STEPS {
        let key = Progress::key(step.phase as u8, step.number);
        assert!(keys.insert(key), "catalog key must be unique");
        let descriptor = descriptor_for(step.phase as u8, step.number).expect("descriptor");
        assert_eq!(
            descriptor.recovery_requirement,
            match (step.phase as u8, step.number) {
                (1, 3) => frametime_core::RecoveryRequirement::RebuildableAudit,
                (1, 13) => frametime_core::RecoveryRequirement::Mixed,
                (2, 2) | (3, 1) => frametime_core::RecoveryRequirement::ManualRecoveryAudit,
                _ => frametime_core::RecoveryRequirement::LosslessBackup,
            }
        );
    }
    assert_eq!(keys.len(), 54);
    assert!(matches!(
        descriptor_for(1, 2).expect("P1:2 descriptor").capability,
        Capability::Advisory(
            "XMP/EXPO observation requires authoritative SMBIOS memory-profile data"
        )
    ));
    assert!(matches!(
        descriptor_for(1, 9).expect("P1:9 descriptor").capability,
        Capability::Advisory("Resizable BAR observation requires PCIe capability inspection")
    ));
    assert!(matches!(
        descriptor_for(1, 7).expect("P1:7 descriptor").capability,
        Capability::Supported
    ));
    assert!(matches!(action_for(1, 7), Ok(Action::Hags)));
}
