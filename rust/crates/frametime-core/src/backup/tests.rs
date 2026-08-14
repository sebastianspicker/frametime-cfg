use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{Cs2ConfigFs, Cs2ConfigRequest, Cs2ConfigTarget, Cs2Install, OptionalCfgAsset};

use super::*;
#[test]
fn unknown_entry_is_retained() {
    let raw = r#"{"entries":[{"type":"future","step":"x","payload":1}],"created":"now"}"#;
    let file: BackupFile = serde_json::from_str(raw).expect("backup");
    assert!(matches!(file.entries[0], BackupEntry::Unknown(_)));
    assert!(
        serde_json::to_string(&file)
            .expect("json")
            .contains("future")
    );
}

#[test]
fn hags_receipt_preserves_absence_and_pending_effective_verification() {
    let raw = serde_json::json!({
        "type": "hags",
        "step": "P1:7",
        "timestamp": "2026-08-13 12:00:00",
        "originalValue": null,
        "targetValue": 2,
        "adapterIds": ["pci-10de-2684-00000000-a1"],
        "effectiveVerificationPending": true,
        "futureField": {"kept": true}
    });
    let entry: BackupEntry = serde_json::from_value(raw.clone()).expect("HAGS receipt");
    let BackupEntry::Hags {
        original_value,
        target_value,
        adapter_ids,
        effective_verification_pending,
        unknown,
        ..
    } = &entry
    else {
        panic!("HAGS receipt");
    };
    assert_eq!(*original_value, None);
    assert_eq!(*target_value, 2);
    assert_eq!(adapter_ids, &["pci-10de-2684-00000000-a1"]);
    assert!(*effective_verification_pending);
    assert!(unknown["futureField"]["kept"].as_bool().unwrap());
    assert_eq!(
        serde_json::to_value(entry).expect("serialize HAGS receipt"),
        raw
    );
}

#[test]
fn all_eleven_legacy_entry_types_round_trip() {
    let raw = r#"{
          "created":"2026-01-01 00:00:00",
          "entries":[
            {"type":"registry","step":"s","timestamp":"t","path":"HKLM:\\X","name":"N","originalValue":1,"originalType":"DWord","existed":true},
            {"type":"service","step":"s","timestamp":"t","name":"Svc","originalStartType":"Automatic","delayedAutoStart":false,"originalStatus":"Running"},
            {"type":"powerplan","step":"s","timestamp":"t","originalGuid":"00000000-0000-0000-0000-000000000000","originalName":"Balanced","suiteOwnedGuids":[]},
            {"type":"bootconfig","step":"s","timestamp":"t","key":"disabledynamictick","originalValue":"No","existed":true},
            {"type":"scheduledtask","step":"s","timestamp":"t","taskName":"Task","taskPath":"\\","existed":false,"wasEnabled":false},
            {"type":"nic_adapter","step":"s","timestamp":"t","adapterName":"Ethernet","interfaceDescription":"NIC","propertyName":"EEE","originalValue":"Enabled","propertyType":"DisplayValue"},
            {"type":"qos_uro","step":"s","timestamp":"t","policies":[],"uroState":false},
            {"type":"defender","step":"s","timestamp":"t","exclusionPaths":[],"exclusionProcesses":[]},
            {"type":"pagefile","step":"s","timestamp":"t","automaticManaged":true,"pagefilePath":"C:\\pagefile.sys","initialSize":0,"maximumSize":0},
            {"type":"dns","step":"s","timestamp":"t","adapterName":"Ethernet","interfaceIndex":1,"originalDnsServers":[]},
            {"type":"drs","step":"s","timestamp":"t","profile":"Counter-Strike 2","profileCreated":false,"settings":[{"id":1,"previousValue":2,"existed":true}]}
          ]
        }"#;
    let file: BackupFile = serde_json::from_str(raw).expect("legacy backup");
    assert_eq!(file.entries.len(), 11);
    assert!(matches!(file.entries[0], BackupEntry::Registry { .. }));
    assert!(matches!(file.entries[10], BackupEntry::Drs { .. }));
    let encoded = serde_json::to_string(&file).expect("encoded backup");
    let decoded: BackupFile = serde_json::from_str(&encoded).expect("decoded backup");
    assert_eq!(decoded, file);
}

