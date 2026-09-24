use super::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn standard_digest_hmac_and_pbkdf2_vectors() {
    assert_eq!(
        hex(&cng::hash("SHA-256", None, b"abc").unwrap()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&cng::hash("SHA-1", None, b"abc").unwrap()),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(&cng::hash(
            "SHA-256",
            Some(b"key"),
            b"The quick brown fox jumps over the lazy dog"
        )
        .unwrap()),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(
        hex(&cng::pbkdf2("SHA-1", b"password", b"salt", 1, 20).unwrap()),
        "0c60c80f961f0e71f3a9b524af6012062fe037a6"
    );
}

#[test]
fn aes_gcm_authenticates_and_rejects_modified_data() {
    let key = [0; 16];
    let nonce = [0; 12];
    let plaintext = [0; 16];
    let encrypted = cng::aes_gcm(false, &key, &nonce, &[], &plaintext, 16).unwrap();
    assert_eq!(
        hex(&encrypted),
        "0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf"
    );
    assert_eq!(
        cng::aes_gcm(true, &key, &nonce, &[], &encrypted, 16).unwrap(),
        plaintext
    );
    for at in [0, 16, 31] {
        let mut corrupt = encrypted.clone();
        corrupt[at] ^= 1;
        assert!(cng::aes_gcm(true, &key, &nonce, &[], &corrupt, 16).is_err());
    }
}

#[test]
fn cbc_ctr_and_hkdf_known_vectors() {
    let from_hex = |text: &str| -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    };
    let key = from_hex("2b7e151628aed2a6abf7158809cf4f3c");
    let input = from_hex("6bc1bee22e409f96e93d7e117393172a");
    let iv = from_hex("000102030405060708090a0b0c0d0e0f");
    let cbc = cng::aes_cbc(false, &key, &iv, &input).unwrap();
    assert_eq!(hex(&cbc[..16]), "7649abac8119b246cee98e9b12e9197d");
    assert_eq!(cng::aes_cbc(true, &key, &iv, &cbc).unwrap(), input);
    let counter = from_hex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    let ctr = cng::aes_ctr(&key, &counter, 128, &input).unwrap();
    assert_eq!(hex(&ctr), "874d6191b620e3261bef6864990db6ce");
    assert_eq!(cng::aes_ctr(&key, &counter, 128, &ctr).unwrap(), input);
    assert_eq!(
        hex(&cng::hkdf(
            "SHA-256",
            &[0x0b; 22],
            &from_hex("000102030405060708090a0b0c"),
            &from_hex("f0f1f2f3f4f5f6f7f8f9"),
            42
        )
        .unwrap()),
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    assert!(cng::aes_cbc(true, &key, &iv, &cbc[..16]).is_err());
}

#[test]
fn host_rejects_unbounded_or_mistyped_input() {
    assert!(
        dispatch(
            "cryptoSubtleDigest",
            &[
                JsValue::String("cryptoSubtleDigest".into()),
                JsValue::String("SHA-256".into()),
                JsValue::Bytes(vec![0; MAX_INPUT + 1])
            ]
        )
        .is_err()
    );
    assert!(
        dispatch(
            "cryptoSubtlePbkdf2",
            &[
                JsValue::String("cryptoSubtlePbkdf2".into()),
                JsValue::String("SHA-256".into()),
                JsValue::Bytes(vec![]),
                JsValue::Bytes(vec![]),
                JsValue::Number(-1.0),
                JsValue::Number(16.0)
            ]
        )
        .is_err()
    );
    assert!(dispatch("cryptoSubtleDigest", &[]).is_err());
}

#[test]
fn aes_key_wrap_matches_rfc3394_and_checks_integrity() {
    let from_hex = |text: &str| -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    };
    let kek = from_hex("000102030405060708090a0b0c0d0e0f");
    let key = from_hex("00112233445566778899aabbccddeeff");
    let wrapped = cng::kw::wrap(&kek, &key).unwrap();
    assert_eq!(
        hex(&wrapped),
        "1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5"
    );
    assert_eq!(cng::kw::unwrap(&kek, &wrapped).unwrap(), key);
    for position in 0..wrapped.len() {
        let mut damaged = wrapped.clone();
        damaged[position] ^= 1;
        assert!(cng::kw::unwrap(&kek, &damaged).is_err());
    }
    assert!(cng::kw::wrap(&kek, &[0; 8]).is_err());
    assert!(cng::kw::unwrap(&kek, &[0; 16]).is_err());
}
