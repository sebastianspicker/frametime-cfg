//! Steam VDF parsing and CS2-install identity contracts.

use std::{collections::BTreeMap, path::PathBuf};

use thiserror::Error;

pub const CS2_APP_ID: &str = "730";
pub const CS2_DIRECTORY: &str = "Counter-Strike Global Offensive";

#[derive(Debug, Error)]
pub enum SteamError {
    #[error("invalid Steam VDF: {0}")]
    InvalidVdf(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2Install {
    pub steam_root: PathBuf,
    pub library_root: PathBuf,
    pub install_root: PathBuf,
}

pub fn library_paths_from_vdf(text: &str) -> Result<Vec<String>, SteamError> {
    let root = parse_vdf(text)?;
    let Some(VdfValue::Object(libraries)) = root.get("libraryfolders") else {
        return Ok(Vec::new());
    };
    Ok(libraries
        .values()
        .filter_map(|value| match value {
            VdfValue::String(path) => Some(path.clone()),
            VdfValue::Object(fields) => match fields.get("path") {
                Some(VdfValue::String(path)) => Some(path.clone()),
                _ => None,
            },
        })
        .collect())
}

#[must_use]
pub fn app_manifest_is_cs2(text: &str) -> bool {
    let Ok(root) = parse_vdf(text) else {
        return false;
    };
    matches!(root.get("AppState"), Some(VdfValue::Object(app_state)) if matches!(app_state.get("appid"), Some(VdfValue::String(value)) if value == CS2_APP_ID))
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
                    return Err(SteamError::InvalidVdf(
                        "non-ASCII VDF text is unsupported".into(),
                    ));
                }
            }
        }
        Err(SteamError::InvalidVdf("unclosed VDF string".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_both_libraryfolder_shapes() {
        let paths = library_paths_from_vdf(
            r#""libraryfolders" { "0" "C:\\Steam" "1" { "path" "D:\\Steam" } }"#,
        )
        .unwrap();
        assert_eq!(paths, [r"C:\Steam", r"D:\Steam"]);
    }
    #[test]
    fn recognizes_only_cs2_manifest() {
        assert!(app_manifest_is_cs2(r#""AppState" { "appid" "730" }"#));
        assert!(!app_manifest_is_cs2(r#""AppState" { "appid" "1" }"#));
    }
}
