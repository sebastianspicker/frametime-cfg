#[cfg(windows)]
use self::contract::{
    SAFE_MODE_HANDOFF_ARGUMENTS, manifest_for, payload_directories, valid_generated_id,
};
use crate::*;
/// Non-cloneable capability for the runtime generation just published from the
/// current package. On Windows it retains the copied executable handle and its
/// ancestor handles, preventing replacement before a caller launches it.
#[derive(Debug)]
pub struct VerifiedPublishedRuntime {
    pub(crate) record: frametime_domain::RuntimeRecord,
    pub(crate) executable_path: std::path::PathBuf,
    #[cfg(windows)]
    pub(crate) _retained: publisher::PublicationRetention,
}

impl VerifiedPublishedRuntime {
    #[must_use]
    pub fn record(&self) -> &frametime_domain::RuntimeRecord {
        &self.record
    }

    #[must_use]
    pub fn executable_path(&self) -> &std::path::Path {
        &self.executable_path
    }
}

/// Copy the compiled portable payload only from handles retained by a fully
/// authenticated package capability, then atomically select it after every
/// destination handle is verified.
pub fn publish_current_packaged_runtime(
    package: &AuthenticatedPackage,
) -> Result<VerifiedPublishedRuntime, String> {
    #[cfg(windows)]
    {
        publisher::publish(package)
    }
    #[cfg(not(windows))]
    {
        let _ = package;
        Err("runtime publication requires supported Windows x64".into())
    }
}

/// Visibly elevate the exact retained executable from a just-published
/// generation and wait for it to durably arm the Safe Mode handoff.
pub fn launch_published_safe_mode_handoff(
    runtime: &VerifiedPublishedRuntime,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        launcher::launch(runtime)
    }
    #[cfg(not(windows))]
    {
        let _ = runtime;
        Err("published runtime launch requires supported Windows x64".into())
    }
}

mod contract;
#[cfg(windows)]
mod launcher;
#[cfg(windows)]
pub(crate) mod publisher;
