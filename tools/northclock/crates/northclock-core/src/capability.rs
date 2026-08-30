use serde::{Deserialize, Serialize};

/// The verification state of a hardware-facing capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Available,
    Unsupported,
    Experimental,
    PermissionRequired,
    Unverified,
}

/// A capability report names both the implementation and its evidence level.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityReport {
    pub name: String,
    pub state: CapabilityState,
    pub backend: String,
    pub detail: String,
    pub hardware_verified: bool,
}

impl CapabilityReport {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        state: CapabilityState,
        backend: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            state,
            backend: backend.into(),
            detail: detail.into(),
            hardware_verified: false,
        }
    }

    #[must_use]
    pub fn is_usable(&self) -> bool {
        matches!(
            self.state,
            CapabilityState::Available | CapabilityState::Experimental
        )
    }
}
