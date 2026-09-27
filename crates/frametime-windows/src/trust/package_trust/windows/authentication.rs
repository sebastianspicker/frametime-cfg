use super::*;

pub(crate) fn authenticate(
    root: &Path,
    pins: &[String],
    current_image: &Path,
) -> Result<AuthenticatedPackage, String> {
    if !is_fixed_local_drive(root)? {
        return Err("package root must be on a local fixed drive".into());
    }
    let root_handle = open(root, true)?;
    let root_path = root_handle.path.clone();
    verify_tree_inventory(&root_path)?;
    let current = open(current_image, false)?;
    if !matches!(
        current.path.file_name().and_then(|name| name.to_str()),
        Some(name) if name.eq_ignore_ascii_case(GUI_EXECUTABLE_NAME) || name.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME)
    ) {
        return Err("current image is not a package GUI or CLI role".into());
    }
    let manifest_file = open(&root_path.join(PACKAGE_MANIFEST_NAME), false)?;
    let manifest = PackageManifest::parse(&read_bounded(&manifest_file, 1024 * 1024)?)?;
    let catalog = open(&root_path.join(PACKAGE_CATALOG_NAME), false)?;
    let mut retained = vec![root_handle, manifest_file, catalog];
    let mut ids = HashSet::new();
    for file in &retained {
        if !ids.insert((file.id.VolumeSerialNumber, file.id.FileId.Identifier)) {
            return Err("package contains duplicate file identities".into());
        }
    }
    let mut payload = Vec::new();
    let mut config_bytes = None;
    let mut directories = BTreeSet::new();
    for entry in manifest.files() {
        for ancestor in ancestors(entry.path()) {
            directories.insert(ancestor);
        }
        let file = open(&root_path.join(entry.path()), false)?;
        if !ids.insert((file.id.VolumeSerialNumber, file.id.FileId.Identifier)) {
            return Err("package contains duplicate file identities".into());
        }
        if file_size(&file)? != entry.size() || hash(&file)? != entry.sha256() {
            return Err(format!(
                "package payload hash or size differs: {}",
                entry.path()
            ));
        }
        config_bytes = config_bytes.or(read_config_snapshot(entry, &file)?);
        payload.push((entry.path(), file));
    }
    for directory in directories {
        retained.push(open(&root_path.join(directory), true)?);
    }
    let manifest_member = &retained[1];
    let catalog_member = &retained[2];
    let gui_index = payload
        .iter()
        .position(|(path, _)| path.eq_ignore_ascii_case(GUI_EXECUTABLE_NAME))
        .ok_or("package manifest omits GUI executable")?;
    let cli_index = payload
        .iter()
        .position(|(path, _)| path.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME))
        .ok_or("package manifest omits CLI executable")?;
    if !same_retained_file(&current, &payload[gui_index].1)
        && !same_retained_file(&current, &payload[cli_index].1)
    {
        return Err("current image does not match the retained package GUI or CLI".into());
    }
    let gui_signer = verify_file(&payload[gui_index].1)?;
    let cli_signer = verify_file(&payload[cli_index].1)?;
    let catalog_signer = verify_catalog_member(catalog_member, manifest_member)?;
    for (_, file) in &payload {
        if verify_catalog_member(catalog_member, file)? != catalog_signer {
            return Err("package catalog signer differs across members".into());
        }
    }
    if gui_signer != cli_signer
        || gui_signer != catalog_signer
        || !pins.iter().any(|pin| pin == &gui_signer)
    {
        return Err("GUI, CLI, and catalog signer do not match a pinned publisher SPKI".into());
    }
    let gui = payload.swap_remove(gui_index).1;
    let cli_index = payload
        .iter()
        .position(|(path, _)| path.eq_ignore_ascii_case(CLI_EXECUTABLE_NAME))
        .expect("CLI index remains after GUI extraction");
    let cli = payload.swap_remove(cli_index).1;
    let payload = payload
        .into_iter()
        .map(|(path, file)| (path.to_ascii_lowercase(), file))
        .collect::<BTreeMap<_, _>>();
    Ok(AuthenticatedPackage {
        root: root_path,
        manifest,
        config: bind_config_snapshot(config_bytes)?,
        gui: AuthenticatedExecutable {
            path: gui.path.clone(),
            _retained: gui,
        },
        cli: AuthenticatedExecutable {
            path: cli.path.clone(),
            _retained: cli,
        },
        payload,
        _retained: retained,
    })
}

pub(crate) fn read_config_snapshot(
    entry: &PackageFile,
    file: &RetainedFile,
) -> Result<Option<(Vec<u8>, u64, String)>, String> {
    if !entry.path().eq_ignore_ascii_case("frametime.toml") {
        return Ok(None);
    }
    Ok(Some((
        read_bounded(file, 1024 * 1024)?,
        entry.size(),
        entry.sha256().to_owned(),
    )))
}

pub(crate) fn bind_config_snapshot(
    snapshot: Option<(Vec<u8>, u64, String)>,
) -> Result<VerifiedConfig, String> {
    let (bytes, size, sha256) = snapshot.ok_or("package manifest omits frametime.toml")?;
    // The byte binder runs only after the exact tree, retained-handle hashes,
    // catalog membership, signer equality, and publisher pin all succeeded.
    VerifiedConfig::from_verified_bytes(bytes, size, &sha256)
}
