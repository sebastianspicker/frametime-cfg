//! Windows-only P1:3 cache-tree primitive.
//!
//! The public backend deliberately leaves this unarmed pending VM qualification.
//! This module nevertheless keeps all object selection and deletion handle-based:
//! it never uses pathname deletion, shells, NT calls, or ACL takeover.

use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::Path,
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::{
            CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_FORCE_IMAGE_SECTION_CHECK,
            FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
            FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0,
            FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FileDispositionInfoEx, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
            FileIdType, GetDriveTypeW, GetFileInformationByHandleEx, OPEN_EXISTING, OpenFileById,
            SetFileInformationByHandle,
        },
        System::WindowsProgramming::DRIVE_FIXED,
    },
    core::PCWSTR,
};

use crate::{
    normalize_windows_dos_path,
    shader_cache_handle_validation::{
        ensure_direct_child, observe_node, parse_directory_buffer, validate_directory_entry,
        validate_node,
    },
};

use super::ShaderCacheRoot;

const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 100_000;
pub(crate) const MAX_PATH_UTF16: usize = 32_767;
const MAX_MEMORY: usize = 64 * 1024 * 1024;
const DIRECTORY_BUFFER: usize = 64 * 1024;
pub(crate) const FILE_ATTRIBUTE_DIRECTORY_BITS: u32 = FILE_ATTRIBUTE_DIRECTORY.0;
pub(crate) const FILE_ATTRIBUTE_REPARSE_BITS: u32 = FILE_ATTRIBUTE_REPARSE_POINT.0;

