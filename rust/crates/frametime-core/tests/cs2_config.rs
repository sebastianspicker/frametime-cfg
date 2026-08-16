use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use frametime_core::{
    BackupEntry, Cs2ConfigController, Cs2ConfigError, Cs2ConfigFs, Cs2ConfigRequest,
    NativeCs2ConfigFs, OptimizationBackup, OptionalCfgAsset, discover_cs2_install,
};

fn install_fixture() -> (tempfile::TempDir, Cs2ConfigController) {
    let temporary = tempfile::tempdir().expect("temporary Steam root");
    let steam = temporary.path();
    let install = steam.join("steamapps/common/Counter-Strike Global Offensive");
    fs::create_dir_all(install.join("game/bin/win64")).expect("game executable directory");
    fs::create_dir_all(install.join("game/csgo")).expect("CS2 config parent");
    fs::write(install.join("game/bin/win64/cs2.exe"), b"cs2").expect("executable");
    fs::write(
        steam.join("steamapps/appmanifest_730.acf"),
        "\"AppState\"\n{\n\"appid\" \"730\"\n}\n",
    )
    .expect("manifest");
    let install = discover_cs2_install(steam)
        .expect("discovery")
        .expect("CS2 install");
    let controller = Cs2ConfigController::new(install).expect("trusted controller");
    (temporary, controller)
}

fn request() -> Cs2ConfigRequest {
    Cs2ConfigRequest::at(
        "2026-08-10 12:34",
        [OptionalCfgAsset::NetStable, OptionalCfgAsset::AudioStable],
    )
    .expect("request")
}

#[test]
fn preview_is_read_only_and_apply_writes_only_fixed_cfg_targets() {
    let (_temporary, controller) = install_fixture();
    let preview = controller.preview(&request()).expect("preview");
    assert!(!preview.cfg_directory.exists());
    assert_eq!(preview.optional_assets.len(), 2);
    assert!(
        preview
            .optional_assets
            .iter()
            .all(|asset| asset.target.parent() == Some(preview.cfg_directory.as_path()))
    );

    let mut files = NativeCs2ConfigFs;
    let report = controller.apply(&request(), &mut files).expect("apply");
    assert!(matches!(
        report.optimization_backup,
        OptimizationBackup::NotNeeded
    ));
    let optimization = fs::read(&report.optimization_path).expect("optimization");
    assert!(!optimization.starts_with(&[0xef, 0xbb, 0xbf]));
    assert!(optimization.starts_with(b"// frametime.cfg - optimization.cfg\n"));
    let autoexec = fs::read_to_string(&report.autoexec_path).expect("autoexec");
    assert_eq!(
        autoexec,
        "// Your CS2 autoexec - add personal CVars above the exec line.\n\nexec optimization.cfg\n"
    );
    assert_eq!(report.optional_assets_written.len(), 2);
    assert_eq!(
        fs::read(&report.optional_assets_written[0]).expect("asset"),
        OptionalCfgAsset::NetStable.bytes()
    );
}

#[test]
fn apply_preserves_autoexec_and_creates_the_first_optimization_backup_once() {
    let (_temporary, controller) = install_fixture();
    let preview = controller.preview(&request()).expect("preview");
    fs::create_dir_all(&preview.cfg_directory).expect("cfg directory");
    fs::write(&preview.optimization_path, b"original optimization\n").expect("optimization");
    fs::write(&preview.autoexec_path, b"bind x y\n").expect("autoexec");

    let mut files = NativeCs2ConfigFs;
    let first = controller
        .apply(&request(), &mut files)
        .expect("first apply");
    assert_eq!(
        first.optimization_backup,
        OptimizationBackup::Created(preview.cfg_directory.join("optimization.cfg.bak"))
    );
    assert_eq!(
        fs::read(preview.cfg_directory.join("optimization.cfg.bak")).expect("backup"),
        b"original optimization\n"
    );
    assert_eq!(
        fs::read_to_string(&preview.autoexec_path).expect("autoexec"),
        "bind x y\n\nexec optimization.cfg\n"
    );

    fs::write(&preview.optimization_path, b"later optimization\n").expect("later optimization");
    fs::write(
        &preview.autoexec_path,
        b"bind z y\nEXEC optimization.cfg // already present\n",
    )
    .expect("commented exec");
    let second = controller
        .apply(&request(), &mut files)
        .expect("second apply");
    assert_eq!(
        second.optimization_backup,
        OptimizationBackup::Retained(preview.cfg_directory.join("optimization.cfg.bak"))
    );
    assert!(!second.autoexec_updated);
    assert_eq!(
        fs::read_to_string(&preview.autoexec_path).expect("preserved autoexec"),
        "bind z y\nEXEC optimization.cfg // already present\n"
    );
}

