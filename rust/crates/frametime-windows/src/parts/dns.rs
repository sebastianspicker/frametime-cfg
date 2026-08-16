use std::net::Ipv4Addr;

use frametime_core::DnsProvider;

const CLOUDFLARE: [&str; 2] = ["1.1.1.1", "1.0.0.1"];
const GOOGLE: [&str; 2] = ["8.8.8.8", "8.8.4.4"];

/// A durable IP Helper identity, deliberately independent of a friendly name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DnsBinding {
    adapter_guid: String,
    interface_guid: String,
    interface_luid: u64,
    interface_index: u32,
    adapter_name: String,
    physical_address: Vec<u8>,
}

/// Narrow seam for host tests. Production uses only IP Helper below.
trait DnsAdapter {
    fn discover_active_physical(&self) -> Result<Vec<DnsBinding>, String>;
    fn read_ipv4_servers(&self, binding: &DnsBinding) -> Result<Vec<String>, String>;
    fn write_ipv4_servers(&self, binding: &DnsBinding, servers: &[String]) -> Result<(), String>;
}

fn provider_servers(config: &Config) -> Result<Option<Vec<String>>, String> {
    let (provider, servers, exact) = match config.dns.provider {
        DnsProvider::Skip => return Ok(None),
        DnsProvider::Cloudflare => ("Cloudflare", &config.dns.cloudflare, &CLOUDFLARE),
        DnsProvider::Google => ("Google", &config.dns.google, &GOOGLE),
    };
    if servers.len() != exact.len()
        || !servers
            .iter()
            .map(String::as_str)
            .zip(exact)
            .all(|(actual, expected)| actual == *expected)
    {
        return Err(format!(
            "DNS {provider} profile does not equal its compiled exact addresses"
        ));
    }
    Ok(Some(servers.clone()))
}

