use frametime_domain::{BackupFile, Config};

#[test]
fn configuration_and_backup_inputs_are_validated_without_fixture_files() {
    assert!(Config::parse_str(include_str!("../../../frametime.toml")).is_ok());
    let config = r#"
autostart_remove = ["OneDrive"]
[fps_cap]
strategy = "raw"
measured_cap = 0
[paths]
shader_cache = ["%ProgramFiles(x86)%\\Steam\\steamapps\\shadercache\\730"]
nvidia_dx_cache = "%LOCALAPPDATA%\\NVIDIA\\DXCache"
nvidia_gl_cache = "%LOCALAPPDATA%\\NVIDIA\\GLCache"
directx_shader_cache = "%LOCALAPPDATA%\\D3DSCache"
"#;
    let parsed = Config::parse_str(config).expect("retained configuration fields");
    assert_eq!(parsed.fps_cap.measured_cap, 0);
    assert_eq!(
        parsed.paths.shader_cache,
        [r"%ProgramFiles(x86)%\Steam\steamapps\shadercache\730"]
    );
    assert_eq!(
        parsed.paths.nvidia_dx_cache,
        r"%LOCALAPPDATA%\NVIDIA\DXCache"
    );
    assert_eq!(
        parsed.paths.nvidia_gl_cache,
        r"%LOCALAPPDATA%\NVIDIA\GLCache"
    );
    assert_eq!(
        parsed.paths.directx_shader_cache,
        r"%LOCALAPPDATA%\D3DSCache"
    );
    assert_eq!(parsed.autostart_remove, ["OneDrive"]);
    assert!(Config::parse_str(&config.replace("measured_cap = 0", "measured_cap = 29")).is_err());
    assert!(
        Config::parse_str(&config.replace(
            "shader_cache = [\"%ProgramFiles(x86)%\\\\Steam\\\\steamapps\\\\shadercache\\\\730\"]",
            "shader_cache = []"
        ))
        .is_err()
    );
    assert!(
        Config::parse_str(&config.replace(
            "shader_cache = [\"%ProgramFiles(x86)%\\\\Steam\\\\steamapps\\\\shadercache\\\\730\"]",
            "shader_cache = [\"%ProgramFiles(x86)%\\\\Steam\\\\steamapps\\\\shadercache\\\\730\", \"%ProgramFiles(x86)%\\\\Steam\\\\steamapps\\\\shadercache\\\\730\"]"
        ))
        .is_err()
    );
    assert!(
        Config::parse_str(&config.replace("shadercache\\\\730", "shadercache\\\\999")).is_err()
    );
    assert!(Config::parse_str(&config.replace("DXCache", "OtherCache")).is_err());
    assert!(Config::parse_str(&config.replace("GLCache", "OtherCache")).is_err());
    assert!(Config::parse_str(&config.replace("D3DSCache", "OtherCache")).is_err());
    assert!(Config::parse_str(&config.replace("autostart_remove", "xbox_services")).is_err());

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
