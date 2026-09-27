//! NVAPI export resolution and bounded native-DLL evidence helpers.

use super::*;

pub(super) struct Functions {
    initialize: Initialize,
    create_session: CreateSession,
    destroy_session: SessionCall,
    load_settings: SessionCall,
    save_settings: SessionCall,
    find_profile: FindProfile,
    get_profile_info: ProfileInfo,
    create_profile: CreateProfile,
    delete_profile: ProfileCall,
    create_application: CreateApplication,
    delete_application: DeleteApplication,
    find_application: FindApplication,
    get_setting: GetSetting,
    set_setting: SetSetting,
    delete_setting: DeleteSetting,
    get_num_profiles: GetNumProfiles,
    enum_profiles: EnumProfiles,
    enum_applications: EnumApplications,
    enum_settings: EnumSettings,
}

impl Functions {
    pub(super) unsafe fn resolve(query: QueryInterface) -> Result<Self, DrsError> {
        macro_rules! resolve {
            ($id:expr, $name:literal, $ty:ty) => {{
                let pointer = unsafe { query($id) };
                if pointer.is_null() {
                    return Err(error($name, "query interface returned null"));
                }
                // The query identifier fixes this ABI-defined function pointer type.
                unsafe { transmute::<*const c_void, $ty>(pointer) }
            }};
        }
        Ok(Self {
            initialize: resolve!(0x0150_e828, "NvAPI_Initialize", Initialize),
            create_session: resolve!(0x0694_d52e, "DRS_CreateSession", CreateSession),
            destroy_session: resolve!(0xdad9_cff8, "DRS_DestroySession", SessionCall),
            load_settings: resolve!(0x375d_bd6b, "DRS_LoadSettings", SessionCall),
            save_settings: resolve!(0xfcbc_7e14, "DRS_SaveSettings", SessionCall),
            find_profile: resolve!(0x7e4a_9a0b, "DRS_FindProfileByName", FindProfile),
            get_profile_info: resolve!(0x61cd_6fd6, "DRS_GetProfileInfo", ProfileInfo),
            create_profile: resolve!(0xcc17_6068, "DRS_CreateProfile", CreateProfile),
            delete_profile: resolve!(0x1709_3206, "DRS_DeleteProfile", ProfileCall),
            create_application: resolve!(0x4347_a9de, "DRS_CreateApplication", CreateApplication),
            // NvAPI_DRS_DeleteApplication: public NVIDIA SDK commit
            // cd6918f60b3c9a0476fdfe7e89bb32330602049d, interface 0x2c694bc6.
            delete_application: resolve!(0x2c69_4bc6, "DRS_DeleteApplication", DeleteApplication),
            find_application: resolve!(0xeee5_66b2, "DRS_FindApplicationByName", FindApplication),
            get_setting: resolve!(0x73bf_8338, "DRS_GetSetting", GetSetting),
            set_setting: resolve!(0x577d_d202, "DRS_SetSetting", SetSetting),
            delete_setting: resolve!(0xe4a2_6362, "DRS_DeleteProfileSetting", DeleteSetting),
            get_num_profiles: resolve!(0x1dae_4fbc, "DRS_GetNumProfiles", GetNumProfiles),
            enum_profiles: resolve!(0xbc37_1ee0, "DRS_EnumProfiles", EnumProfiles),
            enum_applications: resolve!(0x7fa2_173a, "DRS_EnumApplications", EnumApplications),
            enum_settings: resolve!(0xae30_39da, "DRS_EnumSettings", EnumSettings),
        })
    }

    pub(super) fn into_host(self, module: HMODULE, module_sha256: String) -> NativeNvapiDrs {
        NativeNvapiDrs {
            module,
            module_sha256,
            initialize: self.initialize,
            create_session: self.create_session,
            destroy_session: self.destroy_session,
            load_settings: self.load_settings,
            save_settings: self.save_settings,
            find_profile: self.find_profile,
            get_profile_info: self.get_profile_info,
            create_profile: self.create_profile,
            delete_profile: self.delete_profile,
            create_application: self.create_application,
            delete_application: self.delete_application,
            find_application: self.find_application,
            get_setting: self.get_setting,
            set_setting: self.set_setting,
            delete_setting: self.delete_setting,
            get_num_profiles: self.get_num_profiles,
            enum_profiles: self.enum_profiles,
            enum_applications: self.enum_applications,
            enum_settings: self.enum_settings,
        }
    }
}

pub(super) fn status(value: Status, operation: &'static str) -> Result<(), DrsError> {
    if value == OK {
        Ok(())
    } else {
        Err(error(operation, format!("status {value}")))
    }
}

pub(super) fn optional_handle(
    value: Status,
    missing: Status,
    handle: Handle,
    operation: &'static str,
) -> Result<Option<Handle>, DrsError> {
    if value == missing {
        return Ok(None);
    }
    status(value, operation)?;
    (!handle.is_null())
        .then_some(Some(handle))
        .ok_or_else(|| error(operation, "returned a null handle"))
}

pub(super) fn error(operation: &'static str, reason: impl Into<String>) -> DrsError {
    DrsError::new(operation, reason)
}

pub(super) fn system32_nvapi_path() -> Result<PathBuf, String> {
    let mut buffer = vec![0_u16; 32_768];
    let copied = unsafe { GetSystemDirectoryW(Some(&mut buffer)) };
    let copied = usize::try_from(copied).map_err(|_| "System32 path length overflows")?;
    if copied == 0 || copied >= buffer.len() {
        return Err("GetSystemDirectoryW failed or truncated".into());
    }
    buffer.truncate(copied);
    let root = String::from_utf16(&buffer).map_err(|_| "System32 path is invalid UTF-16")?;
    Ok(PathBuf::from(root).join("nvapi64.dll"))
}

pub(super) fn verify_loaded_module_path(module: HMODULE, expected: &Path) -> Result<(), String> {
    let mut buffer = vec![0_u16; 32_768];
    let copied = unsafe { GetModuleFileNameW(Some(module), &mut buffer) };
    let copied = usize::try_from(copied).map_err(|_| "module path length overflows")?;
    if copied == 0 || copied >= buffer.len() {
        return Err("GetModuleFileNameW failed or truncated".into());
    }
    buffer.truncate(copied);
    let actual =
        PathBuf::from(String::from_utf16(&buffer).map_err(|_| "module path is invalid UTF-16")?);
    if actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        Ok(())
    } else {
        Err("loaded NVAPI module is not the absolute System32 nvapi64.dll".into())
    }
}

pub(super) fn wide_path(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut bytes = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut bytes).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&bytes[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