#[test]
fn dns_backup_preserves_durable_adapter_identity() {
    let raw = r#"{"created":"now","entries":[{"type":"dns","step":"P3:9","timestamp":"t","adapterName":"Ethernet","interfaceIndex":12,"adapterGuid":"{11111111-1111-1111-1111-111111111111}","interfaceGuid":"{11111111-1111-1111-1111-111111111111}","interfaceLuid":42,"physicalAddress":[1,2,3,4,5,6],"originalDnsServers":["192.0.2.1"]}]}"#;
    let backup: BackupFile = serde_json::from_str(raw).expect("DNS backup");
    let BackupEntry::Dns {
        adapter_guid,
        interface_guid,
        interface_luid,
        physical_address,
        ..
    } = &backup.entries[0]
    else {
        panic!("DNS record")
    };
    assert_eq!(
        adapter_guid.as_deref(),
        Some("{11111111-1111-1111-1111-111111111111}")
    );
    assert_eq!(interface_guid, adapter_guid);
    assert_eq!(*interface_luid, Some(42));
    assert_eq!(physical_address, &[1, 2, 3, 4, 5, 6]);
}

#[test]
fn legacy_deduplication_is_case_insensitive_but_does_not_merge_pagefile_entries() {
    let mut backup = BackupFile {
        entries: Vec::new(),
        created: "now".into(),
        unknown: BTreeMap::new(),
    };
    let registry = |path: &str, value: i32| BackupEntry::Registry {
        step: "s".into(),
        timestamp: "t".into(),
        path: path.into(),
        name: "Value".into(),
        original_value: value.into(),
        original_type: Some("DWord".into()),
        existed: true,
        unknown: BTreeMap::new(),
    };
    backup.push_first_value(registry(r"HKLM:\X", 1));
    backup.push_first_value(registry(r"hklm:\x", 2));
    backup.push_first_value(BackupEntry::Pagefile {
        step: "s".into(),
        timestamp: "t".into(),
        automatic_managed: true,
        pagefile_path: "C:\\pagefile.sys".into(),
        initial_size: 0,
        maximum_size: 0,
        unknown: BTreeMap::new(),
    });
    backup.push_first_value(BackupEntry::Pagefile {
        step: "later".into(),
        timestamp: "t".into(),
        automatic_managed: false,
        pagefile_path: "D:\\pagefile.sys".into(),
        initial_size: 1,
        maximum_size: 2,
        unknown: BTreeMap::new(),
    });
    assert_eq!(backup.entries.len(), 3);
    assert!(matches!(
        backup.entries[0],
        BackupEntry::Registry { original_value: Value::Number(ref value), .. }
            if value.as_i64() == Some(1)
    ));
}

#[test]
fn same_identity_in_a_different_step_is_captured_again() {
    let service = |step: &str| BackupEntry::Service {
        step: step.into(),
        timestamp: "t".into(),
        name: "SysMain".into(),
        original_start_type: "Automatic".into(),
        delayed_auto_start: false,
        original_status: "Running".into(),
        unknown: BTreeMap::new(),
    };
    let mut backup = BackupFile {
        entries: Vec::new(),
        created: "now".into(),
        unknown: BTreeMap::new(),
    };
    backup.push_first_value(service("P1:37"));
    backup.push_first_value(service("P1:37"));
    backup.push_first_value(service("Recovery"));
    assert_eq!(backup.entries.len(), 2);
}

#[test]
fn pagefile_transaction_deduplicates_p1_8_without_using_timestamp() {
    let transaction = |timestamp: &str, target_path: &str| BackupEntry::PagefileTransaction {
        step: "P1:8".into(),
        timestamp: timestamp.into(),
        automatic_managed: false,
        target_path: target_path.into(),
        target_existed: true,
        settings: vec![PagefileTransactionSetting {
            path: target_path.into(),
            initial_size: 4096,
            maximum_size: 8192,
            object_path: Some(format!("object:{timestamp}")),
            relative_path: None,
            unknown: BTreeMap::new(),
        }],
        computer_object_path: Some(format!("computer:{timestamp}")),
        computer_relative_path: None,
        created_object_path: None,
        created_relative_path: None,
        created_initial_size: None,
        created_maximum_size: None,
        mutation_intent: None,
        unknown: BTreeMap::new(),
    };
    let mut backup = BackupFile {
        entries: Vec::new(),
        created: "now".into(),
        unknown: BTreeMap::new(),
    };

    backup.push_first_value(transaction("first", r"C:\\pagefile.sys"));
    backup.push_first_value(transaction("later", r"D:\\pagefile.sys"));
    assert_eq!(backup.entries.len(), 1);
    assert!(matches!(
        &backup.entries[0],
        BackupEntry::PagefileTransaction {
            timestamp,
            target_path,
            computer_object_path,
            ..
        } if timestamp == "first"
            && target_path == r"C:\\pagefile.sys"
            && computer_object_path.as_deref() == Some("computer:first")
    ));
}

