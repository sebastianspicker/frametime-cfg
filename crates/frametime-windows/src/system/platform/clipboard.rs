#[cfg(windows)]
pub(crate) mod clipboard {
    use std::{ptr, slice};

    use frametime_domain::fps::MAX_VPROF_INPUT_BYTES;

    use windows::Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        },
    };

    const CF_UNICODETEXT: u32 = 13;
    const MAX_VPROF_CLIPBOARD_UTF16_BYTES: usize = MAX_VPROF_INPUT_BYTES * 2 + 2;

    struct ClipboardGuard;
    impl ClipboardGuard {
        fn open() -> Result<Self, String> {
            unsafe { OpenClipboard(None) }.map_err(|error| format!("open clipboard: {error}"))?;
            Ok(Self)
        }
    }
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    pub(crate) fn write(text: &str) -> Result<(), String> {
        let _clipboard = ClipboardGuard::open()?;
        let utf16 = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let bytes = utf16
            .len()
            .checked_mul(2)
            .ok_or("clipboard text is too large")?;
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }
            .map_err(|error| format!("allocate clipboard text: {error}"))?;
        let address = unsafe { GlobalLock(memory) };
        if address.is_null() {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            return Err("lock clipboard allocation failed".into());
        }
        unsafe {
            ptr::copy_nonoverlapping(utf16.as_ptr().cast::<u8>(), address.cast::<u8>(), bytes);
        }
        unsafe {
            let _ = GlobalUnlock(memory);
        }
        unsafe { EmptyClipboard() }.map_err(|error| {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            format!("empty clipboard: {error}")
        })?;
        if let Err(error) = unsafe { SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))) } {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            return Err(format!("set clipboard text: {error}"));
        }
        Ok(())
    }

    pub(crate) fn read() -> Result<String, String> {
        let _clipboard = ClipboardGuard::open()?;
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) }
            .map_err(|error| format!("get clipboard text: {error}"))?;
        let memory = HGLOBAL(handle.0);
        let bytes = unsafe { GlobalSize(memory) };
        if bytes == 0 || bytes % 2 != 0 {
            return Err("clipboard Unicode payload has invalid size".into());
        }
        if bytes > MAX_VPROF_CLIPBOARD_UTF16_BYTES {
            return Err("clipboard Unicode payload exceeds the bounded VProf input limit".into());
        }
        let address = unsafe { GlobalLock(memory) };
        if address.is_null() {
            return Err("lock clipboard text failed".into());
        }
        let units = unsafe { slice::from_raw_parts(address.cast::<u16>(), bytes / 2) };
        let result = match units.iter().position(|value| *value == 0) {
            Some(end) => bounded_utf16_to_string(&units[..end]),
            None => Err("clipboard Unicode payload is unterminated".into()),
        };
        unsafe {
            let _ = GlobalUnlock(memory);
        }
        result
    }

    fn bounded_utf16_to_string(units: &[u16]) -> Result<String, String> {
        let mut utf8_bytes = 0_usize;
        for decoded in char::decode_utf16(units.iter().copied()) {
            let character = decoded.map_err(|error| error.to_string())?;
            utf8_bytes = utf8_bytes
                .checked_add(character.len_utf8())
                .ok_or("clipboard UTF-8 size overflow")?;
            if utf8_bytes > MAX_VPROF_INPUT_BYTES {
                return Err("clipboard VProf text exceeds the 8 MiB UTF-8 limit".into());
            }
        }
        let mut text = String::with_capacity(utf8_bytes);
        for decoded in char::decode_utf16(units.iter().copied()) {
            text.push(decoded.map_err(|error| error.to_string())?);
        }
        Ok(text)
    }
}

#[cfg(not(windows))]
pub(crate) mod clipboard {
    pub(crate) fn write(_: &str) -> Result<(), String> {
        Err("the native clipboard is supported only on Windows".into())
    }
    pub(crate) fn read() -> Result<String, String> {
        Err("the native clipboard is supported only on Windows".into())
    }
}
