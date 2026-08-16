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
            Self::LocalPath => {
                "local package path has no repository-approved signer/hash policy".into()
            }
            Self::LocalArchive => {
                "local package archive has no repository-approved signer/hash policy".into()
            }
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
    // Priority: package_root > archive > url > driver-index > synthetic
    if let Some(ref root) = opts.package_root {
        if !root.is_dir() {
            return Err(InstallError::PackageMissing(root.clone()));
        }
        messages.push(format!("package-source: local ({})", root.display()));
        return Ok(AcquiredPackage {
            root: root.clone(),
            synthetic: false,
            source_label: "local".into(),
            trust: PackageTrust::LocalPath,
        });
    }

    if let Some(ref arch) = opts.package_archive {
        if !arch.is_file() {
            return Err(InstallError::Other(format!(
                "package archive not found: {}",
                arch.display()
            )));
        }
        let dest = work.join("extracted-package");
        note(
            log,
            "S1-Acquire",
            &format!("Extracting archive {}", arch.display()),
        );
        archive::extract_with_helpers(arch, &dest)?;
        // If extract put a single top-level folder with setup.exe, use it
        let root = find_package_root(&dest);
        messages.push(format!(
            "package-source: archive ({}) -> {}",
            arch.display(),
            root.display()
        ));
        return Ok(AcquiredPackage {
            root,
            synthetic: false,
            source_label: "archive".into(),
            trust: PackageTrust::LocalArchive,
        });
    }

    if let Some(ref url) = opts.package_url {
        let dest_file = work.join("downloaded-package.bin");
        note(log, "S1-Acquire", &format!("Downloading {url}"));
        let verified_pin =
            download::download_https(url, &dest_file, opts.package_sha256.as_deref())?;
        let dest = work.join("extracted-package");
        // Try zip first, then helpers
        if dest_file
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("zip"))
            .unwrap_or(false)
            || is_zip_file(&dest_file)
        {
            archive::extract_zip(&dest_file, &dest)?;
        } else if let Err(error) = archive::extract_with_helpers(&dest_file, &dest) {
            fallback_to_raw_setup(&dest_file, &dest, log, "download", &error)?;
        }
        let root = find_package_root(&dest);
        messages.push(format!("package-source: download ({url})"));
        let trust = match verified_pin {
            Some(sha256) => PackageTrust::RemotePinned {
                url: url.clone(),
                sha256,
            },
            None => PackageTrust::RemoteUnpinned { url: url.clone() },
        };
        return Ok(AcquiredPackage {
            root,
            synthetic: false,
            source_label: "download".into(),
            trust,
        });
    }

    if let Some(ref index) = opts.driver_index {
        let resolved = download::resolve_index_url(index, opts.driver_index_id.as_deref())?;
        let dest_file = work.join(&resolved.name);
        note(
            log,
            "S1-Acquire",
            &format!("Downloading from driver-index: {}", resolved.url),
        );
        let verified_pin =
            download::download_https(&resolved.url, &dest_file, resolved.sha256.as_deref())?;
        let dest = work.join("extracted-package");
        if is_zip_file(&dest_file) {
            archive::extract_zip(&dest_file, &dest)?;
        } else if let Err(error) = archive::extract_with_helpers(&dest_file, &dest) {
            fallback_to_raw_setup(&dest_file, &dest, log, "driver-index", &error)?;
        }
        let root = find_package_root(&dest);
        messages.push(format!("package-source: driver-index ({})", resolved.url));
        let trust = match verified_pin {
            Some(sha256) => PackageTrust::RemotePinned {
                url: resolved.url,
                sha256,
            },
            None => PackageTrust::RemoteUnpinned { url: resolved.url },
        };
        return Ok(AcquiredPackage {
            root,
            synthetic: false,
            source_label: "driver-index".into(),
            trust,
        });
    }

    // Synthetic
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

fn find_package_root(dest: &Path) -> PathBuf {
    if dest.join("setup.exe").is_file() {
        return dest.to_path_buf();
    }
    if let Ok(rd) = fs::read_dir(dest) {
        let dirs: Vec<_> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        if dirs.len() == 1 && dirs[0].join("setup.exe").is_file() {
            return dirs[0].clone();
        }
        for d in dirs {
            if d.join("setup.exe").is_file() {
                return d;
            }
        }
    }
    dest.to_path_buf()
}

fn is_zip_file(path: &Path) -> bool {
    if let Ok(bytes) = fs::read(path) {
        return bytes.len() >= 4 && bytes[0] == b'P' && bytes[1] == b'K';
    }
    false
}

pub(crate) fn import_selection_file(path: &Path) -> Result<Vec<String>, InstallError> {
    let text = fs::read_to_string(path)?;
    // Try array of strings
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(&text) {
        return Ok(arr);
    }
    // Try { "selected": [...] }
    #[derive(serde::Deserialize)]
    struct Sel {
        #[serde(default)]
        selected: Vec<String>,
        #[serde(default)]
        components: Vec<String>,
    }
    let s: Sel = serde_json::from_str(&text)?;
    let mut out = s.selected;
    out.extend(s.components);
    Ok(out)
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
            url: "https://vendor.invalid/driver.exe".into(),
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
