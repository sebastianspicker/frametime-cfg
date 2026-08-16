use northclock_core::{CommandEnvelope, CommandStatus, NorthclockError};

#[test]
fn exit_codes_cover_every_error_category() {
    let errors = [
        NorthclockError::Internal("x".into()),
        NorthclockError::InvalidUsage("x".into()),
        NorthclockError::Unavailable("x".into()),
        NorthclockError::PermissionOrSafety("x".into()),
        NorthclockError::HardwareOperation("x".into()),
    ];
    assert_eq!(
        errors.map(|error| error.exit_code()),
        [1_u8, 2_u8, 3_u8, 4_u8, 5_u8]
    );
}

#[test]
fn envelopes_map_every_error_category_to_status() {
    let cases = [
        (
            NorthclockError::Internal("x".into()),
            CommandStatus::Failure,
        ),
        (
            NorthclockError::InvalidUsage("x".into()),
            CommandStatus::Failure,
        ),
        (
            NorthclockError::Unavailable("x".into()),
            CommandStatus::Unavailable,
        ),
        (
            NorthclockError::PermissionOrSafety("x".into()),
            CommandStatus::Rejected,
        ),
        (
            NorthclockError::HardwareOperation("x".into()),
            CommandStatus::Failure,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(
            CommandEnvelope::failure("test", None, error).status,
            expected
        );
    }
}
