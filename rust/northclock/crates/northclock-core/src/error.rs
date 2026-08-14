use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, NorthclockError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Internal,
    InvalidUsage,
    Unavailable,
    PermissionOrSafety,
    HardwareOperation,
}

#[derive(Debug, Error)]
pub enum NorthclockError {
    #[error("internal failure: {0}")]
    Internal(String),
    #[error("invalid usage: {0}")]
    InvalidUsage(String),
    #[error("capability unavailable: {0}")]
    Unavailable(String),
    #[error("permission or safety rejection: {0}")]
    PermissionOrSafety(String),
    #[error("hardware operation or validation failure: {0}")]
    HardwareOperation(String),
}

impl NorthclockError {
    #[must_use]
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Internal(_) => ErrorCategory::Internal,
            Self::InvalidUsage(_) => ErrorCategory::InvalidUsage,
            Self::Unavailable(_) => ErrorCategory::Unavailable,
            Self::PermissionOrSafety(_) => ErrorCategory::PermissionOrSafety,
            Self::HardwareOperation(_) => ErrorCategory::HardwareOperation,
        }
    }

    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self.category() {
            ErrorCategory::Internal => 1,
            ErrorCategory::InvalidUsage => 2,
            ErrorCategory::Unavailable => 3,
            ErrorCategory::PermissionOrSafety => 4,
            ErrorCategory::HardwareOperation => 5,
        }
    }
}