#[test]
fn tokenless_pagefile_transaction_records_remain_compatible() {
    let raw = r#"{
          "created":"now",
          "entries":[{
            "type":"pagefile_transaction",
            "step":"P1:8",
            "timestamp":"then",
            "automaticManaged":true,
            "targetPath":"C:\\pagefile.sys",
            "targetExisted":true,
            "settings":[{"path":"C:\\pagefile.sys","initialSize":0,"maximumSize":0}]
          }]
        }"#;
    let backup: BackupFile = serde_json::from_str(raw).expect("tokenless transaction");
    let BackupEntry::PagefileTransaction {
        computer_object_path,
        computer_relative_path,
        created_object_path,
        created_relative_path,
        mutation_intent,
        settings,
        ..
    } = &backup.entries[0]
    else {
        panic!("expected pagefile transaction");
    };
    assert!(computer_object_path.is_none());
    assert!(computer_relative_path.is_none());
    assert!(created_object_path.is_none());
    assert!(created_relative_path.is_none());
    assert!(mutation_intent.is_none());
    assert!(settings[0].object_path.is_none());
    assert!(settings[0].relative_path.is_none());

    let encoded = serde_json::to_value(backup).expect("transaction JSON");
    assert!(encoded["entries"][0].get("computerObjectPath").is_none());
    assert!(
        encoded["entries"][0]["settings"][0]
            .get("objectPath")
            .is_none()
    );
    assert_eq!(
        encoded,
        serde_json::from_str::<Value>(raw).expect("expected tokenless JSON")
    );
}

#[test]
fn pagefile_transaction_tokens_and_unknown_values_round_trip() {
    let raw = serde_json::json!({
        "created": "now",
        "futureBackup": {"retained": true},
        "entries": [{
            "type": "pagefile_transaction",
            "step": "P1:8",
            "timestamp": "then",
            "automaticManaged": false,
            "targetPath": r"C:\\pagefile.sys",
            "targetExisted": true,
            "settings": [{
                "path": r"C:\\pagefile.sys",
                "initialSize": 4096,
                "maximumSize": 8192,
                "objectPath": "Win32_PageFileSetting:pagefile-c",
                "relativePath": "Win32_PageFileSetting:pagefile-c",
                "futureSetting": {"keep": true}
            }],
            "computerObjectPath": "Win32_ComputerSystem.Name=\"HOST\"",
            "computerRelativePath": "Win32_ComputerSystem.Name=\"HOST\"",
            "createdObjectPath": "Win32_PageFileSetting:pagefile-c",
            "createdRelativePath": "Win32_PageFileSetting:pagefile-c",
            "createdInitialSize": 4096,
            "createdMaximumSize": 8192,
            "mutationIntent": "create_or_update",
            "futureTransaction": {"retained": true}
        }]
    });
    let backup: BackupFile = serde_json::from_value(raw.clone()).expect("transaction");
    let BackupEntry::PagefileTransaction {
        computer_object_path,
        mutation_intent,
        settings,
        unknown,
        ..
    } = &backup.entries[0]
    else {
        panic!("expected pagefile transaction");
    };
    assert_eq!(
        computer_object_path.as_deref(),
        Some("Win32_ComputerSystem.Name=\"HOST\"")
    );
    assert_eq!(mutation_intent.as_deref(), Some("create_or_update"));
    assert_eq!(
        settings[0].object_path.as_deref(),
        Some("Win32_PageFileSetting:pagefile-c")
    );
    assert_eq!(settings[0].unknown["futureSetting"]["keep"], true);
    assert!(unknown["futureTransaction"]["retained"].as_bool().unwrap());
    assert_eq!(serde_json::to_value(backup).expect("transaction JSON"), raw);
}

