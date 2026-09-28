//! Additive, platform-neutral NVIDIA v2 lifecycle contracts.
//!
//! The public surface remains here while cohesive validation and persistence
//! records live in private modules.

use super::Sha256Digest;

pub const NVIDIA_COMPONENT_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const NVIDIA_TRANSACTION_V2_SCHEMA_VERSION: u32 = 2;

const MAX_DRS_PROFILES: usize = 4_096;
const MAX_DRS_ITEMS_PER_PROFILE: usize = 4_096;
const MAX_DRS_RECORDS: usize = 65_536;
const MAX_DRS_UTF16_UNITS: usize = 2_048;
const MAX_DRS_BINARY_BYTES: usize = 4 * 1_024;
const MAX_DRS_SERIALIZED_BYTES: usize = 64 * 1_024 * 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverLifecycleError {
    InvalidCatalog,
    UnknownComponent(String),
    UnsafeComponentDirectory(String),
    InvalidDrsSnapshot(&'static str),
    Serialization,
    IncompatibleDrsItems(Vec<DrsItemKey>),
    InvalidTransaction(&'static str),
}

mod component_catalog;
mod drs_snapshot;
mod transaction_v2;

pub use component_catalog::*;
pub use drs_snapshot::*;
pub use transaction_v2::*;
