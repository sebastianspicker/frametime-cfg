// P1:7 HAGS is a typed WDDM transaction, not a display-name or registry-only
// heuristic. `HwSchMode` is a request; effectiveness is only established by
// the post-reboot D3DKMT feature query retained in the recovery receipt.

const HAGS_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers";
const HAGS_NAME: &str = "HwSchMode";
const HAGS_TARGET: u32 = 2;
#[cfg(any(test, windows))]
const HAGS_MIN_BUILD: u32 = 26_100;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HagsRegistryCompatibility {
    adapter_ids: Vec<String>,
    original: Option<u32>,
}

impl HagsRegistryCompatibility {
    fn capture() -> Result<(Self, frametime_core::BackupEntry), String> {
        let adapter_ids = compatible_adapter_ids()?;
        let original = read_hags_value()?;
        let binding = Self {
            adapter_ids: adapter_ids.clone(),
            original,
        };
        Ok((
            binding,
            frametime_core::BackupEntry::Hags {
                step: "P1:7".into(),
                timestamp: timestamp(),
                original_value: original,
                target_value: HAGS_TARGET,
                adapter_ids,
                effective_verification_pending: true,
                unknown: BTreeMap::new(),
            },
        ))
    }

    fn apply(&self) -> Result<(), String> {
        if compatible_adapter_ids()? != self.adapter_ids {
            return Err("P1:7 display-adapter binding drifted after backup capture".into());
        }
        write_hags_value(HAGS_TARGET)?;
        if read_hags_value()? == Some(HAGS_TARGET) {
            Ok(())
        } else {
            Err("P1:7 HwSchMode immediate DWORD readback did not equal 2".into())
        }
    }

    fn verify_immediate(&self) -> Result<(), String> {
        if read_hags_value()? == Some(HAGS_TARGET) {
            Ok(())
        } else {
            Err("P1:7 HwSchMode immediate DWORD readback did not equal 2".into())
        }
    }
}

fn inspect_hags() -> Result<Inspection, String> {
    match compatible_adapter_ids() {
        Ok(_) if read_hags_value()? == Some(HAGS_TARGET) => Ok(Inspection::Satisfied),
        Ok(_) => Ok(Inspection::NeedsApply),
        Err(error) if error.starts_with("P1:7 inapplicable:") => Ok(Inspection::Inapplicable),
        Err(error) if error.starts_with("P1:7 unavailable:") => Ok(Inspection::Unsupported),
        Err(error) => Err(error),
    }
}

fn restore_hags_entry(
    step: &str,
    original: Option<u32>,
    target: u32,
    adapter_ids: &[String],
    pending: bool,
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if step != "P1:7"
        || target != HAGS_TARGET
        || adapter_ids.is_empty()
        || !pending
        || !unknown.is_empty()
        || original.is_some_and(|value| !matches!(value, 0..=2))
    {
        return Err("P1:7 HAGS recovery receipt is not an exact compatible binding".into());
    }
    match original {
        Some(value) => write_hags_value(value)?,
        None => registry_delete(Hive::LocalMachine, HAGS_KEY, HAGS_NAME)?,
    }
    if read_hags_value()? == original {
        Ok(())
    } else {
        Err("P1:7 HAGS recovery readback did not match the captured DWORD state".into())
    }
}

fn hags_pending_verification(entry: &frametime_core::BackupEntry) -> Option<VerificationItem> {
    let frametime_core::BackupEntry::Hags {
        adapter_ids,
        effective_verification_pending,
        ..
    } = entry
    else {
        return None;
    };
    if !effective_verification_pending {
        return Some(VerificationItem {
            status: VerificationStatus::Info,
            name: "HAGS effective state".into(),
            detail: "the durable HAGS receipt was already acknowledged".into(),
        });
    }
    let result = compatible_adapter_states();
    let (status, detail) = match result {
        Ok(states) if states.iter().map(HagsAdapterState::identity).collect::<Vec<_>>() != *adapter_ids => (
            VerificationStatus::Changed,
            "P1:7 display-adapter identities changed; retained receipt cannot be cleared".into(),
        ),
        Ok(states) if states.iter().all(HagsAdapterState::enabled) => (
            VerificationStatus::Ok,
            "D3DKMT reports HWSCH Enabled on every captured nonsoftware adapter; receipt remains durable until an explicit acknowledgement transaction is added".into(),
        ),
        Ok(_) => (
            VerificationStatus::Missing,
            "HwSchMode requested HAGS, but D3DKMT does not yet report HWSCH Enabled on every adapter; reboot and re-run verification".into(),
        ),
        Err(error) => (VerificationStatus::Missing, error),
    };
    Some(VerificationItem {
        status,
        name: "HAGS effective state".into(),
        detail,
    })
}

