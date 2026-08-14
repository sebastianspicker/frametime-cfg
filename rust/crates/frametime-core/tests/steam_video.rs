use std::{fs, path::Path, time::Duration};

use frametime_core::{
    GpuVendor, VideoFilePlatform, VideoStatus, VideoTier, discover_cs2_install, discover_video_txt,
    parse_video_document, resolve_video_tier, steam::discover_steam_libraries,
    write_trusted_video_config,
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/steam-video");

struct NoopPlatform;
impl VideoFilePlatform for NoopPlatform {
    fn clear_read_only(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

fn copy_fixture(name: &str) -> tempfile::TempDir {
    let temporary = tempfile::tempdir().expect("temporary directory");
    copy_tree(&Path::new(FIXTURES).join(name), temporary.path());
    let library_vdf = temporary.path().join("Steam/steamapps/libraryfolders.vdf");
    if library_vdf.exists() {
        let raw = fs::read_to_string(&library_vdf).expect("library VDF");
        let library = temporary.path().join("Library").display().to_string();
        fs::write(&library_vdf, raw.replace("__FIXTURE_LIBRARY__", &library))
            .expect("configured library VDF");
    }
    temporary
}
fn copy_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).expect("fixture directory") {
        let entry = entry.expect("entry");
        let destination = destination.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            fs::create_dir_all(&destination).expect("directory");
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).expect("copy");
        }
    }
}

#[test]
fn discovers_cs2_only_with_manifest_and_safe_library_path() {
    let fixture = copy_fixture("valid");
    let steam = fixture.path().join("Steam");
    let libraries = discover_steam_libraries(&steam).expect("libraries");
    assert_eq!(libraries.len(), 2);
    let install = discover_cs2_install(&steam)
        .expect("discovery")
        .expect("CS2");
    assert!(
        install
            .install_root
            .ends_with("Counter-Strike Global Offensive")
    );
    assert!(install.library_root.ends_with("Library"));
}

#[test]
fn malformed_or_traversal_vdf_never_discovers_an_install() {
    for fixture in ["malformed-vdf", "traversal-vdf"] {
        let fixture = copy_fixture(fixture);
        let steam = fixture.path().join("Steam");
        assert!(
            discover_cs2_install(&steam).is_err()
                || discover_cs2_install(&steam)
                    .expect("safe failure")
                    .is_none()
        );
    }
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = copy_fixture("valid");
    let steam = fixture.path().join("Steam");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(
        outside.join("steamapps/common/Counter-Strike Global Offensive/game/bin/win64"),
    )
    .expect("outside");
    fs::write(
        outside.join("steamapps/common/Counter-Strike Global Offensive/game/bin/win64/cs2.exe"),
        "x",
    )
    .expect("exe");
    let linked = steam.join("steamapps");
    fs::remove_dir_all(&linked).expect("remove fixture steamapps");
    symlink(outside.join("steamapps"), linked).expect("link");
    assert!(discover_cs2_install(&steam).is_err());
}

#[test]
fn video_document_preserves_unmanaged_content_and_has_exactly_thirteen_managed_values() {
    let source = fs::read_to_string(Path::new(FIXTURES).join("video-source.txt")).expect("source");
    let document = parse_video_document(&source).expect("parse");
    let rendered = document
        .with_preset(VideoTier::Low, GpuVendor::Other)
        .to_utf8();
    assert!(rendered.contains("// keep this comment"));
    assert!(rendered.contains("// keep managed note"));
    assert!(rendered.contains("\"setting.defaultres\" \"1920\""));
    let reparsed = parse_video_document(&rendered).expect("reparse");
    let values = reparsed.values();
    assert_eq!(
        values
            .iter()
            .filter(|(key, _)| key.starts_with("setting.") && key.as_str() != "setting.defaultres")
            .count(),
        13
    );
    assert_eq!(values["setting.r_csgo_cmaa_enable"], "1");
    assert_eq!(values["setting.r_texturefilteringquality"], "0");
}

#[test]
fn auto_policy_and_rows_match_the_gui_contract() {
    assert_eq!(
        resolve_video_tier(VideoTier::Auto, GpuVendor::Nvidia),
        VideoTier::High
    );
    assert_eq!(
        resolve_video_tier(VideoTier::Auto, GpuVendor::Other),
        VideoTier::Mid
    );
    let document =
        parse_video_document("\"VideoConfig\"\n{\n    \"setting.msaa_samples\" \"0\"\n}\n")
            .expect("document");
    let rows = document.rows(VideoTier::Auto, GpuVendor::Nvidia);
    assert_eq!(rows.len(), 13);
    assert_eq!(
        rows.iter()
            .find(|row| row.setting == "msaa_samples")
            .expect("row")
            .status,
        VideoStatus::Differs
    );
}

#[test]
fn newest_video_file_is_selected_and_first_backup_is_preserved() {
    let fixture = copy_fixture("video-tree");
    let steam = fixture.path().join("Steam");
    let old = steam.join("userdata/100/730/local/cfg/video.txt");
    let newest = steam.join("userdata/200/730/local/cfg/video.txt");
    fs::write(&old, "\"VideoConfig\"\n{\n}\n").expect("old");
    std::thread::sleep(Duration::from_millis(20));
    fs::write(
        &newest,
        "\"VideoConfig\"\n{\n    \"setting.defaultres\" \"2560\"\n}\n",
    )
    .expect("newest");
    assert_eq!(
        discover_video_txt(&steam)
            .expect("discover")
            .expect("video"),
        newest
    );
    let source = fs::read_to_string(&newest).expect("source");
    let document = parse_video_document(&source).expect("document");
    let first = write_trusted_video_config(
        &steam,
        &newest,
        &document,
        VideoTier::Mid,
        GpuVendor::Other,
        &NoopPlatform,
    )
    .expect("write");
    assert!(first.backup_created);
    let backup = fs::read(newest.with_extension("txt.bak")).expect("backup");
    fs::write(
        &newest,
        "\"VideoConfig\"\n{\n    \"setting.msaa_samples\" \"0\"\n}\n",
    )
    .expect("mutate");
    let second_document =
        parse_video_document(&fs::read_to_string(&newest).expect("new source")).expect("document");
    let second = write_trusted_video_config(
        &steam,
        &newest,
        &second_document,
        VideoTier::Low,
        GpuVendor::Other,
        &NoopPlatform,
    )
    .expect("write");
    assert!(!second.backup_created);
    assert_eq!(
        fs::read(newest.with_extension("txt.bak")).expect("backup"),
        backup
    );
}
