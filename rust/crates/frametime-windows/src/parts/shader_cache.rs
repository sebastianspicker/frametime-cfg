use frametime_core::audit::RebuildableAudit;

const PROGRAM_FILES_X86: &str = "%ProgramFiles(x86)%";
const PROGRAM_FILES: &str = "%ProgramFiles%";
const LOCAL_APP_DATA: &str = "%LOCALAPPDATA%";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownFolders {
    program_files_x86: PathBuf,
    program_files: PathBuf,
    local_app_data: PathBuf,
}

impl KnownFolders {
    #[cfg(windows)]
    pub(crate) fn local_app_data(&self) -> &Path {
        &self.local_app_data
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShaderCacheRoot {
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShaderCacheInventory {
    roots: Vec<ShaderCacheRoot>,
    entry_count: usize,
}

impl ShaderCacheInventory {
    fn is_empty(&self) -> bool {
        self.entry_count == 0
    }
}

fn shader_cache_inventory(config: Option<&Config>) -> Result<ShaderCacheInventory, String> {
    let config = validated_shader_cache_config(config)?;
    let folders = known_folders()?;
    let roots = shader_cache_roots(config, &folders)?;
    inventory_shader_cache_roots(roots)
}

fn validated_shader_cache_config(config: Option<&Config>) -> Result<&Config, String> {
    let config = config.ok_or("P1:3 requires validated frametime.toml cache templates")?;
    config
        .validate()
        .map_err(|error| format!("invalid P1:3 cache config: {error}"))?;
    Ok(config)
}

fn shader_cache_roots(
    config: &Config,
    folders: &KnownFolders,
) -> Result<Vec<ShaderCacheRoot>, String> {
    let mut roots = Vec::new();
    for value in &config.paths.shader_cache {
        roots.push(ShaderCacheRoot {
            path: resolve_cache_template(value, folders)?,
        });
    }
    for value in [
        &config.paths.nvidia_dx_cache,
        &config.paths.nvidia_gl_cache,
        &config.paths.directx_shader_cache,
    ] {
        roots.push(ShaderCacheRoot {
            path: resolve_cache_template(value, folders)?,
        });
    }
    roots.sort_by(|left, right| {
        left.path
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.path.to_string_lossy().to_ascii_lowercase())
    });
    if roots.windows(2).any(|pair| {
        pair[0]
            .path
            .to_string_lossy()
            .eq_ignore_ascii_case(&pair[1].path.to_string_lossy())
    }) {
        return Err("P1:3 configured cache roots are ambiguous or duplicated".into());
    }
    if roots.windows(2).any(|pair| {
        let parent = pair[0].path.to_string_lossy();
        let child = pair[1].path.to_string_lossy();
        child.len() > parent.len()
            && child[..parent.len()].eq_ignore_ascii_case(&parent)
            && child.as_bytes().get(parent.len()) == Some(&b'\\')
    }) {
        return Err("P1:3 configured cache roots overlap".into());
    }
    Ok(roots)
}

pub(crate) fn resolve_cache_template(
    value: &str,
    folders: &KnownFolders,
) -> Result<PathBuf, String> {
    let (root, suffix) = if let Some(suffix) = value.strip_prefix(PROGRAM_FILES_X86) {
        (&folders.program_files_x86, suffix)
    } else if let Some(suffix) = value.strip_prefix(PROGRAM_FILES) {
        (&folders.program_files, suffix)
    } else if let Some(suffix) = value.strip_prefix(LOCAL_APP_DATA) {
        (&folders.local_app_data, suffix)
    } else if exact_local_drive_template(value) {
        return checked_windows_path(value).map(PathBuf::from);
    } else {
        return Err(
            "P1:3 cache template is not a configured known-folder or exact local-drive path".into(),
        );
    };
    let suffix = suffix
        .strip_prefix('\\')
        .ok_or("P1:3 template lacks a bounded relative suffix")?;
    validate_relative_components(suffix)?;
    let root_text = root.to_string_lossy();
    let root = checked_windows_path(&root_text)?;
    Ok(PathBuf::from(format!(r"{root}\{suffix}")))
}

fn exact_local_drive_template(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && !value.starts_with("\\\\")
}

fn checked_windows_path(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.starts_with("\\\\")
        || value.starts_with(r"\\?\\")
        || value.starts_with(r"\\.\\")
        || value.contains('\0')
        || value.contains('/')
        || value.ends_with([' ', '.'])
        || !exact_local_drive_template(value)
    {
        return Err("P1:3 path is not an exact local drive path".into());
    }
    validate_relative_components(&value[3..])?;
    Ok(value)
}

fn validate_relative_components(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("P1:3 path has no cache-root component".into());
    }
    for component in value.split('\\') {
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with([' ', '.'])
            || component.contains(':')
        {
            return Err("P1:3 path has traversal, alternate-stream, or alias components".into());
        }
    }
    Ok(())
}

#[cfg(any(test, windows))]
fn validate_shader_cache_entry_name(name: &[u16]) -> Result<(), String> {
    if name.is_empty()
        || name == [b'.' as u16]
        || name == [b'.' as u16, b'.' as u16]
        || name.iter().any(|unit| {
            *unit == 0
                || *unit == u16::from(b'\\')
                || *unit == u16::from(b'/')
                || *unit == u16::from(b':')
        })
        || name
            .last()
            .is_some_and(|unit| *unit == u16::from(b' ') || *unit == u16::from(b'.'))
    {
        return Err("P1:3 directory entry has a hostile or alias name".into());
    }
    Ok(())
}

#[cfg(any(test, windows))]
fn normalize_windows_dos_path(path: &[u16]) -> Result<Vec<u16>, String> {
    const UPPER_A: u16 = b'A' as u16;
    const UPPER_Z: u16 = b'Z' as u16;
    const LOWER_A: u16 = b'a' as u16;
    const LOWER_Z: u16 = b'z' as u16;
    let path = if path.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16]) {
        &path[4..]
    } else if path.starts_with(&[b'\\' as u16, b'\\' as u16]) {
        return Err("P1:3 final path has an unapproved device or UNC prefix".into());
    } else {
        path
    };
    if path.len() < 4
        || !matches!(path[0], UPPER_A..=UPPER_Z | LOWER_A..=LOWER_Z)
        || path[1] != u16::from(b':')
        || path[2] != u16::from(b'\\')
        || path.contains(&0)
    {
        return Err("P1:3 final path is not an exact DOS drive path".into());
    }
    Ok(path
        .iter()
        .map(|unit| match *unit {
            upper @ UPPER_A..=UPPER_Z => upper + LOWER_A - UPPER_A,
            unit => unit,
        })
        .collect())
}