#[derive(Debug)]
pub(crate) struct OwnedHandle(pub(crate) HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

impl OwnedHandle {
    fn close(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
        self.0 = HANDLE::default();
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct FileKey {
    pub(crate) volume: u64,
    pub(crate) id: [u8; 16],
}

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) handle: OwnedHandle,
    pub(crate) id: FileKey,
    pub(crate) parent: Option<usize>,
    pub(crate) name: Vec<u16>,
    pub(crate) depth: usize,
    pub(crate) path_units: usize,
    pub(crate) directory: bool,
    pub(crate) root: bool,
    pub(crate) entry_id: Option<i64>,
    pub(crate) entry_attributes: Option<u32>,
    pub(crate) final_path: Vec<u16>,
}

#[derive(Debug)]
struct NodeInput {
    parent: Option<usize>,
    name: Vec<u16>,
    depth: usize,
    path_units: usize,
    directory: bool,
    root: bool,
    entry_id: Option<i64>,
    entry_attributes: Option<u32>,
}

#[derive(Debug)]
pub(crate) struct ObservedNode {
    pub(crate) id: FileKey,
    pub(crate) legacy_id: i64,
    pub(crate) attributes: u32,
    pub(crate) directory: bool,
    pub(crate) final_path: Vec<u16>,
}

#[derive(Debug)]
struct CacheTree {
    nodes: Vec<Node>,
    roots: Vec<usize>,
    memory: usize,
}

pub(crate) fn inspect_roots(roots: &[ShaderCacheRoot]) -> Result<usize, String> {
    let tree = CacheTree::preflight(roots)?;
    Ok(tree.nodes.len().saturating_sub(tree.roots.len()))
}

/// Execute the complete same-handle delete sequence.
///
/// This is intentionally not called by the live backend until Windows VM tests
/// establish the contract. The retained directory handles are restarted and
/// compared before the first tree's retained handles are marked
/// delete-pending in postorder. Configured roots are never dispositioned.
pub(crate) fn delete_roots(roots: &[ShaderCacheRoot]) -> Result<(), String> {
    let tree = CacheTree::preflight(roots)?;
    tree.reenumerate_retained_handles()?;
    tree.delete_postorder()
}

/// Delete the contents, but never the supplied roots, of fixed native paths.
/// Callers may pass only roots selected by their own exact allowlist.
pub(crate) fn delete_fixed_roots(paths: &[std::path::PathBuf]) -> Result<usize, String> {
    let roots = paths
        .iter()
        .cloned()
        .map(|path| ShaderCacheRoot { path })
        .collect::<Vec<_>>();
    let tree = CacheTree::preflight(&roots)?;
    let count = tree.nodes.len().saturating_sub(tree.roots.len());
    tree.reenumerate_retained_handles()?;
    tree.delete_postorder()?;
    Ok(count)
}

/// Delete only leaf `.pf` files from a fixed Prefetch root. Directories and
/// non-prefetch siblings remain retained and are never dispositioned.
pub(crate) fn delete_prefetch_pf_files(root: &std::path::Path) -> Result<usize, String> {
    let roots = [ShaderCacheRoot {
        path: root.to_path_buf(),
    }];
    let mut tree = CacheTree::preflight(&roots)?;
    tree.reenumerate_retained_handles()?;
    tree.delete_leaf_files(|name| {
        name.len() >= 3
            && name[name.len() - 3] == u16::from(b'.')
            && matches!(name[name.len() - 2], 112 | 80)
            && matches!(name[name.len() - 1], 102 | 70)
    })
}

impl CacheTree {
    fn preflight(roots: &[ShaderCacheRoot]) -> Result<Self, String> {
        let mut tree = Self {
            nodes: Vec::new(),
            roots: Vec::new(),
            memory: 0,
        };
        let mut seen = BTreeSet::new();
        for root in roots {
            let Some(handle) = open_root(&root.path)? else {
                continue;
            };
            let index = tree.push_node(
                handle,
                NodeInput {
                    parent: None,
                    name: Vec::new(),
                    depth: 0,
                    path_units: root_units(&root.path)?,
                    directory: true,
                    root: true,
                    entry_id: None,
                    entry_attributes: None,
                },
                &mut seen,
            )?;
            root_final_path_matches_configured(&tree.nodes[index].final_path, &root.path)?;
            tree.roots.push(index);
            tree.walk_directory(index, &mut seen)?;
        }
        Ok(tree)
    }

    fn walk_directory(
        &mut self,
        parent: usize,
        seen: &mut BTreeSet<FileKey>,
    ) -> Result<(), String> {
        let children = enumerate_directory(self.nodes[parent].handle.0)?;
        for entry in children {
            let depth = self.nodes[parent].depth + 1;
            if depth > MAX_DEPTH {
                return Err("P1:3 cache tree exceeds the maximum depth of 64".into());
            }
            let path_units = self.nodes[parent]
                .path_units
                .checked_add(1 + entry.name.len())
                .ok_or("P1:3 cache path length overflow")?;
            if path_units > MAX_PATH_UTF16 {
                return Err("P1:3 cache path exceeds 32767 UTF-16 code units".into());
            }
            if entry.attributes & FILE_ATTRIBUTE_REPARSE_BITS != 0 {
                return Err("P1:3 cache tree contains a reparse point".into());
            }
            let directory = entry.attributes & FILE_ATTRIBUTE_DIRECTORY_BITS != 0;
            let handle = open_by_id(self.nodes[parent].handle.0, entry.id, directory)?;
            let index = self.push_node(
                handle,
                NodeInput {
                    parent: Some(parent),
                    name: entry.name,
                    depth,
                    path_units,
                    directory,
                    root: false,
                    entry_id: Some(entry.id),
                    entry_attributes: Some(entry.attributes),
                },
                seen,
            )?;
            if directory {
                self.walk_directory(index, seen)?;
            }
        }
        Ok(())
    }

    fn push_node(
        &mut self,
        handle: OwnedHandle,
        input: NodeInput,
        seen: &mut BTreeSet<FileKey>,
    ) -> Result<usize, String> {
        if self.nodes.len() == MAX_NODES {
            return Err("P1:3 cache tree exceeds the maximum of 100000 nodes".into());
        }
        let observed = observe_node(handle.0)?;
        if input.root && !observed.directory {
            return Err("P1:3 configured cache root is not a directory".into());
        }
        if let Some(entry_id) = input.entry_id
            && (observed.legacy_id != entry_id
                || input.entry_attributes != Some(observed.attributes)
                || input.directory != observed.directory)
        {
            return Err("P1:3 opened entry does not match its enumerated file ID or type".into());
        }
        if !seen.insert(observed.id) {
            return Err(
                "P1:3 cache tree contains duplicate object identities or hard links".into(),
            );
        }
        let next_memory = self
            .memory
            .checked_add(
                size_of::<Node>()
                    + (input.name.len() + observed.final_path.len()) * size_of::<u16>(),
            )
            .ok_or("P1:3 cache tree memory accounting overflow")?;
        if next_memory > MAX_MEMORY {
            return Err("P1:3 cache tree exceeds the 64 MiB memory limit".into());
        }
        self.memory = next_memory;
        if let Some(parent) = input.parent {
            ensure_direct_child(
                &self.nodes[parent].final_path,
                &observed.final_path,
                &input.name,
            )?;
        }
        self.nodes.push(Node {
            handle,
            id: observed.id,
            parent: input.parent,
            name: input.name,
            depth: input.depth,
            path_units: input.path_units,
            directory: input.directory,
            root: input.root,
            entry_id: input.entry_id,
            entry_attributes: input.entry_attributes,
            final_path: observed.final_path,
        });
        Ok(self.nodes.len() - 1)
    }

    fn reenumerate_retained_handles(&self) -> Result<(), String> {
        let mut expected = BTreeMap::new();
        for node in self.nodes.iter().filter(|node| !node.root) {
            let parent = node
                .parent
                .ok_or("P1:3 non-root node lacks a retained parent")?;
            let entry_id = node
                .entry_id
                .ok_or("P1:3 non-root node lacks its enumerated file ID")?;
            let entry_attributes = node
                .entry_attributes
                .ok_or("P1:3 non-root node lacks its enumerated attributes")?;
            let parent_node = self
                .nodes
                .get(parent)
                .ok_or("P1:3 node refers to an invalid retained parent")?;
            expected.insert((parent, entry_id, node.name.clone()), entry_attributes);
            validate_node(node, Some(parent_node))?;
        }
        let mut actual = BTreeMap::new();
        for (parent, node) in self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.directory)
        {
            validate_node(node, node.parent.and_then(|index| self.nodes.get(index)))?;
            for entry in enumerate_directory(node.handle.0)? {
                validate_directory_entry(&entry)?;
                actual.insert((parent, entry.id, entry.name), entry.attributes);
            }
        }
        if expected != actual {
            return Err(
                "P1:3 cache tree changed between preflight and same-handle re-enumeration".into(),
            );
        }
        Ok(())
    }

    fn delete_postorder(mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        for index in (0..self.nodes.len()).rev() {
            let (ancestors, current) = self.nodes.split_at_mut(index);
            let node = &mut current[0];
            if node.root {
                continue;
            }
            if let Err(error) =
                validate_node(node, node.parent.and_then(|parent| ancestors.get(parent)))
            {
                failures.push(format!("object {:?} revalidation: {error}", node.id));
                node.handle.close();
                continue;
            }
            let disposition = FILE_DISPOSITION_INFO_EX {
                Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                    FILE_DISPOSITION_FLAG_DELETE.0
                        | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                        | FILE_DISPOSITION_FLAG_FORCE_IMAGE_SECTION_CHECK.0
                        | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
                ),
            };
            let result = unsafe {
                SetFileInformationByHandle(
                    node.handle.0,
                    FileDispositionInfoEx,
                    (&raw const disposition).cast(),
                    u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>())
                        .map_err(|_| "P1:3 disposition size overflow")?,
                )
            };
            if let Err(error) = result {
                failures.push(format!("object {:?}: {error}", node.id));
            }
            // Release every delete-pending child before visiting its parent.
            // Configured roots remain retained and are never dispositioned.
            node.handle.close();
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "P1:3 handle deletion retained a pending audit after {} late failure(s): {}",
                failures.len(),
                failures.join("; ")
            ))
        }
    }

    fn delete_leaf_files<F>(&mut self, selected: F) -> Result<usize, String>
    where
        F: Fn(&[u16]) -> bool,
    {
        let mut failures = Vec::new();
        let mut deleted = 0_usize;
        for index in (0..self.nodes.len()).rev() {
            let (ancestors, current) = self.nodes.split_at_mut(index);
            let Some(node) = current.first_mut() else {
                continue;
            };
            if node.root || node.directory || !selected(&node.name) {
                continue;
            }
            let result = validate_node(node, node.parent.and_then(|parent| ancestors.get(parent)))
                .and_then(|_| set_delete_pending(node));
            if let Err(error) = result {
                failures.push(format!("object {:?}: {error}", node.id));
            } else {
                deleted = deleted.saturating_add(1);
            }
            node.handle.close();
        }
        if failures.is_empty() {
            Ok(deleted)
        } else {
            Err(format!(
                "prefetch leaf deletion failed: {}",
                failures.join("; ")
            ))
        }
    }
}