fn exact_binding(captured: &DnsBinding, observed: &[DnsBinding]) -> Result<DnsBinding, String> {
    let matches = observed
        .iter()
        .filter(|candidate| {
            candidate
                .adapter_guid
                .eq_ignore_ascii_case(&captured.adapter_guid)
                && candidate
                    .interface_guid
                    .eq_ignore_ascii_case(&captured.interface_guid)
                && candidate.interface_luid == captured.interface_luid
                && candidate.interface_index == captured.interface_index
                && candidate.physical_address == captured.physical_address
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [binding] => Ok(binding.clone()),
        [] => Err("captured DNS adapter no longer has its exact durable identity".into()),
        _ => Err("DNS adapter identity reobservation is ambiguous".into()),
    }
}

fn capture_dns<A: DnsAdapter>(
    adapter: &A,
    step: String,
) -> Result<(Vec<DnsBinding>, Vec<BackupEntry>), String> {
    let bindings = adapter.discover_active_physical()?;
    if bindings.is_empty() {
        return Err("no active physical Ethernet or Wi-Fi adapter can be proven".into());
    }
    let mut entries = Vec::with_capacity(bindings.len());
    for binding in &bindings {
        let original_dns_servers = adapter.read_ipv4_servers(binding)?;
        if original_dns_servers
            .iter()
            .any(|server| server.parse::<Ipv4Addr>().is_err())
        {
            return Err("DNS IP Helper readback contained a non-IPv4 server".into());
        }
        entries.push(BackupEntry::Dns {
            step: step.clone(),
            timestamp: timestamp(),
            adapter_name: binding.adapter_name.clone(),
            interface_index: binding.interface_index,
            adapter_guid: Some(binding.adapter_guid.clone()),
            interface_guid: Some(binding.interface_guid.clone()),
            interface_luid: Some(binding.interface_luid),
            physical_address: binding.physical_address.clone(),
            original_dns_servers,
            unknown: Default::default(),
        });
    }
    Ok((bindings, entries))
}

fn apply_dns<A: DnsAdapter>(
    adapter: &A,
    bindings: &[DnsBinding],
    config: &Config,
) -> Result<(), String> {
    let Some(servers) = provider_servers(config)? else {
        return Ok(());
    };
    let observed = adapter.discover_active_physical()?;
    for binding in bindings {
        adapter.write_ipv4_servers(&exact_binding(binding, &observed)?, &servers)?;
    }
    Ok(())
}

fn verify_dns<A: DnsAdapter>(
    adapter: &A,
    bindings: &[DnsBinding],
    config: &Config,
) -> Result<(), String> {
    let Some(expected) = provider_servers(config)? else {
        return Ok(());
    };
    let observed = adapter.discover_active_physical()?;
    for binding in bindings {
        let current = adapter.read_ipv4_servers(&exact_binding(binding, &observed)?)?;
        if current != expected {
            return Err("DNS ordered IPv4 readback did not equal the selected provider".into());
        }
    }
    Ok(())
}

fn restore_dns_entry<A: DnsAdapter>(adapter: &A, entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::Dns {
        step,
        adapter_name,
        interface_index,
        adapter_guid,
        interface_guid,
        interface_luid,
        physical_address,
        original_dns_servers,
        unknown,
        ..
    } = entry
    else {
        return Err("DNS restore received a non-DNS backup".into());
    };
    if step != "P3:9"
        || !unknown.is_empty()
        || original_dns_servers
            .iter()
            .any(|server| server.parse::<Ipv4Addr>().is_err())
    {
        return Err("DNS backup is not an exact P3:9 IPv4 record".into());
    }
    let binding = DnsBinding {
        adapter_guid: adapter_guid
            .clone()
            .ok_or("DNS backup has no adapter GUID")?,
        interface_guid: interface_guid
            .clone()
            .ok_or("DNS backup has no interface GUID")?,
        interface_luid: interface_luid.ok_or("DNS backup has no interface LUID")?,
        interface_index: *interface_index,
        adapter_name: adapter_name.clone(),
        physical_address: physical_address.clone(),
    };
    let observed = exact_binding(&binding, &adapter.discover_active_physical()?)?;
    adapter.write_ipv4_servers(&observed, original_dns_servers)?;
    if adapter.read_ipv4_servers(&observed)? == *original_dns_servers {
        Ok(())
    } else {
        Err("DNS restore ordered readback did not match backup".into())
    }
}

fn inspect_dns(config: Option<&Config>) -> Result<frametime_core::Inspection, String> {
    let config = config.ok_or("P3:9 requires a validated frametime.toml DNS provider selection")?;
    if provider_servers(config)?.is_none() {
        Ok(frametime_core::Inspection::Satisfied)
    } else {
        Ok(frametime_core::Inspection::NeedsApply)
    }
}

#[cfg(windows)]
fn capture_native_dns(
    step: String,
    _: &Config,
) -> Result<(Vec<DnsBinding>, Vec<BackupEntry>), String> {
    capture_dns(&NativeDnsAdapter, step)
}
#[cfg(not(windows))]
fn capture_native_dns(
    step: String,
    _: &Config,
) -> Result<(Vec<DnsBinding>, Vec<BackupEntry>), String> {
    capture_dns(&UnavailableDnsAdapter, step)
}
#[cfg(windows)]
fn apply_native_dns(bindings: &[DnsBinding], config: &Config) -> Result<(), String> {
    apply_dns(&NativeDnsAdapter, bindings, config)
}
#[cfg(not(windows))]
fn apply_native_dns(bindings: &[DnsBinding], config: &Config) -> Result<(), String> {
    apply_dns(&UnavailableDnsAdapter, bindings, config)
}
#[cfg(windows)]
fn verify_native_dns(bindings: &[DnsBinding], config: &Config) -> Result<(), String> {
    verify_dns(&NativeDnsAdapter, bindings, config)
}
#[cfg(not(windows))]
fn verify_native_dns(bindings: &[DnsBinding], config: &Config) -> Result<(), String> {
    verify_dns(&UnavailableDnsAdapter, bindings, config)
}
#[cfg(windows)]
fn restore_native_dns(entry: &BackupEntry) -> Result<(), String> {
    restore_dns_entry(&NativeDnsAdapter, entry)
}
#[cfg(not(windows))]
fn restore_native_dns(entry: &BackupEntry) -> Result<(), String> {
    restore_dns_entry(&UnavailableDnsAdapter, entry)
}

#[cfg(not(windows))]
struct UnavailableDnsAdapter;

#[cfg(not(windows))]
impl DnsAdapter for UnavailableDnsAdapter {
    fn discover_active_physical(&self) -> Result<Vec<DnsBinding>, String> {
        Err("DNS adapter discovery requires Windows IP Helper".into())
    }
    fn read_ipv4_servers(&self, _: &DnsBinding) -> Result<Vec<String>, String> {
        Err("DNS read requires Windows IP Helper".into())
    }
    fn write_ipv4_servers(&self, _: &DnsBinding, _: &[String]) -> Result<(), String> {
        Err("DNS write requires Windows IP Helper".into())
    }
}

#[cfg(windows)]
struct NativeDnsAdapter;

#[cfg(windows)]
impl DnsAdapter for NativeDnsAdapter {
    fn discover_active_physical(&self) -> Result<Vec<DnsBinding>, String> {
        native_dns_api::discover()
    }
    fn read_ipv4_servers(&self, binding: &DnsBinding) -> Result<Vec<String>, String> {
        native_dns_api::read(binding)
    }
    fn write_ipv4_servers(&self, binding: &DnsBinding, servers: &[String]) -> Result<(), String> {
        native_dns_api::write(binding, servers)
    }
}

#[cfg(windows)]
mod native_dns_api {
    use super::DnsBinding;
    use std::{collections::HashSet, ffi::CStr, mem};
    use windows::{
        Win32::{
            Foundation::ERROR_BUFFER_OVERFLOW,
            NetworkManagement::{
                IpHelper::{
                    DNS_INTERFACE_SETTINGS, DNS_INTERFACE_SETTINGS_VERSION1,
                    DNS_SETTING_NAMESERVER, FreeInterfaceDnsSettings,
                    GAA_FLAG_INCLUDE_ALL_INTERFACES, GetAdaptersAddresses, GetInterfaceDnsSettings,
                    IP_ADAPTER_ADDRESSES_LH, SetInterfaceDnsSettings,
                },
                Ndis::IfOperStatusUp,
            },
            Networking::WinSock::AF_UNSPEC,
        },
        core::GUID,
    };

    const MAX_ADAPTERS: usize = 4_096;

    fn checked_buffer_offset<T>(buffer: &[u8], pointer: *const T) -> Result<usize, String> {
        let base = buffer.as_ptr() as usize;
        let end = base
            .checked_add(buffer.len())
            .ok_or("adapter buffer extent overflow")?;
        let address = pointer as usize;
        let record_end = address
            .checked_add(mem::size_of::<T>())
            .ok_or("adapter record extent overflow")?;
        if address < base || record_end > end || !address.is_multiple_of(mem::align_of::<T>()) {
            return Err("adapter pointer is outside or misaligned in its returned buffer".into());
        }
        Ok(address - base)
    }

    fn bounded_c_string(buffer: &[u8], pointer: *const u8) -> Result<String, String> {
        let offset = checked_buffer_offset(buffer, pointer)?;
        CStr::from_bytes_until_nul(&buffer[offset..])
            .map_err(|_| "adapter GUID is not terminated inside its returned buffer")?
            .to_str()
            .map(str::to_owned)
            .map_err(|_| "adapter GUID is not ASCII".into())
    }

    fn bounded_wide(buffer: &[u8], pointer: *const u16) -> Result<String, String> {
        if pointer.is_null() {
            return Ok(String::new());
        }
        let offset = checked_buffer_offset(buffer, pointer)?;
        let remaining = &buffer[offset..];
        let units = remaining.len() / mem::size_of::<u16>();
        let values = unsafe { std::slice::from_raw_parts(pointer, units) };
        let length = values
            .iter()
            .position(|unit| *unit == 0)
            .ok_or("adapter UTF-16 string is not terminated inside its returned buffer")?;
        String::from_utf16(&values[..length]).map_err(|_| "adapter UTF-16 string is invalid".into())
    }

    pub(super) fn discover() -> Result<Vec<DnsBinding>, String> {
        let mut bytes = 15_000u32;
        loop {
            let mut buffer = vec![0u8; bytes as usize];
            let code = unsafe {
                GetAdaptersAddresses(
                    AF_UNSPEC.0 as u32,
                    GAA_FLAG_INCLUDE_ALL_INTERFACES,
                    None,
                    Some(buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                    &mut bytes,
                )
            };
            if code == ERROR_BUFFER_OVERFLOW.0 {
                continue;
            }
            if code != 0 {
                return Err(format!("GetAdaptersAddresses failed: {code}"));
            }
            let mut rows = Vec::new();
            let mut row = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            let mut visited = HashSet::new();
            while !row.is_null() {
                if visited.len() >= MAX_ADAPTERS || !visited.insert(row as usize) {
                    return Err("adapter list is cyclic or exceeds its bounded count".into());
                }
                let offset = checked_buffer_offset(&buffer, row)?;
                let value = unsafe { &*row };
                let record_length = unsafe { value.Anonymous1.Anonymous.Length as usize };
                if record_length < mem::size_of::<IP_ADAPTER_ADDRESSES_LH>()
                    || offset
                        .checked_add(record_length)
                        .is_none_or(|end| end > buffer.len())
                {
                    return Err("adapter record length exceeds its returned buffer".into());
                }
                let description = bounded_wide(&buffer, value.Description.0.cast_const())?;
                let friendly = bounded_wide(&buffer, value.FriendlyName.0.cast_const())?;
                let physical_len = usize::try_from(value.PhysicalAddressLength)
                    .map_err(|_| "adapter MAC length overflow")?;
                if physical_len > value.PhysicalAddress.len() {
                    return Err("adapter MAC length exceeds its fixed record field".into());
                }
                let physical = value.PhysicalAddress[..physical_len].to_vec();
                let software = [
                    "virtual",
                    "tunnel",
                    "loopback",
                    "tap",
                    "vpn",
                    "wireguard",
                    "tailscale",
                    "hyper-v",
                    "vethernet",
                ]
                .iter()
                .any(|word| {
                    description.to_ascii_lowercase().contains(word)
                        || friendly.to_ascii_lowercase().contains(word)
                });
                if (value.IfType == 6 || value.IfType == 71)
                    && value.OperStatus == IfOperStatusUp
                    && !software
                    && physical.len() >= 6
                    && physical.iter().any(|byte| *byte != 0)
                {
                    let guid = bounded_c_string(&buffer, value.AdapterName.0.cast())?;
                    let guid = format!("{{{guid}}}");
                    rows.push(DnsBinding {
                        adapter_guid: guid.clone(),
                        interface_guid: guid,
                        interface_luid: unsafe { value.Luid.Value },
                        interface_index: unsafe { value.Anonymous1.Anonymous.IfIndex },
                        adapter_name: friendly,
                        physical_address: physical,
                    });
                }
                row = value.Next;
            }
            rows.sort_by(|left, right| left.adapter_guid.cmp(&right.adapter_guid));
            if rows.windows(2).any(|pair| {
                pair[0]
                    .adapter_guid
                    .eq_ignore_ascii_case(&pair[1].adapter_guid)
            }) {
                return Err("active physical DNS adapters have duplicate durable GUIDs".into());
            }
            return Ok(rows);
        }
    }
    pub(super) fn read(binding: &DnsBinding) -> Result<Vec<String>, String> {
        let mut settings = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            ..Default::default()
        };
        let code =
            unsafe { GetInterfaceDnsSettings(guid(&binding.interface_guid)?, &mut settings) }.0;
        if code != 0 {
            return Err(format!("GetInterfaceDnsSettings failed: {code}"));
        }
        let names = if settings.NameServer.is_null() {
            Vec::new()
        } else {
            wide(settings.NameServer)
                .split(',')
                .flat_map(str::split_whitespace)
                .map(str::to_owned)
                .collect()
        };
        unsafe { FreeInterfaceDnsSettings(&mut settings) };
        Ok(names)
    }
    pub(super) fn write(binding: &DnsBinding, servers: &[String]) -> Result<(), String> {
        let units = (!servers.is_empty()).then(|| {
            servers
                .join(",")
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>()
        });
        let name_server = units
            .as_ref()
            .map_or(windows::core::PWSTR::null(), |value| {
                windows::core::PWSTR(value.as_ptr().cast_mut())
            });
        let settings = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            Flags: DNS_SETTING_NAMESERVER as u64,
            NameServer: name_server,
            ..Default::default()
        };
        let code = unsafe { SetInterfaceDnsSettings(guid(&binding.interface_guid)?, &settings) }.0;
        if code == 0 {
            Ok(())
        } else {
            Err(format!("SetInterfaceDnsSettings failed: {code}"))
        }
    }
    fn wide(value: windows::core::PWSTR) -> String {
        if value.is_null() {
            String::new()
        } else {
            unsafe { value.to_string().unwrap_or_default() }
        }
    }
    fn guid(value: &str) -> Result<GUID, String> {
        let compact = value.trim_matches(['{', '}']).replace('-', "");
        if compact.len() != 32 || !compact.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("DNS interface GUID is invalid".into());
        }
        u128::from_str_radix(&compact, 16)
            .map(GUID::from_u128)
            .map_err(|_| "DNS interface GUID is invalid".into())
    }
}

