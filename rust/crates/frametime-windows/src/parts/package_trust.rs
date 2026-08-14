// Authenticated portable-package capability.  A package is only useful after
// its fixed payload, manifest, catalog, publisher pin, and retained handles
// have all been proven together.

pub const PACKAGE_MANIFEST_NAME: &str = "package.manifest.json";
pub const PACKAGE_CATALOG_NAME: &str = "package.cat";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    path: String,
    size: u64,
    sha256: String,
}

impl PackageFile {
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    version: String,
    files: Vec<PackageFile>,
}

impl PackageManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        package_trust_contract::parse_manifest(bytes)
    }
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
    #[must_use]
    pub fn files(&self) -> &[PackageFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublisherPinConfiguration {
    Configured(Vec<String>),
    Unconfigured,
}

impl PublisherPinConfiguration {
    /// Parse one or two semicolon-separated, SHA-256 SPKI pins.
    pub fn parse(value: &str) -> Result<Self, String> {
        package_trust_contract::parse_publisher_pins(value).map(Self::Configured)
    }

    #[must_use]
    pub fn compiled() -> Self {
        option_env!("FRAMETIME_PUBLISHER_SPKI_SHA256")
            .and_then(|value| Self::parse(value).ok())
            .unwrap_or(Self::Unconfigured)
    }
}

/// A non-cloneable, handle-retained executable role within an authenticated
/// package.  It cannot be constructed from an arbitrary path by callers.
#[derive(Debug)]
pub struct AuthenticatedExecutable {
    path: PathBuf,
    #[cfg(windows)]
    _retained: package_trust_windows::RetainedFile,
}

impl AuthenticatedExecutable {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A non-cloneable capability for the exact package root and every audited
/// package file.  Its private retained objects intentionally outlive launch.
#[derive(Debug)]
pub struct AuthenticatedPackage {
    root: PathBuf,
    manifest: PackageManifest,
    gui: AuthenticatedExecutable,
    cli: AuthenticatedExecutable,
    #[cfg(windows)]
    payload: std::collections::BTreeMap<String, package_trust_windows::RetainedFile>,
    #[cfg(windows)]
    _retained: Vec<package_trust_windows::RetainedFile>,
}

impl AuthenticatedPackage {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[must_use]
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
    #[must_use]
    pub fn gui(&self) -> &AuthenticatedExecutable {
        &self.gui
    }
    #[must_use]
    pub fn cli(&self) -> &AuthenticatedExecutable {
        &self.cli
    }

    #[cfg(windows)]
    pub(crate) fn retained_payload_handle(
        &self,
        path: &str,
    ) -> Result<windows::Win32::Foundation::HANDLE, String> {
        if path.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME) {
            return Ok(self.cli._retained.handle);
        }
        self.payload
            .get(&path.to_ascii_lowercase())
            .map(|file| file.handle)
            .ok_or_else(|| format!("authenticated package omits runtime payload: {path}"))
    }
}

/// Authenticate the package containing the current image.  Absence or a
/// malformed compile-time publisher pin is a hard failure, never a fallback.
pub fn authenticate_current_package() -> Result<AuthenticatedPackage, String> {
    ensure_release_feature_set()?;
    let pins = match PublisherPinConfiguration::compiled() {
        PublisherPinConfiguration::Configured(pins) => pins,
        PublisherPinConfiguration::Unconfigured => {
            return Err("publisher SPKI pin is unconfigured or malformed".into());
        }
    };
    #[cfg(windows)]
    {
        let image =
            std::env::current_exe().map_err(|error| format!("locate package image: {error}"))?;
        let root = image
            .parent()
            .ok_or("current package image has no parent directory")?;
        package_trust_windows::authenticate(root, &pins, &image)
    }
    #[cfg(not(windows))]
    {
        let _ = pins;
        Err("package authentication requires supported Windows x64".into())
    }
}

fn ensure_release_feature_set() -> Result<(), String> {
    if crate::shader_cache_delete_qualified() {
        return Err(
            "qualification-only shader-cache mutation is enabled; this build cannot authenticate as a release package"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod release_feature_tests {
    use super::*;

    #[test]
    fn qualification_only_builds_cannot_mint_package_authority() {
        assert_eq!(
            ensure_release_feature_set().is_ok(),
            !cfg!(feature = "qualified-shader-cache-delete")
        );
    }
}

mod package_trust_contract;
#[cfg(windows)]
mod package_catalog;
#[cfg(windows)]
mod package_trust_windows;
