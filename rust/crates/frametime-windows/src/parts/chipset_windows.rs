#[cfg(windows)]
use windows::{
    Win32::{
        Devices::{
            DeviceAndDriverInstallation::{
                DIGCF_PRESENT, GUID_DEVCLASS_SYSTEM, HDEVINFO, SP_DEVINFO_DATA,
                SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
                SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW,
            },
            Properties::{
                DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_DriverDate,
                DEVPKEY_Device_DriverInfPath, DEVPKEY_Device_DriverProvider,
                DEVPKEY_Device_DriverVersion, DEVPKEY_Device_HardwareIds, DEVPROP_TYPE_FILETIME,
                DEVPROP_TYPE_STRING, DEVPROP_TYPE_STRING_LIST,
            },
        },
        Foundation::{DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, ERROR_NOT_FOUND},
    },
    core::{Error, HRESULT},
};

#[cfg(windows)]
struct DeviceInfoSet(HDEVINFO);

#[cfg(windows)]
impl Drop for DeviceInfoSet {
    fn drop(&mut self) {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

#[cfg(windows)]
fn expected_setupapi_error(error: &Error, expected: u32) -> bool {
    error.code() == HRESULT::from_win32(expected)
}

#[cfg(windows)]
fn property_bytes(
    set: HDEVINFO,
    data: &SP_DEVINFO_DATA,
    key: &DEVPROPKEY,
) -> Result<Option<(u32, Vec<u8>)>, String> {
    let mut property_type = Default::default();
    let mut required = 0;
    let first = unsafe {
        SetupDiGetDevicePropertyW(
            set,
            data,
            key,
            &mut property_type,
            None,
            Some(&mut required),
            0,
        )
    };
    match first {
        Err(error) if expected_setupapi_error(&error, ERROR_NOT_FOUND.0) => return Ok(None),
        Err(error)
            if expected_setupapi_error(&error, ERROR_INSUFFICIENT_BUFFER.0) && required != 0 => {}
        _ => return Err("SetupAPI device property length query failed".into()),
    }
    let mut bytes =
        vec![0; usize::try_from(required).map_err(|_| "SetupAPI property size overflows usize")?];
    unsafe {
        SetupDiGetDevicePropertyW(
            set,
            data,
            key,
            &mut property_type,
            Some(&mut bytes),
            Some(&mut required),
            0,
        )
    }
    .map_err(|error| format!("SetupAPI device property read failed: {error}"))?;
    bytes
        .truncate(usize::try_from(required).map_err(|_| "SetupAPI property size overflows usize")?);
    Ok(Some((property_type.0, bytes)))
}

#[cfg(windows)]
fn wide_values(bytes: &[u8], list: bool) -> Result<Vec<String>, String> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return Err("SetupAPI UTF-16 property has an invalid byte count".into());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    if *units.last().ok_or("SetupAPI UTF-16 property is empty")? != 0 {
        return Err("SetupAPI UTF-16 property is not terminated".into());
    }
    let mut values = Vec::new();
    let mut start = 0;
    for (index, unit) in units.iter().enumerate() {
        if *unit != 0 {
            continue;
        }
        if start == index {
            if list && index + 1 == units.len() {
                break;
            }
            return Err("SetupAPI UTF-16 property contains an empty value".into());
        }
        values.push(
            String::from_utf16(&units[start..index])
                .map_err(|_| "SetupAPI UTF-16 property is malformed")?,
        );
        start = index + 1;
        if !list && start != units.len() {
            return Err("SetupAPI string property contains trailing data".into());
        }
    }
    if values.is_empty() || start != units.len() {
        return Err("SetupAPI UTF-16 property has an invalid terminator".into());
    }
    Ok(values)
}

#[cfg(windows)]
fn string_property(
    set: HDEVINFO,
    data: &SP_DEVINFO_DATA,
    key: &DEVPROPKEY,
) -> Result<String, String> {
    let (kind, bytes) =
        property_bytes(set, data, key)?.ok_or("SetupAPI required driver property is missing")?;
    if kind != DEVPROP_TYPE_STRING.0 {
        return Err("SetupAPI driver property has an unexpected type".into());
    }
    wide_values(&bytes, false)?
        .into_iter()
        .next()
        .ok_or("SetupAPI string property is empty".into())
}

#[cfg(windows)]
fn string_list_property(
    set: HDEVINFO,
    data: &SP_DEVINFO_DATA,
    key: &DEVPROPKEY,
) -> Result<Option<Vec<String>>, String> {
    let Some((kind, bytes)) = property_bytes(set, data, key)? else {
        return Ok(None);
    };
    if kind != DEVPROP_TYPE_STRING_LIST.0 {
        return Err("SetupAPI device-ID property has an unexpected type".into());
    }
    wide_values(&bytes, true).map(Some)
}

#[cfg(windows)]
fn filetime_property(set: HDEVINFO, data: &SP_DEVINFO_DATA) -> Result<u64, String> {
    let (kind, bytes) = property_bytes(set, data, &DEVPKEY_Device_DriverDate)?
        .ok_or("SetupAPI driver date is missing")?;
    if kind != DEVPROP_TYPE_FILETIME.0 || bytes.len() != 8 {
        return Err("SetupAPI driver date has an unexpected type or size".into());
    }
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| "checked driver date length")?,
    ))
}

