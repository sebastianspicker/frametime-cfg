//! Isolated export-only NVIDIA lab package builder.
//!
//! This module has no access to driver-transaction, reboot, installation, or
//! recovery APIs. It accepts only a fresh output path and an explicitly
//! selected certificate already present in the Windows certificate store.

#[cfg(any(test, windows))]
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaLabBuildRequest {
    pub output: PathBuf,
    pub deep_inf: bool,
    pub test_certificate_sha256: String,
    pub acknowledge_unqualified_driver: bool,
}

impl NvidiaLabBuildRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.output.as_os_str().is_empty() || self.output.exists() {
            return Err("lab output path must be unused".into());
        }
        if !self.deep_inf {
            return Err("lab build requires --deep-inf".into());
        }
        if !self.acknowledge_unqualified_driver {
            return Err("lab build requires --acknowledge-unqualified-driver".into());
        }
        if self.test_certificate_sha256.len() != 64
            || !self
                .test_certificate_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(
                "test certificate SHA-256 must be exactly 64 hexadecimal characters".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaLabBuildManifest {
    pub schema: String,
    pub unqualified: bool,
    pub export_only: bool,
    pub certificate_sha256: String,
    pub inf2cat: String,
    pub signtool: String,
    pub modified_infs: Vec<String>,
    pub catalogs: Vec<String>,
    pub files: Vec<super::NvidiaPackageFile>,
}

#[cfg(windows)]
pub fn build_nvidia_lab(request: &NvidiaLabBuildRequest) -> Result<NvidiaLabBuildManifest, String> {
    use std::{fs, process::Command};

    request.validate()?;
    let source = Path::new(crate::WINDOWS_WORK_DIR).join("driver-package");
    if !source.is_dir() {
        return Err(
            "lab build requires a prepared package at the fixed protected driver-package path"
                .into(),
        );
    }
    let (inf2cat, signtool) = locate_authenticated_wdk_tools()?;
    let certificate_sha1 = certificate_sha1_for_sha256(&request.test_certificate_sha256)?;
    super::package_builder::copy_vendor_tree(&source, &request.output)?;
    let modified_infs = rewrite_display_infs(&request.output)?;
    if modified_infs.is_empty() {
        return Err("deep-INF lab build found no Display.* INF files".into());
    }
    for catalog in collect_catalogs(&request.output)? {
        fs::remove_file(&catalog)
            .map_err(|error| format!("remove invalidated vendor catalog: {error}"))?;
    }

    let inf2cat_status = Command::new(&inf2cat)
        .arg(format!("/driver:{}", request.output.display()))
        .arg("/os:10_X64,Server10_X64")
        .status()
        .map_err(|error| format!("launch authenticated Inf2Cat: {error}"))?;
    if !inf2cat_status.success() {
        return Err("authenticated Inf2Cat returned a nonzero status".into());
    }

    let catalogs = collect_catalogs(&request.output)?;
    if catalogs.is_empty() {
        return Err("Inf2Cat produced no catalog files".into());
    }
    for catalog in &catalogs {
        let status = Command::new(&signtool)
            .args(["sign", "/fd", "SHA256", "/sha1", &certificate_sha1])
            .arg(catalog)
            .status()
            .map_err(|error| format!("launch authenticated SignTool: {error}"))?;
        if !status.success() {
            return Err(format!("SignTool failed for {}", catalog.display()));
        }
    }

    let relative_catalogs = catalogs
        .iter()
        .map(|catalog| {
            catalog
                .strip_prefix(&request.output)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .map_err(|_| "lab catalog escaped output".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut manifest = NvidiaLabBuildManifest {
        schema: "frametime.nvidia-lab-package/v1".into(),
        unqualified: true,
        export_only: true,
        certificate_sha256: request.test_certificate_sha256.to_ascii_lowercase(),
        inf2cat: inf2cat.to_string_lossy().into_owned(),
        signtool: signtool.to_string_lossy().into_owned(),
        modified_infs,
        catalogs: relative_catalogs,
        files: super::package_builder::collect_manifest_files(
            &request.output,
            &["frametime-lab-package.json"],
        )?,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize lab manifest: {error}"))?;
    fs::write(request.output.join("frametime-lab-package.json"), bytes)
        .map_err(|error| format!("write lab manifest: {error}"))?;
    manifest.files = super::package_builder::collect_manifest_files(&request.output, &[])?;
    Ok(manifest)
}

#[cfg(any(test, windows))]
fn rewrite_display_infs(root: &Path) -> Result<Vec<String>, String> {
    use std::fs;

    const NEEDLES: &[&str] = &[
        "telemetry",
        "nvtelemetry",
        "gfexperience",
        "nvcontainer",
        "appx",
        "nvapp",
    ];
    let mut directories = fs::read_dir(root)
        .map_err(|error| format!("read lab package: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read lab package entry: {error}"))?;
    directories.sort_by_key(std::fs::DirEntry::file_name);
    let mut modified = Vec::new();
    for directory in directories {
        let name = directory.file_name().to_string_lossy().into_owned();
        if !directory
            .file_type()
            .map_err(|error| format!("inspect lab component: {error}"))?
            .is_dir()
            || !name.to_ascii_lowercase().starts_with("display.")
        {
            continue;
        }
        let mut entries = fs::read_dir(directory.path())
            .map_err(|error| format!("read lab display component: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read lab display entry: {error}"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|error| format!("inspect lab INF candidate: {error}"))?
                .is_file()
                || !path
                    .extension()
                    .is_some_and(|value| value.eq_ignore_ascii_case("inf"))
            {
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| format!("read lab INF: {error}"))?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("lab INF exceeds the 8 MiB rewrite bound".into());
            }
            let (text, utf16) = decode_inf(&bytes)?;
            let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
            let mut lines = Vec::new();
            let mut changes = 0_usize;
            for line in text.lines() {
                let lower = line.to_ascii_lowercase();
                if !line.trim_start().starts_with(';')
                    && NEEDLES.iter().any(|needle| lower.contains(needle))
                {
                    lines.push(format!("; frametime-lab-disabled {line}"));
                    changes += 1;
                } else {
                    lines.push(line.to_owned());
                }
            }
            lines.push("; frametime lab deep-INF export; unqualified".into());
            let rewritten = format!("{}{newline}", lines.join(newline));
            fs::write(&path, encode_inf(&rewritten, utf16))
                .map_err(|error| format!("write lab INF: {error}"))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "lab INF escaped output")?
                .to_string_lossy()
                .replace('\\', "/");
            modified.push(format!("{relative}:{changes}"));
        }
    }
    Ok(modified)
}

#[cfg(any(test, windows))]
fn decode_inf(bytes: &[u8]) -> Result<(String, bool), String> {
    if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        if body.len() % 2 != 0 {
            return Err("UTF-16LE lab INF has an odd byte length".into());
        }
        let units = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>();
        String::from_utf16(&units)
            .map(|text| (text, true))
            .map_err(|_| "lab INF contains invalid UTF-16LE".into())
    } else {
        String::from_utf8(bytes.to_vec())
            .map(|text| (text, false))
            .map_err(|_| "lab INF must be UTF-8 or BOM-marked UTF-16LE".into())
    }
}

#[cfg(any(test, windows))]
fn encode_inf(text: &str, utf16: bool) -> Vec<u8> {
    if utf16 {
        let mut bytes = vec![0xff, 0xfe];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    } else {
        text.as_bytes().to_vec()
    }
}

#[cfg(not(windows))]
pub fn build_nvidia_lab(request: &NvidiaLabBuildRequest) -> Result<NvidiaLabBuildManifest, String> {
    request.validate()?;
    Err("NVIDIA lab building requires authenticated Microsoft WDK tools on Windows".into())
}

#[cfg(windows)]
fn locate_authenticated_wdk_tools() -> Result<(PathBuf, PathBuf), String> {
    use std::fs;

    let program_files =
        std::env::var_os("ProgramFiles(x86)").ok_or("ProgramFiles(x86) is unavailable")?;
    let bin = PathBuf::from(program_files).join("Windows Kits/10/bin");
    let mut versions = fs::read_dir(&bin)
        .map_err(|error| format!("read Windows Kits bin: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read Windows Kits version: {error}"))?;
    versions.sort_by_key(std::fs::DirEntry::file_name);
    versions.reverse();
    for version in versions {
        let x64 = version.path().join("x64");
        let inf2cat = x64.join("Inf2Cat.exe");
        let signtool = x64.join("SignTool.exe");
        if inf2cat.is_file() && signtool.is_file() {
            super::capability::verify_microsoft_signed_tool(&inf2cat)
                .map_err(|error| error.to_string())?;
            super::capability::verify_microsoft_signed_tool(&signtool)
                .map_err(|error| error.to_string())?;
            return Ok((inf2cat, signtool));
        }
    }
    Err("no authenticated x64 Microsoft WDK Inf2Cat and SignTool pair was found".into())
}

#[cfg(windows)]
fn certificate_sha1_for_sha256(expected: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use windows::{
        Win32::Security::Cryptography::{
            CERT_SHA1_HASH_PROP_ID, CertCloseStore, CertEnumCertificatesInStore,
            CertGetCertificateContextProperty, CertOpenSystemStoreW,
        },
        core::w,
    };

    let expected = expected.to_ascii_lowercase();
    let store = unsafe { CertOpenSystemStoreW(None, w!("MY")) }
        .map_err(|error| format!("open current-user certificate store: {error}"))?;
    let result = (|| {
        let mut previous = None;
        loop {
            let context = unsafe { CertEnumCertificatesInStore(store, previous) };
            if context.is_null() {
                break;
            }
            previous = Some(context.cast_const());
            let encoded = unsafe {
                std::slice::from_raw_parts(
                    (*context).pbCertEncoded,
                    usize::try_from((*context).cbCertEncoded)
                        .map_err(|_| "certificate encoding is too large")?,
                )
            };
            if format!("{:x}", Sha256::digest(encoded)) != expected {
                continue;
            }
            let mut bytes = 0_u32;
            unsafe {
                CertGetCertificateContextProperty(context, CERT_SHA1_HASH_PROP_ID, None, &mut bytes)
            }
            .map_err(|error| format!("read certificate SHA-1 size: {error}"))?;
            let mut sha1 = vec![0_u8; bytes as usize];
            unsafe {
                CertGetCertificateContextProperty(
                    context,
                    CERT_SHA1_HASH_PROP_ID,
                    Some(sha1.as_mut_ptr().cast()),
                    &mut bytes,
                )
            }
            .map_err(|error| format!("read certificate SHA-1: {error}"))?;
            return Ok(sha1.iter().map(|byte| format!("{byte:02x}")).collect());
        }
        Err(
            "the explicitly selected test certificate was not found in the current-user store"
                .into(),
        )
    })();
    let _ = unsafe { CertCloseStore(Some(store), 0) };
    result
}

#[cfg(windows)]
fn collect_catalogs(root: &Path) -> Result<Vec<PathBuf>, String> {
    use std::fs;

    fn visit(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in
            fs::read_dir(root).map_err(|error| format!("read lab package directory: {error}"))?
        {
            let path = entry
                .map_err(|error| format!("read lab package entry: {error}"))?
                .path();
            if path.is_dir() {
                visit(&path, out)?;
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cat"))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut catalogs = Vec::new();
    visit(root, &mut catalogs)?;
    catalogs.sort();
    Ok(catalogs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lab_policy_requires_all_three_explicit_guards() {
        let mut request = NvidiaLabBuildRequest {
            output: PathBuf::from("unused-lab-output"),
            deep_inf: false,
            test_certificate_sha256: "ab".repeat(32),
            acknowledge_unqualified_driver: true,
        };
        assert!(request.validate().is_err());
        request.deep_inf = true;
        request.acknowledge_unqualified_driver = false;
        assert!(request.validate().is_err());
        request.acknowledge_unqualified_driver = true;
        request.test_certificate_sha256 = "short".into();
        assert!(request.validate().is_err());
    }

    #[test]
    fn lab_surface_is_export_only() {
        let manifest = NvidiaLabBuildManifest {
            schema: "frametime.nvidia-lab-package/v1".into(),
            unqualified: true,
            export_only: true,
            certificate_sha256: "ab".repeat(32),
            inf2cat: "Inf2Cat.exe".into(),
            signtool: "SignTool.exe".into(),
            modified_infs: vec!["Display.Driver/sample.inf:2".into()],
            catalogs: vec!["Display.Driver/display.cat".into()],
            files: Vec::new(),
        };
        assert!(manifest.unqualified && manifest.export_only);
    }

    #[test]
    fn deep_inf_rewrite_is_bounded_and_marks_utf8_and_utf16_inputs() {
        let temp = tempdir().unwrap();
        let display = temp.path().join("Display.Driver");
        std::fs::create_dir(&display).unwrap();
        std::fs::write(
            display.join("utf8.inf"),
            b"Normal=1\r\nTelemetryService=yes\r\n; Appx stays commented\r\n",
        )
        .unwrap();
        std::fs::write(
            display.join("utf16.inf"),
            encode_inf("NvContainer=1\n", true),
        )
        .unwrap();
        let modified = rewrite_display_infs(temp.path()).unwrap();
        assert_eq!(modified.len(), 2);
        let utf8 = std::fs::read_to_string(display.join("utf8.inf")).unwrap();
        assert!(utf8.contains("frametime-lab-disabled TelemetryService"));
        assert!(utf8.contains("; Appx stays commented"));
        let utf16 = std::fs::read(display.join("utf16.inf")).unwrap();
        let (utf16, encoded) = decode_inf(&utf16).unwrap();
        assert!(encoded && utf16.contains("frametime-lab-disabled NvContainer"));
    }
}
