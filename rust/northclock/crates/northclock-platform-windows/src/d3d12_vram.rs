//! Bounded D3D12 copy-path validation for a physical DXGI adapter.
//!
//! The test writes a deterministic pattern through an UPLOAD buffer, copies it
//! into a DEFAULT heap resource, then copies it into a READBACK buffer for CPU
//! validation. A successful result therefore proves that this exact adapter's
//! D3D12 copy path completed and that the bytes survived the round trip.

use northclock_core::{NorthclockError, Result, WorkloadReport};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::{
    D3D12CreateDevice, ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12Device,
    ID3D12Fence, ID3D12GraphicsCommandList, ID3D12Resource, D3D12_COMMAND_LIST_TYPE_COPY,
    D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12_COMMAND_QUEUE_PRIORITY_NORMAL,
    D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_READBACK,
    D3D12_HEAP_TYPE_UPLOAD, D3D12_RANGE, D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_BARRIER_0,
    D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, D3D12_RESOURCE_BARRIER_FLAG_NONE,
    D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER,
    D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE,
    D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_TRANSITION_BARRIER,
    D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    DXGI_ERROR_NOT_FOUND,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

const MAX_TEST_BYTES: u64 = 1024 * 1024 * 1024;
const FENCE_VALUE: u64 = 1;

pub(crate) fn run_vram_test(
    requested_adapter: Option<&str>,
    bytes: u64,
    timeout: Duration,
) -> Result<WorkloadReport> {
    validate_request(bytes, timeout)?;
    let byte_len = usize::try_from(bytes).map_err(|_| {
        NorthclockError::InvalidUsage(
            "VRAM test size does not fit this process address space".into(),
        )
    })?;
    let started = Instant::now();
    let (adapter, adapter_name) = select_adapter(requested_adapter)?;
    let device = create_device(&adapter, &adapter_name)?;
    let resources = Resources::create(&device, bytes)?;
    fill_upload(&resources.upload, byte_len)?;

    let queue: ID3D12CommandQueue = unsafe {
        device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_COPY,
            Priority: D3D12_COMMAND_QUEUE_PRIORITY_NORMAL.0,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            NodeMask: 0,
        })
    }
    .map_err(|error| d3d_error(&device, "CreateCommandQueue", error))?;
    let allocator: ID3D12CommandAllocator =
        unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY) }
            .map_err(|error| d3d_error(&device, "CreateCommandAllocator", error))?;
    let command_list: ID3D12GraphicsCommandList = unsafe {
        device.CreateCommandList(
            0,
            D3D12_COMMAND_LIST_TYPE_COPY,
            &allocator,
            None::<&windows::Win32::Graphics::Direct3D12::ID3D12PipelineState>,
        )
    }
    .map_err(|error| d3d_error(&device, "CreateCommandList", error))?;

    unsafe {
        command_list.CopyBufferRegion(&resources.default, 0, &resources.upload, 0, bytes);
    }
    let mut transition = transition_barrier(
        &resources.default,
        D3D12_RESOURCE_STATE_COPY_DEST,
        D3D12_RESOURCE_STATE_COPY_SOURCE,
    );
    unsafe {
        command_list.ResourceBarrier(std::slice::from_ref(&transition));
        command_list.CopyBufferRegion(&resources.readback, 0, &resources.default, 0, bytes);
        command_list.Close()
    }
    .map_err(|error| d3d_error(&device, "Close command list", error))?;
    drop_transition_barrier(&mut transition);

    let list: ID3D12CommandList = command_list
        .cast()
        .map_err(|error| d3d_error(&device, "cast command list", error))?;
    unsafe { queue.ExecuteCommandLists(&[Some(list)]) };
    let fence: ID3D12Fence = unsafe { device.CreateFence(0, Default::default()) }
        .map_err(|error| d3d_error(&device, "CreateFence", error))?;
    unsafe { queue.Signal(&fence, FENCE_VALUE) }
        .map_err(|error| d3d_error(&device, "signal fence", error))?;
    wait_for_fence(&device, &fence, timeout)?;

    let validation_errors = validate_readback(&resources.readback, byte_len)?;
    Ok(WorkloadReport {
        duration_ms: started.elapsed().as_millis(),
        iterations: 1,
        validation_errors,
        timed_out: false,
        // A successful readback validates this operation's bytes. It does not
        // establish the project's separate physical-hardware qualification.
        hardware_verified: false,
    })
}

fn validate_request(bytes: u64, timeout: Duration) -> Result<()> {
    if bytes == 0 || bytes > MAX_TEST_BYTES {
        return Err(NorthclockError::InvalidUsage(format!(
            "VRAM test size must be between 1 and {MAX_TEST_BYTES} bytes"
        )));
    }
    if timeout.is_zero() {
        return Err(NorthclockError::InvalidUsage(
            "VRAM test timeout must be non-zero".into(),
        ));
    }
    Ok(())
}