#[cfg(windows)]
fn instance_id(set: HDEVINFO, data: &SP_DEVINFO_DATA) -> Result<String, String> {
    let mut required = 0;
    let first = unsafe { SetupDiGetDeviceInstanceIdW(set, data, None, Some(&mut required)) };
    if !matches!(first, Err(ref error) if expected_setupapi_error(error, ERROR_INSUFFICIENT_BUFFER.0))
        || required < 2
    {
        return Err("SetupAPI instance-ID length query failed".into());
    }
    let mut units = vec![
        0;
        usize::try_from(required)
            .map_err(|_| "SetupAPI instance-ID size overflows usize")?
    ];
    unsafe { SetupDiGetDeviceInstanceIdW(set, data, Some(&mut units), Some(&mut required)) }
        .map_err(|error| format!("SetupAPI instance-ID read failed: {error}"))?;
    if usize::try_from(required).ok() != Some(units.len()) || units.last() != Some(&0) {
        return Err("SetupAPI instance ID has an invalid terminator".into());
    }
    String::from_utf16(&units[..units.len() - 1])
        .map_err(|_| "SetupAPI instance ID is malformed".into())
}

#[cfg(windows)]
fn raw_chipset_driver_record(
    set: HDEVINFO,
    data: &SP_DEVINFO_DATA,
) -> Result<Option<RawChipsetDriverRecord>, String> {
    let hardware_ids = string_list_property(set, data, &DEVPKEY_Device_HardwareIds)?
        .ok_or("SetupAPI system device has no hardware IDs")?;
    let compatible_ids =
        string_list_property(set, data, &DEVPKEY_Device_CompatibleIds)?.unwrap_or_default();
    if chipset_vendor_bindings(&hardware_ids, &compatible_ids)?.is_empty() {
        return Ok(None);
    }
    Ok(Some(RawChipsetDriverRecord {
        instance_id: instance_id(set, data)?,
        hardware_ids,
        compatible_ids,
        inf_path: string_property(set, data, &DEVPKEY_Device_DriverInfPath)?,
        provider: string_property(set, data, &DEVPKEY_Device_DriverProvider)?,
        driver_version: string_property(set, data, &DEVPKEY_Device_DriverVersion)?,
        driver_date_filetime: filetime_property(set, data)?,
    }))
}

#[cfg(windows)]
fn enumerate_chipset_driver_records() -> Result<Vec<RawChipsetDriverRecord>, String> {
    let set =
        unsafe { SetupDiGetClassDevsW(Some(&GUID_DEVCLASS_SYSTEM), None, None, DIGCF_PRESENT) }
            .map_err(|error| format!("open present system-device inventory: {error}"))?;
    let set = DeviceInfoSet(set);
    let mut records = Vec::new();
    let mut index = 0;
    loop {
        let mut data = SP_DEVINFO_DATA {
            cbSize: u32::try_from(std::mem::size_of::<SP_DEVINFO_DATA>())
                .map_err(|_| "SP_DEVINFO_DATA size overflows u32")?,
            ..SP_DEVINFO_DATA::default()
        };
        match unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut data) } {
            Ok(()) => {}
            Err(error) if expected_setupapi_error(&error, ERROR_NO_MORE_ITEMS.0) => break,
            Err(error) => return Err(format!("enumerate present system devices: {error}")),
        }
        index = index
            .checked_add(1)
            .ok_or("SetupAPI system-device inventory exceeds the index range")?;
        if let Some(record) = raw_chipset_driver_record(set.0, &data)? {
            records.push(record);
        }
    }
    Ok(records)
}
