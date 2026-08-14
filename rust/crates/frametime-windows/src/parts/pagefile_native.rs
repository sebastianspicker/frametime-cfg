#[cfg(windows)]
mod native_pagefile_store {
    use super::{
        CreatedPagefileToken, PagefileInventory, PagefileSetting, PagefileStore,
        wmi::{
            boolean, object, on_mta, put_bool, put_string, put_uint32, query, require_class,
            services, string, uint32, uint64,
        },
    };
    use std::mem::size_of;
    use windows::{
        Win32::{
            Foundation::{CloseHandle, ERROR_NOT_ALL_ASSIGNED, GetLastError, HANDLE},
            Security::{
                AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
                SE_CREATE_PAGEFILE_NAME, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
                TOKEN_PRIVILEGES, TOKEN_QUERY,
            },
            System::{
                Threading::{GetCurrentProcess, OpenProcessToken},
                Wmi::{
                    IWbemServices, WBEM_FLAG_CREATE_ONLY, WBEM_FLAG_UPDATE_ONLY,
                    WBEM_GENERIC_FLAG_TYPE,
                },
            },
        },
        core::BSTR,
    };
    const COMPUTER_CLASS: &str = "Win32_ComputerSystem";
    const MEMORY_CLASS: &str = "Win32_PhysicalMemory";
    const OS_CLASS: &str = "Win32_OperatingSystem";
    const LOGICAL_DISK_CLASS: &str = "Win32_LogicalDisk";
    const PAGEFILE_CLASS: &str = "Win32_PageFileSetting";
    const COMPUTER_QUERY: &str =
        "SELECT AutomaticManagedPagefile, __CLASS, __PATH, __RELPATH FROM Win32_ComputerSystem";
    const MEMORY_QUERY: &str = "SELECT Capacity, __CLASS FROM Win32_PhysicalMemory";
    const OS_QUERY: &str = "SELECT SystemDrive, __CLASS FROM Win32_OperatingSystem";
    const DISK_QUERY: &str = "SELECT DeviceID, FreeSpace, __CLASS FROM Win32_LogicalDisk";
    const SETTINGS_QUERY: &str = "SELECT Name, InitialSize, MaximumSize, __CLASS, __PATH, __RELPATH FROM Win32_PageFileSetting";
    /// Restores the caller's prior privilege state before closing its token handle.
    struct ScopedPagefilePrivilege {
        token: HANDLE,
        previous: TOKEN_PRIVILEGES,
    }
    impl ScopedPagefilePrivilege {
        fn enable() -> Result<Self, String> {
            let mut token = HANDLE::default();
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                    &mut token,
                )
            }
            .map_err(|error| {
                format!("open process token for SeCreatePagefilePrivilege: {error}")
            })?;
            let mut luid = Default::default();
            unsafe { LookupPrivilegeValueW(None, SE_CREATE_PAGEFILE_NAME, &mut luid) }
                .map_err(|error| format!("resolve SeCreatePagefilePrivilege: {error}"))?;
            let requested = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let mut previous = TOKEN_PRIVILEGES::default();
            let mut returned = 0;
            unsafe {
                AdjustTokenPrivileges(
                    token,
                    false,
                    Some(&requested),
                    size_of::<TOKEN_PRIVILEGES>() as u32,
                    Some(&mut previous),
                    Some(&mut returned),
                )
            }
            .map_err(|error| format!("enable SeCreatePagefilePrivilege: {error}"))?;
            // AdjustTokenPrivileges can report success while assigning no privilege.
            if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
                close_token(token);
                return Err(
                    "SeCreatePagefilePrivilege is not assigned to the elevated token".into(),
                );
            }
            if returned != size_of::<TOKEN_PRIVILEGES>() as u32 {
                close_token(token);
                return Err(
                    "SeCreatePagefilePrivilege did not return a complete prior token state".into(),
                );
            }
            Ok(Self { token, previous })
        }
    }
    impl Drop for ScopedPagefilePrivilege {
        fn drop(&mut self) {
            // Restore the saved state first; either call is best-effort during unwinding.
            unsafe {
                let _ =
                    AdjustTokenPrivileges(self.token, false, Some(&self.previous), 0, None, None);
            }
            close_token(self.token);
        }
    }
    fn close_token(token: HANDLE) {
        // Token ownership is transferred here only after OpenProcessToken succeeds.
        unsafe {
            let _ = CloseHandle(token);
        }
    }

    fn exact_pagefile(
        services: &IWbemServices,
        setting: &PagefileSetting,
    ) -> Result<windows::Win32::System::Wmi::IWbemClassObject, String> {
        let item = object(services, &setting.object_path)?;
        let is_exact = string(&item, "__CLASS")? == PAGEFILE_CLASS
            && string(&item, "__PATH")? == setting.object_path
            && string(&item, "__RELPATH")? == setting.relative_path
            && string(&item, "Name")? == setting.path
            && uint32(&item, "InitialSize")? == setting.initial_size
            && uint32(&item, "MaximumSize")? == setting.maximum_size;
        if !is_exact {
            return Err(
                "exact pagefile token no longer resolves to its captured class, identity, or values"
                    .into(),
            );
        }
        Ok(item)
    }

    fn exact_computer(
        services: &IWbemServices,
        object_path: &str,
        relative_path: &str,
        expected: Option<bool>,
    ) -> Result<windows::Win32::System::Wmi::IWbemClassObject, String> {
        let item = object(services, object_path)?;
        let has_expected_value = expected
            .is_none_or(|value| boolean(&item, "AutomaticManagedPagefile").ok() == Some(value));
        let is_exact = string(&item, "__CLASS")? == COMPUTER_CLASS
            && string(&item, "__PATH")? == object_path
            && string(&item, "__RELPATH")? == relative_path
            && has_expected_value;
        if !is_exact {
            return Err(
                "exact computer-system token no longer resolves to its captured identity or value"
                    .into(),
            );
        }
        Ok(item)
    }

    struct WmiStore {
        services: IWbemServices,
    }
    impl WmiStore {
        fn connect() -> Result<Self, String> {
            Ok(Self {
                services: services()?,
            })
        }
    }
    impl PagefileStore for WmiStore {
        fn inventory(&mut self) -> Result<PagefileInventory, String> {
            let computer = query(&self.services, COMPUTER_QUERY)?;
            if computer.len() != 1 {
                return Err("Win32_ComputerSystem inventory is not exact".into());
            }
            let computer = &computer[0];
            require_class(computer, COMPUTER_CLASS, "computer-system inventory")?;
            let automatic_managed = boolean(computer, "AutomaticManagedPagefile")?;
            let computer_object_path = string(computer, "__PATH")?;
            let computer_relative_path = string(computer, "__RELPATH")?;
            let memory = query(&self.services, MEMORY_QUERY)?;
            if memory.is_empty() {
                return Err("Win32_PhysicalMemory inventory is empty".into());
            }
            let physical_ram_bytes = memory.iter().try_fold(0_u64, |total, item| {
                require_class(item, MEMORY_CLASS, "physical-memory inventory")?;
                total
                    .checked_add(uint64(item, "Capacity")?)
                    .ok_or_else(|| String::from("physical-memory capacity overflow"))
            })?;
            if physical_ram_bytes == 0 {
                return Err("physical-memory capacity is zero".into());
            }
            let physical_ram_mb = physical_ram_bytes / (1024 * 1024);
            let os = query(&self.services, OS_QUERY)?;
            if os.len() != 1 {
                return Err("Win32_OperatingSystem inventory is not exact".into());
            }
            let os = &os[0];
            require_class(os, OS_CLASS, "operating-system inventory")?;
            let system_drive = string(os, "SystemDrive")?;
            let disks = query(&self.services, DISK_QUERY)?;
            let mut system_drive_disks = Vec::new();
            for disk in &disks {
                // Do not skip malformed rows: an incomplete query response is unsafe inventory.
                require_class(disk, LOGICAL_DISK_CLASS, "logical-disk inventory")?;
                if string(disk, "DeviceID")?.eq_ignore_ascii_case(&system_drive) {
                    system_drive_disks.push(disk);
                }
            }
            if system_drive_disks.len() != 1 {
                return Err("system-drive logical-disk inventory is absent or ambiguous".into());
            }
            let free_space_mb = uint64(system_drive_disks[0], "FreeSpace")? / (1024 * 1024);
            let settings = query(&self.services, SETTINGS_QUERY)?
                .into_iter()
                .map(|item| {
                    require_class(&item, PAGEFILE_CLASS, "pagefile inventory")?;
                    Ok(PagefileSetting {
                        path: string(&item, "Name")?,
                        initial_size: uint32(&item, "InitialSize")?,
                        maximum_size: uint32(&item, "MaximumSize")?,
                        object_path: string(&item, "__PATH")?,
                        relative_path: string(&item, "__RELPATH")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(PagefileInventory {
                automatic_managed,
                computer_object_path,
                computer_relative_path,
                system_drive,
                physical_ram_mb,
                free_space_mb,
                settings,
            })
        }
        fn set_automatic(
            &mut self,
            object_path: &str,
            relative_path: &str,
            expected: Option<bool>,
            value: bool,
        ) -> Result<(), String> {
            let item = exact_computer(&self.services, object_path, relative_path, expected)?;
            put_bool(&item, "AutomaticManagedPagefile", value)?;
            unsafe {
                self.services.PutInstance(
                    &item,
                    WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_UPDATE_ONLY.0),
                    None,
                    None,
                )
            }
            .map_err(|error| format!("UPDATE_ONLY AutomaticManagedPagefile: {error}"))
        }
        fn update(
            &mut self,
            setting: &PagefileSetting,
            initial: u32,
            maximum: u32,
        ) -> Result<(), String> {
            let item = exact_pagefile(&self.services, setting)?;
            put_uint32(&item, "InitialSize", initial)?;
            put_uint32(&item, "MaximumSize", maximum)?;
            unsafe {
                self.services.PutInstance(
                    &item,
                    WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_UPDATE_ONLY.0),
                    None,
                    None,
                )
            }
            .map_err(|error| format!("UPDATE_ONLY pagefile: {error}"))
        }
        fn create(
            &mut self,
            path: &str,
            initial: u32,
            maximum: u32,
        ) -> Result<CreatedPagefileToken, String> {
            let class = object(&self.services, PAGEFILE_CLASS)?;
            let item = unsafe { class.SpawnInstance(0) }
                .map_err(|error| format!("spawn pagefile setting: {error}"))?;
            put_string(&item, "Name", path)?;
            put_uint32(&item, "InitialSize", initial)?;
            put_uint32(&item, "MaximumSize", maximum)?;
            unsafe {
                self.services.PutInstance(
                    &item,
                    WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_CREATE_ONLY.0),
                    None,
                    None,
                )
            }
            .map_err(|error| format!("CREATE_ONLY pagefile: {error}"))?;
            let created = self
                .inventory()?
                .settings
                .into_iter()
                .filter(|item| item.path.eq_ignore_ascii_case(path))
                .collect::<Vec<_>>();
            if created.len() != 1 {
                return Err("CREATE_ONLY pagefile did not produce one exact instance".into());
            }
            let created = &created[0];
            Ok(CreatedPagefileToken {
                object_path: created.object_path.clone(),
                relative_path: created.relative_path.clone(),
                path: created.path.clone(),
                initial_size: created.initial_size,
                maximum_size: created.maximum_size,
            })
        }
        fn delete(&mut self, created: &CreatedPagefileToken) -> Result<(), String> {
            let setting = PagefileSetting {
                path: created.path.clone(),
                initial_size: created.initial_size,
                maximum_size: created.maximum_size,
                object_path: created.object_path.clone(),
                relative_path: created.relative_path.clone(),
            };
            let _ = exact_pagefile(&self.services, &setting)?;
            unsafe {
                self.services.DeleteInstance(
                    &BSTR::from(&created.object_path),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    None,
                )
            }
            .map_err(|error| format!("delete exact created pagefile: {error}"))
        }
    }
    pub(super) fn inventory() -> Result<PagefileInventory, String> {
        on_mta(|| WmiStore::connect()?.inventory())
    }
    pub(super) fn begin(
        binding: &super::PagefileBinding,
    ) -> Result<Option<CreatedPagefileToken>, String> {
        let binding = binding.clone();
        on_mta(move || {
            let _privilege = ScopedPagefilePrivilege::enable()?;
            let mut store = WmiStore::connect()?;
            match super::begin_pagefile_mutation(&mut store, &binding) {
                Ok(created) => Ok(created),
                Err(error) => match super::compensate_pagefile_mutation(&mut store, &binding, None)
                {
                    Ok(()) => Err(format!(
                        "P1:8 mutation: {error}; target and automatic-management compensation completed"
                    )),
                    Err(rollback) => Err(format!(
                        "P1:8 mutation: {error}; compensation also failed: {rollback}"
                    )),
                },
            }
        })
    }
    pub(super) fn verify(
        binding: &super::PagefileBinding,
        created: Option<&CreatedPagefileToken>,
    ) -> Result<(), String> {
        let binding = binding.clone();
        let created = created.cloned();
        on_mta(move || {
            let mut store = WmiStore::connect()?;
            super::verify_pagefile_mutation(&mut store, &binding, created.as_ref())
        })
    }
    pub(super) fn compensate(
        binding: &super::PagefileBinding,
        created: Option<&CreatedPagefileToken>,
    ) -> Result<(), String> {
        let binding = binding.clone();
        let created = created.cloned();
        on_mta(move || {
            let _privilege = ScopedPagefilePrivilege::enable()?;
            let mut store = WmiStore::connect()?;
            super::compensate_pagefile_mutation(&mut store, &binding, created.as_ref())
        })
    }
    pub(super) fn restore(entry: &super::BackupEntry) -> Result<(), String> {
        let entry = entry.clone();
        on_mta(move || {
            let _privilege = ScopedPagefilePrivilege::enable()?;
            let mut store = WmiStore::connect()?;
            super::restore_pagefile_transaction(&mut store, &entry)
        })
    }
}
#[cfg(windows)]
fn native_pagefile_inventory() -> Result<PagefileInventory, String> {
    native_pagefile_store::inventory()
}
#[cfg(not(windows))]
fn native_pagefile_inventory() -> Result<PagefileInventory, String> {
    Err("pagefile inventory requires Windows CIM".into())
}
#[cfg(windows)]
fn native_pagefile_begin(
    binding: &PagefileBinding,
) -> Result<Option<CreatedPagefileToken>, String> {
    native_pagefile_store::begin(binding)
}
#[cfg(not(windows))]
fn native_pagefile_begin(_: &PagefileBinding) -> Result<Option<CreatedPagefileToken>, String> {
    Err("pagefile mutation requires Windows CIM".into())
}
#[cfg(windows)]
fn native_pagefile_verify(
    binding: &PagefileBinding,
    created: Option<&CreatedPagefileToken>,
) -> Result<(), String> {
    native_pagefile_store::verify(binding, created)
}
#[cfg(not(windows))]
fn native_pagefile_verify(
    _: &PagefileBinding,
    _: Option<&CreatedPagefileToken>,
) -> Result<(), String> {
    Err("pagefile verification requires Windows CIM".into())
}
#[cfg(windows)]
fn native_pagefile_compensate(
    binding: &PagefileBinding,
    created: Option<&CreatedPagefileToken>,
) -> Result<(), String> {
    native_pagefile_store::compensate(binding, created)
}
#[cfg(not(windows))]
fn native_pagefile_compensate(
    _: &PagefileBinding,
    _: Option<&CreatedPagefileToken>,
) -> Result<(), String> {
    Err("pagefile compensation requires Windows CIM".into())
}
#[cfg(windows)]
fn native_pagefile_restore(entry: &BackupEntry) -> Result<(), String> {
    native_pagefile_store::restore(entry)
}
#[cfg(not(windows))]
fn native_pagefile_restore(_: &BackupEntry) -> Result<(), String> {
    Err("pagefile recovery requires Windows CIM".into())
}
#[cfg(windows)]
fn native_device_guard_status() -> Result<u32, String> {
    wmi::device_guard_status()
}
#[cfg(not(windows))]
fn native_device_guard_status() -> Result<u32, String> {
    Err("P3:7 DeviceGuard detection requires Windows CIM".into())
}
