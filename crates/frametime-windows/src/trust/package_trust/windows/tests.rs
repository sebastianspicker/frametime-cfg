use super::*;

#[cfg(test)]
mod fixtures {
    use super::*;

    pub(crate) fn algorithm(oid: &[u8]) -> CRYPT_ALGORITHM_IDENTIFIER {
        let mut algorithm = CRYPT_ALGORITHM_IDENTIFIER::default();
        algorithm.pszObjId.0 = oid.as_ptr().cast_mut();
        algorithm
    }

    #[test]
    pub(crate) fn spki_encoding_rejects_null_oid_and_parameter_pointer() {
        let mut null_oid = CRYPT_ALGORITHM_IDENTIFIER::default();
        assert_eq!(
            encode_spki(&null_oid, &CRYPT_BIT_BLOB::default()),
            Err("signer SPKI OID is null".into())
        );

        let oid = b"1.2.840.113549.1.1.1\0";
        null_oid = algorithm(oid);
        null_oid.Parameters.cbData = 1;
        assert_eq!(
            encode_spki(&null_oid, &CRYPT_BIT_BLOB::default()),
            Err("signer SPKI parameters is null".into())
        );
    }

    #[test]
    pub(crate) fn spki_encoding_rejects_malformed_and_oversized_components() {
        assert!(der_oid("2.18446744073709551615").is_err());
        assert!(der_oid("1.2.3.4.5.6.7.8.9.10.11.12.13.14.15.16.17.18.19.20.21.22.23.24.25.26.27.28.29.30.31.32").is_err());

        let oid = b"1.2.840.113549.1.1.1\0";
        let mut oversized_parameters_algorithm = algorithm(oid);
        let mut params = vec![0_u8; MAX_SPKI_PARAMETERS_BYTES + 1];
        oversized_parameters_algorithm.Parameters.pbData = params.as_mut_ptr();
        oversized_parameters_algorithm.Parameters.cbData =
            u32::try_from(params.len()).expect("fixture length fits u32");
        assert_eq!(
            encode_spki(&oversized_parameters_algorithm, &CRYPT_BIT_BLOB::default()),
            Err("signer SPKI parameters exceeds bounded size".into())
        );

        let mut key = vec![0_u8; MAX_SPKI_PUBLIC_KEY_BYTES + 1];
        let oversized_key = CRYPT_BIT_BLOB {
            cbData: u32::try_from(key.len()).expect("fixture length fits u32"),
            pbData: key.as_mut_ptr(),
            ..Default::default()
        };
        assert_eq!(
            encode_spki(&algorithm(oid), &oversized_key),
            Err("signer SPKI key exceeds bounded size".into())
        );
    }

    #[test]
    pub(crate) fn spki_encoding_rejects_invalid_unused_bits() {
        let oid = b"1.2.840.113549.1.1.1\0";
        let key = CRYPT_BIT_BLOB {
            cUnusedBits: 8,
            ..Default::default()
        };
        assert_eq!(
            encode_spki(&algorithm(oid), &key),
            Err("signer SPKI key has invalid unused bits".into())
        );
    }

    #[test]
    pub(crate) fn spki_encoding_keeps_canonical_pinned_publisher_input() {
        let oid = b"1.2.840.113549.1.1.1\0";
        let params = [0x05, 0x00];
        let mut algorithm = algorithm(oid);
        algorithm.Parameters.pbData = params.as_ptr().cast_mut();
        algorithm.Parameters.cbData = u32::try_from(params.len()).expect("fixture length fits u32");
        let key_bytes = [0x01, 0x02, 0x03];
        let key = CRYPT_BIT_BLOB {
            cbData: u32::try_from(key_bytes.len()).expect("fixture length fits u32"),
            pbData: key_bytes.as_ptr().cast_mut(),
            ..Default::default()
        };

        assert_eq!(
            encode_spki(&algorithm, &key),
            Ok(vec![
                0x30, 0x15, 0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01,
                0x01, 0x05, 0x00, 0x03, 0x04, 0x00, 0x01, 0x02, 0x03,
            ])
        );
    }
}
