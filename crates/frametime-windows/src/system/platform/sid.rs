#[cfg(any(windows, test))]
pub(crate) const SID_HEADER_BYTES: usize = 8;
#[cfg(any(windows, test))]
pub(crate) const SID_REVISION: u8 = 1;
#[cfg(any(windows, test))]
pub(crate) const SID_MAX_SUB_AUTHORITIES: u8 = 15;
#[cfg(windows)]
pub(crate) const SID_MAX_BYTES: usize = SID_HEADER_BYTES + SID_MAX_SUB_AUTHORITIES as usize * 4;

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SidExtent {
    pub(crate) offset: usize,
    pub(crate) length: usize,
}

#[cfg(any(windows, test))]
pub(crate) struct TokenUserLayout {
    pub(crate) base_address: usize,
    pub(crate) capacity: usize,
    pub(crate) used: usize,
    pub(crate) token_user_size: usize,
    pub(crate) token_user_alignment: usize,
    pub(crate) sid_buffer_address: usize,
    pub(crate) sid_address: usize,
}

#[cfg(any(windows, test))]
pub(crate) fn validate_token_user_header_extent(
    base_address: usize,
    capacity: usize,
    used: usize,
    token_user_size: usize,
    token_user_alignment: usize,
) -> Result<(), String> {
    if token_user_alignment == 0 || !token_user_alignment.is_power_of_two() {
        return Err("TokenUser alignment is invalid".into());
    }
    if !base_address.is_multiple_of(token_user_alignment) {
        return Err("TokenUser buffer is misaligned".into());
    }
    if used > capacity {
        return Err("TokenUser returned extent exceeds its buffer".into());
    }
    if used < token_user_size {
        return Err("TokenUser returned extent is truncated".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
pub(crate) fn validate_token_user_sid_extent(
    sid_buffer: &[u8],
    layout: TokenUserLayout,
) -> Result<SidExtent, String> {
    validate_token_user_header_extent(
        layout.base_address,
        layout.capacity,
        layout.used,
        layout.token_user_size,
        layout.token_user_alignment,
    )?;

    let returned_end = layout
        .base_address
        .checked_add(layout.used)
        .ok_or("TokenUser returned extent overflows its address space")?;
    let sid_offset = layout
        .sid_address
        .checked_sub(layout.base_address)
        .ok_or("TokenUser SID pointer is outside its returned extent")?;
    if sid_offset < layout.token_user_size || layout.sid_address >= returned_end {
        return Err("TokenUser SID pointer is outside its returned extent".into());
    }
    let sid_buffer_offset = layout
        .sid_address
        .checked_sub(layout.sid_buffer_address)
        .ok_or("TokenUser SID pointer is outside its owned SID storage")?;

    let header_end = sid_offset
        .checked_add(SID_HEADER_BYTES)
        .ok_or("TokenUser SID header extent overflows")?;
    let sid_buffer_header_end = sid_buffer_offset
        .checked_add(SID_HEADER_BYTES)
        .ok_or("TokenUser SID header extent overflows")?;
    if header_end > layout.used || sid_buffer_header_end > sid_buffer.len() {
        return Err("TokenUser SID header is truncated".into());
    }
    if sid_buffer[sid_buffer_offset] != SID_REVISION {
        return Err("TokenUser SID revision is invalid".into());
    }
    let sub_authorities = sid_buffer[sid_buffer_offset + 1];
    if sub_authorities > SID_MAX_SUB_AUTHORITIES {
        return Err("TokenUser SID sub-authority count is invalid".into());
    }
    let length = usize::from(sub_authorities)
        .checked_mul(std::mem::size_of::<u32>())
        .and_then(|bytes| SID_HEADER_BYTES.checked_add(bytes))
        .ok_or("TokenUser SID length overflows")?;
    let sid_end = sid_offset
        .checked_add(length)
        .ok_or("TokenUser SID extent overflows")?;
    let sid_buffer_end = sid_buffer_offset
        .checked_add(length)
        .ok_or("TokenUser SID extent overflows")?;
    if sid_end > layout.used || sid_buffer_end > sid_buffer.len() {
        return Err("TokenUser SID extent is truncated".into());
    }

    Ok(SidExtent {
        offset: sid_offset,
        length,
    })
}

#[cfg(windows)]
#[repr(C)]
pub(crate) struct TokenUserBuffer {
    pub(crate) user: windows::Win32::Security::TOKEN_USER,
    pub(crate) sid: [u8; SID_MAX_BYTES],
}

#[cfg(windows)]
pub(crate) fn current_token_user_sid() -> Result<String, String> {
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HLOCAL, LocalFree},
            Security::Authorization::ConvertSidToStringSidW,
            Security::{
                GetLengthSid, GetTokenInformation, IsValidSid, TOKEN_QUERY, TOKEN_USER, TokenUser,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
        core::PWSTR,
    };

    let mut token = Default::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|error| format!("open current process token: {error}"))?;
    struct TokenGuard(windows::Win32::Foundation::HANDLE);
    impl Drop for TokenGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let token = TokenGuard(token);
    let mut buffer = TokenUserBuffer {
        user: TOKEN_USER::default(),
        sid: [0; SID_MAX_BYTES],
    };
    let capacity = std::mem::size_of::<TokenUserBuffer>();
    let mut used = 0_u32;
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(std::ptr::from_mut(&mut buffer).cast()),
            u32::try_from(capacity).map_err(|_| "token buffer length exceeds u32")?,
            &mut used,
        )
    }
    .map_err(|error| format!("read current TokenUser: {error}"))?;
    let used = usize::try_from(used).map_err(|_| "TokenUser returned extent exceeds usize")?;
    let base_address = std::ptr::from_ref(&buffer).addr();
    let sid_buffer_address = buffer.sid.as_ptr().addr();
    validate_token_user_header_extent(
        base_address,
        capacity,
        used,
        std::mem::size_of::<TOKEN_USER>(),
        std::mem::align_of::<TOKEN_USER>(),
    )?;
    let user = buffer.user;
    let sid_extent = validate_token_user_sid_extent(
        &buffer.sid,
        TokenUserLayout {
            base_address,
            capacity,
            used,
            token_user_size: std::mem::size_of::<TOKEN_USER>(),
            token_user_alignment: std::mem::align_of::<TOKEN_USER>(),
            sid_buffer_address,
            sid_address: user.User.Sid.0.addr(),
        },
    )?;
    // SAFETY: the portable extent validation proves that the SID's header and
    // declared sub-authorities are contained in the returned TokenUser bytes.
    let sid_length = unsafe {
        if !IsValidSid(user.User.Sid).as_bool() {
            return Err("TokenUser SID is invalid".into());
        }
        usize::try_from(GetLengthSid(user.User.Sid))
            .map_err(|_| "TokenUser SID length exceeds usize")?
    };
    if sid_length != sid_extent.length {
        return Err("TokenUser SID length disagrees with its returned extent".into());
    }
    let mut sid = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) }
        .map_err(|error| format!("render current TokenUser SID: {error}"))?;
    struct SidGuard(PWSTR);
    impl Drop for SidGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.0.cast())));
            }
        }
    }
    let sid = SidGuard(sid);
    let length = unsafe { sid.0.len() };
    let units = unsafe { std::slice::from_raw_parts(sid.0.0, length) };
    String::from_utf16(units).map_err(|error| format!("decode TokenUser SID: {error}"))
}

