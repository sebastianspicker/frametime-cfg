//! Native CS2 CFG mutation port with per-operation trusted-path validation.
//!
//! The domain provides the closed target set. This adapter re-observes the
//! bound Steam install and rejects reparse points immediately before every
//! pathname operation, so a stale discovery result cannot redirect mutation.

use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use frametime_domain::{Cs2ConfigFs, Cs2Install, OptionalCfgAsset};

const CS2_DIRECTORY: &str = "Counter-Strike Global Offensive";
const OPTIMIZATION_FILE: &str = "optimization.cfg";
const OPTIMIZATION_BACKUP_FILE: &str = "optimization.cfg.bak";
const AUTOEXEC_FILE: &str = "autoexec.cfg";
const AUTOEXEC_BACKUP_FILE: &str = "autoexec.cfg.bak";

#[derive(Debug, Clone)]
pub struct NativeCs2ConfigFs {
    steam_root: PathBuf,
    library_root: PathBuf,
    install_root: PathBuf,
    cfg_directory: PathBuf,
}

impl NativeCs2ConfigFs {
    /// Binds native CFG access to one freshly discovered CS2 install.
    pub fn new(install: &Cs2Install) -> io::Result<Self> {
        let expected = install
            .library_root
            .join("steamapps")
            .join("common")
            .join(CS2_DIRECTORY);
        if install.install_root != expected {
            return Err(untrusted(
                "CS2 install does not match the Steam library layout",
            ));
        }
        let adapter = Self {
            steam_root: install.steam_root.clone(),
            library_root: install.library_root.clone(),
            install_root: install.install_root.clone(),
            cfg_directory: install.install_root.join("game/csgo/cfg"),
        };
        adapter.validate_binding()?;
        Ok(adapter)
    }

    fn validate_binding(&self) -> io::Result<()> {
        trusted_directory(&self.steam_root)?;
        trusted_directory(&self.library_root)?;
        trusted_directory(&self.install_root)?;
        trusted_file_under(
            &self.library_root,
            &self.install_root.join("game/bin/win64/cs2.exe"),
        )?;
        trusted_directory_under(&self.install_root, &self.install_root.join("game/csgo"))
    }

    fn validate_cfg_directory(&self, require_existing: bool) -> io::Result<()> {
        self.validate_binding()?;
        match fs::symlink_metadata(&self.cfg_directory) {
            Ok(metadata) if metadata_is_reparse(&metadata) || !metadata.is_dir() => {
                Err(untrusted_path(&self.cfg_directory))
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound && !require_existing => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn validate_target(&self, target: &Path) -> io::Result<()> {
        self.validate_cfg_directory(false)?;
        validate_direct_child(&self.cfg_directory, target, is_known_target_name)?;
        validate_existing_file_or_absent(target)
    }

    fn validate_temporary(&self, temporary: &Path) -> io::Result<()> {
        self.validate_cfg_directory(true)?;
        validate_direct_child(&self.cfg_directory, temporary, |name| {
            name.contains(".tmp.")
        })?;
        validate_existing_file_or_absent(temporary)
    }
}

impl Cs2ConfigFs for NativeCs2ConfigFs {
    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        if path != self.cfg_directory {
            return Err(untrusted_path(path));
        }
        self.validate_cfg_directory(false)?;
        fs::create_dir_all(path)?;
        self.validate_cfg_directory(true)
    }

    fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        self.validate_target(path)?;
        fs::read(path)
    }

    fn create_file_new(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.validate_target(path)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        output.write_all(bytes)?;
        output.sync_all()
    }

    fn atomic_replace(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.validate_target(path)?;
        let temporary = temporary_path(path);
        let result = (|| {
            self.validate_temporary(&temporary)?;
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            output.write_all(bytes)?;
            output.sync_all()?;
            self.validate_target(path)?;
            self.validate_temporary(&temporary)?;
            replace_file(&temporary, path)?;
            self.validate_cfg_directory(true)?;
            sync_parent(&self.cfg_directory)
        })();
        if result.is_err() && self.validate_temporary(&temporary).is_ok() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.validate_target(path)?;
        fs::remove_file(path)
    }
}

fn trusted_directory(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || has_unsafe_components(path) {
        return Err(untrusted_path(path));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(untrusted_path(path));
    }
    Ok(())
}

fn trusted_directory_under(root: &Path, candidate: &Path) -> io::Result<()> {
    trusted_directory(root)?;
    reject_reparse_ancestors(candidate, root)?;
    let metadata = fs::symlink_metadata(candidate)?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(untrusted_path(candidate));
    }
    Ok(())
}

