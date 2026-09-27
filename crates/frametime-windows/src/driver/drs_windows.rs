#[cfg(windows)]
mod native_drs_windows {
    use sha2::{Digest, Sha256};
    use std::{
        ffi::c_void,
        fs::File,
        io::Read,
        mem::transmute,
        path::{Path, PathBuf},
    };

    use windows::{
        Win32::{
            Foundation::{FreeLibrary, HMODULE},
            System::{
                LibraryLoader::{
                    GetModuleFileNameW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32,
                    LoadLibraryExW,
                },
                SystemInformation::GetSystemDirectoryW,
            },
        },
        core::{PCSTR, PCWSTR},
    };

    use super::super::drs_abi::{
        NVDRS_DWORD_TYPE, NvDrsApplicationV4, NvDrsProfile, NvDrsSetting, unicode_argument,
    };
    use crate::{DrsError, DrsOriginalSetting, NvapiDrs};
    use frametime_domain::driver::{
        DrsApplicationSnapshot, DrsItemKey, DrsItemKind, DrsProfileSnapshot, DrsSettingSnapshot,
        DrsSnapshot, NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NativeDrsSession(*mut c_void);
    type Handle = *mut c_void;
    type Status = i32;
    type QueryInterface = unsafe extern "system" fn(u32) -> *const c_void;
    type Initialize = unsafe extern "C" fn() -> Status;
    type CreateSession = unsafe extern "C" fn(*mut Handle) -> Status;
    type SessionCall = unsafe extern "C" fn(Handle) -> Status;
    type FindProfile = unsafe extern "C" fn(Handle, *const u16, *mut Handle) -> Status;
    type ProfileCall = unsafe extern "C" fn(Handle, Handle) -> Status;
    type ProfileInfo = unsafe extern "C" fn(Handle, Handle, *mut NvDrsProfile) -> Status;
    type CreateProfile = unsafe extern "C" fn(Handle, *mut NvDrsProfile, *mut Handle) -> Status;
    type CreateApplication =
        unsafe extern "C" fn(Handle, Handle, *mut NvDrsApplicationV4) -> Status;
    type DeleteApplication = unsafe extern "C" fn(Handle, Handle, *const u16) -> Status;
    type FindApplication =
        unsafe extern "C" fn(Handle, *const u16, *mut Handle, *mut NvDrsApplicationV4) -> Status;
    type GetSetting = unsafe extern "C" fn(Handle, Handle, u32, *mut NvDrsSetting) -> Status;
    type SetSetting = unsafe extern "C" fn(Handle, Handle, *mut NvDrsSetting) -> Status;
    type DeleteSetting = unsafe extern "C" fn(Handle, Handle, u32) -> Status;
    type GetNumProfiles = unsafe extern "C" fn(Handle, *mut u32) -> Status;
    type EnumProfiles = unsafe extern "C" fn(Handle, u32, *mut Handle) -> Status;
    type EnumApplications =
        unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut NvDrsApplicationV4) -> Status;
    type EnumSettings =
        unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut NvDrsSetting) -> Status;

    const OK: Status = 0;
    const SETTING_NOT_FOUND: Status = -160;
    const PROFILE_NOT_FOUND: Status = -163;
    const EXECUTABLE_NOT_FOUND: Status = -166;

    #[path = "helpers.rs"]
    mod helpers;
    use helpers::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NativeDrsProfile(Handle);

    #[derive(Debug)]
    pub struct NativeNvapiDrs {
        module: HMODULE,
        module_sha256: String,
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

    impl NativeNvapiDrs {
        pub fn load() -> Result<Self, DrsError> {
            let path =
                system32_nvapi_path().map_err(|reason| error("resolve nvapi64.dll", reason))?;
            let module = unsafe {
                LoadLibraryExW(
                    PCWSTR(wide_path(&path).as_ptr()),
                    None,
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            }
            .map_err(|reason| error("load nvapi64.dll", reason.to_string()))?;
            let module_sha256 = match sha256_file(&path) {
                Ok(value) => value,
                Err(reason) => {
                    unsafe {
                        let _ = FreeLibrary(module);
                    }
                    return Err(error("hash nvapi64.dll", reason));
                }
            };
            if let Err(reason) = verify_loaded_module_path(module, &path) {
                unsafe {
                    let _ = FreeLibrary(module);
                }
                return Err(error("validate nvapi64.dll identity", reason));
            }
            let query =
                unsafe { GetProcAddress(module, PCSTR(c"nvapi_QueryInterface".as_ptr().cast())) }
                    .ok_or_else(|| error("resolve nvapi_QueryInterface", "export is missing"))?;
            let query: QueryInterface = unsafe { transmute(query) };
            let functions = unsafe { Functions::resolve(query) };
            match functions {
                Ok(functions) => Ok(functions.into_host(module, module_sha256)),
                Err(reason) => {
                    unsafe {
                        let _ = FreeLibrary(module);
                    }
                    Err(reason)
                }
            }
        }

        #[must_use]
        pub fn module_sha256(&self) -> &str {
            &self.module_sha256
        }

        #[must_use]
        pub const fn interface_version() -> &'static str {
            "nvapi-public-sdk-cd6918f60b3c9a0476fdfe7e89bb32330602049d"
        }

        /// Enumerates every public customized DRS profile, binding, and typed
        /// value. Predefined profiles with no custom application or setting
        /// records are omitted rather than copied as driver defaults.
        pub fn capture_customized_profiles(&mut self) -> Result<DrsSnapshot, DrsError> {
            crate::driver::drs::with_session(self, |api, session| {
                let mut count = 0_u32;
                status(
                    unsafe { (api.get_num_profiles)(session.0, &mut count) },
                    "DRS_GetNumProfiles",
                )?;
                if count > 4_096 {
                    return Err(error("DRS_EnumProfiles", "profile bound exceeded"));
                }
                let mut profiles = Vec::new();
                for index in 0..count {
                    let mut handle = std::ptr::null_mut();
                    status(
                        unsafe { (api.enum_profiles)(session.0, index, &mut handle) },
                        "DRS_EnumProfiles",
                    )?;
                    if handle.is_null() {
                        return Err(error("DRS_EnumProfiles", "returned a null profile"));
                    }
                    let profile = NativeDrsProfile(handle);
                    let mut info = NvDrsProfile::query()
                        .map_err(|reason| error("prepare profile query", reason))?;
                    status(
                        unsafe { (api.get_profile_info)(session.0, handle, &mut info) },
                        "DRS_GetProfileInfo",
                    )?;
                    if info.application_count > 4_096 || info.setting_count > 4_096 {
                        return Err(error("DRS_GetProfileInfo", "profile item bound exceeded"));
                    }
                    let applications =
                        api.enumerate_applications(session, &profile, info.application_count)?;
                    let settings = api.enumerate_settings(session, &profile, info.setting_count)?;
                    if info.is_predefined == 0 || !applications.is_empty() || !settings.is_empty() {
                        profiles.push(DrsProfileSnapshot {
                            name: info
                                .name()
                                .map_err(|reason| error("decode profile name", reason))?,
                            applications,
                            settings,
                        });
                    }
                }
                let snapshot = DrsSnapshot {
                    schema_version: NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
                    profiles,
                };
                snapshot
                    .validate()
                    .map_err(|reason| error("validate DRS snapshot", format!("{reason:?}")))?;
                Ok(snapshot)
            })
        }

        /// Merge-restores a backup without deleting profiles or driver
        /// defaults. Rejected values and conflicting application bindings are
        /// returned for explicit reconciliation.
        pub fn merge_customized_profiles(
            &mut self,
            snapshot: &DrsSnapshot,
        ) -> Result<Vec<DrsItemKey>, DrsError> {
            snapshot
                .validate()
                .map_err(|reason| error("validate DRS restore", format!("{reason:?}")))?;
            crate::driver::drs::with_session(self, |api, session| {
                let mut incompatible = Vec::new();
                for captured in &snapshot.profiles {
                    let profile = match api.find_profile_by_name(session, &captured.name)? {
                        Some(profile) => profile,
                        None => api.create_profile(session, &captured.name)?,
                    };
                    for application in &captured.applications {
                        match api.find_application_profile(session, &application.executable)? {
                            None => {
                                if api
                                    .bind_application(session, &profile, &application.executable)
                                    .is_err()
                                {
                                    incompatible.push(DrsItemKey {
                                        profile: captured.name.clone(),
                                        kind: DrsItemKind::Application,
                                        key: application.executable.clone(),
                                    });
                                }
                            }
                            Some(owner) if owner == profile => {}
                            Some(_) => incompatible.push(DrsItemKey {
                                profile: captured.name.clone(),
                                kind: DrsItemKind::Application,
                                key: application.executable.clone(),
                            }),
                        }
                    }
                    for setting in &captured.settings {
                        let mut value = NvDrsSetting::typed(setting.setting_id, &setting.value)
                            .map_err(|reason| error("prepare typed DRS setting", reason))?;
                        if status(
                            unsafe { (api.set_setting)(session.0, profile.0, &mut value) },
                            "DRS_SetSetting",
                        )
                        .is_err()
                        {
                            incompatible.push(DrsItemKey {
                                profile: captured.name.clone(),
                                kind: DrsItemKind::Setting,
                                key: setting.setting_id.to_string(),
                            });
                        }
                    }
                }
                api.save_settings(session)?;
                Ok(incompatible)
            })
        }

        fn enumerate_applications(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            count: u32,
        ) -> Result<Vec<DrsApplicationSnapshot>, DrsError> {
            let mut applications = Vec::new();
            for index in 0..count {
                let mut item = NvDrsApplicationV4::named("")
                    .map_err(|reason| error("prepare application enumeration", reason))?;
                let mut returned = 1_u32;
                status(
                    unsafe {
                        (self.enum_applications)(
                            session.0,
                            profile.0,
                            index,
                            &mut returned,
                            &mut item,
                        )
                    },
                    "DRS_EnumApplications",
                )?;
                if returned != 1 {
                    return Err(error("DRS_EnumApplications", "unexpected result count"));
                }
                if !item.is_predefined() {
                    applications.push(DrsApplicationSnapshot {
                        executable: item
                            .name()
                            .map_err(|reason| error("decode application name", reason))?,
                    });
                }
            }
            Ok(applications)
        }

        fn enumerate_settings(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            count: u32,
        ) -> Result<Vec<DrsSettingSnapshot>, DrsError> {
            let mut settings = Vec::new();
            for index in 0..count {
                let mut item = NvDrsSetting::query()
                    .map_err(|reason| error("prepare setting enumeration", reason))?;
                let mut returned = 1_u32;
                status(
                    unsafe {
                        (self.enum_settings)(session.0, profile.0, index, &mut returned, &mut item)
                    },
                    "DRS_EnumSettings",
                )?;
                if returned != 1 {
                    return Err(error("DRS_EnumSettings", "unexpected result count"));
                }
                if !item.current_is_predefined() {
                    settings.push(DrsSettingSnapshot {
                        setting_id: item.setting_id(),
                        value: item
                            .current_value()
                            .map_err(|reason| error("decode typed DRS setting", reason))?,
                    });
                }
            }
            Ok(settings)
        }
    }

    impl Drop for NativeNvapiDrs {
        fn drop(&mut self) {
            unsafe {
                let _ = FreeLibrary(self.module);
            }
        }
    }

    impl NvapiDrs for NativeNvapiDrs {
        type Session = NativeDrsSession;
        type Profile = NativeDrsProfile;

        fn initialize(&mut self) -> Result<(), DrsError> {
            status(unsafe { (self.initialize)() }, "NvAPI_Initialize")
        }
        fn create_session(&mut self) -> Result<NativeDrsSession, DrsError> {
            let mut session = std::ptr::null_mut();
            status(
                unsafe { (self.create_session)(&mut session) },
                "DRS_CreateSession",
            )?;
            (!session.is_null())
                .then_some(NativeDrsSession(session))
                .ok_or_else(|| error("DRS_CreateSession", "returned a null handle"))
        }
        fn destroy_session(&mut self, session: NativeDrsSession) -> Result<(), DrsError> {
            status(
                unsafe { (self.destroy_session)(session.0) },
                "DRS_DestroySession",
            )
        }
        fn load_settings(&mut self, session: &NativeDrsSession) -> Result<(), DrsError> {
            status(
                unsafe { (self.load_settings)(session.0) },
                "DRS_LoadSettings",
            )
        }
        fn save_settings(&mut self, session: &NativeDrsSession) -> Result<(), DrsError> {
            status(
                unsafe { (self.save_settings)(session.0) },
                "DRS_SaveSettings",
            )
        }
        fn find_profile_by_name(
            &mut self,
            session: &NativeDrsSession,
            name: &str,
        ) -> Result<Option<NativeDrsProfile>, DrsError> {
            let name =
                unicode_argument(name).map_err(|reason| error("encode profile name", reason))?;
            let mut profile = std::ptr::null_mut();
            optional_handle(
                unsafe { (self.find_profile)(session.0, name.as_ptr(), &mut profile) },
                PROFILE_NOT_FOUND,
                profile,
                "DRS_FindProfileByName",
            )
            .map(|value| value.map(NativeDrsProfile))
        }
        fn profile_name(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
        ) -> Result<String, DrsError> {
            let mut value =
                NvDrsProfile::query().map_err(|reason| error("prepare profile query", reason))?;
            status(
                unsafe { (self.get_profile_info)(session.0, profile.0, &mut value) },
                "DRS_GetProfileInfo",
            )?;
            value
                .name()
                .map_err(|reason| error("decode profile name", reason))
        }
        fn find_application_profile(
            &mut self,
            session: &NativeDrsSession,
            application: &str,
        ) -> Result<Option<NativeDrsProfile>, DrsError> {
            let name = unicode_argument(application)
                .map_err(|reason| error("encode application name", reason))?;
            let mut app = NvDrsApplicationV4::named(application)
                .map_err(|reason| error("prepare application query", reason))?;
            let mut profile = std::ptr::null_mut();
            optional_handle(
                unsafe {
                    (self.find_application)(session.0, name.as_ptr(), &mut profile, &mut app)
                },
                EXECUTABLE_NOT_FOUND,
                profile,
                "DRS_FindApplicationByName",
            )
            .map(|value| value.map(NativeDrsProfile))
        }
        fn create_profile(
            &mut self,
            session: &NativeDrsSession,
            name: &str,
        ) -> Result<NativeDrsProfile, DrsError> {
            let mut value =
                NvDrsProfile::named(name).map_err(|reason| error("prepare profile", reason))?;
            let mut profile = std::ptr::null_mut();
            status(
                unsafe { (self.create_profile)(session.0, &mut value, &mut profile) },
                "DRS_CreateProfile",
            )?;
            (!profile.is_null())
                .then_some(NativeDrsProfile(profile))
                .ok_or_else(|| error("DRS_CreateProfile", "returned a null handle"))
        }
        fn bind_application(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            application: &str,
        ) -> Result<(), DrsError> {
            let mut app = NvDrsApplicationV4::named(application)
                .map_err(|reason| error("prepare application", reason))?;
            status(
                unsafe { (self.create_application)(session.0, profile.0, &mut app) },
                "DRS_CreateApplication",
            )
        }
        fn delete_application(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            application: &str,
        ) -> Result<(), DrsError> {
            let application = unicode_argument(application)
                .map_err(|reason| error("encode application name", reason))?;
            status(
                unsafe { (self.delete_application)(session.0, profile.0, application.as_ptr()) },
                "DRS_DeleteApplication",
            )
        }
        fn delete_profile(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
        ) -> Result<(), DrsError> {
            status(
                unsafe { (self.delete_profile)(session.0, profile.0) },
                "DRS_DeleteProfile",
            )
        }
        fn read_dword(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            id: u32,
        ) -> Result<Option<u32>, DrsError> {
            let mut value =
                NvDrsSetting::query().map_err(|reason| error("prepare setting query", reason))?;
            let result = unsafe { (self.get_setting)(session.0, profile.0, id, &mut value) };
            if result == SETTING_NOT_FOUND {
                return Ok(None);
            }
            status(result, "DRS_GetSetting")?;
            if value.setting_type() != NVDRS_DWORD_TYPE {
                return Err(error("DRS_GetSetting", "setting is not DWORD"));
            }
            Ok(Some(value.current_dword()))
        }
        fn set_dword(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            id: u32,
            value: u32,
        ) -> Result<(), DrsError> {
            let mut setting = NvDrsSetting::dword(id, value)
                .map_err(|reason| error("prepare DWORD setting", reason))?;
            status(
                unsafe { (self.set_setting)(session.0, profile.0, &mut setting) },
                "DRS_SetSetting",
            )
        }
        fn restore_dword(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            original: DrsOriginalSetting,
        ) -> Result<(), DrsError> {
            match original.value {
                Some(value) => self.set_dword(session, profile, original.id, value),
                None => status(
                    unsafe { (self.delete_setting)(session.0, profile.0, original.id) },
                    "DRS_DeleteProfileSetting",
                ),
            }
        }
    }
}

#[cfg(windows)]
pub use native_drs_windows::NativeNvapiDrs;
