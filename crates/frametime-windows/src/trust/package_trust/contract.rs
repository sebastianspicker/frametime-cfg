use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{CLI_EXECUTABLE_NAME, GUI_EXECUTABLE_NAME, PackageFile, PackageManifest};

pub(crate) const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub(crate) const PAYLOAD_LAYOUT: &str = include_str!("../../../../../package-layout.txt");

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

pub(crate) fn exact_keys(
    object: &serde_json::Map<String, Value>,
    expected: &[&str],
) -> Result<(), String> {
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        Err("package manifest contains unknown or missing fields".into())
    } else {
        Ok(())
    }
}

pub(crate) fn validate_relative_path(path: &str) -> Result<(), String> {
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

pub(crate) fn valid_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn reject_duplicate_keys(bytes: &[u8]) -> Result<(), String> {
    let mut scanner = ManifestScanner { bytes, at: 0 };
    scanner.value()?;
    scanner.gap();
    if scanner.at == bytes.len() {
        Ok(())
    } else {
        Err("trailing package manifest bytes".into())
    }
}

pub(crate) struct ManifestScanner<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) at: usize,
}

impl ManifestScanner<'_> {
    pub(crate) fn gap(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }
    pub(crate) fn string(&mut self) -> Result<String, String> {
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
    pub(crate) fn value(&mut self) -> Result<(), String> {
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
    pub(crate) fn object(&mut self) -> Result<(), String> {
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
    pub(crate) fn array(&mut self) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sha256() -> String {
        "0".repeat(64)
    }

    fn valid_manifest() -> Value {
        let files = expected_payload_paths()
            .into_iter()
            .map(|path| json!({ "path": path, "size": 1, "sha256": sha256() }))
            .collect::<Vec<_>>();
        json!({ "schema_version": 1, "version": "3.0.0-alpha.1", "files": files })
    }

    fn parse_value(value: Value) -> Result<PackageManifest, String> {
        parse_manifest(&serde_json::to_vec(&value).expect("serializable manifest"))
    }

    fn files(value: &mut Value) -> &mut Vec<Value> {
        value
            .get_mut("files")
            .and_then(Value::as_array_mut)
            .expect("manifest files")
    }

    #[test]
    fn publisher_pins_accept_only_one_or_two_distinct_sha256_values() {
        let first = "A".repeat(64);
        let second = "b".repeat(64);
        assert_eq!(
            parse_publisher_pins(&format!(" {first} ; {second} ")),
            Ok(vec![first.to_ascii_lowercase(), second])
        );
        for input in [
            "",
            ";",
            "abc",
            "a; b; c",
            &format!("{first};{}", first.to_ascii_lowercase()),
        ] {
            assert!(parse_publisher_pins(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn manifest_parser_accepts_the_exact_compiled_layout() {
        let manifest = parse_value(valid_manifest()).expect("valid manifest");
        let actual = manifest
            .files()
            .iter()
            .map(|file| file.path().to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected_payload_paths());
        assert_eq!(manifest.version(), "3.0.0-alpha.1");
    }

    #[test]
    fn manifest_parser_rejects_missing_and_unknown_fields_at_each_level() {
        let mut missing_root = valid_manifest();
        missing_root
            .as_object_mut()
            .expect("root object")
            .remove("version");
        assert!(parse_value(missing_root).is_err());

        let mut unknown_root = valid_manifest();
        unknown_root["unexpected"] = json!(true);
        assert!(parse_value(unknown_root).is_err());

        let mut missing_file = valid_manifest();
        files(&mut missing_file)[0]
            .as_object_mut()
            .expect("file object")
            .remove("size");
        assert!(parse_value(missing_file).is_err());

        let mut unknown_file = valid_manifest();
        files(&mut unknown_file)[0]["unexpected"] = json!(true);
        assert!(parse_value(unknown_file).is_err());
    }

    #[test]
    fn manifest_parser_rejects_duplicate_json_fields_including_nested_fields() {
        let hash = sha256();
        let duplicate_root = format!(
            r#"{{"schema_version":1,"schema_version":1,"version":"v","files":[],"hash":"{hash}"}}"#
        );
        let duplicate_file = format!(
            r#"{{"schema_version":1,"version":"v","files":[{{"path":"frametime.exe","path":"frametime.exe","size":1,"sha256":"{hash}"}}]}}"#
        );
        assert!(parse_manifest(duplicate_root.as_bytes()).is_err());
        assert!(parse_manifest(duplicate_file.as_bytes()).is_err());
    }

    #[test]
    fn manifest_parser_rejects_malformed_or_oversized_json() {
        assert!(parse_manifest(b"{").is_err());
        assert!(parse_manifest(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
    }

    #[test]
    fn manifest_parser_rejects_missing_extra_and_case_colliding_payloads() {
        let mut missing = valid_manifest();
        files(&mut missing).pop();
        assert!(parse_value(missing).is_err());

        let mut extra = valid_manifest();
        files(&mut extra).push(json!({
            "path": "assets/extra.cfg",
            "size": 1,
            "sha256": sha256(),
        }));
        assert!(parse_value(extra).is_err());

        let mut collision = valid_manifest();
        let first = files(&mut collision)[0]["path"]
            .as_str()
            .expect("path")
            .to_ascii_uppercase();
        files(&mut collision)[1]["path"] = json!(first);
        assert!(parse_value(collision).is_err());
    }

    #[test]
    fn manifest_parser_rejects_invalid_sizes_hashes_and_paths() {
        let mut invalid_size = valid_manifest();
        files(&mut invalid_size)[0]["size"] = json!("1");
        assert!(parse_value(invalid_size).is_err());

        let mut invalid_hash = valid_manifest();
        files(&mut invalid_hash)[0]["sha256"] = json!("not-a-sha256");
        assert!(parse_value(invalid_hash).is_err());

        for path in [
            "../outside.cfg",
            "/absolute.cfg",
            r"assets\backslash.cfg",
            "C:drive.cfg",
            "assets/file.cfg:stream",
            "assets//empty.cfg",
            "assets/./same.cfg",
        ] {
            assert!(validate_relative_path(path).is_err(), "accepted {path:?}");
        }
        assert!(validate_relative_path("assets/cfgs/frame_raw_uncapped.cfg").is_ok());
    }

    #[test]
    fn exact_key_helper_requires_an_exact_set() {
        let object = json!({ "first": 1, "second": 2 });
        let object = object.as_object().expect("object");
        assert!(exact_keys(object, &["first", "second"]).is_ok());
        assert!(exact_keys(object, &["first"]).is_err());
        assert!(exact_keys(object, &["first", "third"]).is_err());
    }
}