#[derive(Default)]
struct RecordingFs {
    files: BTreeMap<PathBuf, Vec<u8>>,
    events: Vec<String>,
    fail_autoexec_write: bool,
    corrupt_optimization_readback: bool,
}

impl Cs2ConfigFs for RecordingFs {
    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        self.events.push(format!("directory:{}", path.display()));
        Ok(())
    }

    fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        self.events.push(format!("read:{}", path.display()));
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn create_file_new(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.events.push(format!("backup:{}", path.display()));
        if self.files.contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        self.files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn atomic_replace(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.events.push(format!("replace:{}", path.display()));
        if self.fail_autoexec_write && path.file_name().is_some_and(|name| name == "autoexec.cfg") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "forced autoexec failure",
            ));
        }
        let bytes = if self.corrupt_optimization_readback
            && path
                .file_name()
                .is_some_and(|name| name == "optimization.cfg")
        {
            b"corrupt".to_vec()
        } else {
            bytes.to_vec()
        };
        self.files.insert(path.to_path_buf(), bytes);
        Ok(())
    }

    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.events.push(format!("remove:{}", path.display()));
        self.files
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

#[test]
fn backup_precedes_later_failure_and_the_error_retains_recovery_evidence() {
    let (_temporary, controller) = install_fixture();
    let preview = controller.preview(&request()).expect("preview");
    fs::create_dir_all(&preview.cfg_directory).expect("cfg directory");
    let mut files = RecordingFs {
        files: BTreeMap::from([(preview.optimization_path.clone(), b"original".to_vec())]),
        fail_autoexec_write: true,
        ..Default::default()
    };

    let error = controller
        .apply(&request(), &mut files)
        .expect_err("forced failure");
    let backup = preview.cfg_directory.join("optimization.cfg.bak");
    assert_eq!(files.files.get(&backup), Some(&b"original".to_vec()));
    let backup_event = files
        .events
        .iter()
        .position(|event| event.starts_with("backup:"))
        .unwrap();
    let optimization_event = files
        .events
        .iter()
        .position(|event| event.starts_with("replace:") && event.ends_with("optimization.cfg"))
        .unwrap();
    assert!(backup_event < optimization_event);
    assert!(matches!(
        error,
        Cs2ConfigError::Mutation { recovery: Some(path), .. } if path == backup
    ));
}

#[test]
fn incorrect_readback_is_an_error_not_a_false_success() {
    let (_temporary, controller) = install_fixture();
    let preview = controller.preview(&request()).expect("preview");
    fs::create_dir_all(&preview.cfg_directory).expect("cfg directory");
    let mut files = RecordingFs {
        corrupt_optimization_readback: true,
        ..Default::default()
    };
    assert!(matches!(
        controller.apply(&request(), &mut files),
        Err(Cs2ConfigError::ReadbackMismatch { target, .. }) if target == preview.optimization_path
    ));
}

#[test]
fn transaction_restore_rebinds_logical_targets_and_deletes_only_absent_captures() {
    let (_temporary, controller) = install_fixture();
    let request = request();
    let preview = controller.preview(&request).expect("preview");
    fs::create_dir_all(&preview.cfg_directory).expect("cfg directory");
    fs::write(&preview.optimization_path, b"original optimization\n").expect("optimization");
    fs::write(&preview.autoexec_path, b"bind x y\n").expect("autoexec");
    let mut files = NativeCs2ConfigFs;
    let transaction =
        BackupEntry::capture_cs2_config_transaction(controller.install(), &request, &mut files)
            .expect("capture");
    controller.apply(&request, &mut files).expect("apply");
    transaction
        .restore_cs2_config_transaction(controller.install(), &request, &mut files)
        .expect("restore");
    assert_eq!(
        fs::read(&preview.optimization_path).expect("optimization"),
        b"original optimization\n"
    );
    assert_eq!(
        fs::read(&preview.autoexec_path).expect("autoexec"),
        b"bind x y\n"
    );
    for asset in request.optional_assets() {
        assert!(!preview.cfg_directory.join(asset.file_name()).exists());
    }
}

#[cfg(unix)]
#[test]
fn symlinked_cfg_directory_is_rejected_before_any_mutation() {
    use std::os::unix::fs::symlink;

    let (temporary, controller) = install_fixture();
    let cfg_parent = controller.install().install_root.join("game/csgo");
    fs::create_dir_all(&cfg_parent).expect("cfg parent");
    let outside = temporary.path().join("outside");
    fs::create_dir(&outside).expect("outside");
    symlink(&outside, cfg_parent.join("cfg")).expect("cfg link");
    assert!(matches!(
        controller.preview(&request()),
        Err(Cs2ConfigError::UntrustedPath(_))
    ));
}
