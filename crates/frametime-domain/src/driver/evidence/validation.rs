use super::ValidationError;

pub(super) fn timestamp(value: &str, field: &'static str) -> Result<i64, ValidationError> {
    let bytes = value.as_bytes();
    if !has_rfc3339_second_layout(bytes) {
        return Err(ValidationError::Invalid { field });
    }
    let parts = TimestampParts::parse(bytes, field)?;
    parts.validate(field)?;
    parts.epoch_seconds(field)
}

#[derive(Clone, Copy)]
struct TimestampParts {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
}

impl TimestampParts {
    fn parse(bytes: &[u8], field: &'static str) -> Result<Self, ValidationError> {
        Ok(Self {
            year: timestamp_number(bytes, 0, 4, field)?,
            month: timestamp_number(bytes, 5, 7, field)?,
            day: timestamp_number(bytes, 8, 10, field)?,
            hour: timestamp_number(bytes, 11, 13, field)?,
            minute: timestamp_number(bytes, 14, 16, field)?,
            second: timestamp_number(bytes, 17, 19, field)?,
        })
    }

    fn validate(self, field: &'static str) -> Result<(), ValidationError> {
        let month_days = days_in_month(self.year, self.month, field)?;
        if self.values_are_in_range(month_days) {
            Ok(())
        } else {
            Err(ValidationError::Invalid { field })
        }
    }

    fn values_are_in_range(self, month_days: i64) -> bool {
        [
            self.year != 0,
            self.day != 0,
            self.day <= month_days,
            self.hour <= 23,
            self.minute <= 59,
            self.second <= 59,
        ]
        .into_iter()
        .all(|is_valid| is_valid)
    }

    fn epoch_seconds(self, field: &'static str) -> Result<i64, ValidationError> {
        let completed_years = self.year - 1;
        let days_before_year = completed_years * 365 + completed_years / 4 - completed_years / 100
            + completed_years / 400;
        let month_days = [0_i64, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let month_index =
            usize::try_from(self.month - 1).map_err(|_| ValidationError::Invalid { field })?;
        let mut days = days_before_year + month_days[month_index] + self.day - 1;
        if self.month > 2 && is_leap_year(self.year) {
            days += 1;
        }
        Ok(days * 86_400 + self.hour * 3_600 + self.minute * 60 + self.second)
    }
}

fn has_rfc3339_second_layout(bytes: &[u8]) -> bool {
    const SEPARATORS: [(usize, u8); 6] = [
        (4, b'-'),
        (7, b'-'),
        (10, b'T'),
        (13, b':'),
        (16, b':'),
        (19, b'Z'),
    ];
    bytes.len() == 20
        && SEPARATORS
            .iter()
            .all(|(position, expected)| bytes[*position] == *expected)
}

fn timestamp_number(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: &'static str,
) -> Result<i64, ValidationError> {
    bytes[start..end]
        .iter()
        .try_fold(0_i64, |value, byte| match byte {
            b'0'..=b'9' => Ok(value * 10 + i64::from(byte - b'0')),
            _ => Err(ValidationError::Invalid { field }),
        })
}

fn days_in_month(year: i64, month: i64, field: &'static str) -> Result<i64, ValidationError> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Ok(31),
        4 | 6 | 9 | 11 => Ok(30),
        2 if is_leap_year(year) => Ok(29),
        2 => Ok(28),
        _ => Err(ValidationError::Invalid { field }),
    }
}

const fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub(super) fn text(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}
