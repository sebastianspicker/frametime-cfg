//! Package-source acquisition and selection import.

use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::PackageCatalog;
use crate::fixture::create_synthetic_package;
use crate::pipeline::note;
use crate::{archive, copy, download, InstallError, InstallOptions};

/// Provenance carried from acquisition to the live-install gate.
///
/// A filename, PE prefix, and caller-supplied digest are descriptive data, not authorization.
/// A platform signer verifier and authenticated signer policy are required before this crate may
/// execute a downloaded installer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackageTrust {
    SyntheticFixture,
    LocalPath,
    LocalArchive,
    RemoteUnpinned { url: String },
    RemotePinned { url: String, sha256: String },
}

pub(crate) struct AcquiredPackage {
    pub(crate) root: PathBuf,
    pub(crate) synthetic: bool,
    pub(crate) source_label: String,
    pub(crate) trust: PackageTrust,
}

impl PackageTrust {
    fn description(&self) -> String {
        match self {
            Self::SyntheticFixture => "synthetic fixture".into(),
            Self::LocalPath => "local package path has no repository-approved signer/hash policy".into(),
            Self::LocalArchive => "local package archive has no repository-approved signer/hash policy".into(),
            Self::RemoteUnpinned { url } => format!("HTTPS package has no SHA-256 pin: {url}"),
            Self::RemotePinned { url, sha256 } => format!(
                "verified HTTPS SHA-256 pin for {url} ({sha256}); platform signer verification is unavailable"
            ),
        }
    }

    pub(crate) fn authorize_live_install(&self) -> Result<(), InstallError> {
        Err(InstallError::UntrustedInstaller(format!(
            "{}. Force-install is disabled until Driver Foundry ships a platform signer verifier and an authenticated vendor signer policy; a URL and SHA-256 supplied by the same caller are not independent authorization.",
            self.description()
        )))
    }
}

pub(crate) fn acquire_package(
    opts: &InstallOptions,
    work: &Path,
    catalog: &PackageCatalog,
    log: &mut Vec<String>,
    messages: &mut Vec<String>,
) -> Result<AcquiredPackage, InstallError> {
    // Priority: package_root > archive > url > driver-index > synthetic.
    if let Some(root) = opts.package_root.as_ref() {
        return acquire_local(root, messages);
    }
    if let Some(archive_path) = opts.package_archive.as_ref() {
        return acquire_archive(archive_path, work, log, messages);
    }
    if let Some(url) = opts.package_url.as_ref() {
        return acquire_remote_url(url, opts.package_sha256.as_deref(), work, log, messages);
    }
    if let Some(index) = opts.driver_index.as_ref() {
        return acquire_driver_index(index, opts.driver_index_id.as_deref(), work, log, messages);
    }
    acquire_synthetic(work, catalog, messages)
}

fn acquire_local(root: &Path, messages: &mut Vec<String>) -> Result<AcquiredPackage, InstallError> {
    if !root.is_dir() {
        return Err(InstallError::PackageMissing(root.to_path_buf()));
    }
    messages.push(format!("package-source: local ({})", root.display()));
    Ok(AcquiredPackage {
        root: root.to_path_buf(),
        synthetic: false,
        source_label: "local".into(),
        trust: PackageTrust::LocalPath,
    })
}

fn acquire_archive(
    archive_path: &Path,
    work: &Path,
    log: &mut Vec<String>,
    messages: &mut Vec<String>,
) -> Result<AcquiredPackage, InstallError> {
    if !archive_path.is_file() {
        return Err(InstallError::Other(format!(
            "package archive not found: {}",
            archive_path.display()
        )));
    }
    let destination = work.join("extracted-package");
    note(
        log,
        "S1-Acquire",
        &format!("Extracting archive {}", archive_path.display()),
    );
    archive::extract_with_helpers(archive_path, &destination)?;
    let root = find_package_root(&destination);
    messages.push(format!(
        "package-source: archive ({}) -> {}",
        archive_path.display(),
        root.display()
    ));
    Ok(AcquiredPackage {
        root,
        synthetic: false,
        source_label: "archive".into(),
        trust: PackageTrust::LocalArchive,
    })
}

fn acquire_remote_url(
    url: &str,
    pin: Option<&str>,
    work: &Path,
    log: &mut Vec<String>,
    messages: &mut Vec<String>,
) -> Result<AcquiredPackage, InstallError> {
    let downloaded_file = work.join("downloaded-package.bin");
    note(log, "S1-Acquire", &format!("Downloading {url}"));
    let verified_pin = download::download_https(url, &downloaded_file, pin)?;
    let root = extract_download(&downloaded_file, work, log, "download")?;
    messages.push(format!("package-source: download ({url})"));
    Ok(remote_package(
        root,
        "download",
        url.to_string(),
        verified_pin,
    ))
}

