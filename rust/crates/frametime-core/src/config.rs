use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const CS2_SHADER_CACHE_TEMPLATES: [&str; 5] = [
    r"%ProgramFiles(x86)%\Steam\steamapps\shadercache\730",
    r"%ProgramFiles%\Steam\steamapps\shadercache\730",
    r"D:\Steam\steamapps\shadercache\730",
    r"E:\Steam\steamapps\shadercache\730",
    r"F:\Steam\steamapps\shadercache\730",
];
const NVIDIA_DX_CACHE_TEMPLATE: &str = r"%LOCALAPPDATA%\NVIDIA\DXCache";
const NVIDIA_GL_CACHE_TEMPLATE: &str = r"%LOCALAPPDATA%\NVIDIA\GLCache";
const DIRECTX_SHADER_CACHE_TEMPLATE: &str = r"%LOCALAPPDATA%\D3DSCache";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: String,
    pub work_dir: String,
    pub log_max_files: u8,
    pub run_once_execution_policy: ExecutionPolicy,
    pub fps_cap: FpsCap,
    pub benchmark_maps: BenchmarkMaps,
    pub device_guids: DeviceGuids,
    pub paths: RuntimePaths,
    pub chipset_urls: ChipsetUrls,
    pub dns: Dns,
    pub nic: Nic,
    pub autostart_remove: Vec<String>,
    pub xbox_services: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionPolicy {
    Bypass,
    RemoteSigned,
    AllSigned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FpsCap {
    pub percent: f64,
    pub minimum: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkMaps {
    pub dust2: String,
    pub inferno: String,
    pub ancient: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceGuids {
    pub display: String,
    pub network: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePaths {
    pub shader_cache: Vec<String>,
    pub nvidia_dx_cache: String,
    pub nvidia_gl_cache: String,
    pub directx_shader_cache: String,
    pub latency_targets: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChipsetUrls {
    pub amd: String,
    pub intel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dns {
    #[serde(default)]
    pub provider: DnsProvider,
    pub cloudflare: Vec<String>,
    pub google: Vec<String>,
}

/// The only DNS profiles a native transaction may request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DnsProvider {
    Cloudflare,
    Google,
    #[default]
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Nic {
    pub virtual_adapter_filter: String,
    pub eee: String,
    pub flow_control: String,
    pub interrupt_moderation: String,
    pub receive_buffers: u32,
    pub transmit_buffers: u32,
    pub high_speed_buffers: u32,
    pub high_speed_threshold_bps: u64,
    pub alternate_names: BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("invalid TOML configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("fps_cap.percent must be between 0.01 and 0.50")]
    FpsPercent,
    #[error("fps_cap.minimum must be between 30 and 500")]
    FpsMinimum,
    #[error("log_max_files must be between 1 and 50")]
    LogRetention,
    #[error("work_dir must be the fixed path C:\\FRAMETIME_CFG")]
    WorkDir,
    #[error("DNS provider {0} must contain exactly two addresses")]
    DnsCount(&'static str),
    #[error("device GUID {0} is invalid")]
    DeviceGuid(&'static str),
    #[error("latency target asset path is unsafe")]
    LatencyTargets,
    #[error("chipset URL {0} must use HTTPS")]
    ChipsetUrl(&'static str),
    #[error("shader-cache path {0} is not in the compiled cache-root allowlist")]
    ShaderCachePath(String),
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Self::parse_bytes(&fs::read(path)?)
    }

    /// Parse and validate one immutable configuration byte snapshot.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, ConfigError> {
        Self::parse_str(std::str::from_utf8(bytes)?)
    }

    /// Parse and validate UTF-8 configuration text.
    pub fn parse_str(raw: &str) -> Result<Self, ConfigError> {
        let parsed: Self = toml::from_str(raw)?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(0.01..=0.50).contains(&self.fps_cap.percent) {
            return Err(ConfigError::FpsPercent);
        }
        if !(30..=500).contains(&self.fps_cap.minimum) {
            return Err(ConfigError::FpsMinimum);
        }
        if !(1..=50).contains(&self.log_max_files) {
            return Err(ConfigError::LogRetention);
        }
        if !self.work_dir.eq_ignore_ascii_case(r"C:\FRAMETIME_CFG") {
            return Err(ConfigError::WorkDir);
        }
        if self.dns.cloudflare.len() != 2 {
            return Err(ConfigError::DnsCount("cloudflare"));
        }
        if self.dns.google.len() != 2 {
            return Err(ConfigError::DnsCount("google"));
        }
        if !valid_guid(&self.device_guids.display) {
            return Err(ConfigError::DeviceGuid("display"));
        }
        if !valid_guid(&self.device_guids.network) {
            return Err(ConfigError::DeviceGuid("network"));
        }
        if !crate::persistence::safe_relative_path(Path::new(&self.paths.latency_targets)) {
            return Err(ConfigError::LatencyTargets);
        }
        if !self.chipset_urls.amd.starts_with("https://") {
            return Err(ConfigError::ChipsetUrl("amd"));
        }
        if !self.chipset_urls.intel.starts_with("https://") {
            return Err(ConfigError::ChipsetUrl("intel"));
        }
        let mut seen = std::collections::BTreeSet::new();
        if self.paths.shader_cache.is_empty() {
            return Err(ConfigError::ShaderCachePath(
                "at least one CS2 shader-cache template is required".into(),
            ));
        }
        for path in &self.paths.shader_cache {
            if !CS2_SHADER_CACHE_TEMPLATES.contains(&path.as_str())
                || !seen.insert(path.to_ascii_lowercase())
            {
                return Err(ConfigError::ShaderCachePath(path.clone()));
            }
        }
        for (path, expected) in [
            (&self.paths.nvidia_dx_cache, NVIDIA_DX_CACHE_TEMPLATE),
            (&self.paths.nvidia_gl_cache, NVIDIA_GL_CACHE_TEMPLATE),
            (
                &self.paths.directx_shader_cache,
                DIRECTX_SHADER_CACHE_TEMPLATE,
            ),
        ] {
            if path != expected {
                return Err(ConfigError::ShaderCachePath(path.clone()));
            }
        }
        Ok(())
    }
}

fn valid_guid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 38
        && bytes.first() == Some(&b'{')
        && bytes.last() == Some(&b'}')
        && [9, 14, 19, 24].iter().all(|index| bytes[*index] == b'-')
        && bytes[1..37]
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Config {
        toml::from_str(include_str!("../../../frametime.toml")).expect("fixture")
    }

    #[test]
    fn validates_checked_in_defaults() {
        valid().validate().expect("valid defaults");
    }

    #[test]
    fn parse_bytes_binds_utf8_and_validation() {
        assert_eq!(
            Config::parse_bytes(include_bytes!("../../../frametime.toml")).expect("fixture"),
            valid()
        );
        assert!(matches!(
            Config::parse_bytes(&[0xff]),
            Err(ConfigError::Utf8(_))
        ));
        assert!(Config::parse_bytes(b"unknown = true").is_err());
    }

    #[test]
    fn rejects_bounds_instead_of_replacing_them() {
        let mut cfg = valid();
        cfg.fps_cap.percent = 0.0;
        assert!(matches!(cfg.validate(), Err(ConfigError::FpsPercent)));
    }

    #[test]
    fn shader_cache_paths_are_fixed_local_templates() {
        let mut cfg = valid();
        cfg.paths.shader_cache[0] = r"C:\safe\..\redirected".into();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::ShaderCachePath(_))
        ));

        let mut cfg = valid();
        cfg.paths.nvidia_dx_cache = r"\\server\cache".into();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::ShaderCachePath(_))
        ));

        let mut cfg = valid();
        cfg.paths.nvidia_gl_cache = cfg.paths.nvidia_dx_cache.to_ascii_lowercase();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::ShaderCachePath(_))
        ));

        let mut cfg = valid();
        cfg.paths.directx_shader_cache = r"%LOCALAPPDATA%\Windows".into();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::ShaderCachePath(_))
        ));

        let mut cfg = valid();
        cfg.paths.shader_cache.clear();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::ShaderCachePath(_))
        ));
    }
}
