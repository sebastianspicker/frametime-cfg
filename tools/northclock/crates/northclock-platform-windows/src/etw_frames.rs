//! Bounded real-time present capture through documented Event Tracing for Windows APIs.
//!
//! This intentionally uses the `Microsoft-Windows-DxgKrnl` Present_Start event
//! that PresentMon consumes as an input. It reports the interval between two
//! actual present-start records from the same process; it does not infer frame
//! completion, display timing, or samples for processes that did not present.

use crate::abi_validation::{validate_etw_present_header, EtwPresentHeaderFields};
use northclock_core::{DeviceIdentity, FrameSample, Measurement, NorthclockError, Result};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_CANCELLED, ERROR_NOT_SUPPORTED,
    ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    CONTROLTRACE_HANDLE, EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_HEADER, EVENT_RECORD,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG, EVENT_TRACE_LOGFILEW, EVENT_TRACE_LOGFILEW_0,
    EVENT_TRACE_LOGFILEW_1, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE,
    PROCESSTRACE_HANDLE, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME,
    TRACE_LEVEL_VERBOSE, WNODE_FLAG_TRACED_GUID,
};
use windows::Win32::System::Threading::GetCurrentProcessId;

const MAX_CAPTURE_DURATION: Duration = Duration::from_secs(60);
const MAX_SAMPLES: usize = 4_096;
const MAX_FRAME_INTERVAL_100NS: u64 = 60 * 10_000_000;
const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
const INVALID_PROCESSTRACE_HANDLE: u64 = u64::MAX;

// Microsoft-Windows-DxgKrnl, documented by the provider manifest used by
// PresentMon. Event 180 is DxgKrnl/Present_Start.
const DXGKRNL_PROVIDER: GUID = GUID::from_u128(0x802e_c45a_1e99_4b83_9920_87c9_8277_ba9d);
const DXGKRNL_PRESENT_START_EVENT_ID: u16 = 180;
const TRACE_SESSION_GUID: GUID = GUID::from_u128(0x00cd_0e6d_2f6f_4f84_8a20_3135_f1f4_0201);
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Captures real present-start intervals from the local DxgKrnl ETW provider.
pub(crate) fn capture_frames(duration: Duration) -> Result<Vec<FrameSample>> {
    if duration.is_zero() {
        return Err(NorthclockError::InvalidUsage(
            "ETW frame capture requires a non-zero duration".into(),
        ));
    }
    if duration > MAX_CAPTURE_DURATION {
        return Err(NorthclockError::InvalidUsage(format!(
            "ETW frame capture duration must not exceed {} seconds",
            MAX_CAPTURE_DURATION.as_secs()
        )));
    }

    let mut session = TraceSession::start(unique_session_name())?;
    session.enable_provider(&DXGKRNL_PROVIDER)?;

    let close_once = Arc::new(AtomicBool::new(false));
    let state = Arc::new(Mutex::new(CaptureState::default()));
    let consumer = TraceConsumer::open(
        &mut session.name,
        Arc::as_ptr(&state).cast_mut().cast(),
        Arc::clone(&close_once),
    )?;
    let consumer_handle = consumer.handle;
    let worker = std::thread::Builder::new()
        .name("northclock-etw-consumer".into())
        .spawn(move || {
            let status = unsafe { ProcessTrace(&[consumer.handle], None, None) };
            drop(consumer);
            status
        })
        .map_err(|error| {
            NorthclockError::Internal(format!("could not start ETW consumer thread: {error}"))
        })?;

    std::thread::sleep(duration);
    let stop_result = session.stop();
    if stop_result.is_err() {
        // A failed stop must not leave the consumer blocked indefinitely. CloseTrace
        // is the documented way to cancel an open processing handle.
        close_trace_once(consumer_handle, &close_once);
    }
    let process_status = worker.join().map_err(|_| {
        NorthclockError::Internal("ETW consumer thread panicked while processing trace data".into())
    })?;
    stop_result?;
    if process_status != ERROR_SUCCESS && process_status != ERROR_CANCELLED {
        return Err(etw_error("ProcessTrace", process_status));
    }

    let state = state
        .lock()
        .map_err(|_| NorthclockError::Internal("ETW callback state was poisoned".into()))?;
    Ok(state.samples.clone())
}

struct TraceSession {
    handle: CONTROLTRACE_HANDLE,
    name: Vec<u16>,
    properties: TraceProperties,
    active: bool,
}

