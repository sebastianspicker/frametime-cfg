use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeviceIdentity {
    pub kind: String,
    pub stable_id: String,
    pub display_name: String,
    pub vendor: Option<String>,
}

impl DeviceIdentity {
    #[must_use]
    pub fn new(
        kind: impl Into<String>,
        stable_id: impl Into<String>,
        display_name: impl Into<String>,
        vendor: Option<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            stable_id: stable_id.into(),
            display_name: display_name.into(),
            vendor,
        }
    }
}

/// A value returned by a real backend. Production callers must not construct one
/// when the backend did not supply a value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Measurement<T> {
    pub value: T,
    pub unit: String,
    pub timestamp_unix_ms: u128,
    pub device: DeviceIdentity,
    pub source: String,
}

impl<T> Measurement<T> {
    #[must_use]
    pub fn at(
        value: T,
        unit: impl Into<String>,
        timestamp_unix_ms: u128,
        device: DeviceIdentity,
        source: impl Into<String>,
    ) -> Self {
        Self {
            value,
            unit: unit.into(),
            timestamp_unix_ms,
            device,
            source: source.into(),
        }
    }

    pub fn now(
        value: T,
        unit: impl Into<String>,
        device: DeviceIdentity,
        source: impl Into<String>,
    ) -> crate::Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| crate::NorthclockError::Internal(error.to_string()))?
            .as_millis();
        Ok(Self::at(value, unit, timestamp, device, source))
    }
}