fn select_adapter(requested: Option<&str>) -> Result<(IDXGIAdapter1, String)> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(windows_error)?;
    for index in 0..256_u32 {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(windows_error(error)),
        };
        let desc = unsafe { adapter.GetDesc1() }.map_err(windows_error)?;
        let name = utf16z(&desc.Description);
        let stable_id = format!(
            "pci-{:04x}-{:04x}-{:08x}",
            desc.VendorId, desc.DeviceId, desc.SubSysId
        );
        let selected = requested.is_none_or(|needle| {
            needle.eq_ignore_ascii_case(&stable_id) || needle.eq_ignore_ascii_case(&name)
        });
        if !selected {
            continue;
        }
        if name.is_empty() {
            return Err(NorthclockError::HardwareOperation(
                "DXGI returned an empty adapter description".into(),
            ));
        }
        if ((desc.Flags as i32) & DXGI_ADAPTER_FLAG_SOFTWARE.0) != 0 {
            return Err(NorthclockError::Unavailable(format!(
                "refusing software DXGI adapter {name} ({stable_id})"
            )));
        }
        return Ok((adapter, format!("{name} ({stable_id})")));
    }
    let request = requested.unwrap_or("a hardware adapter");
    Err(NorthclockError::Unavailable(format!(
        "DXGI did not find adapter {request}"
    )))
}

