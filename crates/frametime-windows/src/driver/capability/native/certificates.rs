use super::*;

pub(super) fn signer_from_verified_state(
    state: HANDLE,
) -> Result<(String, String), AdapterFailure> {
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return Err(adapter(
            "verify NVIDIA artifact",
            "WinVerifyTrust returned no provider state",
        ));
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, false, 0) };
    if signer.is_null() {
        return Err(adapter(
            "verify NVIDIA artifact",
            "WinVerifyTrust returned no leaf signer",
        ));
    }
    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if certificate.is_null() || unsafe { (*certificate).pCert.is_null() } {
        return Err(adapter(
            "verify NVIDIA artifact",
            "WinVerifyTrust returned no leaf certificate",
        ));
    }
    let context = unsafe { (*certificate).pCert };
    Ok((
        format!("CN={}", certificate_subject(context)?),
        certificate_sha256(context)?,
    ))
}

pub(super) fn certificate_subject(context: *const CERT_CONTEXT) -> Result<String, AdapterFailure> {
    certificate_subject_from_getter(|buffer| unsafe {
        CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, buffer)
    })
}

fn certificate_subject_from_getter(
    mut get_name: impl FnMut(Option<&mut [u16]>) -> u32,
) -> Result<String, AdapterFailure> {
    let needed = get_name(None);
    if needed <= 1 {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf signer subject is absent",
        ));
    }
    let unit_count = usize::try_from(needed).map_err(|_| {
        adapter(
            "verify NVIDIA artifact",
            "subject length exceeds address space",
        )
    })?;
    let mut units = vec![0_u16; unit_count];
    let written = get_name(Some(&mut units));
    if written != needed || written <= 1 {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf signer subject length changed while being read",
        ));
    }
    if units.last() != Some(&0) {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf signer subject is not NUL-terminated",
        ));
    }
    String::from_utf16(&units[..units.len() - 1])
        .map_err(|error| adapter("verify NVIDIA artifact", error.to_string()))
}

pub(super) fn certificate_sha256(context: *const CERT_CONTEXT) -> Result<String, AdapterFailure> {
    let certificate = unsafe { &*context };
    let encoded = certificate_encoded_bytes(certificate)?;
    let mut length = 32;
    let mut bytes = [0_u8; 32];
    unsafe {
        CryptHashCertificate(
            None,
            CALG_SHA_256,
            0,
            encoded,
            Some(bytes.as_mut_ptr()),
            &mut length,
        )
    }
    .map_err(|error| adapter("verify NVIDIA artifact", error.to_string()))?;
    if length != 32 {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf certificate SHA-256 length is invalid",
        ));
    }
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn certificate_encoded_bytes(certificate: &CERT_CONTEXT) -> Result<&[u8], AdapterFailure> {
    if certificate.cbCertEncoded == 0 {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf certificate has no encoded bytes",
        ));
    }
    if certificate.pbCertEncoded.is_null() {
        return Err(adapter(
            "verify NVIDIA artifact",
            "leaf certificate encoded bytes are null",
        ));
    }
    let length = usize::try_from(certificate.cbCertEncoded)
        .map_err(|_| adapter("verify NVIDIA artifact", "leaf certificate length overflow"))?;
    Ok(unsafe { std::slice::from_raw_parts(certificate.pbCertEncoded, length) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_bytes_reject_zero_length_contexts_before_reading_the_pointer() {
        let mut byte = 0_u8;
        let certificate = CERT_CONTEXT {
            pbCertEncoded: &raw mut byte,
            ..Default::default()
        };
        let error = certificate_encoded_bytes(&certificate).expect_err("zero length is invalid");
        assert_eq!(error.reason, "leaf certificate has no encoded bytes");
    }

    #[test]
    fn certificate_bytes_reject_null_pointers_before_constructing_a_slice() {
        let certificate = CERT_CONTEXT {
            cbCertEncoded: 1,
            ..Default::default()
        };
        let error = certificate_encoded_bytes(&certificate).expect_err("null pointer is invalid");
        assert_eq!(error.reason, "leaf certificate encoded bytes are null");
    }

    #[test]
    fn certificate_subject_rejects_changed_or_unterminated_lengths() {
        let error = certificate_subject_from_getter(|buffer| {
            if let Some(units) = buffer {
                units.copy_from_slice(&[u16::from(b'A'), 0, 0]);
                2
            } else {
                3
            }
        })
        .expect_err("lengths must agree");
        assert_eq!(
            error.reason,
            "leaf signer subject length changed while being read"
        );
    }
}
