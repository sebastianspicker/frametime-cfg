//! Trusted, install-bound persistence for the CS2 CFG portion of Step 34.

const OPTIMIZATION_FILE: &str = "optimization.cfg";
const AUTOEXEC_FILE: &str = "autoexec.cfg";

mod assets;
mod controller;
mod request;
mod safe_backup;
mod targets;
#[cfg(test)]
mod tests;
mod transaction;

pub use assets::{Cs2ConfigError, OptionalCfgAsset};
pub use controller::Cs2ConfigController;
pub use request::{
    CfgAssetDeployment, Cs2ConfigFs, Cs2ConfigPreview, Cs2ConfigRequest, Cs2ConfigWriteReport,
    OptimizationBackup,
};
pub use targets::Cs2ConfigTarget;
