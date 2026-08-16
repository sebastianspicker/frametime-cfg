// Immutable configuration authority minted only by retained package/runtime
// verifiers after their provenance checks complete. This module has no path
// or handle API, so clones cannot reopen an ambient configuration file.

use std::sync::Arc;

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIdentity {
    size: u64,
    sha256: String,
}

impl ConfigIdentity {
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug)]
struct VerifiedConfigInner {
    // Retain the exact authenticated bytes with their parsed meaning. The
    // bytes are intentionally private and never reinterpreted from a path.
    _bytes: Vec<u8>,
    value: Config,
    identity: ConfigIdentity,
}

/// Handle-free, cloneable authority for the exact authenticated
/// `frametime.toml` byte snapshot.
#[derive(Debug, Clone)]
pub struct VerifiedConfig(Arc<VerifiedConfigInner>);

impl VerifiedConfig {
    #[must_use]
    pub fn value(&self) -> &Config {
        &self.0.value
    }

    #[must_use]
    pub fn identity(&self) -> &ConfigIdentity {
        &self.0.identity
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn from_verified_bytes(
        bytes: Vec<u8>,
        expected_size: u64,
        expected_sha256: &str,
    ) -> Result<Self, String> {
        let actual_size = u64::try_from(bytes.len())
            .map_err(|_| "verified configuration size overflows u64")?;
        if actual_size != expected_size {
            return Err("verified configuration size differs from its trusted identity".into());
        }
        let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
        if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
            return Err("verified configuration SHA-256 differs from its trusted identity".into());
        }
        let value = Config::parse_bytes(&bytes)
            .map_err(|error| format!("parse verified frametime.toml: {error}"))?;
        Ok(Self(Arc::new(VerifiedConfigInner {
            _bytes: bytes,
            value,
            identity: ConfigIdentity {
                size: actual_size,
                sha256: actual_sha256,
            },
        })))
    }
}

#[cfg(test)]
mod config_authority_tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        include_bytes!("../../../../frametime.toml").to_vec()
    }

    #[test]
    fn verified_config_is_cloneable_and_snapshot_bound() {
        let mut bytes = fixture();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let config = VerifiedConfig::from_verified_bytes(bytes.clone(), bytes.len() as u64, &digest)
            .expect("verified fixture");
        bytes[0] = b'#';
        let clone = config.clone();
        assert_eq!(clone.identity().size(), bytes.len() as u64);
        assert_eq!(clone.value().version, config.value().version);
    }

    #[test]
    fn verified_config_rejects_identity_or_content_tampering() {
        let bytes = fixture();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        assert!(VerifiedConfig::from_verified_bytes(bytes.clone(), bytes.len() as u64 + 1, &digest)
            .is_err());
        assert!(VerifiedConfig::from_verified_bytes(bytes.clone(), bytes.len() as u64, "00").is_err());
        assert!(VerifiedConfig::from_verified_bytes(vec![0xff], 1, &format!("{:x}", Sha256::digest([0xff])))
            .is_err());
    }
}