#[cfg(test)]
mod dns_tests {
    use super::*;
    #[derive(Default)]
    struct Mock {
        bindings: Vec<DnsBinding>,
        servers: std::cell::RefCell<Vec<Vec<String>>>,
    }
    impl DnsAdapter for Mock {
        fn discover_active_physical(&self) -> Result<Vec<DnsBinding>, String> {
            Ok(self.bindings.clone())
        }
        fn read_ipv4_servers(&self, binding: &DnsBinding) -> Result<Vec<String>, String> {
            Ok(self.servers.borrow()[usize::try_from(binding.interface_index).unwrap()].clone())
        }
        fn write_ipv4_servers(
            &self,
            binding: &DnsBinding,
            servers: &[String],
        ) -> Result<(), String> {
            self.servers.borrow_mut()[usize::try_from(binding.interface_index).unwrap()] =
                servers.to_vec();
            Ok(())
        }
    }
    fn binding() -> DnsBinding {
        DnsBinding {
            adapter_guid: "{11111111-1111-1111-1111-111111111111}".into(),
            interface_guid: "{11111111-1111-1111-1111-111111111111}".into(),
            interface_luid: 1,
            interface_index: 0,
            adapter_name: "Ethernet".into(),
            physical_address: vec![1, 2, 3, 4, 5, 6],
        }
    }
    #[test]
    fn exact_identity_rejects_index_drift() {
        let saved = binding();
        let mut changed = binding();
        changed.interface_index = 1;
        assert!(exact_binding(&saved, &[changed]).is_err());
    }
    #[test]
    fn capture_apply_and_verify_preserve_order_through_the_adapter_seam() {
        let adapter = Mock {
            bindings: vec![binding()],
            servers: std::cell::RefCell::new(vec![vec!["192.0.2.1".into()]]),
        };
        let (bindings, backups) = capture_dns(&adapter, "P3:9".into()).unwrap();
        let mut config = Config::load(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frametime.toml"),
        )
        .unwrap();
        config.dns.provider = DnsProvider::Cloudflare;
        apply_dns(&adapter, &bindings, &config).unwrap();
        verify_dns(&adapter, &bindings, &config).unwrap();
        assert!(
            matches!(backups.as_slice(), [BackupEntry::Dns { original_dns_servers, .. }] if original_dns_servers == &vec![String::from("192.0.2.1")])
        );
    }
}
