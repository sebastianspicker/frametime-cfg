//! Bounded, read-only WHEA observation through the Windows Event Log API.
//!
//! `EvtQuery` returns the newest matching records first.  This lets the caller's
//! duration bound both the history window and the time spent enumerating it,
//! while the record and XML limits keep malformed or unusually large records
//! from causing unbounded allocation.

use northclock_core::{NorthclockError, ObservedEvent, Result};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::HRESULT;
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, ERROR_TIMEOUT,
};
use windows::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml, EVT_HANDLE,
};

const WHEA_PROVIDER: &str = "Microsoft-Windows-WHEA-Logger";
const WHEA_QUERY: &str = "*[System[Provider[@Name='Microsoft-Windows-WHEA-Logger']]]";
const MAX_RECORDS: usize = 128;
const MAX_RENDERED_XML_BYTES: usize = 64 * 1024;
const MAX_ENUMERATION_TIME: Duration = Duration::from_secs(5);

/// Reads recent WHEA records from the local System event channel.
///
/// This is a snapshot, not a subscription: `duration` selects the recent
/// history window. Enumeration never waits for future records and is capped at
/// [`MAX_RECORDS`] records and five seconds of local processing.
pub(crate) fn observe_whea(duration: Duration) -> Result<Vec<ObservedEvent>> {
    if duration.is_zero() {
        return Err(NorthclockError::InvalidUsage(
            "WHEA observation requires a non-zero history duration".into(),
        ));
    }

    let started = Instant::now();
    let deadline = started
        .checked_add(duration.min(MAX_ENUMERATION_TIME))
        .unwrap_or(started);
    let earliest = SystemTime::now()
        .checked_sub(duration)
        .unwrap_or(UNIX_EPOCH);
    let query = EventHandle::query()?;
    let mut observations = Vec::new();

    while observations.len() < MAX_RECORDS && Instant::now() < deadline {
        let remaining = MAX_RECORDS - observations.len();
        let mut raw_events = vec![0_isize; remaining.min(16)];
        let mut returned = 0_u32;
        let next = unsafe { EvtNext(query.0, &mut raw_events, 0, 0, &raw mut returned) };
        match next {
            Ok(()) => {}
            Err(error) if is_win32_error(&error, ERROR_NO_MORE_ITEMS.0) => break,
            Err(error) if is_win32_error(&error, ERROR_TIMEOUT.0) => break,
            Err(error) => return Err(event_log_error("EvtNext", error)),
        }

        let returned = usize::try_from(returned).map_err(|error| {
            NorthclockError::Internal(format!("EvtNext returned an invalid count: {error}"))
        })?;
        if returned == 0 || returned > raw_events.len() {
            return Err(NorthclockError::Internal(
                "EvtNext returned an invalid event count".into(),
            ));
        }

        // Wrap every returned handle before parsing. If one record fails to
        // render or is outside the time window, the remaining handles still
        // close during Vec cleanup.
        let events = raw_events
            .drain(..returned)
            .map(|raw_event| EventHandle(EVT_HANDLE(raw_event)))
            .collect::<Vec<_>>();
        for event in events {
            let xml = render_event_xml(event.0)?;
            let Some(observation) = parse_whea_event(&xml)? else {
                continue;
            };
            let timestamp = UNIX_EPOCH
                .checked_add(Duration::from_millis(
                    u64::try_from(observation.timestamp_unix_ms).map_err(|error| {
                        NorthclockError::Internal(format!(
                            "WHEA timestamp could not fit a Windows duration: {error}"
                        ))
                    })?,
                ))
                .ok_or_else(|| {
                    NorthclockError::Internal("WHEA timestamp overflowed SystemTime".into())
                })?;
            if timestamp < earliest {
                // The query is reverse chronological, so all later records are older too.
                return Ok(observations);
            }
            observations.push(observation);
            if observations.len() == MAX_RECORDS || Instant::now() >= deadline {
                return Ok(observations);
            }
        }
    }

    Ok(observations)
}

struct EventHandle(EVT_HANDLE);

impl EventHandle {
    fn query() -> Result<Self> {
        let path = to_utf16z("System");
        let query = to_utf16z(WHEA_QUERY);
        let handle = unsafe {
            EvtQuery(
                None,
                windows::core::PCWSTR(path.as_ptr()),
                windows::core::PCWSTR(query.as_ptr()),
                EvtQueryChannelPath.0 | EvtQueryReverseDirection.0,
            )
        }
        .map_err(|error| event_log_error("EvtQuery(System WHEA)", error))?;
        Ok(Self(handle))
    }
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        // `EvtClose` is the documented cleanup path for query and event handles.
        let _ = unsafe { EvtClose(self.0) };
    }
}

