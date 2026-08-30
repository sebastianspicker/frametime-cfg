//! Read-only NVIDIA telemetry through the installed System32 NVAPI runtime.
//!
//! ABI constants and layouts follow NVIDIA's MIT-licensed `nvapi.h` and
//! `nvapi_interface.h` (Release 590). The library is loaded only from System32;
//! this module neither ships nor links a vendor DLL.

use crate::abi_validation::{
    validate_nvapi_load_fields, validate_nvapi_temperature_fields, validate_nvapi_thermal_header,
};
use northclock_core::{DeviceIdentity, Measurement, NorthclockError, Result};
use std::ffi::c_void;
use std::mem::{size_of, transmute};
use windows::core::{w, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

const NVAPI_OK: i32 = 0;
const NVAPI_MAX_PHYSICAL_GPUS: usize = 64;
const NVAPI_SHORT_STRING_MAX: usize = 64;
const NVAPI_MAX_GPU_UTILIZATIONS: usize = 8;
const NVAPI_MAX_THERMAL_SENSORS: usize = 3;
const NVAPI_GPU_UTILIZATION_DOMAIN_GPU: usize = 0;
const NVAPI_THERMAL_TARGET_GPU: i32 = 1;
const NVAPI_THERMAL_TARGET_ALL: u32 = 15;

const ID_INITIALIZE: u32 = 0x0150_e828;
const ID_UNLOAD: u32 = 0xd22b_dd7e;
const ID_ENUM_PHYSICAL_GPUS: u32 = 0xe5ac_921f;
const ID_GPU_GET_FULL_NAME: u32 = 0xceee_8e9f;
const ID_GPU_GET_PCI_IDENTIFIERS: u32 = 0x2ddf_b66e;
const ID_GPU_GET_DYNAMIC_PSTATES: u32 = 0x60de_d2ed;
const ID_GPU_GET_THERMAL_SETTINGS: u32 = 0xe364_0a56;

type NvStatus = i32;
type PhysicalGpu = *mut c_void;
type QueryInterface = unsafe extern "C" fn(u32) -> *const c_void;
type Initialize = unsafe extern "C" fn() -> NvStatus;
type Unload = unsafe extern "C" fn() -> NvStatus;
type EnumPhysicalGpus = unsafe extern "C" fn(*mut PhysicalGpu, *mut u32) -> NvStatus;
type GetFullName = unsafe extern "C" fn(PhysicalGpu, *mut u8) -> NvStatus;
type GetPciIdentifiers =
    unsafe extern "C" fn(PhysicalGpu, *mut u32, *mut u32, *mut u32, *mut u32) -> NvStatus;
type GetDynamicPstates = unsafe extern "C" fn(PhysicalGpu, *mut DynamicPstates) -> NvStatus;
type GetThermalSettings = unsafe extern "C" fn(PhysicalGpu, u32, *mut ThermalSettings) -> NvStatus;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DynamicUtilization {
    present: u32,
    percentage: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DynamicPstates {
    version: u32,
    flags: u32,
    utilization: [DynamicUtilization; NVAPI_MAX_GPU_UTILIZATIONS],
}

impl Default for DynamicPstates {
    fn default() -> Self {
        Self {
            version: nvapi_version::<Self>(1),
            flags: 0,
            utilization: [DynamicUtilization::default(); NVAPI_MAX_GPU_UTILIZATIONS],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ThermalSensor {
    controller: i32,
    default_min_temp: i32,
    default_max_temp: i32,
    current_temp: i32,
    target: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ThermalSettings {
    version: u32,
    count: u32,
    sensors: [ThermalSensor; NVAPI_MAX_THERMAL_SENSORS],
}

impl Default for ThermalSettings {
    fn default() -> Self {
        Self {
            version: nvapi_version::<Self>(2),
            count: 0,
            sensors: [ThermalSensor::default(); NVAPI_MAX_THERMAL_SENSORS],
        }
    }
}

struct NvApi {
    module: HMODULE,
    unload: Unload,
    enumerate: EnumPhysicalGpus,
    full_name: GetFullName,
    pci_identifiers: GetPciIdentifiers,
    dynamic_pstates: GetDynamicPstates,
    thermal_settings: GetThermalSettings,
}

impl NvApi {
    fn load() -> Result<Self> {
        let module =
            unsafe { LoadLibraryExW(w!("nvapi64.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(windows_error)?;
        let query = match query_interface(module) {
            Ok(query) => query,
            Err(error) => {
                let _ = unsafe { FreeLibrary(module) };
                return Err(error);
            }
        };
        let bindings = resolve_bindings(query);
        let (
            initialize,
            unload,
            enumerate,
            full_name,
            pci_identifiers,
            dynamic_pstates,
            thermal_settings,
        ) = match bindings {
            Ok(bindings) => bindings,
            Err(error) => {
                let _ = unsafe { FreeLibrary(module) };
                return Err(error);
            }
        };
        let status = unsafe { initialize() };
        if status != NVAPI_OK {
            let _ = unsafe { FreeLibrary(module) };
            return Err(nvapi_error("NvAPI_Initialize", status));
        }
        Ok(Self {
            module,
            unload,
            enumerate,
            full_name,
            pci_identifiers,
            dynamic_pstates,
            thermal_settings,
        })
    }

    fn physical_gpus(&self) -> Result<Vec<PhysicalGpu>> {
        let mut handles = [std::ptr::null_mut(); NVAPI_MAX_PHYSICAL_GPUS];
        let mut count = 0_u32;
        check_status("NvAPI_EnumPhysicalGPUs", unsafe {
            (self.enumerate)(handles.as_mut_ptr(), &mut count)
        })?;
        let count =
            usize::try_from(count).map_err(|error| NorthclockError::Internal(error.to_string()))?;
        if count == 0 || count > handles.len() {
            return Err(NorthclockError::HardwareOperation(format!(
                "NvAPI_EnumPhysicalGPUs returned invalid GPU count {count}"
            )));
        }
        let selected = &handles[..count];
        if selected.iter().any(|handle| handle.is_null()) {
            return Err(NorthclockError::HardwareOperation(
                "NvAPI_EnumPhysicalGPUs returned a null physical GPU handle".into(),
            ));
        }
        Ok(selected.to_vec())
    }
}

pub(crate) fn probe() -> Result<()> {
    NvApi::load()?.physical_gpus().map(|_| ())
}

type Bindings = (
    Initialize,
    Unload,
    EnumPhysicalGpus,
    GetFullName,
    GetPciIdentifiers,
    GetDynamicPstates,
    GetThermalSettings,
);

fn resolve_bindings(query: QueryInterface) -> Result<Bindings> {
    let initialize: Initialize =
        unsafe { transmute(resolve(query, ID_INITIALIZE, "NvAPI_Initialize")?) };
    let unload: Unload = unsafe { transmute(resolve(query, ID_UNLOAD, "NvAPI_Unload")?) };
    let enumerate: EnumPhysicalGpus = unsafe {
        transmute(resolve(
            query,
            ID_ENUM_PHYSICAL_GPUS,
            "NvAPI_EnumPhysicalGPUs",
        )?)
    };
    let full_name: GetFullName = unsafe {
        transmute(resolve(
            query,
            ID_GPU_GET_FULL_NAME,
            "NvAPI_GPU_GetFullName",
        )?)
    };
    let pci_identifiers: GetPciIdentifiers = unsafe {
        transmute(resolve(
            query,
            ID_GPU_GET_PCI_IDENTIFIERS,
            "NvAPI_GPU_GetPCIIdentifiers",
        )?)
    };
    let dynamic_pstates: GetDynamicPstates = unsafe {
        transmute(resolve(
            query,
            ID_GPU_GET_DYNAMIC_PSTATES,
            "NvAPI_GPU_GetDynamicPstatesInfoEx",
        )?)
    };
    let thermal_settings: GetThermalSettings = unsafe {
        transmute(resolve(
            query,
            ID_GPU_GET_THERMAL_SETTINGS,
            "NvAPI_GPU_GetThermalSettings",
        )?)
    };
    Ok((
        initialize,
        unload,
        enumerate,
        full_name,
        pci_identifiers,
        dynamic_pstates,
        thermal_settings,
    ))
}

impl Drop for NvApi {
    fn drop(&mut self) {
        let _ = unsafe { (self.unload)() };
        let _ = unsafe { FreeLibrary(self.module) };
    }
}

pub(crate) fn gpu_measurements(stable_id: Option<&str>) -> Result<Vec<Measurement<f64>>> {
    let api = NvApi::load()?;
    let mut measurements = Vec::new();
    let mut matched = false;
    for handle in api.physical_gpus()? {
        let identity = gpu_identity(&api, handle)?;
        if stable_id.is_some_and(|requested| !requested.eq_ignore_ascii_case(&identity.stable_id)) {
            continue;
        }
        matched = true;
        measurements.extend(read_measurements(&api, handle, identity)?);
    }
    if !matched {
        return Err(NorthclockError::Unavailable(format!(
            "NVAPI did not find NVIDIA adapter {}",
            stable_id.unwrap_or("with valid PCI identifiers")
        )));
    }
    if measurements.is_empty() {
        return Err(NorthclockError::Unavailable(
            "NVAPI returned no validated load or temperature measurements".into(),
        ));
    }
    Ok(measurements)
}

fn gpu_identity(api: &NvApi, handle: PhysicalGpu) -> Result<DeviceIdentity> {
    let mut name = [0_u8; NVAPI_SHORT_STRING_MAX];
    check_status("NvAPI_GPU_GetFullName", unsafe {
        (api.full_name)(handle, name.as_mut_ptr())
    })?;
    let name = parse_short_name(&name)?;
    let mut device = 0_u32;
    let mut subsystem = 0_u32;
    let mut revision = 0_u32;
    let mut external_device = 0_u32;
    check_status("NvAPI_GPU_GetPCIIdentifiers", unsafe {
        (api.pci_identifiers)(
            handle,
            &mut device,
            &mut subsystem,
            &mut revision,
            &mut external_device,
        )
    })?;
    let stable_id = stable_id_from_pci(device, subsystem)?;
    Ok(DeviceIdentity::new(
        "gpu",
        stable_id,
        name,
        Some("NVIDIA".into()),
    ))
}

fn read_measurements(
    api: &NvApi,
    handle: PhysicalGpu,
    device: DeviceIdentity,
) -> Result<Vec<Measurement<f64>>> {
    let mut dynamic = DynamicPstates::default();
    check_status("NvAPI_GPU_GetDynamicPstatesInfoEx", unsafe {
        (api.dynamic_pstates)(handle, &mut dynamic)
    })?;
    let load = validate_load(&dynamic)?;

    let mut thermal = ThermalSettings::default();
    check_status("NvAPI_GPU_GetThermalSettings", unsafe {
        (api.thermal_settings)(handle, NVAPI_THERMAL_TARGET_ALL, &mut thermal)
    })?;
    let temperature = validate_temperature(&thermal)?;

    Ok(vec![
        Measurement::now(
            load,
            "%",
            device.clone(),
            "NVAPI nvapi64.dll DynamicPstates 0x60ded2ed",
        )?,
        Measurement::now(
            temperature,
            "C",
            device,
            "NVAPI nvapi64.dll ThermalSettings 0xe3640a56",
        )?,
    ])
}

fn validate_load(info: &DynamicPstates) -> Result<f64> {
    let gpu = info.utilization[NVAPI_GPU_UTILIZATION_DOMAIN_GPU];
    validate_nvapi_load_fields(
        info.version,
        nvapi_version::<DynamicPstates>(1),
        gpu.present,
        gpu.percentage,
    )
    .map_err(|error| {
        NorthclockError::HardwareOperation(format!(
            "NVAPI returned invalid DynamicPstates load fields: {error:?}"
        ))
    })
}

fn validate_temperature(info: &ThermalSettings) -> Result<f64> {
    validate_nvapi_thermal_header(
        info.version,
        nvapi_version::<ThermalSettings>(2),
        info.count as usize,
        info.sensors.len(),
    )
    .map_err(|error| {
        NorthclockError::HardwareOperation(format!(
            "NVAPI returned an invalid ThermalSettings structure header: {error:?}"
        ))
    })?;
    let sensor = info.sensors[..info.count as usize]
        .iter()
        .find(|sensor| sensor.target == NVAPI_THERMAL_TARGET_GPU)
        .ok_or_else(|| {
            NorthclockError::HardwareOperation(
                "NVAPI did not return a GPU-core thermal sensor".into(),
            )
        })?;
    validate_nvapi_temperature_fields(
        sensor.default_min_temp,
        sensor.default_max_temp,
        sensor.current_temp,
    )
    .map_err(|error| {
        NorthclockError::HardwareOperation(format!(
            "NVAPI returned invalid GPU thermal values: {error:?}"
        ))
    })
}

fn stable_id_from_pci(device_id: u32, subsystem_id: u32) -> Result<String> {
    let (vendor_id, device_id) = if device_id & 0xffff == 0x10de {
        (0x10de_u32, device_id >> 16)
    } else {
        (0x10de_u32, device_id)
    };
    if device_id == 0 || device_id > 0xffff {
        return Err(NorthclockError::HardwareOperation(format!(
            "NVAPI returned invalid PCI identifiers device={device_id:#x} subsystem={subsystem_id:#x}"
        )));
    }
    Ok(format!(
        "pci-{vendor_id:04x}-{device_id:04x}-{subsystem_id:08x}"
    ))
}

fn parse_short_name(value: &[u8; NVAPI_SHORT_STRING_MAX]) -> Result<String> {
    let end = value.iter().position(|byte| *byte == 0).ok_or_else(|| {
        NorthclockError::HardwareOperation("NVAPI returned an unterminated GPU name".into())
    })?;
    let name = std::str::from_utf8(&value[..end])
        .map_err(|_| {
            NorthclockError::HardwareOperation("NVAPI returned a non-UTF-8 GPU name".into())
        })?
        .trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return Err(NorthclockError::HardwareOperation(
            "NVAPI returned an invalid GPU name".into(),
        ));
    }
    Ok(name.into())
}

fn query_interface(module: HMODULE) -> Result<QueryInterface> {
    let procedure =
        unsafe { GetProcAddress(module, PCSTR(c"nvapi_QueryInterface".as_ptr().cast::<u8>())) }
            .ok_or_else(|| {
                NorthclockError::Unavailable(
                    "installed nvapi64.dll does not export nvapi_QueryInterface".into(),
                )
            })?;
    // NVAPI's current x64 headers declare the resolver as `__cdecl`; x64 has a
    // single ABI for these conventions, and this module is cfg-gated to x64.
    Ok(unsafe { transmute::<unsafe extern "system" fn() -> isize, QueryInterface>(procedure) })
}

fn resolve(query: QueryInterface, id: u32, name: &str) -> Result<*const c_void> {
    let pointer = unsafe { query(id) };
    if pointer.is_null() {
        return Err(NorthclockError::Unavailable(format!(
            "installed nvapi64.dll does not expose {name} ({id:#010x})"
        )));
    }
    Ok(pointer)
}

fn check_status(operation: &str, status: NvStatus) -> Result<()> {
    if status == NVAPI_OK {
        Ok(())
    } else {
        Err(nvapi_error(operation, status))
    }
}

fn nvapi_version<T>(revision: u32) -> u32 {
    (size_of::<T>() as u32) | (revision << 16)
}

fn nvapi_error(operation: &str, status: NvStatus) -> NorthclockError {
    NorthclockError::HardwareOperation(format!(
        "{operation} failed with NVAPI status {status:#010x}"
    ))
}

fn windows_error(error: windows::core::Error) -> NorthclockError {
    NorthclockError::Unavailable(format!(
        "could not load System32 nvapi64.dll: {}: {}",
        error.code(),
        error.message()
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        nvapi_version, parse_short_name, stable_id_from_pci, validate_load, validate_temperature,
        DynamicPstates, ThermalSensor, ThermalSettings,
    };

    #[test]
    fn short_name_requires_terminated_printable_text() {
        let mut name = [0_u8; 64];
        name[..12].copy_from_slice(b"NVIDIA RTX 0");
        assert_eq!(
            parse_short_name(&name).unwrap_or_else(|error| panic!("name failed: {error}")),
            "NVIDIA RTX 0"
        );
        assert!(parse_short_name(&[b'x'; 64]).is_err());
    }

    #[test]
    fn pci_identifier_matches_dxgi_stable_identifier() {
        assert_eq!(
            stable_id_from_pci(0x2684_10de, 0x1458_0001)
                .unwrap_or_else(|error| panic!("PCI identifier failed: {error}")),
            "pci-10de-2684-14580001"
        );
        assert!(stable_id_from_pci(0, 1).is_err());
    }

    #[test]
    fn telemetry_validators_reject_invalid_headers_and_ranges() {
        let mut dynamic = DynamicPstates::default();
        dynamic.utilization[0].present = 1;
        dynamic.utilization[0].percentage = 73;
        assert_eq!(
            validate_load(&dynamic).unwrap_or_else(|error| panic!("load failed: {error}")),
            73.0
        );
        dynamic.version = 0;
        assert!(validate_load(&dynamic).is_err());

        let mut thermal = ThermalSettings {
            count: 1,
            ..ThermalSettings::default()
        };
        thermal.sensors[0] = ThermalSensor {
            controller: 1,
            default_min_temp: 0,
            default_max_temp: 110,
            current_temp: 64,
            target: 1,
        };
        assert_eq!(
            validate_temperature(&thermal)
                .unwrap_or_else(|error| panic!("temperature failed: {error}")),
            64.0
        );
        thermal.sensors[0].current_temp = 201;
        assert!(validate_temperature(&thermal).is_err());
        assert_eq!(dynamic.version, 0);
        assert_eq!(nvapi_version::<ThermalSettings>(2), 0x0002_0044);
    }
}
