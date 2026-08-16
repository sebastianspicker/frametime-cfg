#[cfg(windows)]
mod wmi {
    use std::{mem::ManuallyDrop, sync::OnceLock, thread};
    use windows::{
        Win32::{
            Foundation::VARIANT_BOOL,
            System::{
                Com::{
                    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                    CoInitializeSecurity, CoSetProxyBlanket, CoUninitialize, EOAC_NONE,
                    RPC_C_AUTHN_LEVEL_CALL, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
                },
                Ole::{
                    SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
                    SafeArrayGetVartype,
                },
                Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE},
                Variant::{
                    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_ARRAY, VT_BOOL, VT_BSTR,
                    VT_I1, VT_I2, VT_I4, VT_NULL, VT_UI1,
                },
                Wmi::{
                    CIM_BOOLEAN, CIM_SINT8, CIM_STRING, CIM_UINT8, CIM_UINT16, CIM_UINT32,
                    CIM_UINT64, IEnumWbemClassObject, IWbemClassObject, IWbemLocator,
                    IWbemServices, WBEM_FLAG_ENSURE_LOCATABLE, WBEM_FLAG_FORWARD_ONLY,
                    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE,
                    WBEM_S_FALSE, WbemLocator,
                },
            },
        },
        core::{BSTR, PCWSTR},
    };

    const WQL: &str = "WQL";
    const DEVICE_GUARD_NAMESPACE: &str = "ROOT\\Microsoft\\Windows\\DeviceGuard";
    const DEVICE_GUARD_CLASS: &str = "Win32_DeviceGuard";
    const DEVICE_GUARD_QUERY: &str =
        "SELECT VirtualizationBasedSecurityStatus, __CLASS FROM Win32_DeviceGuard";

    /// Balances a successful MTA initialization on the dedicated WMI thread.
    struct ScopedCom;

    impl ScopedCom {
        fn initialize() -> Result<Self, String> {
            // Every WMI operation below runs on this dedicated MTA thread.
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                .ok()
                .map_err(|error| format!("CoInitializeEx(MTA): {error}"))?;
            Ok(Self)
        }
    }

    impl Drop for ScopedCom {
        fn drop(&mut self) {
            // CoUninitialize is paired only with the successful call above.
            unsafe { CoUninitialize() };
        }
    }

    pub(crate) fn on_mta<T: Send + 'static>(
        operation: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        thread::spawn(move || {
            let _com = ScopedCom::initialize()?;
            static SECURITY: OnceLock<Result<(), String>> = OnceLock::new();
            // Process-wide COM security is initialized once, after this thread enters the MTA.
            SECURITY
                .get_or_init(|| unsafe {
                    CoInitializeSecurity(
                        None,
                        -1,
                        None,
                        None,
                        RPC_C_AUTHN_LEVEL_DEFAULT,
                        RPC_C_IMP_LEVEL_IMPERSONATE,
                        None,
                        EOAC_NONE,
                        None,
                    )
                    .map_err(|error| format!("CoInitializeSecurity: {error}"))
                })
                .clone()?;
            operation()
        })
        .join()
        .map_err(|_| String::from("pagefile MTA thread panicked"))?
    }

    pub(super) fn services() -> Result<IWbemServices, String> {
        services_at("ROOT\\CIMV2")
    }

    pub(crate) fn services_at(namespace: &str) -> Result<IWbemServices, String> {
        let locator: IWbemLocator =
            unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("create IWbemLocator: {error}"))?;
        let empty = BSTR::new();
        let services = unsafe {
            locator.ConnectServer(
                &BSTR::from(namespace),
                &empty,
                &empty,
                &empty,
                0,
                &empty,
                None,
            )
        }
        .map_err(|error| format!("connect {namespace}: {error}"))?;
        set_services_proxy_blanket(&services)?;
        Ok(services)
    }

    fn set_services_proxy_blanket(services: &IWbemServices) -> Result<(), String> {
        unsafe {
            CoSetProxyBlanket(
                services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
        }
        .map_err(|error| format!("CoSetProxyBlanket(IWbemServices): {error}"))
    }

    fn set_enumerator_proxy_blanket(enumerator: &IEnumWbemClassObject) -> Result<(), String> {
        unsafe {
            CoSetProxyBlanket(
                enumerator,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
        }
        .map_err(|error| format!("CoSetProxyBlanket(IEnumWbemClassObject): {error}"))
    }

    pub(crate) fn query(
        services: &IWbemServices,
        query: &str,
    ) -> Result<Vec<IWbemClassObject>, String> {
        let enumerator: IEnumWbemClassObject = unsafe {
            services.ExecQuery(
                &BSTR::from(WQL),
                &BSTR::from(query),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY | WBEM_FLAG_ENSURE_LOCATABLE,
                None,
            )
        }
        .map_err(|error| format!("WMI query: {error}"))?;
        // ExecQuery returns a proxy with independent authentication settings.
        set_enumerator_proxy_blanket(&enumerator)?;
        let mut values = Vec::new();
        loop {
            let mut object = [None];
            let mut returned = 0;
            let result = unsafe { enumerator.Next(WBEM_INFINITE, &mut object, &mut returned) };
            // One-element output storage means any larger reported count is invalid.
            if returned > 1 || !matches!(result.0, 0 | 1) {
                return Err("WMI enumeration returned an unexpected status or object count".into());
            }
            if returned == 0 {
                // WMI completion must be explicit, not merely an empty success result.
                if result.0 != WBEM_S_FALSE.0 {
                    return Err("WMI enumeration ended without WBEM_S_FALSE".into());
                }
                break;
            }
            values.push(
                object[0]
                    .take()
                    .ok_or("WMI returned an empty object slot")?,
            );
        }
        Ok(values)
    }

    fn property_name(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(Some(0)).collect()
    }

    fn value(
        object: &IWbemClassObject,
        name: &str,
        expected_cim: i32,
        expected_vt: u16,
    ) -> Result<VARIANT, String> {
        let mut variant = VARIANT::default();
        let mut cim = 0;
        let wide_name = property_name(name);
        unsafe {
            object.Get(
                PCWSTR(wide_name.as_ptr()),
                0,
                &mut variant,
                Some(&mut cim),
                None,
            )
        }
        .map_err(|error| format!("read WMI property {name}: {error}"))?;
        let vt = unsafe { variant.Anonymous.Anonymous.vt.0 };
        if cim != expected_cim || vt != expected_vt {
            return Err(format!(
                "WMI property {name} has unexpected CIM/VARIANT type"
            ));
        }
        Ok(variant)
    }

    fn bstr_value(
        object: &IWbemClassObject,
        name: &str,
        expected_cim: i32,
    ) -> Result<String, String> {
        let variant = value(object, name, expected_cim, VT_BSTR.0)?;
        // VT_BSTR guarantees bstrVal is initialized for the lifetime of variant.
        unsafe {
            let raw = &variant.Anonymous.Anonymous.Anonymous.bstrVal as *const ManuallyDrop<BSTR>
                as *const BSTR;
            Ok((*raw).to_string())
        }
    }

    pub(crate) fn string(object: &IWbemClassObject, name: &str) -> Result<String, String> {
        bstr_value(object, name, CIM_STRING.0)
    }

    pub(crate) fn nullable_string(object: &IWbemClassObject, name: &str) -> Result<String, String> {
        let mut variant = VARIANT::default();
        let mut cim = 0;
        let wide_name = property_name(name);
        unsafe {
            object.Get(
                PCWSTR(wide_name.as_ptr()),
                0,
                &mut variant,
                Some(&mut cim),
                None,
            )
        }
        .map_err(|error| format!("read WMI property {name}: {error}"))?;
        let vt = unsafe { variant.Anonymous.Anonymous.vt.0 };
        if vt == VT_NULL.0 {
            return Ok(String::new());
        }
        if cim != CIM_STRING.0 || vt != VT_BSTR.0 {
            return Err(format!(
                "WMI property {name} has unexpected CIM/VARIANT type"
            ));
        }
        unsafe {
            let raw = &variant.Anonymous.Anonymous.Anonymous.bstrVal as *const ManuallyDrop<BSTR>
                as *const BSTR;
            Ok((*raw).to_string())
        }
    }

    /// Reads a schema qualifier represented by WMI as a one-dimensional
    /// `SAFEARRAY(BSTR)`.  It is deliberately strict: a provider changing
    /// the qualifier type or shape must stop mutation rather than be guessed.
    pub(crate) fn property_qualifier_bstr_array(
        object: &IWbemClassObject,
        property: &str,
        qualifier: &str,
    ) -> Result<Vec<String>, String> {
        let property = property_name(property);
        let qualifiers = unsafe { object.GetPropertyQualifierSet(PCWSTR(property.as_ptr())) }
            .map_err(|error| format!("read WMI property qualifier set: {error}"))?;
        let mut value = VARIANT::default();
        let mut flavor = 0;
        let qualifier_name = property_name(qualifier);
        unsafe { qualifiers.Get(PCWSTR(qualifier_name.as_ptr()), 0, &mut value, &mut flavor) }
            .map_err(|error| format!("read WMI qualifier {qualifier}: {error}"))?;
        let vt = unsafe { value.Anonymous.Anonymous.vt.0 };
        if vt != VT_ARRAY.0 | VT_BSTR.0 {
            return Err(format!("WMI qualifier {qualifier} is not SAFEARRAY(BSTR)"));
        }
        let array = unsafe { value.Anonymous.Anonymous.Anonymous.parray };
        if array.is_null() || unsafe { (*array).cDims } != 1 {
            return Err(format!(
                "WMI qualifier {qualifier} has an invalid SAFEARRAY shape"
            ));
        }
        if unsafe { SafeArrayGetVartype(array) }
            .map_err(|error| format!("inspect WMI qualifier {qualifier}: {error}"))?
            != VT_BSTR
        {
            return Err(format!("WMI qualifier {qualifier} SAFEARRAY is not BSTR"));
        }
        let lower = unsafe { SafeArrayGetLBound(array, 1) }
            .map_err(|error| format!("read WMI qualifier {qualifier} lower bound: {error}"))?;
        let upper = unsafe { SafeArrayGetUBound(array, 1) }
            .map_err(|error| format!("read WMI qualifier {qualifier} upper bound: {error}"))?;
        if upper < lower || upper - lower >= 16 {
            return Err(format!(
                "WMI qualifier {qualifier} has an invalid item count"
            ));
        }
        (lower..=upper)
            .map(|index| {
                let mut item = BSTR::new();
                unsafe { SafeArrayGetElement(array, &index, (&mut item as *mut BSTR).cast()) }
                    .map_err(|error| format!("read WMI qualifier {qualifier} item: {error}"))?;
                Ok(item.to_string())
            })
            .collect()
    }

    pub(crate) fn uint32(object: &IWbemClassObject, name: &str) -> Result<u32, String> {
        // WMI represents CIM_UINT32 as the signed Automation VT_I4 payload.
        let variant = value(object, name, CIM_UINT32.0, VT_I4.0)?;
        u32::try_from(unsafe { variant.Anonymous.Anonymous.Anonymous.lVal })
            .map_err(|_| format!("WMI property {name} contains a negative uint32 representation"))
    }

    pub(crate) fn uint16(object: &IWbemClassObject, name: &str) -> Result<u16, String> {
        let variant = value(object, name, CIM_UINT16.0, VT_I2.0)?;
        u16::try_from(unsafe { variant.Anonymous.Anonymous.Anonymous.iVal })
            .map_err(|_| format!("WMI property {name} contains an invalid uint16 representation"))
    }

    pub(crate) fn uint8(object: &IWbemClassObject, name: &str) -> Result<u8, String> {
        let variant = value(object, name, CIM_UINT8.0, VT_UI1.0)?;
        Ok(unsafe { variant.Anonymous.Anonymous.Anonymous.bVal })
    }

    pub(crate) fn sint8(object: &IWbemClassObject, name: &str) -> Result<i8, String> {
        let variant = value(object, name, CIM_SINT8.0, VT_I1.0)?;
        Ok(unsafe { variant.Anonymous.Anonymous.Anonymous.cVal })
    }

    pub(crate) fn boolean(object: &IWbemClassObject, name: &str) -> Result<bool, String> {
        let variant = value(object, name, CIM_BOOLEAN.0, VT_BOOL.0)?;
        Ok(unsafe { variant.Anonymous.Anonymous.Anonymous.boolVal == VARIANT_BOOL(-1) })
    }

    pub(super) fn uint64(object: &IWbemClassObject, name: &str) -> Result<u64, String> {
        // WMI represents CIM_UINT64 as a decimal string in a VT_BSTR payload.
        bstr_value(object, name, CIM_UINT64.0)?
            .parse()
            .map_err(|_| format!("WMI property {name} is not a decimal uint64 string"))
    }

    pub(crate) fn put_string(
        object: &IWbemClassObject,
        name: &str,
        value: &str,
    ) -> Result<(), String> {
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_BSTR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        bstrVal: ManuallyDrop::new(BSTR::from(value)),
                    },
                }),
            },
        };
        put(object, name, &variant, CIM_STRING.0)
    }

    pub(crate) fn put_uint32(
        object: &IWbemClassObject,
        name: &str,
        value: u32,
    ) -> Result<(), String> {
        // CIM_UINT32 still requires the signed VT_I4 representation used by WMI.
        let value = i32::try_from(value)
            .map_err(|_| format!("WMI property {name} exceeds the signed Automation range"))?;
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I4,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { lVal: value },
                }),
            },
        };
        put(object, name, &variant, CIM_UINT32.0)
    }

    pub(crate) fn put_uint16(
        object: &IWbemClassObject,
        name: &str,
        value: u16,
    ) -> Result<(), String> {
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I2,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { iVal: value as i16 },
                }),
            },
        };
        put(object, name, &variant, CIM_UINT16.0)
    }

    pub(crate) fn put_uint8(
        object: &IWbemClassObject,
        name: &str,
        value: u8,
    ) -> Result<(), String> {
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_UI1,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { bVal: value },
                }),
            },
        };
        put(object, name, &variant, CIM_UINT8.0)
    }

    pub(crate) fn put_uint64(
        object: &IWbemClassObject,
        name: &str,
        value: u64,
    ) -> Result<(), String> {
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_BSTR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        bstrVal: ManuallyDrop::new(BSTR::from(value.to_string())),
                    },
                }),
            },
        };
        put(object, name, &variant, CIM_UINT64.0)
    }

    pub(crate) fn put_sint8(
        object: &IWbemClassObject,
        name: &str,
        value: i8,
    ) -> Result<(), String> {
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I1,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { cVal: value },
                }),
            },
        };
        put(object, name, &variant, CIM_SINT8.0)
    }

    pub(crate) fn put_bool(
        object: &IWbemClassObject,
        name: &str,
        value: bool,
    ) -> Result<(), String> {
        let variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_BOOL,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        boolVal: VARIANT_BOOL(if value { -1 } else { 0 }),
                    },
                }),
            },
        };
        put(object, name, &variant, CIM_BOOLEAN.0)
    }

    fn put(
        object: &IWbemClassObject,
        name: &str,
        variant: &VARIANT,
        cim_type: i32,
    ) -> Result<(), String> {
        let wide_name = property_name(name);
        unsafe { object.Put(PCWSTR(wide_name.as_ptr()), 0, variant, cim_type) }
            .map_err(|error| format!("write WMI property {name}: {error}"))
    }

    pub(crate) fn require_class(
        object: &IWbemClassObject,
        expected: &str,
        scope: &str,
    ) -> Result<(), String> {
        if string(object, "__CLASS")? != expected {
            return Err(format!("{scope} returned the wrong class"));
        }
        Ok(())
    }

    pub(crate) fn object(services: &IWbemServices, path: &str) -> Result<IWbemClassObject, String> {
        let mut object = None;
        unsafe {
            services.GetObject(
                &BSTR::from(path),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                Some(&mut object),
                None,
            )
        }
        .map_err(|error| format!("resolve exact WMI object: {error}"))?;
        object.ok_or("WMI object lookup returned no object".into())
    }

    pub(super) fn device_guard_status() -> Result<u32, String> {
        on_mta(|| {
            let services = services_at(DEVICE_GUARD_NAMESPACE)?;
            let devices = query(&services, DEVICE_GUARD_QUERY)?;
            if devices.len() != 1 {
                return Err("Win32_DeviceGuard query did not return one exact instance".into());
            }
            let device = &devices[0];
            require_class(device, DEVICE_GUARD_CLASS, "DeviceGuard detection")?;
            uint32(device, "VirtualizationBasedSecurityStatus")
        })
    }
}