fn render_event_xml(event: EVT_HANDLE) -> Result<String> {
    let mut needed_bytes = 0_u32;
    let mut property_count = 0_u32;
    let sizing = unsafe {
        EvtRender(
            None,
            event,
            EvtRenderEventXml.0,
            0,
            None,
            &raw mut needed_bytes,
            &raw mut property_count,
        )
    };
    if let Err(error) = sizing {
        if !is_win32_error(&error, ERROR_INSUFFICIENT_BUFFER.0) {
            return Err(event_log_error("EvtRender(size)", error));
        }
    }
    if needed_bytes == 0
        || needed_bytes as usize > MAX_RENDERED_XML_BYTES
        || !needed_bytes.is_multiple_of(2)
    {
        return Err(NorthclockError::Unavailable(format!(
            "EvtRender requested an invalid or oversized XML buffer ({needed_bytes} bytes)"
        )));
    }
    let unit_count = usize::try_from(needed_bytes / 2).map_err(|error| {
        NorthclockError::Internal(format!(
            "EvtRender XML buffer size overflowed usize: {error}"
        ))
    })?;
    let mut buffer = vec![0_u16; unit_count];
    let mut used_bytes = 0_u32;
    let mut property_count = 0_u32;
    unsafe {
        EvtRender(
            None,
            event,
            EvtRenderEventXml.0,
            needed_bytes,
            Some(buffer.as_mut_ptr().cast()),
            &raw mut used_bytes,
            &raw mut property_count,
        )
    }
    .map_err(|error| event_log_error("EvtRender(XML)", error))?;
    if used_bytes == 0 || used_bytes > needed_bytes || !used_bytes.is_multiple_of(2) {
        return Err(NorthclockError::Internal(
            "EvtRender returned an invalid XML byte count".into(),
        ));
    }
    let used_units = usize::try_from(used_bytes / 2).map_err(|error| {
        NorthclockError::Internal(format!("EvtRender XML length overflowed usize: {error}"))
    })?;
    let units = buffer
        .get(..used_units)
        .ok_or_else(|| NorthclockError::Internal("EvtRender exceeded its XML buffer".into()))?;
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units.len());
    String::from_utf16(&units[..end]).map_err(|error| {
        NorthclockError::HardwareOperation(format!(
            "EvtRender returned invalid UTF-16 XML: {error}"
        ))
    })
}

fn parse_whea_event(xml: &str) -> Result<Option<ObservedEvent>> {
    let provider = xml_attribute(xml, "Provider", "Name").ok_or_else(|| {
        NorthclockError::HardwareOperation(
            "WHEA Event Log XML omitted System/Provider/@Name".into(),
        )
    })?;
    if provider != WHEA_PROVIDER {
        return Ok(None);
    }
    let event_id = xml_element_text(xml, "EventID")
        .ok_or_else(|| {
            NorthclockError::HardwareOperation("WHEA Event Log XML omitted System/EventID".into())
        })?
        .parse::<u32>()
        .map_err(|error| {
            NorthclockError::HardwareOperation(format!(
                "WHEA Event Log XML had an invalid EventID: {error}"
            ))
        })?;
    let timestamp = xml_attribute(xml, "TimeCreated", "SystemTime").ok_or_else(|| {
        NorthclockError::HardwareOperation(
            "WHEA Event Log XML omitted System/TimeCreated/@SystemTime".into(),
        )
    })?;
    let timestamp_unix_ms = parse_windows_timestamp(&timestamp)?;

    Ok(Some(ObservedEvent {
        provider,
        event_id,
        timestamp_unix_ms,
        // The OS-rendered XML contains the System and EventData fields without
        // inventing a localized message or invoking a shell command.
        detail: xml.to_owned(),
    }))
}

fn xml_attribute(xml: &str, element: &str, attribute: &str) -> Option<String> {
    let element_start = format!("<{element}");
    let start = xml.find(&element_start)?;
    let tag_end = xml[start..].find('>')? + start;
    let tag = &xml[start..tag_end];
    let attribute_start = format!(" {attribute}=\"");
    let value_start = tag.find(&attribute_start)? + attribute_start.len();
    let value_end = tag[value_start..].find('"')? + value_start;
    Some(xml_unescape(&tag[value_start..value_end]))
}

