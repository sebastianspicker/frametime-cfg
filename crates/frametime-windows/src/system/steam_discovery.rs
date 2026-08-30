//! Windows-owned Steam and CS2 video discovery with reparse-point rejection.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use frametime_domain::{
    Cs2Install, VideoDocument, app_manifest_is_cs2, library_paths_from_vdf, parse_video_document,
};

const CS2_DIRECTORY: &str = "Counter-Strike Global Offensive";
const USERDATA_DIR: &str = "userdata";
const CS2_VIDEO_FILE: &str = "cs2_video.txt";

pub fn discover_cs2_install(steam_root: &Path) -> Result<Option<Cs2Install>, String> {
    for library_root in discover_steam_libraries(steam_root)? {
        let manifest = library_root.join("steamapps/appmanifest_730.acf");
        if trusted_file_under(&library_root, &manifest).is_err() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        if !app_manifest_is_cs2(&text) {
            continue;
        }
        let install_root = library_root.join("steamapps/common").join(CS2_DIRECTORY);
        let executable = install_root.join("game/bin/win64/cs2.exe");
        if trusted_file_under(&library_root, &executable).is_ok() {
            return Ok(Some(Cs2Install {
                steam_root: steam_root.to_path_buf(),
                library_root,
                install_root,
            }));
        }
    }
    Ok(None)
}

pub fn discover_steam_libraries(steam_root: &Path) -> Result<Vec<PathBuf>, String> {
    trusted_directory(steam_root)?;
    let mut libraries = vec![steam_root.to_path_buf()];
    let steamapps = steam_root.join("steamapps");
    match fs::symlink_metadata(&steamapps) {
        Ok(metadata) if metadata_is_reparse(&metadata) || !metadata.is_dir() => {
            return Err(format!(
                "Steam path escaped trusted root: {}",
                steamapps.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(libraries),
        Err(error) => return Err(error.to_string()),
    }
    let vdf = steamapps.join("libraryfolders.vdf");
    if trusted_file_under(steam_root, &vdf).is_err() {
        return Ok(libraries);
    }
    let text = fs::read_to_string(vdf).map_err(|error| error.to_string())?;
    for value in library_paths_from_vdf(&text).map_err(|error| error.to_string())? {
        let library = native_path(&value);
        if trusted_directory(&library).is_ok() && !libraries.contains(&library) {
            libraries.push(library);
        }
    }
    Ok(libraries)
}

pub fn discover_video_txt(steam_root: &Path) -> Result<Option<PathBuf>, String> {
    trusted_directory(steam_root)?;
    let userdata = steam_root.join(USERDATA_DIR);
    if !userdata.is_dir() {
        return Ok(None);
    }
    trusted_directory(&userdata)?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(userdata).map_err(|error| error.to_string())? {
        let account = entry.map_err(|error| error.to_string())?.path();
        let numeric = account.file_name().is_some_and(|name| {
            let value = name.to_string_lossy();
            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
        });
        if !numeric || trusted_directory(&account).is_err() {
            continue;
        }
        let video = account.join("730/local/cfg").join(CS2_VIDEO_FILE);
        if trusted_file_under(steam_root, &video).is_ok() {
            candidates.push(video);
        }
    }
    candidates.sort();
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(candidates.pop()),
        _ => Err(format!(
            "multiple trusted CS2 video files were found; select one Steam account before inspection: {candidates:?}"
        )),
    }
}

pub fn read_trusted_video_document(
    steam_root: &Path,
    path: &Path,
) -> Result<VideoDocument, String> {
    ensure_video_path(steam_root, path)?;
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    parse_video_document(&text).map_err(|error| error.to_string())
}

pub fn trusted_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || has_unsafe_components(path) {
        return Err(format!(
            "Steam path is not a trusted real directory: {}",
            path.display()
        ));
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Steam path is not a trusted real directory: {}",
            path.display()
        ));
    }
    Ok(())
}

pub fn trusted_file_under(root: &Path, candidate: &Path) -> Result<(), String> {
    trusted_directory(root)?;
    if has_unsafe_components(candidate) {
        return Err(format!(
            "Steam path escaped its trusted root: {}",
            candidate.display()
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let candidate_canonical = fs::canonicalize(candidate).map_err(|error| error.to_string())?;
    if !candidate_canonical.starts_with(canonical_root) {
        return Err(format!(
            "Steam path escaped its trusted root: {}",
            candidate.display()
        ));
    }
    reject_reparse_ancestors(candidate, root)?;
    let metadata = fs::symlink_metadata(candidate).map_err(|error| error.to_string())?;
    if metadata_is_reparse(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Steam path escaped its trusted root: {}",
            candidate.display()
        ));
    }
    Ok(())
}

fn ensure_video_path(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "CS2 video document is not a trusted Steam userdata file: {}",
            path.display()
        )
    })?;
    let parts = relative.components().collect::<Vec<_>>();
    let numeric = parts.get(1).is_some_and(|part| {
        let value = part.as_os_str().to_string_lossy();
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
    });
    let valid = parts.len() == 6
        && parts[0].as_os_str() == USERDATA_DIR
        && numeric
        && parts[2].as_os_str() == "730"
        && parts[3].as_os_str() == "local"
        && parts[4].as_os_str() == "cfg"
        && parts[5].as_os_str() == CS2_VIDEO_FILE;
    if !valid {
        return Err(format!(
            "CS2 video document is not a trusted Steam userdata file: {}",
            path.display()
        ));
    }
    trusted_file_under(root, path)
}

fn reject_reparse_ancestors(path: &Path, stop: &Path) -> Result<(), String> {
    let mut current = Some(path);
    while let Some(value) = current {
        if metadata_is_reparse(&fs::symlink_metadata(value).map_err(|error| error.to_string())?) {
            return Err(format!(
                "Steam path escaped its trusted root: {}",
                value.display()
            ));
        }
        if value == stop {
            return Ok(());
        }
        current = value.parent();
    }
    Err("Steam path escaped its trusted root".into())
}
fn has_unsafe_components(path: &Path) -> bool {
    path.components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
}
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
fn native_path(value: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(value.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(value.replace('\\', "/"))
    }
}