fn hags_pending_verification_items(
    trusted: &TrustedWorkDir,
) -> Result<Vec<VerificationItem>, String> {
    let path = trusted.path().join(BACKUP_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let backup: frametime_core::BackupFile = read_json_trusted(trusted, BACKUP_FILE)
        .map_err(|error| format!("read P1:7 HAGS receipt: {error}"))?;
    Ok(backup
        .entries
        .iter()
        .filter_map(hags_pending_verification)
        .collect())
}

fn require_hags_effective_before_final_checklist(trusted: &TrustedWorkDir) -> Result<(), String> {
    let items = hags_pending_verification_items(trusted)?;
    if items
        .iter()
        .all(|item| item.status == VerificationStatus::Ok)
    {
        Ok(())
    } else {
        Err("P3:12 requires the retained P1:7 HAGS receipt to report D3DKMT HWSCH Enabled after reboot".into())
    }
}

fn read_hags_value() -> Result<Option<u32>, String> {
    let change = hags_change(HAGS_TARGET);
    match registry_read_exact(&change)? {
        None => Ok(None),
        Some(RegValue::Dword(value @ 0..=2)) => Ok(Some(value)),
        Some(RegValue::Dword(_)) => {
            Err("P1:7 HwSchMode DWORD is outside the supported 0/1/2 contract".into())
        }
        Some(_) => Err("P1:7 HwSchMode is not a DWORD".into()),
    }
}

fn write_hags_value(value: u32) -> Result<(), String> {
    if !matches!(value, 0..=2) {
        return Err("P1:7 only permits HwSchMode DWORD 0, 1, or 2".into());
    }
    registry_write(&hags_change(value))
}

fn hags_change(value: u32) -> RegistryChange {
    RegistryChange {
        hive: Hive::LocalMachine,
        key: HAGS_KEY,
        name: HAGS_NAME,
        value: RegValue::Dword(value),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HagsAdapterState {
    identity: String,
    supported: bool,
    enabled: bool,
}
impl HagsAdapterState {
    fn identity(&self) -> String {
        self.identity.clone()
    }
    const fn enabled(&self) -> bool {
        self.enabled
    }
}

fn compatible_adapter_ids() -> Result<Vec<String>, String> {
    let states = compatible_adapter_states()?;
    if states.iter().all(|state| state.supported) {
        Ok(states.into_iter().map(|state| state.identity).collect())
    } else {
        Err("P1:7 unavailable: HWSCH is not stably supported by every nonsoftware adapter".into())
    }
}

#[cfg(not(windows))]
fn compatible_adapter_states() -> Result<Vec<HagsAdapterState>, String> {
    Err("P1:7 unavailable: native HAGS inspection requires Windows".into())
}

#[cfg(windows)]
fn compatible_adapter_states() -> Result<Vec<HagsAdapterState>, String> {
    native::inspect()
}

#[cfg(windows)]
mod native {
    use crate::{HagsAdapterState, PciDeviceClass, PciDeviceEnumerator, WindowsSetupApiEnumerator};
    use raw_cpuid::CpuId;
    use std::{
        collections::BTreeSet,
        mem::{align_of, size_of},
        path::PathBuf,
    };
    use windows::{
        Wdk::Graphics::Direct3D::{
            D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID, D3DKMTCloseAdapter,
            D3DKMTOpenAdapterFromLuid,
        },
        Win32::{
            Foundation::{FreeLibrary, HMODULE, LUID},
            Graphics::Dxgi::{
                CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND, IDXGIFactory1,
            },
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

    const FEATURE_HWSCH: u32 = 0;
    #[repr(C)]
    struct IsFeatureEnabled {
        adapter: u32,
        feature_id: u32,
        result: FeatureResult,
    }
    #[repr(C)]
    #[derive(Default)]
    struct FeatureResult {
        version: u16,
        flags: u16,
    }
    const _: () = assert!(size_of::<IsFeatureEnabled>() == 12);
    const _: () = assert!(align_of::<IsFeatureEnabled>() == 4);
    const _: () = assert!(size_of::<FeatureResult>() == 4);
    type IsFeatureEnabledFn = unsafe extern "system" fn(*mut IsFeatureEnabled) -> i32;

    #[derive(Clone, Copy)]
    struct NumericAdapter {
        luid: LUID,
        vendor: u16,
        device: u16,
        subsystem: u32,
        revision: u8,
    }

    pub(super) fn inspect() -> Result<Vec<HagsAdapterState>, String> {
        let vendor = CpuId::new()
            .get_vendor_info()
            .map(|value| value.as_str().to_owned());
        if vendor.as_deref() == Some("AuthenticAMD") || vendor.is_none() {
            return Err("P1:7 inapplicable: AMD or ambiguous CPUID vendor is retained as a legacy safety gate; X3D is never inferred".into());
        }
        if crate::platform::build_number()? < super::HAGS_MIN_BUILD {
            return Err(
                "P1:7 unavailable: D3DKMTIsFeatureEnabled requires Windows build 26100 or later"
                    .into(),
            );
        }
        let adapters = dxgi_adapters()?;
        let setup = WindowsSetupApiEnumerator
            .enumerate_pci_devices()
            .map_err(|error| format!("P1:7 SetupAPI display binding: {error}"))?;
        let mut used = BTreeSet::new();
        let query = Gdi32FeatureQuery::load()?;
        let mut states = Vec::with_capacity(adapters.len());
        for adapter in adapters {
            let candidates = setup
                .iter()
                .enumerate()
                .filter(|(_, observed)| {
                    observed
                        .binding
                        .class_guid
                        .eq_ignore_ascii_case(PciDeviceClass::Display.class_guid())
                        && observed.present
                        && observed.status_ok
                        && observed.binding.vendor_id == adapter.vendor
                        && observed.binding.device_id == adapter.device
                        && observed.binding.subsystem_vendor_id == (adapter.subsystem >> 16) as u16
                        && observed.binding.subsystem_device_id == adapter.subsystem as u16
                        && observed.binding.revision_id == adapter.revision
                })
                .collect::<Vec<_>>();
            if candidates.len() != 1
                || !used.insert(candidates.first().map(|item| item.0).unwrap_or(usize::MAX))
            {
                return Err("P1:7 display binding is ambiguous or does not exactly match DXGI numeric PCI identity".into());
            }
            let identity = format!(
                "pci-{:04x}-{:04x}-{:08x}-{:02x}",
                adapter.vendor, adapter.device, adapter.subsystem, adapter.revision
            );
            let flags = query.query(adapter.luid)?;
            states.push(HagsAdapterState {
                identity,
                supported: flags.stably_supported(),
                enabled: flags.enabled(),
            });
        }
        if states.is_empty() {
            Err("P1:7 unavailable: DXGI returned no nonsoftware adapters".into())
        } else {
            Ok(states)
        }
    }

    fn dxgi_adapters() -> Result<Vec<NumericAdapter>, String> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|e| e.to_string())?;
        let mut adapters = Vec::new();
        for index in 0_u32..256 {
            let adapter = match unsafe { factory.EnumAdapters1(index) } {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(error.to_string()),
            };
            let desc = unsafe { adapter.GetDesc1() }.map_err(|e| e.to_string())?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                adapters.push(NumericAdapter {
                    luid: desc.AdapterLuid,
                    vendor: desc.VendorId as u16,
                    device: desc.DeviceId as u16,
                    subsystem: desc.SubSysId,
                    revision: desc.Revision as u8,
                });
            }
        }
        Ok(adapters)
    }

    #[derive(Default)]
    struct FeatureFlags(u16);
    impl FeatureFlags {
        const fn enabled(&self) -> bool {
            self.0 & 1 != 0
        }
        const fn stably_supported(&self) -> bool {
            self.0 & 0b0001_1110 == 0b0001_1110
                && self.0 & 0b0010_0000 == 0
                && self.0 & !0b0011_1111 == 0
        }
    }
    struct Gdi32FeatureQuery {
        module: HMODULE,
        query: IsFeatureEnabledFn,
    }
    impl Gdi32FeatureQuery {
        fn load() -> Result<Self, String> {
            let path = system32_gdi32_path()?;
            let module = unsafe {
                LoadLibraryExW(
                    PCWSTR(wide(&path).as_ptr()),
                    None,
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            }
            .map_err(|e| format!("P1:7 load System32 gdi32.dll: {e}"))?;
            if loaded_path(module)? != path {
                unsafe {
                    let _ = FreeLibrary(module);
                }
                return Err("P1:7 loaded gdi32.dll is not the absolute System32 identity".into());
            }
            let export =
                unsafe { GetProcAddress(module, PCSTR(c"D3DKMTIsFeatureEnabled".as_ptr().cast())) };
            let Some(export) = export else {
                unsafe {
                    let _ = FreeLibrary(module);
                }
                return Err("P1:7 System32 gdi32.dll has no D3DKMTIsFeatureEnabled export".into());
            };
            // The verified System32 export has the documented C ABI and 12-byte input layout.
            let query = unsafe {
                std::mem::transmute::<unsafe extern "system" fn() -> isize, IsFeatureEnabledFn>(
                    export,
                )
            };
            Ok(Self { module, query })
        }
        fn query(&self, luid: LUID) -> Result<FeatureFlags, String> {
            let mut open = D3DKMT_OPENADAPTERFROMLUID {
                AdapterLuid: luid,
                hAdapter: 0,
            };
            if unsafe { D3DKMTOpenAdapterFromLuid(&mut open) }.0 != 0 || open.hAdapter == 0 {
                return Err("P1:7 D3DKMTOpenAdapterFromLuid failed".into());
            }
            let mut args = IsFeatureEnabled {
                adapter: open.hAdapter,
                feature_id: FEATURE_HWSCH,
                result: FeatureResult::default(),
            };
            let status = unsafe { (self.query)(&mut args) };
            let close = unsafe {
                D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
                    hAdapter: open.hAdapter,
                })
            };
            if close.0 != 0 {
                return Err("P1:7 D3DKMTCloseAdapter failed".into());
            }
            if status != 0 {
                return Err(format!(
                    "P1:7 D3DKMTIsFeatureEnabled failed with NTSTATUS {status}"
                ));
            }
            Ok(FeatureFlags(args.result.flags))
        }
    }
    impl Drop for Gdi32FeatureQuery {
        fn drop(&mut self) {
            unsafe {
                let _ = FreeLibrary(self.module);
            }
        }
    }
    fn wide(path: &std::path::Path) -> Vec<u16> {
        path.as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }
    fn system32_gdi32_path() -> Result<PathBuf, String> {
        let mut buffer = vec![0_u16; 32_768];
        let copied = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
        if copied == 0 || copied >= buffer.len() {
            return Err("P1:7 GetSystemDirectoryW failed or was truncated".into());
        }
        buffer.truncate(copied);
        let root =
            String::from_utf16(&buffer).map_err(|_| "P1:7 System32 path is invalid UTF-16")?;
        Ok(PathBuf::from(root).join("gdi32.dll"))
    }
    fn loaded_path(module: HMODULE) -> Result<PathBuf, String> {
        let mut buffer = vec![0_u16; 32_768];
        let copied = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
        if copied == 0 || copied >= buffer.len() {
            return Err("P1:7 GetModuleFileNameW failed or was truncated".into());
        }
        buffer.truncate(copied);
        String::from_utf16(&buffer)
            .map(PathBuf::from)
            .map_err(|_| "P1:7 loaded module path is invalid UTF-16".into())
    }
}

#[cfg(test)]
mod hags_tests {
    use super::*;
    #[test]
    fn hags_registry_contract_is_fixed() {
        assert_eq!(hags_change(2).key, HAGS_KEY);
        assert_eq!(hags_change(2).name, HAGS_NAME);
        assert_eq!(hags_change(2).value, RegValue::Dword(2));
        assert_eq!(HAGS_MIN_BUILD, 26_100);
    }
}
