//! Bounded, read-only WHEA observations through the Windows Event Log API.

use frametime_hardware::{DiagnosticError, WheaEvent};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS};
use windows::Win32::System::EventLog::{
    EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection,
    EvtRender, EvtRenderEventXml,
};

const WHEA_PROVIDER: &str = "Microsoft-Windows-WHEA-Logger";
const WHEA_QUERY: &str = "*[System[Provider[@Name='Microsoft-Windows-WHEA-Logger']]]";
const MAX_RENDERED_XML_BYTES: usize = 64 * 1024;

pub(crate) fn read_whea_events(max_records: u16) -> Result<Vec<WheaEvent>, DiagnosticError> {
    let query = EventHandle::query()?;
    let mut records = Vec::with_capacity(usize::from(max_records));
    while records.len() < usize::from(max_records) {
        let remaining = usize::from(max_records) - records.len();
        let mut raw_events = vec![0_isize; remaining.min(16)];
        let mut returned = 0_u32;
        match unsafe { EvtNext(query.0, &mut raw_events, 0, 0, &raw mut returned) } {
            Ok(()) => {}
            Err(error) if is_win32_error(&error, ERROR_NO_MORE_ITEMS.0) => break,
            Err(error) => return Err(event_log_error("EvtNext", error)),
        }
        let returned = usize::try_from(returned)
            .map_err(|error| DiagnosticError::system(format!("EvtNext count: {error}")))?;
        if returned == 0 || returned > raw_events.len() {
            return Err(DiagnosticError::system(
                "EvtNext returned an invalid event count",
            ));
        }
        for raw_event in raw_events.drain(..returned) {
            let event = EventHandle(EVT_HANDLE(raw_event));
            let xml = render_event_xml(event.0)?;
            if let Some(record) = parse_whea_event(&xml)? {
                records.push(record);
            }
        }
    }
    Ok(records)
}

struct EventHandle(EVT_HANDLE);

impl EventHandle {
    fn query() -> Result<Self, DiagnosticError> {
        let path = utf16z("System");
        let query = utf16z(WHEA_QUERY);
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
        let _ = unsafe { EvtClose(self.0) };
    }
}

fn render_event_xml(event: EVT_HANDLE) -> Result<String, DiagnosticError> {
    let mut needed_bytes = 0_u32;
    let mut property_count = 0_u32;
    if let Err(error) = unsafe {
        EvtRender(
            None,
            event,
            EvtRenderEventXml.0,
            0,
            None,
            &raw mut needed_bytes,
            &raw mut property_count,
        )
    } && !is_win32_error(&error, ERROR_INSUFFICIENT_BUFFER.0)
    {
        return Err(event_log_error("EvtRender(size)", error));
    }
    if needed_bytes == 0
        || needed_bytes as usize > MAX_RENDERED_XML_BYTES
        || !needed_bytes.is_multiple_of(2)
    {
        return Err(DiagnosticError::system(
            "EvtRender requested an invalid XML buffer size",
        ));
    }
    let units = usize::try_from(needed_bytes / 2)
        .map_err(|error| DiagnosticError::system(format!("EvtRender buffer: {error}")))?;
    let mut buffer = vec![0_u16; units];
    let mut used_bytes = 0_u32;
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
        return Err(DiagnosticError::system(
            "EvtRender returned an invalid XML byte count",
        ));
    }
    let end = usize::try_from(used_bytes / 2)
        .map_err(|error| DiagnosticError::system(format!("EvtRender length: {error}")))?;
    let used = buffer
        .get(..end)
        .ok_or_else(|| DiagnosticError::system("EvtRender exceeded its XML buffer"))?;
    let nul = used
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(used.len());
    String::from_utf16(&used[..nul])
        .map_err(|error| DiagnosticError::system(format!("EvtRender invalid UTF-16: {error}")))
}

fn parse_whea_event(xml: &str) -> Result<Option<WheaEvent>, DiagnosticError> {
    let provider = xml_attribute(xml, "Provider", "Name")
        .ok_or_else(|| DiagnosticError::system("WHEA XML omitted provider name"))?;
    if provider != WHEA_PROVIDER {
        return Ok(None);
    }
    let event_id = xml_element_text(xml, "EventID")
        .ok_or_else(|| DiagnosticError::system("WHEA XML omitted event id"))?
        .parse()
        .map_err(|error| DiagnosticError::system(format!("WHEA XML event id: {error}")))?;
    Ok(Some(WheaEvent {
        provider,
        event_id,
        timestamp_utc: xml_attribute(xml, "TimeCreated", "SystemTime"),
        detail_xml: xml.into(),
    }))
}

fn xml_attribute(xml: &str, element: &str, attribute: &str) -> Option<String> {
    let start = xml.find(&format!("<{element}"))?;
    let end = xml[start..].find('>')? + start;
    let tag = &xml[start..end];
    let prefix = format!(" {attribute}=\"");
    let value_start = tag.find(&prefix)? + prefix.len();
    let value_end = tag[value_start..].find('"')? + value_start;
    Some(xml_unescape(&tag[value_start..value_end]))
}

fn xml_element_text<'a>(xml: &'a str, element: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{element}"))?;
    let value_start = xml[start..].find('>')? + start + 1;
    let value_end = xml[value_start..].find(&format!("</{element}>"))? + value_start;
    Some(xml[value_start..value_end].trim())
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn utf16z(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn is_win32_error(error: &windows::core::Error, expected: u32) -> bool {
    error.code().0 as u32 == (0x8007_0000 | expected)
}

fn event_log_error(api: &str, error: windows::core::Error) -> DiagnosticError {
    DiagnosticError::system(format!("{api} failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::parse_whea_event;

    #[test]
    fn parses_os_rendered_whea_event_without_message_lookup() {
        let xml = r#"<Event><System><Provider Name="Microsoft-Windows-WHEA-Logger"/><EventID>18</EventID><TimeCreated SystemTime="2026-01-02T03:04:05Z"/></System></Event>"#;
        let event = parse_whea_event(xml)
            .unwrap_or_else(|error| panic!("parse failed: {error:?}"))
            .unwrap_or_else(|| panic!("WHEA event was not recognized"));
        assert_eq!(event.event_id, 18);
        assert_eq!(event.timestamp_utc.as_deref(), Some("2026-01-02T03:04:05Z"));
    }
}
