use super::*;

#[derive(Debug)]
pub(crate) struct RetainedFile {
    pub(crate) handle: HANDLE,
    pub(crate) path: PathBuf,
    pub(crate) id: FILE_ID_INFO,
}
impl Drop for RetainedFile {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
