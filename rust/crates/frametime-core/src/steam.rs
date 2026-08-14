//! Safe Steam and Counter-Strike 2 discovery.
//!
//! This module deliberately accepts candidate Steam roots rather than reading
//! the Windows registry. The Windows crate can supply the registry value while
//! keeping all parsing and filesystem trust decisions testable and portable.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

pub const CS2_APP_ID: &str = "730";
pub const CS2_DIRECTORY: &str = "Counter-Strike Global Offensive";

#[derive(Debug, Error)]
pub enum SteamError {
    #[error("Steam path is not a trusted real directory: {0}")]
    UntrustedRoot(PathBuf),
    #[error("Steam path escaped its trusted root: {0}")]
    EscapedPath(PathBuf),
    #[error("invalid Steam VDF: {0}")]
    InvalidVdf(String),
    #[error("Steam filesystem error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2Install {
    pub steam_root: PathBuf,
    pub library_root: PathBuf,
    pub install_root: PathBuf,
}

/// Finds all real, contained Steam libraries. The primary Steam library is
/// always included; additional VDF paths are accepted only after trust checks.
pub fn discover_steam_libraries(steam_root: &Path) -> Result<Vec<PathBuf>, SteamError> {
    trusted_directory(steam_root)?;
    let mut libraries = vec![steam_root.to_path_buf()];
    let steamapps = steam_root.join("steamapps");
    match fs::symlink_metadata(&steamapps) {
        Ok(metadata) if metadata_is_reparse(&metadata) || !metadata.is_dir() => {
            return Err(SteamError::EscapedPath(steamapps));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(libraries),
        Err(error) => return Err(SteamError::Io(error)),
    }
    let vdf = steamapps.join("libraryfolders.vdf");
    if !vdf.exists() {
        return Ok(libraries);
    }
    trusted_file_under(steam_root, &vdf)?;
    let text = fs::read_to_string(&vdf)?;
    let parsed = parse_vdf(&text)?;
    for value in library_paths(&parsed) {
        let library = vdf_path_to_path(&value);
        if trusted_directory(&library).is_ok() && !libraries.contains(&library) {
            libraries.push(library);
        }
    }
    Ok(libraries)
}

/// Finds CS2 only when a trusted Steam library contains a valid app 730
/// manifest and the expected executable below that library.
pub fn discover_cs2_install(steam_root: &Path) -> Result<Option<Cs2Install>, SteamError> {
    for library_root in discover_steam_libraries(steam_root)? {
        let manifest = library_root.join("steamapps").join("appmanifest_730.acf");
        if !manifest.is_file() || trusted_file_under(&library_root, &manifest).is_err() {
            continue;
        }
        let manifest_text = match fs::read_to_string(&manifest) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let manifest = match parse_vdf(&manifest_text) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !app_manifest_is_cs2(&manifest) {
            continue;
        }
        let install_root = library_root
            .join("steamapps")
            .join("common")
            .join(CS2_DIRECTORY);
        let executable = install_root
            .join("game")
            .join("bin")
            .join("win64")
            .join("cs2.exe");
        if executable.is_file() && trusted_file_under(&library_root, &executable).is_ok() {
            return Ok(Some(Cs2Install {
                steam_root: steam_root.to_path_buf(),
                library_root,
                install_root,
            }));
        }
    }
    Ok(None)
}

/// Verifies that an existing path is a real directory with no symbolic-link or
/// Windows reparse-point hop. It also rejects lexical traversal before the OS
/// can normalize it away.
pub(crate) fn trusted_directory(path: &Path) -> Result<(), SteamError> {
    if !path.is_absolute() || has_unsafe_components(path) {
        return Err(SteamError::UntrustedRoot(path.to_path_buf()));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(SteamError::UntrustedRoot(path.to_path_buf()));
    }
    Ok(())
}

/// Checks a file lives under a trusted library/Steam root without following a
/// reparse point. `candidate` must already exist.
pub(crate) fn trusted_file_under(root: &Path, candidate: &Path) -> Result<(), SteamError> {
    trusted_directory(root)?;
    if has_unsafe_components(candidate) {
        return Err(SteamError::EscapedPath(candidate.to_path_buf()));
    }
    let canonical_root = fs::canonicalize(root)?;
    let candidate_canonical = fs::canonicalize(candidate)?;
    if !candidate_canonical.starts_with(&canonical_root) {
        return Err(SteamError::EscapedPath(candidate.to_path_buf()));
    }
    reject_reparse_ancestors(candidate, Some(root))?;
    let metadata = fs::symlink_metadata(candidate)?;
    if metadata_is_reparse(&metadata) || !metadata.is_file() {
        return Err(SteamError::EscapedPath(candidate.to_path_buf()));
    }
    Ok(())
}

fn reject_reparse_ancestors(path: &Path, stop_at: Option<&Path>) -> Result<(), SteamError> {
    let mut current = Some(path);
    while let Some(value) = current {
        let metadata = fs::symlink_metadata(value)?;
        if metadata_is_reparse(&metadata) {
            return Err(SteamError::EscapedPath(value.to_path_buf()));
        }
        if stop_at.is_some_and(|stop| value == stop) {
            break;
        }
        current = value.parent();
    }
    Ok(())
}

fn has_unsafe_components(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum VdfValue {
    String(String),
    Object(BTreeMap<String, VdfValue>),
}

fn parse_vdf(text: &str) -> Result<BTreeMap<String, VdfValue>, SteamError> {
    let tokens = VdfLexer::new(text).tokens()?;
    let mut cursor = 0;
    let object = parse_object(&tokens, &mut cursor, false)?;
    if cursor != tokens.len() {
        return Err(SteamError::InvalidVdf("trailing VDF tokens".into()));
    }
    Ok(object)
}

fn parse_object(
    tokens: &[VdfToken],
    cursor: &mut usize,
    expect_close: bool,
) -> Result<BTreeMap<String, VdfValue>, SteamError> {
    let mut object = BTreeMap::new();
    loop {
        let Some(token) = tokens.get(*cursor) else {
            return if expect_close {
                Err(SteamError::InvalidVdf("unclosed VDF object".into()))
            } else {
                Ok(object)
            };
        };
        if *token == VdfToken::Close {
            if !expect_close {
                return Err(SteamError::InvalidVdf("unexpected VDF close brace".into()));
            }
            *cursor += 1;
            return Ok(object);
        }
        let VdfToken::Text(key) = token else {
            return Err(SteamError::InvalidVdf("VDF key must be quoted".into()));
        };
        *cursor += 1;
        let Some(next) = tokens.get(*cursor) else {
            return Err(SteamError::InvalidVdf("VDF key has no value".into()));
        };
        let value = match next {
            VdfToken::Text(value) => {
                *cursor += 1;
                VdfValue::String(value.clone())
            }
            VdfToken::Open => {
                *cursor += 1;
                VdfValue::Object(parse_object(tokens, cursor, true)?)
            }
            VdfToken::Close => return Err(SteamError::InvalidVdf("VDF key has no value".into())),
        };
        if object.insert(key.clone(), value).is_some() {
            return Err(SteamError::InvalidVdf(format!("duplicate VDF key: {key}")));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VdfToken {
    Text(String),
    Open,
    Close,
}

struct VdfLexer<'a> {
    source: &'a [u8],
    offset: usize,
}

impl<'a> VdfLexer<'a> {
    const fn new(source: &'a str) -> Self {
        Self {
            source: source.as_bytes(),
            offset: 0,
        }
    }

    fn tokens(mut self) -> Result<Vec<VdfToken>, SteamError> {
        let mut tokens = Vec::new();
        while self.skip_space_and_comments()? {
            match self.source[self.offset] {
                b'{' => {
                    self.offset += 1;
                    tokens.push(VdfToken::Open);
                }
                b'}' => {
                    self.offset += 1;
                    tokens.push(VdfToken::Close);
                }
                b'"' => tokens.push(VdfToken::Text(self.quoted()?)),
                _ => return Err(SteamError::InvalidVdf("VDF values must be quoted".into())),
            }
        }
        Ok(tokens)
    }

    fn skip_space_and_comments(&mut self) -> Result<bool, SteamError> {
        loop {
            while self.offset < self.source.len() && self.source[self.offset].is_ascii_whitespace()
            {
                self.offset += 1;
            }
            if self.offset >= self.source.len() {
                return Ok(false);
            }
            if self.source[self.offset..].starts_with(b"//") {
                self.offset += 2;
                while self.offset < self.source.len() && self.source[self.offset] != b'\n' {
                    self.offset += 1;
                }
                continue;
            }
            if self.source[self.offset] == b'/' {
                return Err(SteamError::InvalidVdf("unsupported VDF comment".into()));
            }
            return Ok(true);
        }
    }

    fn quoted(&mut self) -> Result<String, SteamError> {
        self.offset += 1;
        let mut result = String::new();
        while self.offset < self.source.len() {
            let byte = self.source[self.offset];
            self.offset += 1;
            match byte {
                b'"' => return Ok(result),
                b'\\' => {
                    let Some(escaped) = self.source.get(self.offset) else {
                        break;
                    };
                    self.offset += 1;
                    match escaped {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        _ => return Err(SteamError::InvalidVdf("unsupported VDF escape".into())),
                    }
                }
                0 => return Err(SteamError::InvalidVdf("NUL in VDF".into())),
                value if value.is_ascii() => result.push(char::from(value)),
                _ => {
                    let start = self.offset - 1;
                    while self.offset < self.source.len() && !self.source[self.offset].is_ascii() {
                        self.offset += 1;
                    }
                    let value = std::str::from_utf8(&self.source[start..self.offset])
                        .map_err(|_| SteamError::InvalidVdf("invalid UTF-8 in VDF".into()))?;
                    result.push_str(value);
                }
            }
        }
        Err(SteamError::InvalidVdf("unclosed VDF string".into()))
    }
}

fn library_paths(root: &BTreeMap<String, VdfValue>) -> Vec<String> {
    let Some(VdfValue::Object(libraries)) = root.get("libraryfolders") else {
        return Vec::new();
    };
    libraries
        .values()
        .filter_map(|value| match value {
            VdfValue::String(path) => Some(path.clone()),
            VdfValue::Object(fields) => match fields.get("path") {
                Some(VdfValue::String(path)) => Some(path.clone()),
                _ => None,
            },
        })
        .collect()
}

fn app_manifest_is_cs2(root: &BTreeMap<String, VdfValue>) -> bool {
    let Some(VdfValue::Object(app_state)) = root.get("AppState") else {
        return false;
    };
    matches!(app_state.get("appid"), Some(VdfValue::String(value)) if value == CS2_APP_ID)
}

fn vdf_path_to_path(value: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(value.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(value.replace('\\', "/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf_parser_rejects_code_like_and_malformed_input() {
        assert!(parse_vdf("\"libraryfolders\" { \"1\" { \"path\" \"/games\" } }").is_ok());
        assert!(parse_vdf("\"libraryfolders\" { \"1\" { \"path\" \"/games\" }").is_err());
        assert!(parse_vdf("Get-ChildItem C:\\").is_err());
    }
}
