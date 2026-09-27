use std::path::Path;

use frametime_domain::driver::{
    AdapterFailure, ExactGpuIdentity, InspectionAdapter, OemPublishedName, PackageExecutionAdapter,
    PackageRemovalDisposition, PackageRemovalOutcome, PublishedDriverPackage,
};

#[cfg(windows)]
use super::native;
use super::{System32ToolRunner, WindowsDriverInspection, adapter};
use crate::PciDeviceEnumerator;

fn pnputil_path(system32: &Path) -> Result<std::path::PathBuf, AdapterFailure> {
    if !system32.is_absolute()
        || system32
            .file_name()
            .is_none_or(|name| !name.eq_ignore_ascii_case("System32"))
    {
        return Err(adapter(
            "resolve PnPUtil",
            "trusted System32 path is not absolute",
        ));
    }
    Ok(system32.join("pnputil.exe"))
}

pub struct PnpUtilDriverRemoval<R, E> {
    runner: R,
    inspection: WindowsDriverInspection<E>,
}

impl<R, E> PnpUtilDriverRemoval<R, E> {
    #[must_use]
    pub fn new(runner: R, inspection: WindowsDriverInspection<E>) -> Self {
        Self { runner, inspection }
    }

    fn driver_store_package_is_present(
        &self,
        target: &ExactGpuIdentity,
        name: &OemPublishedName,
    ) -> Result<bool, AdapterFailure>
    where
        E: PciDeviceEnumerator,
    {
        #[cfg(windows)]
        {
            let _ = target;
            native::published_inf_is_present(name)
        }
        #[cfg(not(windows))]
        {
            Ok(self
                .inspection
                .inspect_published_packages(target)?
                .iter()
                .any(|package| package.published_name == *name))
        }
    }
}

impl<R: System32ToolRunner, E: PciDeviceEnumerator> PackageExecutionAdapter
    for PnpUtilDriverRemoval<R, E>
{
    fn published_package_is_present(
        &self,
        target: &ExactGpuIdentity,
        name: &OemPublishedName,
    ) -> Result<bool, AdapterFailure> {
        self.driver_store_package_is_present(target, name)
    }

    fn remove_published_package(
        &self,
        target: &ExactGpuIdentity,
        expected: &PublishedDriverPackage,
    ) -> Result<PackageRemovalOutcome, AdapterFailure> {
        let name = &expected.published_name;
        let present = self.driver_store_package_is_present(target, name)?;
        if !present {
            return Ok(PackageRemovalOutcome {
                published_name: name.clone(),
                disposition: PackageRemovalDisposition::AlreadyAbsent,
                observed_at_utc: crate::timestamp(),
            });
        }
        if expected.driver_store_package_sha256.is_none() {
            return Err(adapter(
                "remove driver package",
                "the captured OEM INF has no immutable Driver Store identity",
            ));
        }
        let current = self.inspection.inspect_published_packages(target)?;
        if !current.iter().any(|package| package == expected) {
            return Err(adapter(
                "remove driver package",
                "the present OEM INF no longer has its exact captured Store and target binding",
            ));
        }
        let executable = pnputil_path(&self.runner.system32()?)?;
        let argv = vec![
            "/delete-driver".into(),
            name.as_str().into(),
            "/uninstall".into(),
            "/force".into(),
        ];
        let result = self.runner.run(&executable, &argv)?;
        let remains = self.driver_store_package_is_present(target, name)?;
        let disposition = classify_package_removal(result.exit_code, remains);
        Ok(PackageRemovalOutcome {
            published_name: name.clone(),
            disposition,
            observed_at_utc: crate::timestamp(),
        })
    }

    fn inspect_published_packages(
        &self,
        target: &ExactGpuIdentity,
    ) -> Result<Vec<PublishedDriverPackage>, AdapterFailure> {
        self.inspection.inspect_published_packages(target)
    }
}

fn classify_package_removal(
    exit_code: Option<i32>,
    package_remains: bool,
) -> PackageRemovalDisposition {
    match (exit_code, package_remains) {
        (Some(0), false) => PackageRemovalDisposition::Removed,
        (Some(_) | None, false) => PackageRemovalDisposition::AlreadyAbsent,
        (Some(0), true) => PackageRemovalDisposition::Failed {
            reason: "PnPUtil reported success but the package remains in the Driver Store".into(),
        },
        (Some(_) | None, true) => PackageRemovalDisposition::Failed {
            reason: "PnPUtil returned a nonzero or unavailable exit status".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageRemovalDisposition, classify_package_removal};

    #[test]
    fn driver_store_readback_controls_retry_disposition() {
        assert_eq!(
            classify_package_removal(Some(0), false),
            PackageRemovalDisposition::Removed
        );
        assert_eq!(
            classify_package_removal(Some(1), false),
            PackageRemovalDisposition::AlreadyAbsent
        );
        assert!(matches!(
            classify_package_removal(Some(0), true),
            PackageRemovalDisposition::Failed { .. }
        ));
        assert!(matches!(
            classify_package_removal(None, true),
            PackageRemovalDisposition::Failed { .. }
        ));
    }
}
