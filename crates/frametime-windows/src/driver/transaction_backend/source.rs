//! Source routing for a prepared NVIDIA transaction.

use super::*;

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub(super) fn resolve_source(
    request: &NvidiaPreparationRequest,
) -> Result<(String, String, NvidiaArtifactLocation), String> {
    let supplied_name = request.artifact_file_name.clone();
    let id = request
        .artifact_id
        .clone()
        .unwrap_or_else(|| "nvidia-driver".into());
    match &request.source {
        NvidiaInstallerSource::OfficialUrl(url) => official_source(url, supplied_name, id),
        NvidiaInstallerSource::LegacyServerPath(path) => {
            let name = supplied_name.ok_or("legacy server path requires artifact file name")?;
            Ok((
                id,
                name,
                NvidiaArtifactLocation::Official {
                    host: NvidiaDownloadHost::International,
                    path: path.clone(),
                },
            ))
        }
        NvidiaInstallerSource::LocalInstaller(source) => local_source(source, supplied_name, id),
    }
}

fn official_source(
    url: &str,
    supplied_name: Option<String>,
    id: String,
) -> Result<(String, String, NvidiaArtifactLocation), String> {
    const PREFIX: &str = "https://international.download.nvidia.com/";
    let suffix = url
        .strip_prefix(PREFIX)
        .filter(|value| !value.is_empty())
        .ok_or("official NVIDIA URL must use the fixed HTTPS download authority")?;
    if suffix.contains(['?', '#', '\\']) {
        return Err("official NVIDIA URL contains unsupported routing syntax".into());
    }
    let name = suffix
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or("official NVIDIA URL has no installer leaf")?
        .to_owned();
    if supplied_name.as_ref().is_some_and(|value| value != &name) {
        return Err("deprecated artifact file name conflicts with official URL".into());
    }
    Ok((
        id,
        name,
        NvidiaArtifactLocation::Official {
            host: NvidiaDownloadHost::International,
            path: format!("/{suffix}"),
        },
    ))
}

fn local_source(
    source: &std::path::Path,
    supplied_name: Option<String>,
    id: String,
) -> Result<(String, String, NvidiaArtifactLocation), String> {
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("local NVIDIA installer has no Unicode file leaf")?
        .to_owned();
    if supplied_name.as_ref().is_some_and(|value| value != &name) {
        return Err("deprecated artifact file name conflicts with local installer".into());
    }
    Ok((id, name.clone(), NvidiaArtifactLocation::LocalLeaf(name)))
}

pub(super) fn stage_local_installer(
    source: &std::path::Path,
    trusted: &TrustedWorkDir,
    leaf: &str,
) -> Result<(), String> {
    let source_metadata = std::fs::symlink_metadata(source)
        .map_err(|error| format!("inspect local NVIDIA installer: {error}"))?;
    if !source_metadata.is_file()
        || std::os::windows::fs::MetadataExt::file_attributes(&source_metadata)
            & FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    {
        return Err("local NVIDIA installer must be a regular non-symlink file".into());
    }
    let _artifact_directory = trusted.driver_artifact_directory_handle()?;
    let destination = trusted.path().join("driver-artifacts").join(leaf);
    if source == destination.as_path() {
        return Ok(());
    }
    match std::fs::symlink_metadata(&destination) {
        Ok(metadata)
            if metadata.is_file()
                && std::os::windows::fs::MetadataExt::file_attributes(&metadata)
                    & FILE_ATTRIBUTE_REPARSE_POINT
                    == 0 =>
        {
            std::fs::remove_file(&destination)
                .map_err(|error| format!("replace staged NVIDIA installer: {error}"))?;
        }
        Ok(_) => {
            return Err(
                "staged NVIDIA installer is not an ordinary file in the trusted root".into(),
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("inspect staged NVIDIA installer: {error}")),
    }
    let result = copy_local_installer(source, &destination);
    if result.is_err() {
        let _ = std::fs::remove_file(&destination);
    }
    result
}

fn copy_local_installer(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    use std::{
        fs::OpenOptions,
        io::{Read, Write},
    };

    let mut input = std::fs::File::open(source)
        .map_err(|error| format!("open local NVIDIA installer: {error}"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("copy local NVIDIA installer into trusted root: {error}"))?;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("read local NVIDIA installer: {error}"))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| format!("write trusted NVIDIA installer: {error}"))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("flush trusted NVIDIA installer: {error}"))
}