#[cfg(not(windows))]
pub(crate) fn current_token_user_sid() -> Result<String, String> {
    Err("current TokenUser SID is supported only on Windows".into())
}

#[cfg(test)]
mod token_user_sid_tests {
    use super::{SID_HEADER_BYTES, SidExtent, TokenUserLayout, validate_token_user_sid_extent};

    const BASE: usize = 0x1000;
    const TOKEN_USER_SIZE: usize = 16;
    const TOKEN_USER_ALIGNMENT: usize = 8;

    pub(crate) fn valid_buffer(sub_authorities: u8) -> (Vec<u8>, usize) {
        let sid_length = SID_HEADER_BYTES + usize::from(sub_authorities) * 4;
        let mut buffer = vec![0_u8; TOKEN_USER_SIZE + sid_length];
        buffer[TOKEN_USER_SIZE] = 1;
        buffer[TOKEN_USER_SIZE + 1] = sub_authorities;
        (buffer, sid_length)
    }

    pub(crate) fn layout(
        base_address: usize,
        capacity: usize,
        used: usize,
        sid_address: usize,
    ) -> TokenUserLayout {
        TokenUserLayout {
            base_address,
            capacity,
            used,
            token_user_size: TOKEN_USER_SIZE,
            token_user_alignment: TOKEN_USER_ALIGNMENT,
            sid_buffer_address: base_address + TOKEN_USER_SIZE,
            sid_address,
        }
    }

