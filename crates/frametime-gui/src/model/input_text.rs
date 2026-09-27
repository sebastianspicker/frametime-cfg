#[derive(Default)]
pub struct VprofInputState {
    rejected: bool,
}
impl VprofInputState {
    pub fn reject_incomplete(&mut self) {
        self.rejected = true;
    }
    pub fn edited(&mut self, is_empty: bool) {
        if is_empty {
            self.rejected = false;
        }
    }
    pub fn is_rejected(&self) -> bool {
        self.rejected
    }
}

/// Validate the decoded size before allocating UTF-8 control text.
pub fn decode_bounded_control_text(units: &[u16], max_bytes: usize) -> Result<String, String> {
    let mut bytes = 0_usize;
    for character in char::decode_utf16(units.iter().copied()) {
        let character = character.map_err(|_| "Input is not valid Unicode")?;
        bytes = bytes
            .checked_add(character.len_utf8())
            .ok_or("Input length overflow")?;
        if bytes > max_bytes {
            return Err("Input exceeds the permitted UTF-8 byte limit".into());
        }
    }
    String::from_utf16(units).map_err(|_| "Input is not valid Unicode".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_native_limit_event_requires_a_complete_replacement() {
        let mut state = VprofInputState::default();
        state.reject_incomplete();
        state.edited(false);
        assert!(state.is_rejected());
        state.edited(true);
        assert!(!state.is_rejected());
    }
    #[test]
    fn decoded_size_is_checked_in_bytes_without_truncation() {
        let units = "a€😀".encode_utf16().collect::<Vec<_>>();
        assert_eq!(decode_bounded_control_text(&units, 8).unwrap(), "a€😀");
        assert!(decode_bounded_control_text(&units, 7).is_err());
        assert!(decode_bounded_control_text(&[0xd800], 8).is_err());
        assert_eq!(decode_bounded_control_text(&[], 0).unwrap(), "");
    }
}
