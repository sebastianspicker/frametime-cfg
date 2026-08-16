use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{CLI_EXECUTABLE_NAME, GUI_EXECUTABLE_NAME, PackageFile, PackageManifest};

const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const PAYLOAD_LAYOUT: &str = include_str!("../../../../package-layout.txt");

pub(super) fn parse_publisher_pins(value: &str) -> Result<Vec<String>, String> {
    let pins: Vec<_> = value.split(';').map(str::trim).collect();
    if !(1..=2).contains(&pins.len()) || pins.iter().any(|pin| !valid_hex(pin)) {
        return Err("publisher SPKI pins must contain one or two SHA-256 hex values".into());
    }
    let pins: Vec<_> = pins.into_iter().map(str::to_ascii_lowercase).collect();
    if pins[0] == *pins.last().expect("non-empty pins") && pins.len() == 2 {
        return Err("publisher SPKI pins must be distinct".into());
    }
    Ok(pins)
}

pub(super) fn parse_manifest(bytes: &[u8]) -> Result<PackageManifest, String> {
    if bytes.is_empty() || bytes.len() > MAX_MANIFEST_BYTES {
        return Err("package manifest exceeds bounded size".into());
    }
    reject_duplicate_keys(bytes)?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("parse package manifest: {error}"))?;
    let object = value
        .as_object()
        .ok_or("package manifest must be an object")?;
    exact_keys(object, &["schema_version", "version", "files"])?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("package manifest schema_version must be 1".into());
    }
    let version = object
        .get("version")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .ok_or("package manifest version is invalid")?
        .to_owned();
    let values = object
        .get("files")
        .and_then(Value::as_array)
        .ok_or("package manifest files must be an array")?;
    let expected = expected_payload_paths();
    if values.len() != expected.len() {
        return Err("package manifest file count differs from fixed payload layout".into());
    }
    let mut by_path = BTreeMap::new();
    for value in values {
        let object = value
            .as_object()
            .ok_or("package manifest file must be an object")?;
        exact_keys(object, &["path", "size", "sha256"])?;
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or("package manifest file path is invalid")?;
        validate_relative_path(path)?;
        let file = PackageFile {
            path: path.to_owned(),
            size: object
                .get("size")
                .and_then(Value::as_u64)
                .ok_or("package manifest file size is invalid")?,
            sha256: object
                .get("sha256")
                .and_then(Value::as_str)
                .filter(|hash| valid_hex(hash))
                .ok_or("package manifest file SHA-256 is invalid")?
                .to_ascii_lowercase(),
        };
        if by_path.insert(path.to_ascii_lowercase(), file).is_some() {
            return Err("package manifest has case-colliding paths".into());
        }
    }
    if by_path.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err("package manifest paths differ from fixed payload layout".into());
    }
    Ok(PackageManifest {
        version,
        files: by_path.into_values().collect(),
    })
}

pub(super) fn expected_payload_paths() -> BTreeSet<String> {
    PAYLOAD_LAYOUT
        .lines()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn exact_keys(object: &serde_json::Map<String, Value>, expected: &[&str]) -> Result<(), String> {
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        Err("package manifest contains unknown or missing fields".into())
    } else {
        Ok(())
    }
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 240
        || path.contains('\\')
        || path.starts_with('/')
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'\\')
    {
        return Err("package manifest path is not a normalized forward relative path".into());
    }
    if path.eq_ignore_ascii_case(GUI_EXECUTABLE_NAME)
        || path.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME)
    {
        return Ok(());
    }
    Ok(())
}

fn valid_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn reject_duplicate_keys(bytes: &[u8]) -> Result<(), String> {
    let mut scanner = ManifestScanner { bytes, at: 0 };
    scanner.value()?;
    scanner.gap();
    if scanner.at == bytes.len() {
        Ok(())
    } else {
        Err("trailing package manifest bytes".into())
    }
}

struct ManifestScanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl ManifestScanner<'_> {
    fn gap(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }
    fn string(&mut self) -> Result<String, String> {
        let start = self.at;
        if self.bytes.get(self.at) != Some(&b'"') {
            return Err("invalid JSON string".into());
        }
        self.at += 1;
        let mut escaped = false;
        while let Some(&byte) = self.bytes.get(self.at) {
            self.at += 1;
            if escaped {
                escaped = false;
                continue;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                return serde_json::from_slice(&self.bytes[start..self.at])
                    .map_err(|_| "invalid JSON string".into());
            }
        }
        Err("unterminated JSON string".into())
    }
    fn value(&mut self) -> Result<(), String> {
        self.gap();
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => {
                self.string()?;
                Ok(())
            }
            Some(_) => {
                let start = self.at;
                while self.at < self.bytes.len()
                    && !matches!(
                        self.bytes[self.at],
                        b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t'
                    )
                {
                    self.at += 1;
                }
                serde_json::from_slice::<Value>(&self.bytes[start..self.at])
                    .map(|_| ())
                    .map_err(|_| "invalid JSON scalar".into())
            }
            None => Err("missing JSON value".into()),
        }
    }
    fn object(&mut self) -> Result<(), String> {
        self.at += 1;
        let mut keys = BTreeSet::new();
        loop {
            self.gap();
            if self.bytes.get(self.at) == Some(&b'}') {
                self.at += 1;
                return Ok(());
            }
            let key = self.string()?;
            if !keys.insert(key) {
                return Err("package manifest has duplicate JSON field".into());
            }
            self.gap();
            if self.bytes.get(self.at) != Some(&b':') {
                return Err("invalid JSON object".into());
            }
            self.at += 1;
            self.value()?;
            self.gap();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err("invalid JSON object".into()),
            }
        }
    }
    fn array(&mut self) -> Result<(), String> {
        self.at += 1;
        loop {
            self.gap();
            if self.bytes.get(self.at) == Some(&b']') {
                self.at += 1;
                return Ok(());
            }
            self.value()?;
            self.gap();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err("invalid JSON array".into()),
            }
        }
    }
}