impl TraceSession {
    fn start(name: Vec<u16>) -> Result<Self> {
        let mut session = Self {
            handle: CONTROLTRACE_HANDLE::default(),
            properties: TraceProperties::new(&name)?,
            name,
            active: false,
        };
        let status = unsafe {
            StartTraceW(
                &raw mut session.handle,
                PCWSTR(session.name.as_ptr()),
                session.properties.as_mut_ptr(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(etw_error("StartTraceW", status));
        }
        session.active = true;
        Ok(session)
    }

    fn enable_provider(&self, provider: &GUID) -> Result<()> {
        let status = unsafe {
            EnableTraceEx2(
                self.handle,
                provider,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                TRACE_LEVEL_VERBOSE as u8,
                0,
                0,
                0,
                None,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(etw_error(
                "EnableTraceEx2(Microsoft-Windows-DxgKrnl)",
                status,
            ))
        }
    }

    fn stop(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let status = unsafe {
            ControlTraceW(
                self.handle,
                PCWSTR::null(),
                self.properties.as_mut_ptr(),
                EVENT_TRACE_CONTROL_STOP,
            )
        };
        if status == ERROR_SUCCESS {
            self.active = false;
            Ok(())
        } else {
            Err(etw_error("ControlTraceW(stop)", status))
        }
    }
}

impl Drop for TraceSession {
    fn drop(&mut self) {
        if self.active {
            let _ = unsafe {
                ControlTraceW(
                    self.handle,
                    PCWSTR::null(),
                    self.properties.as_mut_ptr(),
                    EVENT_TRACE_CONTROL_STOP,
                )
            };
        }
    }
}

struct TraceProperties {
    // `EVENT_TRACE_PROPERTIES` contains 64-bit fields. A byte vector does not
    // promise enough alignment for a typed Windows structure, so retain storage
    // in u64 words and address it as bytes only for the trailing UTF-16 name.
    storage: Vec<u64>,
}

impl TraceProperties {
    fn new(name: &[u16]) -> Result<Self> {
        let name_bytes = name.len().checked_mul(size_of::<u16>()).ok_or_else(|| {
            NorthclockError::Internal("ETW session name length overflowed".into())
        })?;
        let total_bytes = size_of::<EVENT_TRACE_PROPERTIES>()
            .checked_add(name_bytes)
            .ok_or_else(|| NorthclockError::Internal("ETW properties length overflowed".into()))?;
        let total_u32 = u32::try_from(total_bytes).map_err(|error| {
            NorthclockError::Internal(format!("ETW properties were too large: {error}"))
        })?;
        let storage_words = total_bytes.div_ceil(size_of::<u64>());
        let mut storage = vec![0_u64; storage_words];
        let properties = storage.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
        unsafe {
            properties.write(EVENT_TRACE_PROPERTIES::default());
            (*properties).Wnode.BufferSize = total_u32;
            (*properties).Wnode.Guid = TRACE_SESSION_GUID;
            // Client context 1 requests system time, expressed as FILETIME
            // 100-nanosecond ticks, for every EVENT_RECORD timestamp.
            (*properties).Wnode.ClientContext = 1;
            (*properties).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*properties).BufferSize = 64;
            (*properties).MinimumBuffers = 2;
            (*properties).MaximumBuffers = 8;
            (*properties).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            (*properties).EnableFlags = EVENT_TRACE_FLAG(0);
            (*properties).LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
            let destination = storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(size_of::<EVENT_TRACE_PROPERTIES>())
                .cast::<u16>();
            std::ptr::copy_nonoverlapping(name.as_ptr(), destination, name.len());
        }
        Ok(Self { storage })
    }

    fn as_mut_ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.storage.as_mut_ptr().cast()
    }
}

struct TraceConsumer {
    handle: PROCESSTRACE_HANDLE,
    closed: Arc<AtomicBool>,
}

impl TraceConsumer {
    fn open(name: &mut [u16], context: *mut c_void, closed: Arc<AtomicBool>) -> Result<Self> {
        let mut logfile = EVENT_TRACE_LOGFILEW {
            LoggerName: PWSTR(name.as_mut_ptr()),
            Anonymous1: EVENT_TRACE_LOGFILEW_0 {
                ProcessTraceMode: PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD,
            },
            Anonymous2: EVENT_TRACE_LOGFILEW_1 {
                EventRecordCallback: Some(on_event_record),
            },
            Context: context,
            ..Default::default()
        };
        let handle = unsafe { OpenTraceW(&raw mut logfile) };
        if handle.Value == INVALID_PROCESSTRACE_HANDLE {
            return Err(etw_error("OpenTraceW", unsafe { GetLastError() }));
        }
        Ok(Self { handle, closed })
    }
}

impl Drop for TraceConsumer {
    fn drop(&mut self) {
        close_trace_once(self.handle, &self.closed);
    }
}

#[derive(Default)]
struct CaptureState {
    last_present_100ns: BTreeMap<u32, u64>,
    samples: Vec<FrameSample>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PresentStart {
    process_id: u32,
    timestamp_100ns: u64,
}

unsafe extern "system" fn on_event_record(record: *mut EVENT_RECORD) {
    if record.is_null() {
        return;
    }
    let record = unsafe { &*record };
    let Some(event) = parse_event_record(record) else {
        return;
    };
    let state = unsafe { record.UserContext.cast::<Mutex<CaptureState>>().as_ref() };
    let Some(state) = state else {
        return;
    };
    let Ok(mut state) = state.lock() else {
        return;
    };
    if state.samples.len() >= MAX_SAMPLES {
        return;
    }
    let previous = state
        .last_present_100ns
        .insert(event.process_id, event.timestamp_100ns);
    let Some(previous) = previous else {
        return;
    };
    let interval_100ns = event.timestamp_100ns.saturating_sub(previous);
    if interval_100ns == 0 || interval_100ns > MAX_FRAME_INTERVAL_100NS {
        return;
    }
    let timestamp_unix_ms =
        u128::from((event.timestamp_100ns - WINDOWS_TO_UNIX_EPOCH_100NS) / 10_000);
    let device = DeviceIdentity::new(
        "process",
        format!("pid-{}", event.process_id),
        format!("Process {}", event.process_id),
        None,
    );
    state.samples.push(FrameSample {
        process_id: event.process_id,
        frame_time: Measurement::at(
            interval_100ns as f64 / 10_000.0,
            "ms",
            timestamp_unix_ms,
            device,
            "ETW Microsoft-Windows-DxgKrnl/Present_Start",
        ),
    });
}

fn parse_event_record(record: &EVENT_RECORD) -> Option<PresentStart> {
    let header = &record.EventHeader;
    let total_size = usize::from(header.Size);
    let header_size = size_of::<EVENT_HEADER>();
    if !validate_etw_present_header(EtwPresentHeaderFields {
        total_size,
        header_size,
        user_data_length: usize::from(record.UserDataLength),
        user_data_present: !record.UserData.is_null(),
        provider_matches: header.ProviderId == DXGKRNL_PROVIDER,
        event_id: header.EventDescriptor.Id,
        expected_event_id: DXGKRNL_PRESENT_START_EVENT_ID,
        process_id: header.ProcessId,
        timestamp_100ns: header.TimeStamp,
        minimum_timestamp_100ns: WINDOWS_TO_UNIX_EPOCH_100NS as i64,
    }) {
        return None;
    }
    Some(PresentStart {
        process_id: header.ProcessId,
        timestamp_100ns: header.TimeStamp as u64,
    })
}

fn close_trace_once(handle: PROCESSTRACE_HANDLE, closed: &AtomicBool) {
    if !closed.swap(true, Ordering::AcqRel) {
        let _ = unsafe { CloseTrace(handle) };
    }
}

fn unique_session_name() -> Vec<u16> {
    let process_id = unsafe { GetCurrentProcessId() };
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("Northclock-ETW-Frames-{process_id}-{counter}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

fn etw_error(api: &str, code: WIN32_ERROR) -> NorthclockError {
    if code == ERROR_ACCESS_DENIED {
        NorthclockError::Unavailable(format!(
            "{api} was denied; real-time DxgKrnl ETW capture requires an elevated session or Performance Log Users permission"
        ))
    } else if code == ERROR_ALREADY_EXISTS {
        NorthclockError::Unavailable(format!(
            "{api} encountered an ETW session-name collision; retry the bounded capture"
        ))
    } else if code == ERROR_NOT_SUPPORTED {
        NorthclockError::Unavailable(format!(
            "{api} is not supported by this Windows installation or ETW provider"
        ))
    } else {
        NorthclockError::HardwareOperation(format!("{api} failed with Win32 error {}", code.0))
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_event_record, DXGKRNL_PRESENT_START_EVENT_ID, DXGKRNL_PROVIDER};
    use std::mem::size_of;
    use windows::Win32::System::Diagnostics::Etw::{EVENT_HEADER, EVENT_RECORD};

    #[test]
    fn accepts_a_valid_present_start_record() {
        let record = EVENT_RECORD {
            EventHeader: EVENT_HEADER {
                Size: size_of::<EVENT_HEADER>() as u16,
                ProcessId: 42,
                TimeStamp: 116_444_736_010_000_000,
                ProviderId: DXGKRNL_PROVIDER,
                EventDescriptor: windows::Win32::System::Diagnostics::Etw::EVENT_DESCRIPTOR {
                    Id: DXGKRNL_PRESENT_START_EVENT_ID,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let parsed = parse_event_record(&record)
            .unwrap_or_else(|| panic!("valid present event was rejected"));
        assert_eq!(parsed.process_id, 42);
        assert_eq!(parsed.timestamp_100ns, 116_444_736_010_000_000);
    }

    #[test]
    fn rejects_wrong_provider_and_undersized_records() {
        let record = EVENT_RECORD {
            EventHeader: EVENT_HEADER {
                Size: (size_of::<EVENT_HEADER>() - 1) as u16,
                ProviderId: DXGKRNL_PROVIDER,
                EventDescriptor: windows::Win32::System::Diagnostics::Etw::EVENT_DESCRIPTOR {
                    Id: DXGKRNL_PRESENT_START_EVENT_ID,
                    ..Default::default()
                },
                ProcessId: 7,
                TimeStamp: 116_444_736_010_000_000,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(parse_event_record(&record).is_none());
    }
}
