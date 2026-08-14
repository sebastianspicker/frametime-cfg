use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationState {
    Observed,
    Partial,
    NotFound,
    Unavailable,
    PermissionRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Observation<T> {
    pub state: ObservationState,
    pub source: String,
    pub value: Option<T>,
    pub error: Option<String>,
}

impl<T> Observation<T> {
    #[must_use]
    pub fn observed(source: impl Into<String>, value: T) -> Self {
        Self {
            state: ObservationState::Observed,
            source: source.into(),
            value: Some(value),
            error: None,
        }
    }

    #[must_use]
    pub fn not_found(source: impl Into<String>) -> Self {
        Self {
            state: ObservationState::NotFound,
            source: source.into(),
            value: None,
            error: None,
        }
    }

    #[must_use]
    pub fn partial(source: impl Into<String>, value: T, error: impl Into<String>) -> Self {
        Self {
            state: ObservationState::Partial,
            source: source.into(),
            value: Some(value),
            error: Some(error.into()),
        }
    }

    #[must_use]
    pub fn unavailable(source: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            state: ObservationState::Unavailable,
            source: source.into(),
            value: None,
            error: Some(error.into()),
        }
    }

    #[must_use]
    pub fn permission_required(source: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            state: ObservationState::PermissionRequired,
            source: source.into(),
            value: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledTaskState {
    Unknown,
    Disabled,
    Queued,
    Ready,
    Running,
}

impl ScheduledTaskState {
    #[must_use]
    pub const fn from_raw(value: i32) -> Self {
        match value {
            1 => Self::Disabled,
            2 => Self::Queued,
            3 => Self::Ready,
            4 => Self::Running,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegisteredTask {
    pub name: String,
    pub path: String,
    pub state: ScheduledTaskState,
    pub state_raw: i32,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskSchedulerStatus {
    pub folder_path: String,
    pub tasks: Vec<RegisteredTask>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VbsRuntimeState {
    NotEnabled,
    EnabledNotRunning,
    Running,
    Unknown,
}

impl VbsRuntimeState {
    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::NotEnabled,
            1 => Self::EnabledNotRunning,
            2 => Self::Running,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VbsStatus {
    pub runtime_state: VbsRuntimeState,
    pub runtime_state_raw: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    Process,
    Service,
    Driver,
    Device,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PotentialConflict {
    pub kind: ConflictKind,
    pub identifier: String,
    pub display_name: String,
    pub process_id: Option<u32>,
    pub active: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SystemStatusReport {
    pub task_scheduler: Observation<TaskSchedulerStatus>,
    pub virtualization_based_security: Observation<VbsStatus>,
    pub potential_conflicts: Observation<Vec<PotentialConflict>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_states_preserve_unknown_values() {
        assert_eq!(ScheduledTaskState::from_raw(4), ScheduledTaskState::Running);
        assert_eq!(
            ScheduledTaskState::from_raw(99),
            ScheduledTaskState::Unknown
        );
        assert_eq!(VbsRuntimeState::from_raw(2), VbsRuntimeState::Running);
        assert_eq!(VbsRuntimeState::from_raw(99), VbsRuntimeState::Unknown);
    }

    #[test]
    fn unavailable_observation_has_no_value() {
        let observation = Observation::<VbsStatus>::permission_required("WMI", "access denied");
        assert!(observation.value.is_none());
        assert_eq!(observation.error.as_deref(), Some("access denied"));
    }
}
