use crate::adapters::OsEnvironment;
use crate::support::{vendor_install_caches, vendor_shader_caches};
use crate::{catalog::load_lines, CleanError, CleanOptions, CleanVendor};
use driver_foundry_common::ActionJournal;
use std::path::Path;

pub(super) fn run_cache_only(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
) -> Result<(), CleanError> {
    let vendor = opts.vendor;
    let folder = vendor.folder();
    let dry = !live;

    stages.push("0_resolve_vendor".into());
    let _services = load_lines(&opts.settings_root, folder, "services.cfg")?;
    messages.push(format!(
        "[Stage] 0_resolve_vendor ({folder}) cache-only dryRun={dry}"
    ));

    stages.push("0b_cache_only".into());
    messages.push(format!("[Stage] 0b_cache_only ({folder}) dryRun={dry}"));
    for cache in vendor_shader_caches(vendor) {
        let path = driver_foundry_common::expand_path_tokens(cache);
        env.wipe_path(&path, journal);
    }
    if opts.scopes.remove_install_cache || opts.scopes.remove_gfe {
        for cache in vendor_install_caches(vendor) {
            let path = driver_foundry_common::expand_path_tokens(cache);
            env.wipe_path(&path, journal);
        }
    }
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_unpack_nvidia {
        env.wipe_path(Path::new(r"C:\NVIDIA"), journal);
    }
    if vendor == CleanVendor::Amd && opts.scopes.remove_unpack_amd {
        env.wipe_path(Path::new(r"C:\AMD"), journal);
    }
    messages.push(format!("[CacheOnly] planned cache wipes dryRun={dry}"));
    Ok(())
}
