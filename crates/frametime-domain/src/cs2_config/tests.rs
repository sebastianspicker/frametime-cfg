use std::collections::BTreeMap;

use super::*;

use crate::steam::Cs2Install;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct FakeFs {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Cs2ConfigFs for FakeFs {
    fn create_directory(&mut self, _: &Path) -> io::Result<()> {
        Ok(())
    }
    fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    fn create_file_new(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if self.files.contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        self.files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn atomic_replace(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.files
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

fn controller() -> Cs2ConfigController {
    Cs2ConfigController::new(Cs2Install {
        steam_root: PathBuf::from("/steam"),
        library_root: PathBuf::from("/steam"),
        install_root: PathBuf::from("/steam/steamapps/common/Counter-Strike Global Offensive"),
    })
    .expect("lexically bound install")
}

#[test]
fn preview_and_apply_use_only_the_injected_port() {
    let request = Cs2ConfigRequest::at("2026-08-23 12:34", [OptionalCfgAsset::NetStable]).unwrap();
    let controller = controller();
    let mut files = FakeFs::default();
    let preview = controller.preview(&request, &mut files).unwrap();
    assert!(
        String::from_utf8(preview.optimization_bytes.clone())
            .unwrap()
            .contains("Generated: 2026-08-23 12:34")
    );
    let report = controller.apply(&request, &mut files).unwrap();
    assert_eq!(
        files.files.get(&report.optimization_path).unwrap(),
        &preview.optimization_bytes
    );
    assert_eq!(report.optional_assets_written.len(), 1);
}

#[test]
fn request_timestamp_is_supplied_not_observed() {
    assert!(Cs2ConfigRequest::at("bad", []).is_err());
    assert!(Cs2ConfigRequest::new("2026-08-23 12:34", []).is_ok());
}
