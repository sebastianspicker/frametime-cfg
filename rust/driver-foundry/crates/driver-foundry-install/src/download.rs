//! HTTPS download, digest pinning, and driver-index helpers.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::InstallError;

const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// A resolved remote package together with the digest that authorizes its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDownload {
    pub url: String,
    pub name: PathBuf,
    pub sha256: Option<String>,
}

/// Reject URLs that could send package data over an unauthenticated transport.
pub(crate) fn require_https_url(raw_url: &str) -> Result<(), InstallError> {
    let url = Url::parse(raw_url).map_err(|error| {
        InstallError::Other(format!("invalid package URL {raw_url:?}: {error}"))
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(InstallError::Other(format!(
            "package URL must be an absolute HTTPS URL without credentials: {raw_url}"
        )));
    }
    Ok(())
}

/// Reject HTTP redirects instead of following a destination whose scheme or host was not pinned.
pub(crate) fn reject_redirect_status(status: u16, url: &str) -> Result<(), InstallError> {
    if (300..400).contains(&status) {
        return Err(InstallError::Other(format!(
            "package download redirect refused for {url}; use the final HTTPS URL and pin its SHA-256"
        )));
    }
    Ok(())
}

pub(crate) fn normalize_sha256(expected: &str) -> Result<String, InstallError> {
    let trimmed = expected.trim();
    if trimmed.len() != 64 || !trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InstallError::Other(
            "SHA-256 pin must be exactly 64 hexadecimal characters".into(),
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, InstallError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn verify_sha256_file(path: &Path, expected: &str) -> Result<String, InstallError> {
    let expected = normalize_sha256(expected)?;
    let actual = sha256_file(path)?;
    if actual != expected {
        return Err(InstallError::Other(format!(
            "download SHA-256 mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(actual)
}

/// Download a URL to `dest` through HTTPS, refusing every redirect.
///
/// The optional SHA-256 pin is verified before this function returns. Without a pin the caller
/// may use the bytes for dry-run planning only; it must not authorize a process launch.
pub(crate) fn download_https(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
) -> Result<Option<String>, InstallError> {
    require_https_url(url)?;
    if let Some(expected) = expected_sha256 {
        normalize_sha256(expected)?;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(120))
        .redirects(0)
        .build();
    let response = agent.get(url).call();
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(status, _)) => {
            reject_redirect_status(status, url)?;
            return Err(InstallError::Other(format!(
                "download {url}: HTTP {status}"
            )));
        }
        Err(error) => return Err(InstallError::Other(format!("download {url}: {error}"))),
    };
    reject_redirect_status(response.status(), url)?;
    validate_content_length(response.header("Content-Length"), url)?;

    let mut reader = response.into_reader().take(MAX_PACKAGE_BYTES + 1);
    let mut file = crate::copy::create_new_output(dest)?;
    let written = std::io::copy(&mut reader, &mut file)?;
    file.flush()?;
    if written > MAX_PACKAGE_BYTES {
        drop(file);
        let _ = fs::remove_file(dest);
        return Err(InstallError::Other(format!(
            "download {url} exceeds maximum package size of {MAX_PACKAGE_BYTES} bytes"
        )));
    }

    match expected_sha256 {
        Some(expected) => match verify_sha256_file(dest, expected) {
            Ok(actual) => Ok(Some(actual)),
            Err(error) => {
                let _ = fs::remove_file(dest);
                Err(error)
            }
        },
        None => Ok(None),
    }
}

fn validate_content_length(content_length: Option<&str>, url: &str) -> Result<(), InstallError> {
    let Some(length) = content_length else {
        return Ok(());
    };
    let length = length.parse::<u64>().map_err(|error| {
        InstallError::Other(format!("invalid Content-Length from {url}: {error}"))
    })?;
    if length > MAX_PACKAGE_BYTES {
        return Err(InstallError::Other(format!(
            "download {url} exceeds maximum package size of {MAX_PACKAGE_BYTES} bytes"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriverIndexEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriverIndexFile {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub drivers: Vec<DriverIndexEntry>,
    /// Alternate keys used by shipped and older indexes.
    #[serde(default)]
    pub packages: Vec<DriverIndexEntry>,
    #[serde(default)]
    pub entries: Vec<DriverIndexEntry>,
}

/// Load driver-index.v1.json if present.
pub fn load_driver_index(path: &Path) -> Result<Vec<DriverIndexEntry>, InstallError> {
    if !path.is_file() {
        return Err(InstallError::Other(format!(
            "driver-index not found: {}",
            path.display()
        )));
    }
    let text = fs::read_to_string(path)?;
    let file: DriverIndexFile = serde_json::from_str(&text)?;
    let mut entries = file.drivers;
    entries.extend(file.packages);
    entries.extend(file.entries);
    let _ = file.schema;
    Ok(entries)
}

/// Resolve a download URL from driver-index by id or first matching channel.
pub fn resolve_index_url(
    index_path: &Path,
    package_id: Option<&str>,
) -> Result<ResolvedDownload, InstallError> {
    let entries = load_driver_index(index_path)?;
    if entries.is_empty() {
        return Err(InstallError::Other("driver-index has no entries".into()));
    }
    let entry = if let Some(id) = package_id {
        entries
            .iter()
            .find(|entry| entry.id.eq_ignore_ascii_case(id) || entry.name.eq_ignore_ascii_case(id))
            .ok_or_else(|| InstallError::Other(format!("driver-index id not found: {id}")))?
    } else {
        &entries[0]
    };
    require_https_url(&entry.url)?;
    let name = if !entry.filename.trim().is_empty() {
        safe_filename(&entry.filename)
    } else if entry.id.is_empty() {
        PathBuf::from("driver-package.bin")
    } else {
        PathBuf::from(format!("{}.exe", entry.id.replace(['/', '\\', ' '], "_")))
    };
    let sha256 = entry.sha256.as_deref().map(normalize_sha256).transpose()?;
    Ok(ResolvedDownload {
        url: entry.url.clone(),
        name,
        sha256,
    })
}

fn safe_filename(filename: &str) -> PathBuf {
    Path::new(filename)
        .file_name()
        .filter(|name| !name.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("driver-package.bin"))
}

/// Write a minimal driver-index fixture for tests.
#[cfg(test)]
pub(crate) fn write_test_index(path: &Path, url: &str) -> Result<(), InstallError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::json!({
        "schema": "driver-foundry.driver-index/v1",
        "drivers": [{
            "id": "test-driver",
            "name": "Test Driver",
            "version": "1.0",
            "url": url,
            "channel": "studio",
            "sha256": "0".repeat(64)
        }]
    });
    fs::write(path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("dfoundry-download-{name}-{nonce}"))
    }

    #[test]
    fn https_policy_rejects_http_and_credentials() {
        assert!(require_https_url("http://example.invalid/driver.exe").is_err());
        assert!(require_https_url("https://user:password@example.invalid/driver.exe").is_err());
        assert!(require_https_url("https://example.invalid/driver.exe").is_ok());
    }

    #[test]
    fn redirect_policy_rejects_downgrades_before_following() {
        let error = reject_redirect_status(302, "https://example.invalid/driver.exe").unwrap_err();
        assert!(error.to_string().contains("redirect refused"));
        assert!(reject_redirect_status(200, "https://example.invalid/driver.exe").is_ok());
    }

    #[test]
    fn rejects_oversized_or_invalid_download_length_before_writing() {
        assert!(validate_content_length(Some("invalid"), "https://example.invalid/a").is_err());
        assert!(validate_content_length(
            Some(&(MAX_PACKAGE_BYTES + 1).to_string()),
            "https://example.invalid/a"
        )
        .is_err());
        assert!(validate_content_length(Some("0"), "https://example.invalid/a").is_ok());
    }

    #[test]
    fn digest_fixture_accepts_expected_bytes_and_rejects_wrong_pin() {
        let path = unique_path("digest.bin");
        fs::write(&path, b"fixture package bytes").unwrap();
        let actual = sha256_file(&path).unwrap();
        assert_eq!(verify_sha256_file(&path, &actual).unwrap(), actual);
        let error = verify_sha256_file(&path, &"0".repeat(64)).unwrap_err();
        assert!(error.to_string().contains("mismatch"));
        assert!(normalize_sha256("not-a-digest").is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_index_preserves_pin_and_rejects_http_entry() {
        let dir = unique_path("index");
        let path = dir.join("driver-index.v1.json");
        write_test_index(&path, "https://example.com/driver.exe").unwrap();
        let resolved = resolve_index_url(&path, Some("test-driver")).unwrap();
        assert_eq!(resolved.url, "https://example.com/driver.exe");
        assert_eq!(
            resolved.sha256.as_deref(),
            Some("0000000000000000000000000000000000000000000000000000000000000000")
        );
        fs::write(
            &path,
            r#"{"entries":[{"id":"http","url":"http://example.invalid/a.exe"}]}"#,
        )
        .unwrap();
        assert!(resolve_index_url(&path, Some("http")).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