fn cs2_install_fixture() -> (tempfile::TempDir, Cs2Install) {
    let temporary = tempfile::tempdir().expect("temporary Steam root");
    let steam_root = temporary.path().to_path_buf();
    let install_root = steam_root.join("steamapps/common/Counter-Strike Global Offensive");
    fs::create_dir_all(install_root.join("game/bin/win64")).expect("executable directory");
    fs::create_dir_all(install_root.join("game/csgo")).expect("CS2 config parent");
    fs::write(install_root.join("game/bin/win64/cs2.exe"), b"cs2").expect("executable");
    let install = Cs2Install {
        steam_root: steam_root.clone(),
        library_root: steam_root,
        install_root,
    };
    (temporary, install)
}

#[derive(Default)]
struct SnapshotFs {
    files: BTreeMap<PathBuf, Vec<u8>>,
    reads: Vec<PathBuf>,
    fail_after_reads: Option<usize>,
    write_attempts: usize,
}

impl Cs2ConfigFs for SnapshotFs {
    fn create_directory(&mut self, _: &Path) -> io::Result<()> {
        self.write_attempts += 1;
        Ok(())
    }

    fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        self.reads.push(path.to_path_buf());
        if self
            .fail_after_reads
            .is_some_and(|limit| self.reads.len() > limit)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "forced read failure",
            ));
        }
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn create_file_new(&mut self, _: &Path, _: &[u8]) -> io::Result<()> {
        self.write_attempts += 1;
        Ok(())
    }

    fn atomic_replace(&mut self, _: &Path, _: &[u8]) -> io::Result<()> {
        self.write_attempts += 1;
        Ok(())
    }

    fn remove_file(&mut self, _: &Path) -> io::Result<()> {
        self.write_attempts += 1;
        Ok(())
    }
}

fn cfg_path(install: &Cs2Install, target: Cs2ConfigTarget) -> PathBuf {
    install
        .install_root
        .join("game/csgo/cfg")
        .join(target.file_name())
}

fn captured_empty_request() -> (tempfile::TempDir, Cs2Install, Cs2ConfigRequest, BackupEntry) {
    let (temporary, install) = cs2_install_fixture();
    let request = Cs2ConfigRequest::at("2026-08-10 12:34", []).expect("request");
    let entry =
        BackupEntry::capture_cs2_config_transaction(&install, &request, &mut SnapshotFs::default())
            .expect("capture");
    (temporary, install, request, entry)
}

