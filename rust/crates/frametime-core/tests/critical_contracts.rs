use std::{fs, path::Path};

use frametime_core::{
    BackupFile, Config, SteamError, discover_cs2_install, discover_steam_libraries,
};

fn write_cs2_install(root: &Path) {
    let install = root.join("steamapps/common/Counter-Strike Global Offensive");
    fs::create_dir_all(install.join("game/bin/win64")).expect("CS2 directory");
    fs::write(install.join("game/bin/win64/cs2.exe"), b"cs2").expect("CS2 executable");
    fs::write(
        root.join("steamapps/appmanifest_730.acf"),
        "\"AppState\" { \"appid\" \"730\" }",
    )
    .expect("CS2 manifest");
}

#[test]
fn steam_vdf_and_cs2_discovery_accept_only_trusted_paths() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let steam = temporary.path().join("Steam");
    let library = temporary.path().join("Library");
    fs::create_dir_all(steam.join("steamapps")).expect("Steam directory");
    write_cs2_install(&library);
    fs::write(
        steam.join("steamapps/libraryfolders.vdf"),
        format!(
            "\"libraryfolders\" {{ \"1\" {{ \"path\" \"{}\" }} }}",
            library.display()
        ),
    )
    .expect("library VDF");

    assert_eq!(
        discover_steam_libraries(&steam).expect("libraries"),
        vec![steam.clone(), library.clone()]
    );
    assert_eq!(
        discover_cs2_install(&steam)
            .expect("discovery")
            .expect("CS2")
            .library_root,
        library
    );

    fs::write(
        steam.join("steamapps/libraryfolders.vdf"),
        "\"libraryfolders\" {",
    )
    .expect("malformed VDF");
    assert!(matches!(
        discover_steam_libraries(&steam),
        Err(SteamError::InvalidVdf(_))
    ));
}

#[cfg(unix)]
#[test]
fn steam_rejects_a_reparse_point_before_reading_it() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary root");
    let steam = temporary.path().join("Steam");
    let outside = temporary.path().join("outside");
    fs::create_dir_all(&outside).expect("outside directory");
    fs::create_dir_all(&steam).expect("Steam directory");
    symlink(&outside, steam.join("steamapps")).expect("steamapps link");

    assert!(matches!(
        discover_steam_libraries(&steam),
        Err(SteamError::EscapedPath(_))
    ));
}

#[test]
fn configuration_and_backup_inputs_are_validated_without_fixture_files() {
    let config = r#"
version = "test"
work_dir = "C:\\FRAMETIME_CFG"
log_max_files = 5
run_once_execution_policy = "Bypass"
autostart_remove = []
xbox_services = []
[fps_cap]
percent = 0.09
minimum = 60
[benchmark_maps]
dust2 = "https://example.test/dust2"
inferno = "https://example.test/inferno"
ancient = "https://example.test/ancient"
[device_guids]
display = "{4d36e968-e325-11ce-bfc1-08002be10318}"
network = "{4d36e972-e325-11ce-bfc1-08002be10318}"
[paths]
shader_cache = ["%ProgramFiles(x86)%\\Steam\\steamapps\\shadercache\\730"]
nvidia_dx_cache = "%LOCALAPPDATA%\\NVIDIA\\DXCache"
nvidia_gl_cache = "%LOCALAPPDATA%\\NVIDIA\\GLCache"
directx_shader_cache = "%LOCALAPPDATA%\\D3DSCache"
latency_targets = "assets/cfgs/valve-latency-targets.json"
[chipset_urls]
amd = "https://example.test/amd"
intel = "https://example.test/intel"
[dns]
provider = "skip"
cloudflare = ["1.1.1.1", "1.0.0.1"]
google = ["8.8.8.8", "8.8.4.4"]
[nic]
virtual_adapter_filter = "Virtual"
eee = "Disabled"
flow_control = "Disabled"
interrupt_moderation = "Medium"
receive_buffers = 512
transmit_buffers = 512
high_speed_buffers = 2048
high_speed_threshold_bps = 5000000000
[nic.alternate_names]
EEE = "Energy Efficient Ethernet"
"#;
    assert!(Config::parse_str(config).is_ok());
    assert!(Config::parse_str(&config.replace("C:\\\\FRAMETIME_CFG", "C:\\\\elsewhere")).is_err());

    let mut backup: BackupFile = serde_json::from_str(
        r#"{"created":"now","entries":[{"type":"registry","step":"P1:1","timestamp":"now","path":"HKLM\\A","name":"Value","originalValue":1,"originalType":"DWORD","existed":true}]}"#,
    )
    .expect("backup input");
    backup.push_first_value(backup.entries[0].clone());
    assert_eq!(backup.entries.len(), 1);
    assert_eq!(
        backup.restore_order().next().and_then(|entry| entry.step()),
        Some("P1:1")
    );
}