fn trusted_file_under(root: &Path, candidate: &Path) -> io::Result<()> {
    trusted_directory(root)?;
    reject_reparse_ancestors(candidate, root)?;
    let canonical_root = fs::canonicalize(root)?;
    let canonical_candidate = fs::canonicalize(candidate)?;
    if !canonical_candidate.starts_with(canonical_root) {
        return Err(untrusted_path(candidate));
    }
    let metadata = fs::symlink_metadata(candidate)?;
    if metadata_is_reparse(&metadata) || !metadata.is_file() {
        return Err(untrusted_path(candidate));
    }
    Ok(())
}

fn validate_direct_child(
    root: &Path,
    candidate: &Path,
    expected_name: impl FnOnce(&str) -> bool,
) -> io::Result<()> {
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| untrusted_path(candidate))?;
    let mut components = relative.components();
    let Some(Component::Normal(name)) = components.next() else {
        return Err(untrusted_path(candidate));
    };
    if components.next().is_some() || !expected_name(&name.to_string_lossy()) {
        return Err(untrusted_path(candidate));
    }
    Ok(())
}

fn validate_existing_file_or_absent(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_reparse(&metadata) || !metadata.is_file() => {
            Err(untrusted_path(path))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn reject_reparse_ancestors(candidate: &Path, root: &Path) -> io::Result<()> {
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| untrusted_path(candidate))?;
    if has_unsafe_components(candidate)
        || relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(untrusted_path(candidate));
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata_is_reparse(&metadata) => return Err(untrusted_path(&current)),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn has_unsafe_components(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
}

fn is_known_target_name(name: &str) -> bool {
    matches!(
        name,
        OPTIMIZATION_FILE | OPTIMIZATION_BACKUP_FILE | AUTOEXEC_FILE | AUTOEXEC_BACKUP_FILE
    ) || OptionalCfgAsset::ALL
        .into_iter()
        .any(|asset| asset.file_name() == name)
}

fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn untrusted_path(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("CS2 CFG path is not trusted: {}", path.display()),
    )
}

fn untrusted(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

fn temporary_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(".tmp.{}.{}", std::process::id(), nonce));
    PathBuf::from(temporary)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)
    }
}

#[cfg(not(windows))]
fn sync_parent(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(windows)]
const fn sync_parent(_: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, Cs2Install) {
        let temporary = TempDir::new().expect("temporary test root");
        let steam_root = temporary.path().join("steam");
        let library_root = temporary.path().join("library");
        let install_root = library_root.join("steamapps/common").join(CS2_DIRECTORY);
        fs::create_dir_all(steam_root.join("steamapps")).expect("Steam root");
        fs::create_dir_all(install_root.join("game/bin/win64")).expect("CS2 executable parent");
        fs::write(
            install_root.join("game/bin/win64/cs2.exe"),
            b"test executable",
        )
        .expect("CS2 executable");
        fs::create_dir_all(install_root.join("game/csgo/cfg")).expect("CS2 cfg directory");
        (
            temporary,
            Cs2Install {
                steam_root,
                library_root,
                install_root,
            },
        )
    }

    #[test]
    fn only_accepts_closed_cfg_targets() {
        let (_temporary, install) = fixture();
        let files = NativeCs2ConfigFs::new(&install).expect("trusted fixture");
        let rejected = files.validate_target(&files.cfg_directory.join("operator.cfg"));
        assert_eq!(
            rejected.expect_err("arbitrary CFG must fail").kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn ordinary_target_stays_usable_after_reobservation() {
        let (_temporary, install) = fixture();
        let mut files = NativeCs2ConfigFs::new(&install).expect("trusted fixture");
        let target = install.install_root.join("game/csgo/cfg/optimization.cfg");
        files
            .atomic_replace(&target, b"fps_max 400\n")
            .expect("write target");
        assert_eq!(
            files.read_file(&target).expect("read target"),
            b"fps_max 400\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_reparse_cfg_directory_before_target_mutation() {
        use std::os::unix::fs::symlink;

        let (temporary, install) = fixture();
        let cfg_directory = install.install_root.join("game/csgo/cfg");
        let external = temporary.path().join("external");
        fs::create_dir(&external).expect("external directory");
        fs::remove_dir(&cfg_directory).expect("replace cfg directory");
        symlink(&external, &cfg_directory).expect("test reparse point");
        let files =
            NativeCs2ConfigFs::new(&install).expect("binding does not require cfg directory");
        let error = files
            .validate_target(&cfg_directory.join(OPTIMIZATION_FILE))
            .expect_err("reparse cfg directory must fail closed");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
