use std::{fs, path::Path};

use frametime_domain::driver::Sha256Digest;
use sha2::{Digest, Sha256};

use super::{PreparedNvidiaPackageManifest, collect_manifest_files};

pub(crate) fn verify_prepared_nvidia_package(
    root: &Path,
) -> Result<PreparedNvidiaPackageManifest, String> {
    let bytes = fs::read(root.join("frametime-package.json"))
        .map_err(|error| format!("read prepared NVIDIA manifest: {error}"))?;
    let manifest: PreparedNvidiaPackageManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse prepared NVIDIA manifest: {error}"))?;
    if manifest.schema != "frametime.nvidia-package/v1" {
        return Err("prepared NVIDIA manifest schema is unsupported".into());
    }
    let actual = collect_manifest_files(root, &["frametime-package.json"])?;
    if actual != manifest.files {
        return Err("prepared NVIDIA package bytes differ from the build manifest".into());
    }
    if !root.join("setup.exe").is_file() || !root.join("setup.cfg").is_file() {
        return Err("prepared NVIDIA package lacks setup.exe or setup.cfg".into());
    }
    Ok(manifest)
}

pub(crate) fn prepared_nvidia_package_digest(
    manifest: &PreparedNvidiaPackageManifest,
) -> Result<Sha256Digest, String> {
    let canonical = serde_json::to_vec(manifest)
        .map_err(|error| format!("serialize prepared NVIDIA manifest: {error}"))?;
    Sha256Digest::parse(format!("{:x}", Sha256::digest(canonical)))
        .map_err(|error| error.to_string())
}