fn create_device(adapter: &IDXGIAdapter1, adapter_name: &str) -> Result<ID3D12Device> {
    let mut device: Option<ID3D12Device> = None;
    unsafe { D3D12CreateDevice(adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }
        .map_err(windows_error)?;
    let device = device.ok_or_else(|| {
        NorthclockError::HardwareOperation("D3D12CreateDevice returned a null device".into())
    })?;
    if unsafe { device.GetNodeCount() } == 0 {
        return Err(NorthclockError::HardwareOperation(format!(
            "D3D12 device for {adapter_name} reported zero nodes"
        )));
    }
    Ok(device)
}

struct Resources {
    default: ID3D12Resource,
    upload: ID3D12Resource,
    readback: ID3D12Resource,
}

impl Resources {
    fn create(device: &ID3D12Device, bytes: u64) -> Result<Self> {
        let desc = buffer_desc(bytes);
        let default = committed_resource(
            device,
            &desc,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;
        let upload = committed_resource(
            device,
            &desc,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_STATE_GENERIC_READ,
        )?;
        let readback = committed_resource(
            device,
            &desc,
            D3D12_HEAP_TYPE_READBACK,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;
        verify_buffer(&default, bytes, "DEFAULT")?;
        verify_buffer(&upload, bytes, "UPLOAD")?;
        verify_buffer(&readback, bytes, "READBACK")?;
        Ok(Self {
            default,
            upload,
            readback,
        })
    }
}

fn buffer_desc(bytes: u64) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: bytes,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_NONE,
    }
}

fn committed_resource(
    device: &ID3D12Device,
    desc: &D3D12_RESOURCE_DESC,
    heap_type: windows::Win32::Graphics::Direct3D12::D3D12_HEAP_TYPE,
    initial_state: windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATES,
) -> Result<ID3D12Resource> {
    let heap = D3D12_HEAP_PROPERTIES {
        Type: heap_type,
        CPUPageProperty: Default::default(),
        MemoryPoolPreference: Default::default(),
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    };
    let mut resource: Option<ID3D12Resource> = None;
    unsafe {
        device.CreateCommittedResource(
            &heap,
            D3D12_HEAP_FLAG_NONE,
            desc,
            initial_state,
            None,
            &mut resource,
        )
    }
    .map_err(|error| d3d_error(device, "CreateCommittedResource", error))?;
    resource.ok_or_else(|| {
        NorthclockError::HardwareOperation(
            "CreateCommittedResource returned a null resource".into(),
        )
    })
}

fn verify_buffer(resource: &ID3D12Resource, bytes: u64, heap_name: &str) -> Result<()> {
    let desc = unsafe { resource.GetDesc() };
    if desc.Dimension != D3D12_RESOURCE_DIMENSION_BUFFER
        || desc.Width != bytes
        || desc.Height != 1
        || desc.DepthOrArraySize != 1
        || desc.MipLevels != 1
        || desc.Format != DXGI_FORMAT_UNKNOWN
        || desc.SampleDesc.Count != 1
        || desc.SampleDesc.Quality != 0
        || desc.Layout != D3D12_TEXTURE_LAYOUT_ROW_MAJOR
        || desc.Flags != D3D12_RESOURCE_FLAG_NONE
    {
        return Err(NorthclockError::HardwareOperation(format!(
            "D3D12 returned an invalid {heap_name} buffer descriptor"
        )));
    }
    Ok(())
}

fn fill_upload(resource: &ID3D12Resource, byte_len: usize) -> Result<()> {
    let read_range = D3D12_RANGE { Begin: 0, End: 0 };
    let mut pointer = std::ptr::null_mut::<c_void>();
    unsafe { resource.Map(0, Some(&read_range), Some(&mut pointer)) }.map_err(windows_error)?;
    if pointer.is_null() {
        unsafe { resource.Unmap(0, None) };
        return Err(NorthclockError::HardwareOperation(
            "D3D12 upload Map returned a null pointer".into(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts_mut(pointer.cast::<u8>(), byte_len) };
    for (offset, value) in bytes.iter_mut().enumerate() {
        *value = pattern_byte(offset);
    }
    let written_range = D3D12_RANGE {
        Begin: 0,
        End: byte_len,
    };
    unsafe { resource.Unmap(0, Some(&written_range)) };
    Ok(())
}

fn transition_barrier(
    resource: &ID3D12Resource,
    before: windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATES,
    after: windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

fn drop_transition_barrier(barrier: &mut D3D12_RESOURCE_BARRIER) {
    // ResourceBarrier copies the descriptor before returning; this only releases
    // the temporary COM reference stored in the generated union wrapper.
    unsafe { ManuallyDrop::drop(&mut barrier.Anonymous.Transition) };
}

fn wait_for_fence(device: &ID3D12Device, fence: &ID3D12Fence, timeout: Duration) -> Result<()> {
    if unsafe { fence.GetCompletedValue() } >= FENCE_VALUE {
        return Ok(());
    }
    let event = Event::new()?;
    unsafe { fence.SetEventOnCompletion(FENCE_VALUE, event.0) }
        .map_err(|error| d3d_error(device, "SetEventOnCompletion", error))?;
    let wait = unsafe { WaitForSingleObject(event.0, timeout_millis(timeout)) };
    if wait == WAIT_OBJECT_0 && unsafe { fence.GetCompletedValue() } >= FENCE_VALUE {
        return Ok(());
    }
    if wait == WAIT_TIMEOUT {
        return Err(NorthclockError::HardwareOperation(format!(
            "D3D12 VRAM test timed out after {} ms; {}",
            timeout.as_millis(),
            device_status(device)
        )));
    }
    let detail = if wait == WAIT_FAILED {
        format!(
            "WaitForSingleObject failed: {}",
            std::io::Error::last_os_error()
        )
    } else {
        format!("WaitForSingleObject returned unexpected status {}", wait.0)
    };
    Err(NorthclockError::HardwareOperation(format!(
        "{detail}; {}",
        device_status(device)
    )))
}

fn validate_readback(resource: &ID3D12Resource, byte_len: usize) -> Result<u64> {
    let read_range = D3D12_RANGE {
        Begin: 0,
        End: byte_len,
    };
    let mut pointer = std::ptr::null_mut::<c_void>();
    unsafe { resource.Map(0, Some(&read_range), Some(&mut pointer)) }.map_err(windows_error)?;
    if pointer.is_null() {
        unsafe { resource.Unmap(0, None) };
        return Err(NorthclockError::HardwareOperation(
            "D3D12 readback Map returned a null pointer".into(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), byte_len) };
    let validation_errors = bytes
        .iter()
        .enumerate()
        .filter(|(offset, value)| **value != pattern_byte(*offset))
        .count() as u64;
    let no_write_range = D3D12_RANGE { Begin: 0, End: 0 };
    unsafe { resource.Unmap(0, Some(&no_write_range)) };
    Ok(validation_errors)
}

fn pattern_byte(offset: usize) -> u8 {
    let value = (offset as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(17)
        ^ 0xD1B5_4A32_D192_ED03;
    (value ^ (value >> 32) ^ (value >> 16) ^ (value >> 8)) as u8
}

fn timeout_millis(timeout: Duration) -> u32 {
    timeout.as_millis().min(u128::from(u32::MAX)) as u32
}

struct Event(HANDLE);

impl Event {
    fn new() -> Result<Self> {
        let handle = unsafe { CreateEventW(None, false, false, None) }.map_err(windows_error)?;
        if handle.is_invalid() {
            return Err(NorthclockError::HardwareOperation(
                "CreateEventW returned an invalid handle".into(),
            ));
        }
        Ok(Self(handle))
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn device_status(device: &ID3D12Device) -> String {
    match unsafe { device.GetDeviceRemovedReason() } {
        Ok(()) => "D3D12 device still reports healthy".into(),
        Err(error) => format!(
            "D3D12 device removed or reset: {}: {}",
            error.code(),
            error.message()
        ),
    }
}

fn utf16z(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

fn d3d_error(
    device: &ID3D12Device,
    operation: &str,
    error: windows::core::Error,
) -> NorthclockError {
    NorthclockError::HardwareOperation(format!(
        "{operation} failed with {}: {}; {}",
        error.code(),
        error.message(),
        device_status(device)
    ))
}

fn windows_error(error: windows::core::Error) -> NorthclockError {
    NorthclockError::HardwareOperation(format!(
        "Windows API failure {}: {}",
        error.code(),
        error.message()
    ))
}

#[cfg(test)]
mod tests {
    use super::{pattern_byte, timeout_millis};
    use std::time::Duration;

    #[test]
    fn pattern_is_deterministic_and_not_constant() {
        assert_eq!(pattern_byte(37), pattern_byte(37));
        assert_ne!(pattern_byte(37), pattern_byte(38));
    }

    #[test]
    fn timeout_conversion_saturates_to_windows_abi() {
        assert_eq!(timeout_millis(Duration::from_millis(7)), 7);
        assert_eq!(timeout_millis(Duration::MAX), u32::MAX);
    }
}