fn set_delete_pending(node: &Node) -> Result<(), String> {
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_FLAG_FORCE_IMAGE_SECTION_CHECK.0
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };
    unsafe {
        SetFileInformationByHandle(
            node.handle.0,
            FileDispositionInfoEx,
            (&raw const disposition).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>())
                .map_err(|_| "cleanup disposition size overflow")?,
        )
    }
    .map_err(|error| error.to_string())
}

#[derive(Debug)]
pub(crate) struct DirectoryEntry {
    pub(crate) id: i64,
    pub(crate) name: Vec<u16>,
    pub(crate) attributes: u32,
}

fn open_root(path: &Path) -> Result<Option<OwnedHandle>, String> {
    reject_remote_drive(path)?;
    let wide = wide_path(path)?;
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            DELETE.0 | FILE_READ_ATTRIBUTES.0 | FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    };
    match handle {
        Ok(handle) => Ok(Some(OwnedHandle(handle))),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(format!("open configured P1:3 cache root: {error}")),
    }
}

fn open_by_id(root: HANDLE, id: i64, directory: bool) -> Result<OwnedHandle, String> {
    let descriptor = FILE_ID_DESCRIPTOR {
        dwSize: u32::try_from(size_of::<FILE_ID_DESCRIPTOR>())
            .map_err(|_| "P1:3 file-id descriptor size overflow")?,
        Type: FileIdType,
        Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: id },
    };
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            Default::default()
        };
    unsafe {
        OpenFileById(
            root,
            &raw const descriptor,
            DELETE.0 | FILE_READ_ATTRIBUTES.0 | if directory { FILE_LIST_DIRECTORY.0 } else { 0 },
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            flags,
        )
        .map(OwnedHandle)
        .map_err(|error| format!("open P1:3 cache entry by file ID: {error}"))
    }
}