    #[test]
    pub(crate) fn rejects_truncated_token_user_extent() {
        let buffer = [0_u8; TOKEN_USER_SIZE];
        let result = validate_token_user_sid_extent(
            &buffer[TOKEN_USER_SIZE..],
            layout(
                BASE,
                buffer.len(),
                TOKEN_USER_SIZE - 1,
                BASE + TOKEN_USER_SIZE,
            ),
        );
        assert_eq!(
            result.unwrap_err(),
            "TokenUser returned extent is truncated"
        );
    }

    #[test]
    pub(crate) fn rejects_misaligned_token_user_buffer() {
        let (buffer, _) = valid_buffer(1);
        let result = validate_token_user_sid_extent(
            &buffer[TOKEN_USER_SIZE..],
            layout(
                BASE + 1,
                buffer.len(),
                buffer.len(),
                BASE + 1 + TOKEN_USER_SIZE,
            ),
        );
        assert_eq!(result.unwrap_err(), "TokenUser buffer is misaligned");
    }

    #[test]
    pub(crate) fn rejects_sid_pointer_outside_returned_extent() {
        let (buffer, _) = valid_buffer(1);
        let result = validate_token_user_sid_extent(
            &buffer[TOKEN_USER_SIZE..],
            layout(BASE, buffer.len(), buffer.len(), BASE + buffer.len()),
        );
        assert_eq!(
            result.unwrap_err(),
            "TokenUser SID pointer is outside its returned extent"
        );
    }

    #[test]
    pub(crate) fn rejects_invalid_sid_structure_and_length() {
        let (mut invalid_revision, _) = valid_buffer(1);
        invalid_revision[TOKEN_USER_SIZE] = 2;
        assert_eq!(
            validate_token_user_sid_extent(
                &invalid_revision[TOKEN_USER_SIZE..],
                layout(
                    BASE,
                    invalid_revision.len(),
                    invalid_revision.len(),
                    BASE + TOKEN_USER_SIZE,
                ),
            )
            .unwrap_err(),
            "TokenUser SID revision is invalid"
        );

        let (mut truncated, _) = valid_buffer(1);
        truncated[TOKEN_USER_SIZE + 1] = 2;
        assert_eq!(
            validate_token_user_sid_extent(
                &truncated[TOKEN_USER_SIZE..],
                layout(
                    BASE,
                    truncated.len(),
                    truncated.len(),
                    BASE + TOKEN_USER_SIZE,
                ),
            )
            .unwrap_err(),
            "TokenUser SID extent is truncated"
        );
    }

    #[test]
    pub(crate) fn accepts_sid_ending_at_exact_returned_boundary() {
        let (buffer, sid_length) = valid_buffer(2);
        assert_eq!(
            validate_token_user_sid_extent(
                &buffer[TOKEN_USER_SIZE..],
                layout(BASE, buffer.len(), buffer.len(), BASE + TOKEN_USER_SIZE,),
            )
            .unwrap(),
            SidExtent {
                offset: TOKEN_USER_SIZE,
                length: sid_length,
            }
        );
    }
}
