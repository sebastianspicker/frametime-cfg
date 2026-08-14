//! Bounded real-time DxgKrnl `Present_Start` capture through native ETW.
//!
//! A sample is the interval between consecutive present-start records for one
//! process. It does not claim presentation completion or display timing.

use frametime_hardware::{DiagnosticError, FrameSample};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_CANCELLED, ERROR_SUCCESS, GetLastError,
    WIN32_ERROR,
};
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
    EVENT_HEADER, EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_LOGFILEW_0, EVENT_TRACE_LOGFILEW_1, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2, OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE, ProcessTrace, StartTraceW,
    TRACE_LEVEL_VERBOSE, WNODE_FLAG_TRACED_GUID,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::{GUID, PCWSTR, PWSTR};

const MAX_SAMPLES: usize = 4_096;
const MAX_FRAME_INTERVAL_100NS: u64 = 60 * 10_000_000;
const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
const INVALID_PROCESSTRACE_HANDLE: u64 = u64::MAX;
const DXGKRNL_PROVIDER: GUID = GUID::from_u128(0x802e_c45a_1e99_4b83_9920_87c9_8277_ba9d);
const DXGKRNL_PRESENT_START_EVENT_ID: u16 = 180;
const TRACE_SESSION_GUID: GUID = GUID::from_u128(0x0056_f276_88b1_4744_a73a_8940_8a0c_465e);
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn capture_present_starts(
    duration_ms: u32,
) -> Result<Vec<FrameSample>, DiagnosticError> {
    let mut session = TraceSession::start(unique_session_name())?;
    session.enable_provider()?;
    let close_once = Arc::new(AtomicBool::new(false));
    let state = Arc::new(Mutex::new(CaptureState::default()));
    let consumer = TraceConsumer::open(
        &mut session.name,
        Arc::as_ptr(&state).cast_mut().cast(),
        Arc::clone(&close_once),
    )?;
    let consumer_handle = consumer.handle;
    let worker = std::thread::Builder::new()
        .name("frametime-etw-consumer".into())
        .spawn(move || {
            let status = unsafe { ProcessTrace(&[consumer.handle], None, None) };
            drop(consumer);
            status
        })
        .map_err(|error| DiagnosticError::system(format!("ETW consumer thread: {error}")))?;
    std::thread::sleep(std::time::Duration::from_millis(u64::from(duration_ms)));
    let stop = session.stop();
    if stop.is_err() {
        close_trace_once(consumer_handle, &close_once);
    }
    let process_status = worker
        .join()
        .map_err(|_| DiagnosticError::system("ETW consumer thread panicked"))?;
    stop?;
    if process_status != ERROR_SUCCESS && process_status != ERROR_CANCELLED {
        return Err(etw_error("ProcessTrace", process_status));
    }
    state
        .lock()
        .map_err(|_| DiagnosticError::system("ETW capture state was poisoned"))
        .map(|state| state.samples.clone())
}

struct TraceSession {
    handle: CONTROLTRACE_HANDLE,
    name: Vec<u16>,
    properties: TraceProperties,
    active: bool,
}

impl TraceSession {
    fn start(name: Vec<u16>) -> Result<Self, DiagnosticError> {
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

    fn enable_provider(&self) -> Result<(), DiagnosticError> {
        let status = unsafe {
            EnableTraceEx2(
                self.handle,
                &DXGKRNL_PROVIDER,
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
            Err(etw_error("EnableTraceEx2(DxgKrnl)", status))
        }
    }

    fn stop(&mut self) -> Result<(), DiagnosticError> {
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
    // u64 backing storage preserves required structure alignment.
    storage: Vec<u64>,
}

impl TraceProperties {
    fn new(name: &[u16]) -> Result<Self, DiagnosticError> {
        let name_bytes = name
            .len()
            .checked_mul(size_of::<u16>())
            .ok_or_else(|| DiagnosticError::system("ETW session-name length overflow"))?;
        let total_bytes = size_of::<EVENT_TRACE_PROPERTIES>()
            .checked_add(name_bytes)
            .ok_or_else(|| DiagnosticError::system("ETW properties length overflow"))?;
        let total_u32 = u32::try_from(total_bytes)
            .map_err(|error| DiagnosticError::system(format!("ETW properties size: {error}")))?;
        let mut storage = vec![0_u64; total_bytes.div_ceil(size_of::<u64>())];
        let properties = storage.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
        unsafe {
            properties.write(EVENT_TRACE_PROPERTIES::default());
            (*properties).Wnode.BufferSize = total_u32;
            (*properties).Wnode.Guid = TRACE_SESSION_GUID;
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
    fn open(
        name: &mut [u16],
        context: *mut c_void,
        closed: Arc<AtomicBool>,
    ) -> Result<Self, DiagnosticError> {
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
    previous_present: BTreeMap<u32, u64>,
    samples: Vec<FrameSample>,
}

unsafe extern "system" fn on_event_record(record: *mut EVENT_RECORD) {
    let Some(record) = (unsafe { record.as_ref() }) else {
        return;
    };
    let header = &record.EventHeader;
    if header.ProviderId != DXGKRNL_PROVIDER
        || header.EventDescriptor.Id != DXGKRNL_PRESENT_START_EVENT_ID
        || header.ProcessId == 0
        || header.TimeStamp < WINDOWS_TO_UNIX_EPOCH_100NS as i64
        || usize::from(header.Size) < size_of::<EVENT_HEADER>()
    {
        return;
    }
    let Some(state) = (unsafe { record.UserContext.cast::<Mutex<CaptureState>>().as_ref() }) else {
        return;
    };
    let Ok(mut state) = state.lock() else {
        return;
    };
    if state.samples.len() >= MAX_SAMPLES {
        return;
    }
    let timestamp = header.TimeStamp as u64;
    let previous = state.previous_present.insert(header.ProcessId, timestamp);
    let Some(previous) = previous else {
        return;
    };
    let interval = timestamp.saturating_sub(previous);
    if interval == 0 || interval > MAX_FRAME_INTERVAL_100NS {
        return;
    }
    state.samples.push(FrameSample {
        process_id: header.ProcessId,
        present_start_unix_ms: (timestamp - WINDOWS_TO_UNIX_EPOCH_100NS) / 10_000,
        frame_time_us: interval / 10,
        source: "ETW Microsoft-Windows-DxgKrnl/Present_Start".into(),
    });
}

fn close_trace_once(handle: PROCESSTRACE_HANDLE, closed: &AtomicBool) {
    if !closed.swap(true, Ordering::AcqRel) {
        let _ = unsafe { CloseTrace(handle) };
    }
}

fn unique_session_name() -> Vec<u16> {
    let process_id = unsafe { GetCurrentProcessId() };
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("frametime-etw-{process_id}-{counter}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

fn etw_error(api: &str, code: WIN32_ERROR) -> DiagnosticError {
    if code == ERROR_ACCESS_DENIED {
        DiagnosticError {
            code: frametime_hardware::DiagnosticErrorCode::PermissionDenied,
            message: format!(
                "{api} was denied; ETW capture needs elevation or Performance Log Users"
            ),
        }
    } else if code == ERROR_ALREADY_EXISTS {
        DiagnosticError::unavailable(format!("{api} encountered an ETW session-name collision"))
    } else {
        DiagnosticError::system(format!("{api} failed with Win32 error {}", code.0))
    }
}
