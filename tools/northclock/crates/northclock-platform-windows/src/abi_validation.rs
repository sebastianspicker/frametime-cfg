#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbiValidationError {
    InvalidVersion,
    MissingValue,
    OutOfRange,
    InvalidBounds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EtwPresentHeaderFields {
    pub total_size: usize,
    pub header_size: usize,
    pub user_data_length: usize,
    pub user_data_present: bool,
    pub provider_matches: bool,
    pub event_id: u16,
    pub expected_event_id: u16,
    pub process_id: u32,
    pub timestamp_100ns: i64,
    pub minimum_timestamp_100ns: i64,
}

#[must_use]
pub fn validate_etw_present_header(fields: EtwPresentHeaderFields) -> bool {
    fields.total_size >= fields.header_size
        && fields.total_size <= 64 * 1024
        && fields.user_data_length <= fields.total_size - fields.header_size
        && (fields.user_data_length == 0 || fields.user_data_present)
        && fields.provider_matches
        && fields.event_id == fields.expected_event_id
        && fields.process_id != 0
        && fields.timestamp_100ns >= fields.minimum_timestamp_100ns
}

pub fn validate_nvapi_load_fields(
    version: u32,
    expected_version: u32,
    present: u32,
    percentage: u32,
) -> Result<f64, AbiValidationError> {
    if version != expected_version {
        return Err(AbiValidationError::InvalidVersion);
    }
    if present & 1 == 0 {
        return Err(AbiValidationError::MissingValue);
    }
    if percentage > 100 {
        return Err(AbiValidationError::OutOfRange);
    }
    Ok(f64::from(percentage))
}

pub fn validate_nvapi_thermal_header(
    version: u32,
    expected_version: u32,
    count: usize,
    capacity: usize,
) -> Result<(), AbiValidationError> {
    if version != expected_version {
        return Err(AbiValidationError::InvalidVersion);
    }
    if count == 0 {
        return Err(AbiValidationError::MissingValue);
    }
    if count > capacity {
        return Err(AbiValidationError::OutOfRange);
    }
    Ok(())
}

pub fn validate_nvapi_temperature_fields(
    default_minimum: i32,
    default_maximum: i32,
    current: i32,
) -> Result<f64, AbiValidationError> {
    if default_minimum > default_maximum {
        return Err(AbiValidationError::InvalidBounds);
    }
    if !(-50..=200).contains(&default_minimum)
        || !(-50..=200).contains(&default_maximum)
        || !(-50..=200).contains(&current)
    {
        return Err(AbiValidationError::OutOfRange);
    }
    Ok(f64::from(current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etw_header_rejects_underflow_before_subtraction() {
        let fields = EtwPresentHeaderFields {
            total_size: 7,
            header_size: 8,
            user_data_length: usize::MAX,
            user_data_present: false,
            provider_matches: true,
            event_id: 180,
            expected_event_id: 180,
            process_id: 1,
            timestamp_100ns: 1,
            minimum_timestamp_100ns: 0,
        };
        assert!(!validate_etw_present_header(fields));
    }

    #[test]
    fn vendor_ranges_are_closed() {
        assert_eq!(validate_nvapi_load_fields(1, 1, 1, 100), Ok(100.0));
        assert_eq!(
            validate_nvapi_load_fields(1, 1, 1, 101),
            Err(AbiValidationError::OutOfRange)
        );
        assert_eq!(validate_nvapi_temperature_fields(-50, 200, 200), Ok(200.0));
        assert_eq!(
            validate_nvapi_temperature_fields(0, 100, 201),
            Err(AbiValidationError::OutOfRange)
        );
        assert_eq!(validate_nvapi_thermal_header(3, 3, 1, 3), Ok(()));
        assert_eq!(
            validate_nvapi_thermal_header(3, 3, 0, 3),
            Err(AbiValidationError::MissingValue)
        );
    }
}
