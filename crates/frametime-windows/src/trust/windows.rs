//! Fixed-root trust boundary.

#[cfg(any(windows, test))]
mod acl_validation;
#[cfg(windows)]
pub(crate) mod trusted_work_dir;
