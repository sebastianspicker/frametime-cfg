use crate::cli::InstallArgs;
use crate::print_banner;
use driver_foundry_common::version_line;
use driver_foundry_install::{run_install, InstallOptions};
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn list_packages(catalog: Option<PathBuf>) -> ExitCode {
    print_banner();
    let path = catalog.unwrap_or_else(|| {
        driver_foundry_common::catalog_path(&driver_foundry_common::resolve_data_root())
    });
    match driver_foundry_install::catalog::PackageCatalog::load_from_file(&path) {
        Ok(catalog) => {
            println!(
                "catalog: {} ({} packages)",
                path.display(),
                catalog.packages.len()
            );
            for (id, definition) in &catalog.packages {
                let kind = if definition.required {
                    "required"
                } else {
                    "optional"
                };
                println!("  {id} [{kind}] {}", definition.title);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

pub(crate) fn list_languages() -> ExitCode {
    print_banner();
    let settings = driver_foundry_clean::resolve_settings_root(None);
    let packs = driver_foundry_common::i18n::list_language_packs(&settings);
    println!("languages_dir: {}", settings.join("Languages").display());
    println!("count: {}", packs.len());
    for pack in packs {
        println!("  {pack}");
    }
    ExitCode::SUCCESS
}

pub(crate) fn run(args: InstallArgs) -> ExitCode {
    print_banner();
    println!("domain: install (native Rust package pipeline)");
    println!("{}", version_line());

    let work_directory = args.work.unwrap_or_else(default_work_directory);
    if args.materialize_embedded {
        eprintln!(
            "error: --materialize-embedded is disabled: embedded helpers have no authenticated release manifest"
        );
        return ExitCode::from(1);
    }

    let options = InstallOptions {
        work_directory,
        preset: args.preset,
        package_root: args.package_root,
        package_archive: args.package_archive,
        package_url: args.package_url,
        package_sha256: args.package_sha256,
        driver_index: args.driver_index,
        driver_index_id: args.driver_index_id,
        catalog_path: args.catalog,
        enable_install: args.install || args.force_install,
        dry_run_install: !args.force_install,
        enable_run_report: !args.no_report,
        run_report_path: args.report,
        select: args.select,
        deselect: args.deselect,
        import_selection: args.import_selection,
        export_path: args.export,
        archive_out: args.archive,
        archive_format: args.archive_format,
        uninstall_drivers: args.uninstall_drivers,
        deep_inf: args.deep_inf,
        disable_telemetry: args.disable_telemetry,
        disable_installer_telemetry: args.disable_installer_telemetry,
        disable_nvcontainer: args.disable_nvcontainer,
        disable_nvcamera: args.disable_nvcamera,
        disable_hdcp: args.disable_hdcp,
        disable_mpo: args.disable_mpo,
        disable_hdaudio_sleep: args.disable_hdaudio_sleep,
        enable_msi: args.enable_msi,
        clean_install: args.clean_install,
        unattended: args.unattended,
        live_registry_apply: args.live_registry_apply,
        try_sign: args.try_sign,
        setup_args: args.setup_args,
    };
    emit(run_install(&options))
}

fn default_work_directory() -> PathBuf {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("dfoundry-{millis}"))
}

fn emit(
    result: Result<driver_foundry_install::InstallResult, driver_foundry_install::InstallError>,
) -> ExitCode {
    match result {
        Ok(result) => {
            for message in &result.messages {
                println!("{message}");
            }
            println!(
                "exit: {} dryRunInstall={} kept={} stripped={} synthetic={}",
                result.exit_code,
                result.dry_run_install,
                result.kept_components.len(),
                result.stripped_components.len(),
                result.used_synthetic_fixture
            );
            ExitCode::from(result.exit_code as u8)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}
