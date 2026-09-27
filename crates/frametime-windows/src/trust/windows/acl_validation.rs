const ACL_HEADER_BYTES: usize = 8;
const ACE_HEADER_BYTES: usize = 4;
const ACE_MASK_OFFSET: usize = 4;
const ACE_SID_OFFSET: usize = 8;
const SID_HEADER_BYTES: usize = 8;
const SID_REVISION: u8 = 1;
const SID_MAX_SUB_AUTHORITIES: u8 = 15;
const ACE_ALIGNMENT: usize = 4;
const SID_ALIGNMENT: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AceExtent {
    pub(crate) ace_type: u8,
    pub(crate) ace_flags: u8,
    pub(crate) mask: u32,
    pub(crate) sid_offset: usize,
}

pub(crate) fn acl_size_from_header(header: &[u8]) -> Result<usize, String> {
    Ok(usize::from(read_u16(header, 2, "ACL size")?))
}

pub(crate) fn validate_acl(acl: &[u8], base_address: usize) -> Result<Vec<AceExtent>, String> {
    if !base_address.is_multiple_of(ACE_ALIGNMENT) {
        return Err("suite root DACL base is misaligned".into());
    }
    if acl.len() < ACL_HEADER_BYTES {
        return Err("suite root DACL header is truncated".into());
    }
    if acl_size_from_header(acl)? != acl.len() {
        return Err("suite root DACL extent disagrees with AclSize".into());
    }
    let ace_count = usize::from(read_u16(acl, 4, "ACL ACE count")?);
    let mut next_ace = ACL_HEADER_BYTES;
    let mut aces = Vec::with_capacity(ace_count);
    for _ in 0..ace_count {
        if !next_ace.is_multiple_of(ACE_ALIGNMENT) {
            return Err("suite root DACL ACE is misaligned".into());
        }
        let header_end = next_ace
            .checked_add(ACE_HEADER_BYTES)
            .ok_or("suite root DACL ACE header extent overflows")?;
        if header_end > acl.len() {
            return Err("suite root DACL ACE header is truncated".into());
        }
        let ace_size = usize::from(read_u16(acl, next_ace + 2, "ACE size")?);
        let ace_end = next_ace
            .checked_add(ace_size)
            .ok_or("suite root DACL ACE extent overflows")?;
        if ace_size < ACE_SID_OFFSET + SID_HEADER_BYTES
            || !ace_size.is_multiple_of(ACE_ALIGNMENT)
            || ace_end > acl.len()
        {
            return Err("suite root DACL ACE extent is invalid".into());
        }
        let sid_offset = next_ace
            .checked_add(ACE_SID_OFFSET)
            .ok_or("suite root DACL SID offset overflows")?;
        let sid_address = base_address
            .checked_add(sid_offset)
            .ok_or("suite root DACL SID address overflows")?;
        if !sid_address.is_multiple_of(SID_ALIGNMENT) {
            return Err("suite root DACL SID is misaligned".into());
        }
        let sid_header_end = sid_offset
            .checked_add(SID_HEADER_BYTES)
            .ok_or("suite root DACL SID header extent overflows")?;
        if sid_header_end > ace_end {
            return Err("suite root DACL SID header is truncated".into());
        }
        if acl[sid_offset] != SID_REVISION {
            return Err("suite root DACL SID revision is invalid".into());
        }
        let sub_authorities = acl[sid_offset + 1];
        if sub_authorities > SID_MAX_SUB_AUTHORITIES {
            return Err("suite root DACL SID sub-authority count is invalid".into());
        }
        let sid_size = usize::from(sub_authorities)
            .checked_mul(std::mem::size_of::<u32>())
            .and_then(|tail| SID_HEADER_BYTES.checked_add(tail))
            .ok_or("suite root DACL SID extent overflows")?;
        let sid_end = sid_offset
            .checked_add(sid_size)
            .ok_or("suite root DACL SID extent overflows")?;
        if sid_end > ace_end {
            return Err("suite root DACL SID extent is truncated".into());
        }
        aces.push(AceExtent {
            ace_type: acl[next_ace],
            ace_flags: acl[next_ace + 1],
            mask: read_u32(acl, next_ace + ACE_MASK_OFFSET, "ACE mask")?,
            sid_offset,
        });
        next_ace = ace_end;
    }
    Ok(aces)
}

fn read_u16(bytes: &[u8], offset: usize, field: &str) -> Result<u16, String> {
    let end = offset
        .checked_add(std::mem::size_of::<u16>())
        .ok_or_else(|| format!("suite root DACL {field} extent overflows"))?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| format!("suite root DACL {field} is truncated"))?;
    Ok(u16::from_ne_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize, field: &str) -> Result<u32, String> {
    let end = offset
        .checked_add(std::mem::size_of::<u32>())
        .ok_or_else(|| format!("suite root DACL {field} extent overflows"))?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| format!("suite root DACL {field} is truncated"))?;
    Ok(u32::from_ne_bytes([value[0], value[1], value[2], value[3]]))
}

#[cfg(test)]
mod tests {
    use super::validate_acl;

    const BASE: usize = 0x1000;
    const ACE_BYTES: usize = 20;

    fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_ne_bytes());
    }

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }

    fn valid_acl() -> Vec<u8> {
        let mut acl = vec![0_u8; 8 + 2 * ACE_BYTES];
        let acl_size = u16::try_from(acl.len()).expect("fixture fits u16");
        acl[0] = 2;
        put_u16(&mut acl, 2, acl_size);
        put_u16(&mut acl, 4, 2);
        for ace_offset in [8, 8 + ACE_BYTES] {
            acl[ace_offset] = 0;
            acl[ace_offset + 1] = 3;
            put_u16(
                &mut acl,
                ace_offset + 2,
                u16::try_from(ACE_BYTES).expect("fixture fits u16"),
            );
            put_u32(&mut acl, ace_offset + 4, 0x001F_01FF);
            acl[ace_offset + 8] = 1;
            acl[ace_offset + 9] = 1;
        }
        acl
    }

    #[test]
    fn acl_extent_accepts_exactly_contained_aces() {
        let acl = valid_acl();
        let aces = validate_acl(&acl, BASE).expect("valid ACL extent");
        assert_eq!(aces.len(), 2);
        assert_eq!(aces[1].sid_offset, 8 + ACE_BYTES + 8);
    }

    #[test]
    fn acl_extent_rejects_declared_size_outside_buffer() {
        let mut acl = valid_acl();
        let mismatched_size = u16::try_from(acl.len() + 4).expect("fixture fits u16");
        put_u16(&mut acl, 2, mismatched_size);
        assert_eq!(
            validate_acl(&acl, BASE),
            Err("suite root DACL extent disagrees with AclSize".into())
        );
    }

    #[test]
    fn acl_extent_rejects_ace_and_sid_overruns() {
        let mut ace_overrun = valid_acl();
        put_u16(&mut ace_overrun, 10, u16::MAX);
        assert_eq!(
            validate_acl(&ace_overrun, BASE),
            Err("suite root DACL ACE extent is invalid".into())
        );
        let mut sid_overrun = valid_acl();
        sid_overrun[16 + 1] = 15;
        assert_eq!(
            validate_acl(&sid_overrun, BASE),
            Err("suite root DACL SID extent is truncated".into())
        );
    }

    #[test]
    fn acl_extent_rejects_misaligned_sid_address() {
        let acl = valid_acl();
        assert_eq!(
            validate_acl(&acl, BASE + 2),
            Err("suite root DACL base is misaligned".into())
        );
    }
}