#[test]
fn cs2_config_transaction_capture_is_ordered_lossless_and_path_free() {
    let (_temporary, install) = cs2_install_fixture();
    let all_optional = [
        OptionalCfgAsset::NetStable,
        OptionalCfgAsset::NetHighPing,
        OptionalCfgAsset::NetUnstable,
        OptionalCfgAsset::NetBad,
        OptionalCfgAsset::DebugHud,
        OptionalCfgAsset::DebugHudOff,
        OptionalCfgAsset::AudioStable,
        OptionalCfgAsset::AudioLowLatency025,
        OptionalCfgAsset::AudioLowLatency001,
    ];
    let request = Cs2ConfigRequest::at("2026-08-10 12:34", all_optional).expect("request");
    let mut files = SnapshotFs::default();
    files.files.insert(
        cfg_path(&install, Cs2ConfigTarget::Optimization),
        vec![0, 255, 1],
    );
    files.files.insert(
        cfg_path(&install, Cs2ConfigTarget::NetStable),
        b"original net stable".to_vec(),
    );

    let mut entry = BackupEntry::capture_cs2_config_transaction(&install, &request, &mut files)
        .expect("complete capture");
    let BackupEntry::Cs2ConfigTransaction {
        step,
        install_identity,
        targets,
        ..
    } = &entry
    else {
        panic!("CS2 CFG transaction");
    };
    assert_eq!(step, CS2_CONFIG_TRANSACTION_STEP);
    assert_eq!(targets.len(), 11);
    assert_eq!(
        targets
            .iter()
            .map(|snapshot| snapshot.target)
            .collect::<Vec<_>>(),
        Cs2ConfigTarget::for_request(&request)
    );
    assert_eq!(targets[0].original_bytes.as_deref(), Some(&[0, 255, 1][..]));
    assert_eq!(targets[1].original_bytes, None);
    assert_eq!(targets[1].original_sha256, None);
    assert_eq!(
        targets[2].original_bytes.as_deref(),
        Some(b"original net stable".as_slice())
    );
    assert!(
        targets[0]
            .original_sha256
            .as_deref()
            .is_some_and(|digest| digest.len() == 64)
    );
    assert!(install_identity.unknown.is_empty());
    assert_eq!(files.reads.len(), 11);
    assert_eq!(files.write_attempts, 0);
    entry
        .validate_cs2_config_transaction(&install, &request)
        .expect("matches controller request");
    let BackupEntry::Cs2ConfigTransaction {
        install_identity,
        targets,
        unknown,
        ..
    } = &mut entry
    else {
        panic!("CS2 CFG transaction");
    };
    install_identity
        .unknown
        .insert("futureIdentity".into(), serde_json::json!({"kept": true}));
    targets[0]
        .unknown
        .insert("futureTarget".into(), serde_json::json!("kept"));
    unknown.insert("futureTransaction".into(), serde_json::json!({"schema": 2}));
    entry
        .validate_cs2_config_transaction(&install, &request)
        .expect("unknown fields do not change controller authorization");

    let encoded = serde_json::to_string(&entry).expect("serialized entry");
    assert!(!encoded.contains(&install.install_root.display().to_string()));
    let decoded: BackupEntry = serde_json::from_str(&encoded).expect("decoded entry");
    assert_eq!(decoded, entry);

    let mut backup = BackupFile {
        entries: Vec::new(),
        created: "now".into(),
        unknown: BTreeMap::new(),
    };
    backup.push_first_value(entry.clone());
    backup.push_first_value(entry);
    assert_eq!(backup.entries.len(), 1);
    assert_eq!(backup.entries[0].step(), Some(CS2_CONFIG_TRANSACTION_STEP));
}

#[test]
fn cs2_config_transaction_rejects_incomplete_duplicate_and_invalid_bytes() {
    let (_temporary, install, request, entry) = captured_empty_request();

    let mut duplicate = entry.clone();
    let BackupEntry::Cs2ConfigTransaction { targets, .. } = &mut duplicate else {
        panic!("CS2 CFG transaction");
    };
    targets.push(targets[0].clone());
    assert!(matches!(
        duplicate.validate_cs2_config_transaction(&install, &request),
        Err(Cs2ConfigBackupError::DuplicateTarget(
            Cs2ConfigTarget::Optimization
        ))
    ));

    let mut missing = entry.clone();
    let BackupEntry::Cs2ConfigTransaction { targets, .. } = &mut missing else {
        panic!("CS2 CFG transaction");
    };
    targets.pop();
    assert!(matches!(
        missing.validate_cs2_config_transaction(&install, &request),
        Err(Cs2ConfigBackupError::MissingTarget(
            Cs2ConfigTarget::Autoexec
        ))
    ));

    let mut oversized = entry.clone();
    let BackupEntry::Cs2ConfigTransaction { targets, .. } = &mut oversized else {
        panic!("CS2 CFG transaction");
    };
    targets[0].existed = true;
    targets[0].original_bytes = Some(vec![0; CS2_CONFIG_MAX_FILE_BYTES + 1]);
    assert!(matches!(
        oversized.validate_cs2_config_transaction(&install, &request),
        Err(Cs2ConfigBackupError::FileTooLarge {
            target: Cs2ConfigTarget::Optimization,
            ..
        })
    ));

    let mut missing_digest = entry.clone();
    let BackupEntry::Cs2ConfigTransaction { targets, .. } = &mut missing_digest else {
        panic!("CS2 CFG transaction");
    };
    targets[0].existed = true;
    targets[0].original_bytes = Some(b"original".to_vec());
    assert!(matches!(
        missing_digest.validate_cs2_config_transaction(&install, &request),
        Err(Cs2ConfigBackupError::MissingDigest(
            Cs2ConfigTarget::Optimization
        ))
    ));

    let mut absent_with_bytes = entry;
    let BackupEntry::Cs2ConfigTransaction { targets, .. } = &mut absent_with_bytes else {
        panic!("CS2 CFG transaction");
    };
    targets[0].original_bytes = Some(b"forbidden".to_vec());
    assert!(matches!(
        absent_with_bytes.validate_cs2_config_transaction(&install, &request),
        Err(Cs2ConfigBackupError::AbsentTargetHasBytes(
            Cs2ConfigTarget::Optimization
        ))
    ));
}

