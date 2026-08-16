use crate::cli::CleanArgs;
use crate::print_banner;
use driver_foundry_clean::{
    preflight, resolve_settings_root, run_clean, CleanOptions, CleanVendor, RemoveScopes,
};
use driver_foundry_common::version_line;
use std::process::ExitCode;

pub(crate) fn run(args: CleanArgs) -> ExitCode {
    print_banner();
    println!("domain: clean (native Rust / vendor catalogs)");
    println!("{}", version_line());
    let settings_root = resolve_settings_root(args.settings.as_deref());

    if args.preflight && args.vendor.is_none() && !args.prepare_safeboot && !args.clear_safeboot {
        let (ok, messages) = preflight(&settings_root);
        for message in messages {
            println!("preflight: {message}");
        }
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(3)
        };
    }

    if args.vendor.is_none() && (args.prepare_safeboot || args.clear_safeboot) {
        let options = CleanOptions {
            vendor: CleanVendor::Nvidia,
            dry_run: !args.execute,
            settings_root,
            prepare_safeboot: args.prepare_safeboot,
            clear_safeboot: args.clear_safeboot,
            safeboot_network: args.safeboot_network,
            attempt_elevation: args.execute,
            elevation_args: std::env::args().skip(1).collect(),
            host_probe: !args.no_host_probe,
            ..CleanOptions::default()
        };
        return emit(run_clean(&options));
    }

    let Some(vendor_name) = args.vendor.as_deref() else {
        eprintln!("error: clean requires --vendor <nvidia|amd|intel|lisuan|realtek> (or --preflight / safeboot flags)");
        return ExitCode::from(2);
    };
    let Some(vendor) = CleanVendor::parse(vendor_name) else {
        eprintln!("error: unknown vendor '{vendor_name}'. Use nvidia|amd|intel|lisuan|realtek");
        return ExitCode::from(2);
    };

    let scopes = if args.clean_complete {
        RemoveScopes::clean_complete()
    } else {
        RemoveScopes {
            remove_gfe: args.remove_gfe,
            remove_nv_broadcast: args.remove_nvbroadcast,
            remove_amd_kmpfd: args.remove_amdkmpfd,
            remove_intel_igs: args.remove_intel_igs,
            remove_intel_npu: args.remove_intel_npu,
            remove_oneapi: args.remove_oneapi,
            remove_endurance: args.remove_endurance_gaming,
            remove_vulkan: args.remove_vulkan,
            remove_physx: args.remove_physx,
            remove_audiobus: args.remove_audiobus,
            remove_monitors: args.remove_monitors,
            remove_unpack_nvidia: args.remove_unpack_nvidia,
            remove_unpack_amd: args.remove_unpack_amd,
            remove_install_cache: args.remove_install_cache,
        }
    };
    let options = CleanOptions {
        vendor,
        dry_run: !args.execute && args.dry_run,
        settings_root,
        scopes,
        cache_only: args.cache_only,
        block_driver_search: args.block_driver_search,
        no_restore_point: args.no_restore_point,
        no_setup_api: args.no_setup_api,
        restart: args.restart,
        shutdown: args.shutdown,
        plan_report_path: args.plan_report,
        prepare_safeboot: args.prepare_safeboot,
        clear_safeboot: args.clear_safeboot,
        safeboot_network: args.safeboot_network,
        host_probe: !args.no_host_probe,
        attempt_elevation: !driver_foundry_common::elevation::uac_relaunch_disabled(),
        elevation_args: std::env::args().skip(1).collect(),
    };
    emit(run_clean(&options))
}

fn emit(
    result: Result<driver_foundry_clean::CleanResult, driver_foundry_clean::CleanError>,
) -> ExitCode {
    match result {
        Ok(result) => {
            for message in &result.messages {
                println!("{message}");
            }
            if result.elevation_relaunched {
                println!("exit: 0 elevation_relaunched=true");
                return ExitCode::SUCCESS;
            }
            println!(
                "exit: {} dryRun={} planned={} executed={}",
                result.exit_code, result.dry_run, result.planned, result.executed
            );
            ExitCode::from(result.exit_code as u8)
        }
        Err(driver_foundry_clean::CleanError::ElevationRequired(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(5)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}