fn acquire_driver_index(
    index: &Path,
    package_id: Option<&str>,
    work: &Path,
    log: &mut Vec<String>,
    messages: &mut Vec<String>,
) -> Result<AcquiredPackage, InstallError> {
    let resolved = download::resolve_index_url(index, package_id)?;
    let downloaded_file = work.join(&resolved.name);
    note(
        log,
        "S1-Acquire",
        &format!("Downloading from driver-index: {}", resolved.url),
    );
    let verified_pin =
        download::download_https(&resolved.url, &downloaded_file, resolved.sha256.as_deref())?;
    let root = extract_download(&downloaded_file, work, log, "driver-index")?;
    messages.push(format!("package-source: driver-index ({})", resolved.url));
    Ok(remote_package(
        root,
        "driver-index",
        resolved.url,
        verified_pin,
    ))
}

fn extract_download(
    downloaded_file: &Path,
    work: &Path,
    log: &mut Vec<String>,
    source_kind: &str,
) -> Result<PathBuf, InstallError> {
    let destination = work.join("extracted-package");
    if is_zip_file(downloaded_file) {
        archive::extract_zip(downloaded_file, &destination)?;
    } else if let Err(error) = archive::extract_with_helpers(downloaded_file, &destination) {
        fallback_to_raw_setup(downloaded_file, &destination, log, source_kind, &error)?;
    }
    Ok(find_package_root(&destination))
}

fn remote_package(
    root: PathBuf,
    source_label: &str,
    url: String,
    verified_pin: Option<String>,
) -> AcquiredPackage {
    let trust = match verified_pin {
        Some(sha256) => PackageTrust::RemotePinned { url, sha256 },
        None => PackageTrust::RemoteUnpinned { url },
    };
    AcquiredPackage {
        root,
        synthetic: false,
        source_label: source_label.into(),
        trust,
    }
}

fn acquire_synthetic(
    work: &Path,
    catalog: &PackageCatalog,
    messages: &mut Vec<String>,
) -> Result<AcquiredPackage, InstallError> {
    let root = work.join("fixture-package");
    create_synthetic_package(&root, catalog.packages.keys().cloned())?;
    messages.push(format!(
        "package-source: local synthetic fixture ({})",
        root.display()
    ));
    Ok(AcquiredPackage {
        root,
        synthetic: true,
        source_label: "synthetic-fixture".into(),
        trust: PackageTrust::SyntheticFixture,
    })
}

/// Preserve raw-installer support when an optional extraction helper cannot handle a download.
/// The fallback is visible in the stage log, which becomes part of the result messages.
fn fallback_to_raw_setup(
    downloaded_file: &Path,
    destination: &Path,
    log: &mut Vec<String>,
    source_kind: &str,
    extraction_error: &InstallError,
) -> Result<(), InstallError> {
    note(
        log,
        "S1-Acquire",
        &format!(
            "{source_kind} extraction unavailable/failed ({extraction_error}); retaining raw download as setup.exe"
        ),
    );
    copy::create_new_directory(destination)?;
    fs::copy(downloaded_file, destination.join("setup.exe"))?;
    Ok(())
}

fn find_package_root(destination: &Path) -> PathBuf {
    if destination.join("setup.exe").is_file() {
        return destination.to_path_buf();
    }
    let Ok(entries) = fs::read_dir(destination) else {
        return destination.to_path_buf();
    };
    let directories: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    if directories.len() == 1 && directories[0].join("setup.exe").is_file() {
        return directories[0].clone();
    }
    directories
        .into_iter()
        .find(|path| path.join("setup.exe").is_file())
        .unwrap_or_else(|| destination.to_path_buf())
}

fn is_zip_file(path: &Path) -> bool {
    fs::read(path).is_ok_and(|bytes| bytes.len() >= 4 && bytes[0] == b'P' && bytes[1] == b'K')
}

pub(crate) fn import_selection_file(path: &Path) -> Result<Vec<String>, InstallError> {
    let text = fs::read_to_string(path)?;
    if let Ok(array) = serde_json::from_str::<Vec<String>>(&text) {
        return Ok(array);
    }
    #[derive(serde::Deserialize)]
    struct SelectionFile {
        #[serde(default)]
        selected: Vec<String>,
        #[serde(default)]
        components: Vec<String>,
    }
    let selection: SelectionFile = serde_json::from_str(&text)?;
    Ok(selection
        .selected
        .into_iter()
        .chain(selection.components)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_pin_without_platform_signer_refuses_live_launch() {
        let error = PackageTrust::RemotePinned {
            url: "https://vendor.invalid/driver.exe".into(),
            sha256: "a".repeat(64),
        }
        .authorize_live_install()
        .unwrap_err();
        assert!(error.to_string().contains("platform signer verifier"));
        assert!(PackageTrust::RemoteUnpinned {
            url: "https://vendor.invalid/driver.exe".into()
        }
        .authorize_live_install()
        .is_err());
    }

    #[test]
    fn renamed_mz_bytes_and_local_sources_never_authorize_launch() {
        for trust in [
            PackageTrust::SyntheticFixture,
            PackageTrust::LocalPath,
            PackageTrust::LocalArchive,
        ] {
            let error = trust.authorize_live_install().unwrap_err();
            assert!(
                error.to_string().contains("dry-run-only")
                    || error.to_string().contains("synthetic")
                    || error.to_string().contains("platform signer verifier")
            );
        }
    }
}
