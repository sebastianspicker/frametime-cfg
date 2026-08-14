use northclock_core::{NorthclockError, Result};
use std::path::PathBuf;

pub(crate) fn resolve<E, D>(elevation_probe: E, local_app_data: D) -> Result<PathBuf>
where
    E: FnOnce() -> Result<bool>,
    D: FnOnce() -> Option<PathBuf>,
{
    match elevation_probe() {
        Ok(true) => Err(NorthclockError::Unavailable(
            "persistent storage is unavailable in an elevated Northclock process; no inherited LOCALAPPDATA will be used"
                .into(),
        )),
        Ok(false) => local_app_data().map_or_else(
            || {
                Err(NorthclockError::Unavailable(
                    "LOCALAPPDATA is not available; Northclock will not fall back to the working directory"
                        .into(),
                ))
            },
            |path| Ok(path.join("Northclock")),
        ),
        Err(error) => Err(NorthclockError::Unavailable(format!(
            "persistent storage is unavailable because Northclock could not determine whether it is elevated: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_process_does_not_use_inherited_local_app_data() {
        let result = resolve(
            || Ok(true),
            || panic!("LOCALAPPDATA must not be read while elevated"),
        );

        assert!(
            matches!(result, Err(NorthclockError::Unavailable(message)) if message.contains("elevated"))
        );
    }

    #[test]
    fn unelevated_process_uses_local_app_data() {
        let result = resolve(
            || Ok(false),
            || Some(PathBuf::from(r"C:\Users\operator\AppData\Local")),
        );

        assert_eq!(
            result.unwrap_or_else(|error| panic!("storage root failed: {error}")),
            PathBuf::from(r"C:\Users\operator\AppData\Local").join("Northclock")
        );
    }

    #[test]
    fn elevation_probe_failure_disables_persistence_without_reading_local_app_data() {
        let result = resolve(
            || Err(NorthclockError::Unavailable("token query failed".into())),
            || panic!("LOCALAPPDATA must not be read when elevation is unknown"),
        );

        assert!(
            matches!(result, Err(NorthclockError::Unavailable(message)) if message.contains("could not determine"))
        );
    }
}
