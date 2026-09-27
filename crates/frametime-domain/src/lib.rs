//! Platform-neutral workflow, persistence, migration, and recovery contracts.

pub mod audit;
pub mod backup;
pub mod benchmark;
pub mod binding;
pub mod catalog;
pub mod cleanup;
pub mod config;
pub mod cs2;
pub mod cs2_config;
pub mod engine;
pub mod evidence;
pub mod fps;
pub mod handoff;
pub mod latency;
pub mod logging;
pub mod migration;
pub mod operations;
pub mod orchestration;
pub mod persistence;
pub mod policy;
pub mod runtime;
pub mod state;
pub mod steam;
pub mod verification;
pub mod video;

/// Platform-neutral, fail-closed driver evidence and dry-run planning contracts.
pub mod driver;

/// Versioned, platform-neutral contracts for native hardware diagnostics.
pub mod hardware;

pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");