fn enumerate_directory(handle: HANDLE) -> Result<Vec<DirectoryEntry>, String> {
    let mut output = Vec::new();
    let mut restart = true;
    loop {
        let mut buffer = vec![0_u8; DIRECTORY_BUFFER];
        let class = if restart {
            FileIdBothDirectoryRestartInfo
        } else {
            FileIdBothDirectoryInfo
        };
        match unsafe {
            GetFileInformationByHandleEx(
                handle,
                class,
                buffer.as_mut_ptr().cast(),
                DIRECTORY_BUFFER as u32,
            )
        } {
            Ok(()) => {
                restart = false;
                parse_directory_buffer(&buffer, &mut output)?;
            }
            Err(error) if is_no_more_files(&error) => break,
            Err(error) => {
                return Err(format!(
                    "enumerate P1:3 cache directory by file ID: {error}"
                ));
            }
        }
    }
    Ok(output)
}

fn reject_remote_drive(path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    let root: Vec<u16> = path
        .chars()
        .take(3)
        .collect::<String>()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    if root.len() != 4 {
        return Err("P1:3 cache root is not an exact drive path".into());
    }
    if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } != DRIVE_FIXED {
        return Err("P1:3 cache root is not a fixed local drive".into());
    }
    Ok(())
}

fn wide_path(path: &Path) -> Result<Vec<u16>, String> {
    let value: Vec<u16> = path.as_os_str().encode_wide().collect();
    if value.is_empty() || value.len() > MAX_PATH_UTF16 || value.contains(&0) {
        return Err("P1:3 cache root has an invalid UTF-16 path".into());
    }
    Ok(value.into_iter().chain(Some(0)).collect())
}

fn root_final_path_matches_configured(final_path: &[u16], configured: &Path) -> Result<(), String> {
    let mut configured = wide_path(configured)?;
    let _ = configured.pop();
    if normalize_windows_dos_path(final_path)? != normalize_windows_dos_path(&configured)? {
        return Err("P1:3 configured root does not match its retained final path".into());
    }
    Ok(())
}

fn root_units(path: &Path) -> Result<usize, String> {
    let units = path.as_os_str().encode_wide().count();
    if units == 0 || units > MAX_PATH_UTF16 {
        return Err("P1:3 cache root exceeds the UTF-16 path bound".into());
    }
    Ok(units)
}

fn is_not_found(error: &windows::core::Error) -> bool {
    error.code().0 == -2147024894 // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
        || error.code().0 == -2147024893 // HRESULT_FROM_WIN32(ERROR_PATH_NOT_FOUND)
}

fn is_no_more_files(error: &windows::core::Error) -> bool {
    error.code().0 == -2147024878 // HRESULT_FROM_WIN32(ERROR_NO_MORE_FILES)
}
