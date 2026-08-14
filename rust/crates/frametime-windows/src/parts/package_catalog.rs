use windows::{
    Win32::{
        Foundation::HANDLE,
        Security::Cryptography::Catalog::{
            CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2,
            CryptCATAdminReleaseContext,
        },
    },
    core::PCWSTR,
};

pub(super) struct CatalogContext {
    pub(super) handle: isize,
}

impl CatalogContext {
    pub(super) fn acquire() -> Result<Self, String> {
        let mut handle = 0_isize;
        let algorithm: Vec<_> = "SHA256".encode_utf16().chain(Some(0)).collect();
        unsafe {
            CryptCATAdminAcquireContext2(
                &mut handle,
                None,
                PCWSTR(algorithm.as_ptr()),
                None,
                Some(0),
            )
        }
        .map_err(|error| format!("acquire catalog context: {error}"))?;
        Ok(Self { handle })
    }

    pub(super) fn hash(&self, file: HANDLE) -> Result<Vec<u8>, String> {
        let mut length = 0;
        unsafe {
            CryptCATAdminCalcHashFromFileHandle2(
                self.handle,
                file,
                &mut length,
                None,
                Some(0),
            )
        }
        .map_err(|error| format!("measure catalog member hash: {error}"))?;
        let mut hash = vec![0; length as usize];
        unsafe {
            CryptCATAdminCalcHashFromFileHandle2(
                self.handle,
                file,
                &mut length,
                Some(hash.as_mut_ptr()),
                Some(0),
            )
        }
        .map_err(|error| format!("calculate catalog member hash: {error}"))?;
        hash.truncate(length as usize);
        Ok(hash)
    }
}

impl Drop for CatalogContext {
    fn drop(&mut self) {
        unsafe {
            let _ = CryptCATAdminReleaseContext(self.handle, 0);
        }
    }
}