#[test]
fn cs2_config_transaction_rejects_wrong_step_install_request_and_capture_errors() {
    let (_temporary, install, request, entry) = captured_empty_request();
    let mut wrong_step = entry.clone();
    let BackupEntry::Cs2ConfigTransaction { step, .. } = &mut wrong_step else {
        panic!("CS2 CFG transaction");
    };
    *step = "P1:35".into();
    assert!(matches!(
        wrong_step.validate_cs2_config_transaction(&install, &request),
        Err(Cs2ConfigBackupError::WrongStep { .. })
    ));

    let (_other_temporary, other_install) = cs2_install_fixture();
    assert!(matches!(
        entry.validate_cs2_config_transaction(&other_install, &request),
        Err(Cs2ConfigBackupError::InstallMismatch)
    ));
    let different_request = Cs2ConfigRequest::at("2026-08-10 12:34", [OptionalCfgAsset::NetStable])
        .expect("different request");
    assert!(matches!(
        entry.validate_cs2_config_transaction(&install, &different_request),
        Err(Cs2ConfigBackupError::MissingTarget(
            Cs2ConfigTarget::NetStable
        ))
    ));

    let mut files = SnapshotFs {
        fail_after_reads: Some(1),
        ..Default::default()
    };
    assert!(matches!(
        BackupEntry::capture_cs2_config_transaction(&install, &request, &mut files),
        Err(Cs2ConfigBackupError::Read {
            target: Cs2ConfigTarget::Autoexec,
            ..
        })
    ));
    assert_eq!(files.reads.len(), 2);
    assert_eq!(files.write_attempts, 0);
}

#[test]
fn cs2_config_transaction_preserves_unknown_fields_on_round_trip() {
    let raw = serde_json::json!({
        "type": "cs2_config_transaction",
        "step": "P1:34",
        "timestamp": "2026-08-10 12:34:56",
        "installIdentity": {
            "steamAppId": "730",
            "installFingerprint": "0123456789abcdef",
            "futureIdentity": {"kept": true}
        },
        "targets": [
            {"target": "optimization", "existed": false, "futureTarget": null},
            {"target": "autoexec", "existed": true, "originalBytes": [0, 255], "futureTarget": {"kept": true}}
        ],
        "futureTransaction": {"schema": 2}
    });
    let entry: BackupEntry = serde_json::from_value(raw.clone()).expect("transaction");
    let BackupEntry::Cs2ConfigTransaction {
        install_identity,
        targets,
        unknown,
        ..
    } = &entry
    else {
        panic!("CS2 CFG transaction");
    };
    assert!(
        install_identity.unknown["futureIdentity"]["kept"]
            .as_bool()
            .unwrap()
    );
    assert!(targets[0].unknown["futureTarget"].is_null());
    assert!(
        targets[1].unknown["futureTarget"]["kept"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(unknown["futureTransaction"]["schema"], 2);
    assert_eq!(
        serde_json::to_value(entry).expect("serialized transaction"),
        raw
    );
}

#[test]
fn cs2_config_restore_rejects_unknown_fields_before_any_mutation() {
    let (_temporary, install, request, mut entry) = captured_empty_request();
    let BackupEntry::Cs2ConfigTransaction { unknown, .. } = &mut entry else {
        panic!("CS2 CFG transaction");
    };
    unknown.insert("futureTransaction".into(), serde_json::json!(true));
    let mut files = SnapshotFs::default();
    assert!(matches!(
        entry.restore_cs2_config_transaction(&install, &request, &mut files),
        Err(Cs2ConfigBackupError::UnknownFields)
    ));
    assert_eq!(files.write_attempts, 0);
}
