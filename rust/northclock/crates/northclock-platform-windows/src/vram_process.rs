use northclock_core::{NorthclockError, Result, WorkloadReport};
use std::time::Duration;

pub(crate) fn run_isolated(
    _adapter: Option<&str>,
    _bytes: u64,
    _timeout: Duration,
) -> Result<WorkloadReport> {
    reject_unauthenticated_worker_launch(super::is_elevated()?)
}

fn reject_unauthenticated_worker_launch<T>(elevated: bool) -> Result<T> {
    if elevated {
        return Err(NorthclockError::PermissionOrSafety(
            "isolated VRAM worker launch is disabled from an elevated Northclock process".into(),
        ));
    }
    Err(NorthclockError::Unavailable(
        "isolated VRAM worker launch is unavailable because this build has no authenticated worker-image capability".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::reject_unauthenticated_worker_launch;
    use northclock_core::NorthclockError;

    #[test]
    fn elevated_process_fails_before_any_worker_launch() {
        let error = reject_unauthenticated_worker_launch::<()>(true)
            .expect_err("an elevated parent must fail before worker launch");
        assert!(matches!(error, NorthclockError::PermissionOrSafety(_)));
    }

    #[test]
    fn unelevated_process_requires_an_authenticated_worker_capability() {
        let error = reject_unauthenticated_worker_launch::<()>(false)
            .expect_err("an unauthenticated worker must never launch");
        assert!(matches!(error, NorthclockError::Unavailable(_)));
    }
}