fn xml_element_text<'a>(xml: &'a str, element: &str) -> Option<&'a str> {
    let open = format!("<{element}");
    let open_start = xml.find(&open)?;
    let content_start = xml[open_start..].find('>')? + open_start + 1;
    let close = format!("</{element}>");
    let content_end = xml[content_start..].find(&close)? + content_start;
    Some(xml[content_start..content_end].trim())
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn parse_windows_timestamp(value: &str) -> Result<u128> {
    let value = value.strip_suffix('Z').ok_or_else(|| {
        NorthclockError::HardwareOperation("WHEA Event Log timestamp was not UTC".into())
    })?;
    let (date, time) = value.split_once('T').ok_or_else(|| {
        NorthclockError::HardwareOperation("WHEA Event Log timestamp was not ISO-8601".into())
    })?;
    let mut date_parts = date.split('-');
    let year = parse_timestamp_part(date_parts.next(), "year")? as i64;
    let month = parse_timestamp_part(date_parts.next(), "month")? as i64;
    let day = parse_timestamp_part(date_parts.next(), "day")? as i64;
    if date_parts.next().is_some()
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
    {
        return Err(NorthclockError::HardwareOperation(
            "WHEA Event Log timestamp had an invalid date".into(),
        ));
    }
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut time_parts = clock.split(':');
    let hour = parse_timestamp_part(time_parts.next(), "hour")? as i64;
    let minute = parse_timestamp_part(time_parts.next(), "minute")? as i64;
    let second = parse_timestamp_part(time_parts.next(), "second")? as i64;
    if time_parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return Err(NorthclockError::HardwareOperation(
            "WHEA Event Log timestamp had an invalid time".into(),
        ));
    }
    let milliseconds = fraction_to_milliseconds(fraction)? as i64;
    let days = days_from_civil(year, month, day).ok_or_else(|| {
        NorthclockError::HardwareOperation(
            "WHEA Event Log timestamp had an invalid calendar date".into(),
        )
    })?;
    let seconds = days
        .checked_mul(86_400)
        .and_then(|value| value.checked_add(hour * 3_600 + minute * 60 + second))
        .ok_or_else(|| {
            NorthclockError::HardwareOperation("WHEA Event Log timestamp overflowed".into())
        })?;
    if seconds < 0 {
        return Err(NorthclockError::HardwareOperation(
            "WHEA Event Log timestamp predates Unix time".into(),
        ));
    }
    Ok((seconds as u128) * 1_000 + milliseconds as u128)
}

fn parse_timestamp_part(value: Option<&str>, name: &str) -> Result<u32> {
    value
        .ok_or_else(|| {
            NorthclockError::HardwareOperation(format!("WHEA timestamp omitted {name}"))
        })?
        .parse::<u32>()
        .map_err(|error| {
            NorthclockError::HardwareOperation(format!(
                "WHEA timestamp had an invalid {name}: {error}"
            ))
        })
}

fn fraction_to_milliseconds(fraction: &str) -> Result<u32> {
    if fraction.is_empty() {
        return Ok(0);
    }
    if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(NorthclockError::HardwareOperation(
            "WHEA Event Log timestamp had an invalid fraction".into(),
        ));
    }
    let mut milliseconds = 0_u32;
    for (index, byte) in fraction.bytes().take(3).enumerate() {
        let place = match index {
            0 => 100,
            1 => 10,
            _ => 1,
        };
        milliseconds += u32::from(byte - b'0') * place;
    }
    Ok(milliseconds)
}

// Howard Hinnant's civil-date conversion, returning days since 1970-01-01.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let year = year.checked_sub(i64::from(month <= 2))?;
    let era = if year >= 0 {
        year
    } else {
        year.checked_sub(399)?
    } / 400;
    let year_of_era = year.checked_sub(era.checked_mul(400)?)?;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era.checked_mul(146_097)?
        .checked_add(day_of_era)?
        .checked_sub(719_468)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn to_utf16z(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn is_win32_error(error: &windows::core::Error, code: u32) -> bool {
    error.code() == HRESULT::from_win32(code)
}

fn event_log_error(api: &str, error: windows::core::Error) -> NorthclockError {
    if is_win32_error(&error, ERROR_ACCESS_DENIED.0) {
        NorthclockError::Unavailable(format!(
            "{api} was denied access; read permission for the System event log (for example Event Log Readers membership) is required"
        ))
    } else {
        NorthclockError::HardwareOperation(format!(
            "{api} failed with Windows error {}: {}",
            error.code(),
            error.message()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_whea_event, parse_windows_timestamp};

    #[test]
    fn parses_rendered_whea_event_without_fabricating_details() {
        let xml = r#"<Event><System><Provider Name="Microsoft-Windows-WHEA-Logger"/><EventID>18</EventID><TimeCreated SystemTime="2026-08-10T08:15:30.125Z"/></System><EventData><Data Name="ErrorSource">3</Data></EventData></Event>"#;
        let event = parse_whea_event(xml)
            .unwrap_or_else(|error| panic!("fixture failed: {error}"))
            .unwrap_or_else(|| panic!("WHEA fixture was not recognized"));
        assert_eq!(event.provider, "Microsoft-Windows-WHEA-Logger");
        assert_eq!(event.event_id, 18);
        assert_eq!(event.detail, xml);
        assert_eq!(event.timestamp_unix_ms, 1_786_349_730_125);
    }

    #[test]
    fn rejects_non_utc_timestamps() {
        assert!(parse_windows_timestamp("2026-08-10T08:15:30+02:00").is_err());
    }
}
