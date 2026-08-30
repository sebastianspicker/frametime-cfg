use serde::{Deserialize, Serialize};
use thiserror::Error;

const NVIDIA_DX_CACHE_TEMPLATE: &str = r"%LOCALAPPDATA%\NVIDIA\DXCache";
const NVIDIA_GL_CACHE_TEMPLATE: &str = r"%LOCALAPPDATA%\NVIDIA\GLCache";
const DIRECTX_SHADER_CACHE_TEMPLATE: &str = r"%LOCALAPPDATA%\D3DSCache";
const CS2_SHADER_CACHE_TEMPLATES: [&str; 5] = [
    r"%ProgramFiles(x86)%\Steam\steamapps\shadercache\730",
    r"%ProgramFiles%\Steam\steamapps\shadercache\730",
    r"D:\Steam\steamapps\shadercache\730",
    r"E:\Steam\steamapps\shadercache\730",
    r"F:\Steam\steamapps\shadercache\730",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub fps_cap: FpsCap,
    pub paths: RuntimePaths,
    pub autostart_remove: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FpsCap {
    pub strategy: FpsCapMode,
    /// Zero means uncapped for the raw-latency strategy.
    #[serde(default)]
    pub measured_cap: u32,
    /// Required only by the VRR strategy.
    #[serde(default)]
    pub refresh_hz: u32,
    /// Amount reserved below the display ceiling for the VRR strategy.
    #[serde(default = "default_vrr_margin_hz")]
    pub ceiling_margin_hz: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FpsCapMode {
    Raw,
    Vrr,
}

const fn default_vrr_margin_hz() -> u32 {
    3
}

impl FpsCap {
    #[must_use]
    pub const fn strategy(&self) -> crate::fps::FpsCapStrategy {
        match self.strategy {
            FpsCapMode::Raw => crate::fps::FpsCapStrategy::RawLatency {
                measured_cap: if self.measured_cap == 0 {
                    None
                } else {
                    Some(self.measured_cap)
                },
            },
            FpsCapMode::Vrr => crate::fps::FpsCapStrategy::Vrr {
                refresh_hz: self.refresh_hz,
                ceiling_margin_hz: self.ceiling_margin_hz,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePaths {
    pub shader_cache: Vec<String>,
    pub nvidia_dx_cache: String,
    pub nvidia_gl_cache: String,
    pub directx_shader_cache: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("invalid TOML configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("fps_cap.measured_cap must be zero (uncapped) or between 30 and 1000")]
    FpsMeasuredCap,
    #[error(
        "VRR requires fps_cap.refresh_hz between 30 and 1000 and ceiling_margin_hz below refresh_hz"
    )]
    FpsVrr,
    #[error("cache path {0} is not the compiled value")]
    CachePath(String),
    #[error("CS2 shader-cache path {0} is not in the compiled cache-root allowlist")]
    ShaderCachePath(String),
}

impl Config {
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
        if self.fps_cap.measured_cap != 0 && !(30..=1000).contains(&self.fps_cap.measured_cap) {
            return Err(ConfigError::FpsMeasuredCap);
        }
        if self.fps_cap.strategy == FpsCapMode::Vrr
            && (!(30..=1000).contains(&self.fps_cap.refresh_hz)
                || self.fps_cap.ceiling_margin_hz == 0
                || self.fps_cap.ceiling_margin_hz >= self.fps_cap.refresh_hz)
        {
            return Err(ConfigError::FpsVrr);
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
                return Err(ConfigError::CachePath(path.clone()));
            }
        }
        Ok(())
    }
}
