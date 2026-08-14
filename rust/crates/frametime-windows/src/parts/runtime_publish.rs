/// Non-cloneable capability for the runtime generation just published from the
/// current package. On Windows it retains the copied executable handle and its
/// ancestor handles, preventing replacement before a caller launches it.
#[derive(Debug)]
pub struct VerifiedPublishedRuntime {
    record: frametime_core::RuntimeRecord,
    executable_path: std::path::PathBuf,
    #[cfg(windows)]
    _retained: runtime_publisher::PublicationRetention,
}

impl VerifiedPublishedRuntime {
    #[must_use]
    pub fn record(&self) -> &frametime_core::RuntimeRecord {
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
        runtime_publisher::publish(package)
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
        runtime_launcher::launch(runtime)
    }
    #[cfg(not(windows))]
    {
        let _ = runtime;
        Err("published runtime launch requires supported Windows x64".into())
    }
}

#[cfg(windows)]
#[path = "runtime_publish/runtime_launcher.rs"]
mod runtime_launcher;
#[cfg(windows)]
#[path = "runtime_publish/runtime_publisher.rs"]
mod runtime_publisher;
