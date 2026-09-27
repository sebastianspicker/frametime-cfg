use super::*;

pub(crate) fn spki_sha256(context: *const CERT_CONTEXT) -> Result<String, String> {
    let info = unsafe { (*context).pCertInfo.as_ref() }
        .ok_or("signer certificate lacks certificate information")?;
    let encoded = encode_spki(
        &info.SubjectPublicKeyInfo.Algorithm,
        &info.SubjectPublicKeyInfo.PublicKey,
    )?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}
pub(crate) fn encode_spki(
    algorithm: &CRYPT_ALGORITHM_IDENTIFIER,
    key: &CRYPT_BIT_BLOB,
) -> Result<Vec<u8>, String> {
    if algorithm.pszObjId.0.is_null() {
        return Err("signer SPKI OID is null".into());
    }
    let oid = unsafe { CStr::from_ptr(algorithm.pszObjId.0.cast()) }
        .to_str()
        .map_err(|_| "signer SPKI OID is invalid")?;
    if oid.len() > MAX_SPKI_OID_BYTES {
        return Err("signer SPKI OID exceeds bounded size".into());
    }
    let mut algorithm_der = der_oid(oid)?;
    let params = checked_bytes(
        algorithm.Parameters.pbData,
        algorithm.Parameters.cbData,
        MAX_SPKI_PARAMETERS_BYTES,
        "signer SPKI parameters",
    )?;
    checked_extend(&mut algorithm_der, &params, "signer SPKI algorithm")?;
    let algorithm_der = der(0x30, &algorithm_der)?;
    let key_bytes = checked_bytes(
        key.pbData,
        key.cbData,
        MAX_SPKI_PUBLIC_KEY_BYTES,
        "signer SPKI key",
    )?;
    if key.cUnusedBits > 7 {
        return Err("signer SPKI key has invalid unused bits".into());
    }
    let unused_bits =
        u8::try_from(key.cUnusedBits).map_err(|_| "signer SPKI key has invalid unused bits")?;
    let mut bits = vec![unused_bits];
    checked_extend(&mut bits, &key_bytes, "signer SPKI key")?;
    let mut content = algorithm_der;
    checked_extend(&mut content, &der(0x03, &bits)?, "signer SPKI")?;
    if content.len() > MAX_SPKI_DER_BYTES {
        return Err("signer SPKI exceeds bounded size".into());
    }
    der(0x30, &content)
}
pub(crate) fn checked_bytes(
    pointer: *const u8,
    length: u32,
    maximum: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let length = usize::try_from(length).map_err(|_| format!("{label} is too large"))?;
    if length > maximum {
        return Err(format!("{label} exceeds bounded size"));
    }
    if length == 0 {
        Ok(Vec::new())
    } else if pointer.is_null() {
        Err(format!("{label} is null"))
    } else {
        Ok(unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec())
    }
}
pub(crate) fn der_oid(oid: &str) -> Result<Vec<u8>, String> {
    let mut values = oid.split('.');
    let first = values
        .next()
        .ok_or("signer SPKI OID is invalid")?
        .parse::<u64>()
        .map_err(|_| "signer SPKI OID is invalid")?;
    let second = values
        .next()
        .ok_or("signer SPKI OID is invalid")?
        .parse::<u64>()
        .map_err(|_| "signer SPKI OID is invalid")?;
    if first > 2 || (first < 2 && second > 39) {
        return Err("signer SPKI OID is invalid".into());
    }
    let first_value = first
        .checked_mul(40)
        .and_then(|value| value.checked_add(second))
        .ok_or("signer SPKI OID is invalid")?;
    let mut body = base128(first_value)?;
    let mut components: usize = 2;
    for value in values {
        components = components
            .checked_add(1)
            .ok_or("signer SPKI OID is invalid")?;
        if components > MAX_SPKI_OID_COMPONENTS {
            return Err("signer SPKI OID exceeds bounded size".into());
        }
        let value = value
            .parse::<u64>()
            .map_err(|_| "signer SPKI OID is invalid")?;
        checked_extend(&mut body, &base128(value)?, "signer SPKI OID")?;
    }
    if body.len() > MAX_SPKI_OID_BYTES {
        return Err("signer SPKI OID exceeds bounded size".into());
    }
    der(0x06, &body)
}
pub(crate) fn base128(mut value: u64) -> Result<Vec<u8>, String> {
    let mut out = vec![u8::try_from(value & 127).map_err(|_| "signer SPKI OID is invalid")?];
    value >>= 7;
    while value > 0 {
        let byte = u8::try_from(value & 127).map_err(|_| "signer SPKI OID is invalid")?;
        out.push(byte | 128);
        value >>= 7;
    }
    out.reverse();
    Ok(out)
}
pub(crate) fn checked_extend(
    target: &mut Vec<u8>,
    bytes: &[u8],
    label: &str,
) -> Result<(), String> {
    target
        .try_reserve(bytes.len())
        .map_err(|_| format!("{label} exceeds bounded size"))?;
    target.extend_from_slice(bytes);
    Ok(())
}
pub(crate) fn der(tag: u8, body: &[u8]) -> Result<Vec<u8>, String> {
    let length_bytes = if body.len() < 128 {
        1
    } else {
        let mut length = body.len();
        let mut bytes = 0_usize;
        while length > 0 {
            bytes = bytes
                .checked_add(1)
                .ok_or("signer SPKI DER length overflows")?;
            length >>= 8;
        }
        1_usize
            .checked_add(bytes)
            .ok_or("signer SPKI DER length overflows")?
    };
    let capacity = 1_usize
        .checked_add(length_bytes)
        .and_then(|value| value.checked_add(body.len()))
        .ok_or("signer SPKI DER length overflows")?;
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| "signer SPKI DER exceeds bounded size")?;
    out.push(tag);
    if body.len() < 128 {
        out.push(u8::try_from(body.len()).map_err(|_| "signer SPKI DER length overflows")?);
    } else {
        let mut length = body.len();
        let mut bytes = Vec::new();
        while length > 0 {
            bytes.push(u8::try_from(length & 255).map_err(|_| "signer SPKI DER length overflows")?);
            length >>= 8;
        }
        bytes.reverse();
        let length_of_length =
            u8::try_from(bytes.len()).map_err(|_| "signer SPKI DER length overflows")?;
        out.push(128 | length_of_length);
        out.extend(bytes);
    }
    out.extend(body);
    Ok(out)
}
