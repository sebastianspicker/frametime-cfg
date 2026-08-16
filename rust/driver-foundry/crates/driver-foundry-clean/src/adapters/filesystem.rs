use super::OsEnvironment;
use driver_foundry_common::ActionJournal;
use std::fs;
use std::path::{Path, PathBuf};

pub fn file_match_candidates(token: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let raw = token.trim();
    if raw.is_empty() {
        return v;
    }
    // Normalize: strip leading separators used in catalog (\opencl64.dll)
    let rel = raw.trim_start_matches(['\\', '/']);
    if rel.is_empty() {
        return v;
    }
    // Refuse path tokens that are just a drive root
    if is_unsafe_path_token(rel) {
        return v;
    }

    let expanded = driver_foundry_common::expand_path_tokens(raw);
    if is_safe_candidate(&expanded) {
        v.push(expanded);
    }
    let expanded_rel = driver_foundry_common::expand_path_tokens(rel);
    if is_safe_candidate(&expanded_rel) && !v.contains(&expanded_rel) {
        v.push(expanded_rel);
    }

    let sys_root = std::env::var("SystemRoot")
        .or_else(|_| std::env::var("WINDIR"))
        .unwrap_or_else(|_| r"C:\Windows".into());
    let program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    let program_files_x86 =
        std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());

    let system32 = PathBuf::from(&sys_root).join("System32");
    let syswow64 = PathBuf::from(&sys_root).join("SysWOW64");
    let drivers = system32.join("drivers");
    let driverstore_repo = system32.join("DriverStore").join("FileRepository");

    let roots: Vec<PathBuf> = vec![
        system32.clone(),
        syswow64.clone(),
        drivers.clone(),
        PathBuf::from(&sys_root),
        PathBuf::from(&program_files),
        PathBuf::from(&program_files_x86),
        PathBuf::from(&program_data),
    ];

    for root in &roots {
        let p = root.join(rel);
        if is_safe_candidate(&p) && !v.contains(&p) {
            v.push(p);
        }
        // Basename-only under root (token may be nested path)
        if let Some(name) = Path::new(rel).file_name() {
            let p2 = root.join(name);
            if is_safe_candidate(&p2) && !v.contains(&p2) {
                v.push(p2);
            }
        }
    }

    // Extension variants under System32 / drivers
    let base_name = Path::new(rel)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string());
    let lower = base_name.to_ascii_lowercase();
    if !lower.ends_with(".sys") && !lower.ends_with(".dll") && !lower.ends_with(".exe") {
        for (dir, ext) in [
            (drivers.as_path(), "sys"),
            (system32.as_path(), "dll"),
            (syswow64.as_path(), "dll"),
            (system32.as_path(), "exe"),
        ] {
            let p = dir.join(format!("{base_name}.{ext}"));
            if is_safe_candidate(&p) && !v.contains(&p) {
                v.push(p);
            }
        }
    }

    // DriverStore FileRepository: practical wildcards without per-token full readdir.
    // Direct join for basename; optional single-level prefix match only for short bare names
    // (package-like tokens without path separators / common extensions).
    let bare = !rel.contains('\\') && !rel.contains('/');
    let looks_like_package = bare
        && !lower.ends_with(".sys")
        && !lower.ends_with(".dll")
        && !lower.ends_with(".exe")
        && !lower.ends_with(".inf")
        && base_name.len() >= 4;
    let direct = driverstore_repo.join(&base_name);
    if is_safe_candidate(&direct) && !v.contains(&direct) {
        v.push(direct);
    }
    // One shallow pass only for package-like bare tokens (not every .dll catalog entry)
    if looks_like_package && driverstore_repo.is_dir() {
        let needle = base_name.to_ascii_lowercase();
        if let Ok(rd) = fs::read_dir(&driverstore_repo) {
            for entry in rd.flatten().take(128) {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with(&needle) || name.contains(&format!("{needle}_")) {
                    let p = entry.path();
                    if is_safe_candidate(&p) && !v.contains(&p) {
                        v.push(p);
                    }
                }
            }
        }
    }

    v
}

fn is_unsafe_path_token(token: &str) -> bool {
    let t = token.trim().trim_end_matches(['\\', '/']);
    if t.is_empty() {
        return true;
    }
    // Bare drive roots: C: or C:\
    if t.len() <= 3 {
        let bytes = t.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' {
            return true;
        }
    }
    false
}

fn is_safe_candidate(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.is_empty() {
        return false;
    }
    // Never yield bare drive root
    let trimmed = s.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':') {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("C:") || trimmed.eq_ignore_ascii_case(r"C:\") {
        return false;
    }
    true
}

/// Map a planned journal entry to the adapter call that would execute it (for unit tests).
pub fn apply_planned_entry(
    env: &mut dyn OsEnvironment,
    entry: &driver_foundry_common::JournalEntry,
) {
    let mut j = ActionJournal::default();
    match (entry.surface.as_str(), entry.action.as_str()) {
        (
            "Service",
            "stop_delete" | "stop_delete_post" | "audio_service" | "pci_filter_stop_delete",
        ) => {
            env.stop_delete_service(&entry.target, &mut j);
        }
        ("Process", _) => env.kill_process_match(&entry.target, &mut j),
        ("File", "wipe_path" | "wipe_install_cache") => {
            env.wipe_path(Path::new(&entry.target), &mut j);
        }
        ("File", "clean_driverstore") => {
            env.clean_driverstore(&entry.target, "", &mut j);
        }
        ("File", "pnp_lockdown_orphans") => {
            env.pnp_lockdown_orphans(&entry.target, &mut j);
        }
        ("File", _) => env.delete_file_match(&entry.target, &mut j),
        ("Device", "clean_pci_root" | "pci_root_summary") => {
            // target is PCI\VEN_* or vendor folder — extract ven when possible
            let ven = entry.target.rsplit('\\').next().unwrap_or(&entry.target);
            env.clean_pci_root("", ven, &mut j);
        }
        (
            "Registry",
            "clean_mmdevices" | "mmdevices_flow" | "mmdevices_summary" | "mmdevices_control",
        ) => {
            let tokens: Vec<String> = entry
                .target
                .split('|')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if tokens.is_empty() {
                env.clean_mmdevices(std::slice::from_ref(&entry.target), &mut j);
            } else {
                env.clean_mmdevices(&tokens, &mut j);
            }
        }
        ("Registry", action) => env.registry_cleanup(action, &entry.target, &mut j),
        ("AppX", _) => env.remove_appx_match(&entry.target, &mut j),
        ("Device", _) => env.uninstall_device(&entry.target, &mut j),
        ("Task", _) => env.delete_scheduled_task(&entry.target, &mut j),
        ("Policy", "block_driver_search") => env.set_block_driver_search(true, &mut j),
        ("Policy", "create_restore_point") => env.create_restore_point(&entry.target, &mut j),
        ("DriverStore", _) => {}
        _ => {}
    }
}