#[cfg(windows)]
fn inventory_shader_cache_roots(
    roots: Vec<ShaderCacheRoot>,
) -> Result<ShaderCacheInventory, String> {
    let entry_count = shader_cache_handles::inspect_roots(&roots)?;
    Ok(ShaderCacheInventory { roots, entry_count })
}

#[cfg(not(windows))]
fn inventory_shader_cache_roots(
    roots: Vec<ShaderCacheRoot>,
) -> Result<ShaderCacheInventory, String> {
    let mut entries = Vec::new();
    for root in &roots {
        let metadata = match fs::symlink_metadata(&root.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "inspect P1:3 cache root {}: {error}",
                    root.path.display()
                ));
            }
        };
        inventory_tree(&root.path, metadata, &mut entries)?;
    }
    Ok(ShaderCacheInventory {
        roots,
        entry_count: entries.len(),
    })
}

#[cfg(not(windows))]
fn inventory_tree(
    root: &Path,
    metadata: fs::Metadata,
    entries: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if is_reparse_point(&metadata) {
        return Err(format!(
            "P1:3 cache root is a reparse point: {}",
            root.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "P1:3 cache root is not a directory: {}",
            root.display()
        ));
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("enumerate P1:3 cache root {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("read P1:3 cache entry: {error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect P1:3 cache entry {}: {error}", path.display()))?;
        if is_reparse_point(&metadata) {
            return Err(format!(
                "P1:3 cache tree contains a reparse point: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            inventory_tree(&path, metadata, entries)?;
        }
        entries.push(path);
    }
    Ok(())
}

fn inspect_shader_cache(config: Option<&Config>) -> Result<Inspection, String> {
    inspection_from_shader_cache_inventory(&shader_cache_inventory(config)?)
}

fn inspection_from_shader_cache_inventory(
    inventory: &ShaderCacheInventory,
) -> Result<Inspection, String> {
    if inventory.is_empty() {
        Ok(Inspection::Satisfied)
    } else if shader_cache_delete_qualified() {
        Ok(Inspection::NeedsApply)
    } else {
        // Inventory is intentionally useful even while mutation is unavailable,
        // but the engine must not create a pending audit for an operation whose
        // no-follow delete primitive is not yet proven.
        Ok(Inspection::Unsupported)
    }
}

fn inspection_from_shader_cache_audit(
    inventory: &ShaderCacheInventory,
    pending: Option<&RebuildableAudit>,
) -> Result<Inspection, String> {
    if inventory.is_empty() && pending.is_none() {
        return Ok(Inspection::Satisfied);
    }
    // A previous, exact pending record is the only record an interrupted run
    // may retry. New work remains unavailable until a Windows VM proves the
    // native delete contract.
    if pending.is_some() {
        return Ok(Inspection::NeedsApply);
    }
    inspection_from_shader_cache_inventory(inventory)
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn verify_shader_cache(inventory: &ShaderCacheInventory) -> Result<(), String> {
    let current = inventory_shader_cache_roots(inventory.roots.clone())?;
    if current.is_empty() {
        Ok(())
    } else {
        Err("P1:3 cache verification found remaining entries".into())
    }
}

/// Deliberately fail closed until the path-walk is replaced by a fully
/// handle-relative deletion primitive.  `std::fs` can inventory without
/// following an observed reparse point, but pathname removal cannot prove the
/// same object remains selected at deletion time.
fn clear_shader_cache(inventory: &ShaderCacheInventory) -> Result<(), String> {
    if inventory.is_empty() {
        return Ok(());
    }
    // Do not arm based on source compilation. The primitive below is retained
    // for Windows VM qualification, but this release has no VM evidence for
    // reparse, rename-race, sharing, or disposition behavior.
    if !shader_cache_delete_qualified() {
        return Err("P1:3 is unavailable: handle-backed deletion is not armed without Windows VM qualification".into());
    }
    #[cfg(windows)]
    {
        shader_cache_handles::delete_roots(&inventory.roots)
    }
    #[cfg(not(windows))]
    {
        let _ = inventory;
        Err("P1:3 live shader-cache deletion is supported only on Windows".into())
    }
}

/// The default release remains fail-closed. A Windows qualification build can
/// exercise the exact production primitive without another source edit; the
/// feature must become a release policy only after the VM matrix is recorded.
pub(crate) const fn shader_cache_delete_qualified() -> bool {
    cfg!(feature = "qualified-shader-cache-delete")
}

#[cfg(windows)]
pub(crate) fn known_folders() -> Result<KnownFolders, String> {
    use windows::{
        Win32::UI::Shell::{
            FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
            SHGetKnownFolderPath,
        },
        core::PWSTR,
    };
    struct KnownFolderAllocation(PWSTR);
    impl Drop for KnownFolderAllocation {
        fn drop(&mut self) {
            unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(self.0.0.cast())) };
        }
    }
    fn get(id: &windows::core::GUID) -> Result<PathBuf, String> {
        let raw = KnownFolderAllocation(
            unsafe { SHGetKnownFolderPath(id, Default::default(), None) }
                .map_err(|error| format!("resolve known cache folder: {error}"))?,
        );
        let value = unsafe { raw.0.to_string() }
            .map_err(|error| format!("decode known cache folder: {error}"))?;
        checked_windows_path(&value).map(PathBuf::from)
    }
    Ok(KnownFolders {
        program_files_x86: get(&FOLDERID_ProgramFilesX86)?,
        program_files: get(&FOLDERID_ProgramFiles)?,
        local_app_data: get(&FOLDERID_LocalAppData)?,
    })
}

#[cfg(not(windows))]
pub(crate) fn known_folders() -> Result<KnownFolders, String> {
    Err("P1:3 live shader-cache inspection is supported only on Windows".into())
}

#[cfg(test)]
mod shader_cache_tests {
    use super::*;

    #[test]
    fn template_resolver_accepts_only_known_folders_or_local_drive_defaults() {
        let folders = KnownFolders {
            program_files_x86: PathBuf::from(r"C:\Program Files (x86)"),
            program_files: PathBuf::from(r"C:\Program Files"),
            local_app_data: PathBuf::from(r"C:\Users\operator\AppData\Local"),
        };
        assert_eq!(
            resolve_cache_template(r"%LOCALAPPDATA%\NVIDIA\DXCache", &folders).expect("template"),
            PathBuf::from(r"C:\Users\operator\AppData\Local\NVIDIA\DXCache")
        );
        assert!(resolve_cache_template(r"\\server\cache", &folders).is_err());
        assert!(resolve_cache_template(r"C:\safe\..\cache", &folders).is_err());
        assert!(resolve_cache_template(r"C:\safe:stream\cache", &folders).is_err());
        assert!(resolve_cache_template(r"C:\safe\alias.\cache", &folders).is_err());
    }

    #[test]
    fn four_target_audit_contract_remains_exact() {
        assert_eq!(frametime_core::P1_3_REBUILDABLE_TARGETS.len(), 4);
        assert_eq!(
            frametime_core::P1_3_REBUILDABLE_TARGETS[0],
            frametime_core::audit::RebuildableTarget::Cs2ShaderCache
        );
    }

    #[test]
    fn nonempty_inventory_follows_the_explicit_qualification_gate() {
        let empty = ShaderCacheInventory {
            roots: Vec::new(),
            entry_count: 0,
        };
        assert_eq!(
            inspection_from_shader_cache_inventory(&empty).expect("empty inspection"),
            Inspection::Satisfied
        );
        let nonempty = ShaderCacheInventory {
            roots: Vec::new(),
            entry_count: 1,
        };
        let expected = if shader_cache_delete_qualified() {
            Inspection::NeedsApply
        } else {
            Inspection::Unsupported
        };
        assert_eq!(
            inspection_from_shader_cache_inventory(&nonempty).expect("nonempty inspection"),
            expected
        );
    }

    #[test]
    fn pending_audit_permits_only_a_retry_of_nonempty_work() {
        let pending =
            RebuildableAudit::pending("P1:3", "captured", frametime_core::P1_3_REBUILDABLE_TARGETS)
                .expect("pending audit");
        let nonempty = ShaderCacheInventory {
            roots: Vec::new(),
            entry_count: 1,
        };
        assert_eq!(
            inspection_from_shader_cache_audit(&nonempty, Some(&pending)).expect("retry"),
            Inspection::NeedsApply
        );
        let empty = ShaderCacheInventory {
            roots: Vec::new(),
            entry_count: 0,
        };
        assert_eq!(
            inspection_from_shader_cache_audit(&empty, Some(&pending)).expect("empty"),
            Inspection::NeedsApply
        );
        assert!(clear_shader_cache(&empty).is_ok());
    }

    #[test]
    fn pending_audit_rejects_malformed_and_ambiguous_records() {
        let valid =
            RebuildableAudit::pending("P1:3", "captured", frametime_core::P1_3_REBUILDABLE_TARGETS)
                .expect("valid audit");
        let file = AuditFile {
            entries: vec![AuditEntry::Rebuildable(valid.clone())],
            created: "now".into(),
            unknown: BTreeMap::new(),
        };
        assert_eq!(
            p1_3_pending_audit(&file).expect("one record"),
            Some(valid.clone())
        );

        let duplicate = AuditFile {
            entries: vec![
                AuditEntry::Rebuildable(valid.clone()),
                AuditEntry::Rebuildable(valid.clone()),
            ],
            created: "now".into(),
            unknown: BTreeMap::new(),
        };
        assert!(p1_3_pending_audit(&duplicate).is_err());

        let mut malformed = valid;
        malformed.unknown.insert("future".into(), Value::Bool(true));
        let malformed = AuditFile {
            entries: vec![AuditEntry::Rebuildable(malformed)],
            created: "now".into(),
            unknown: BTreeMap::new(),
        };
        assert!(p1_3_pending_audit(&malformed).is_err());
    }

    #[test]
    fn live_handle_delete_gate_matches_the_explicit_build_feature() {
        assert_eq!(
            shader_cache_delete_qualified(),
            cfg!(feature = "qualified-shader-cache-delete")
        );
    }

    #[test]
    fn directory_entry_names_reject_alias_and_separator_forms() {
        for value in [
            [b'.' as u16].as_slice(),
            &[b'a' as u16, b':' as u16],
            &[b'a' as u16, b' ' as u16],
            &[0_u16],
        ] {
            assert!(validate_shader_cache_entry_name(value).is_err());
        }
        assert!(validate_shader_cache_entry_name(&[0x4e2d, 0x6587]).is_ok());
    }

    #[test]
    fn final_path_normalization_accepts_only_dos_or_documented_prefix() {
        let configured = r"C:\Cache\Shaders".encode_utf16().collect::<Vec<_>>();
        let final_path = r"\\?\c:\cache\shaders".encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            normalize_windows_dos_path(&configured).expect("configured path"),
            normalize_windows_dos_path(&final_path).expect("final path")
        );
        assert!(
            normalize_windows_dos_path(&r"\\.\C:\Cache".encode_utf16().collect::<Vec<_>>())
                .is_err()
        );
        assert!(
            normalize_windows_dos_path(&r"\\server\cache".encode_utf16().collect::<Vec<_>>())
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_root_link_is_not_misclassified_as_an_absent_cache() {
        use std::os::unix::fs::symlink;

        let fixture = tempfile::tempdir().expect("fixture directory");
        let root = fixture.path().join("cache-link");
        symlink(fixture.path().join("missing"), &root).expect("dangling link");
        let error = inventory_shader_cache_roots(vec![ShaderCacheRoot { path: root }])
            .expect_err("dangling reparse root must fail closed");
        assert!(error.contains("reparse point"));
    }
}
