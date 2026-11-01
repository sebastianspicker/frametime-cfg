#[derive(Debug, Clone, Copy)]
pub enum NativeAccess {
    ReadOnly,
    Write,
    ElevatedWrite,
}
impl NativeAccess {
    pub const fn requires_elevation(self) -> bool {
        matches!(self, Self::ElevatedWrite)
    }
    pub const fn blocks_close(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Ready,
    Running,
    Complete,
    Warning,
    Failed,
}
impl StatusKind {
    pub const fn text(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Complete => "Complete",
            Self::Warning => "Warning",
            Self::Failed => "Failed",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationState {
    pub status: StatusKind,
    pub detail: String,
    pub cancellable: bool,
}
impl OperationState {
    pub fn ready(detail: impl Into<String>) -> Self {
        Self {
            status: StatusKind::Ready,
            detail: detail.into(),
            cancellable: false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statuses_remain_explicit() {
        assert!(!NativeAccess::ReadOnly.blocks_close());
        assert!(!NativeAccess::Write.requires_elevation());
        assert!(NativeAccess::Write.blocks_close());
        assert!(NativeAccess::ElevatedWrite.blocks_close());
        assert!(NativeAccess::ElevatedWrite.requires_elevation());
        let ready = OperationState::ready("Awaiting an operator action.");
        assert_eq!(ready.status.text(), "Ready");
        assert_eq!(StatusKind::Running.text(), "Running");
        assert_eq!(StatusKind::Complete.text(), "Complete");
        assert_eq!(StatusKind::Warning.text(), "Warning");
        assert_eq!(StatusKind::Failed.text(), "Failed");
        assert!(!ready.cancellable);
    }
}
