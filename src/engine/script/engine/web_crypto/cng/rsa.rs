//! Windows CNG RSA operations. Key blobs stay internal to the browser; Web Crypto
//! serialization and usage checks are performed by the JS binding before dispatch.
use super::{checked, hash, hash_name, open};
use std::ffi::c_void;
use std::ptr::null_mut;
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_KEY_HANDLE, BCRYPT_OAEP_PADDING_INFO, BCRYPT_PAD_OAEP, BCRYPT_PAD_PKCS1, BCRYPT_PAD_PSS,
    BCRYPT_PKCS1_PADDING_INFO, BCRYPT_PSS_PADDING_INFO, BCRYPT_RSA_ALGORITHM,
    BCRYPT_RSAFULLPRIVATE_BLOB, BCRYPT_RSAFULLPRIVATE_MAGIC, BCRYPT_RSAPUBLIC_BLOB,
    BCRYPT_RSAPUBLIC_MAGIC, BCryptDecrypt, BCryptDestroyKey, BCryptEncrypt, BCryptExportKey,
    BCryptFinalizeKeyPair, BCryptGenerateKeyPair, BCryptImportKeyPair, BCryptSignHash,
    BCryptVerifySignature,
};

struct RsaKey(BCRYPT_KEY_HANDLE);
impl Drop for RsaKey {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroyKey(self.0);
        }
    }
}

fn word(blob: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(blob[offset..offset + 4].try_into().unwrap())
}

fn validate_blob(blob: &[u8], private: bool) -> Result<usize, String> {
    if blob.len() < 24 {
        return Err("RSA key blob is truncated".into());
    }
    let exponent = word(blob, 8) as usize;
    let modulus = word(blob, 12) as usize;
    let p = word(blob, 16) as usize;
    let q = word(blob, 20) as usize;
    if !(1..=8).contains(&exponent)
        || !(128..=512).contains(&modulus)
        || if private {
            word(blob, 0) != BCRYPT_RSAFULLPRIVATE_MAGIC
                || p == 0
                || q == 0
                || p > modulus
                || q > modulus
        } else {
            word(blob, 0) != BCRYPT_RSAPUBLIC_MAGIC || p != 0 || q != 0
        }
    {
        return Err("invalid RSA key blob".into());
    }
    let size = 24 + exponent + modulus + if private { modulus + 3 * p + 2 * q } else { 0 };
    if size != blob.len()
        || word(blob, 4) as usize > modulus * 8
        || word(blob, 4) as usize <= (modulus - 1) * 8
    {
        return Err("invalid RSA key length".into());
    }
    Ok(modulus)
}

fn import(blob: &[u8], private: bool) -> Result<(RsaKey, usize), String> {
    let size = validate_blob(blob, private)?;
    let provider = open(BCRYPT_RSA_ALGORITHM, 0)?;
    let mut handle = null_mut();
    checked(
        unsafe {
            BCryptImportKeyPair(
                provider.0,
                null_mut(),
                if private {
                    BCRYPT_RSAFULLPRIVATE_BLOB
                } else {
                    BCRYPT_RSAPUBLIC_BLOB
                },
                &mut handle,
                blob.as_ptr(),
                blob.len() as u32,
                0,
            )
        },
        "import RSA key",
    )?;
    Ok((RsaKey(handle), size))
}

fn export(key: &RsaKey, private: bool) -> Result<Vec<u8>, String> {
    let kind = if private {
        BCRYPT_RSAFULLPRIVATE_BLOB
    } else {
        BCRYPT_RSAPUBLIC_BLOB
    };
    let mut needed = 0;
    checked(
        unsafe { BCryptExportKey(key.0, null_mut(), kind, null_mut(), 0, &mut needed, 0) },
        "size RSA key",
    )?;
    if needed > 4096 {
        return Err("RSA key blob exceeds limit".into());
    }
    let mut blob = vec![0; needed as usize];
    checked(
        unsafe {
            BCryptExportKey(
                key.0,
                null_mut(),
                kind,
                blob.as_mut_ptr(),
                needed,
                &mut needed,
                0,
            )
        },
        "export RSA key",
    )?;
    blob.truncate(needed as usize);
    validate_blob(&blob, private)?;
    Ok(blob)
}

pub(in crate::engine::script::engine::web_crypto) fn generate(
    bits: u32,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if !(1024..=4096).contains(&bits) || !bits.is_multiple_of(8) {
        return Err("unsupported RSA modulus length".into());
    }
    let provider = open(BCRYPT_RSA_ALGORITHM, 0)?;
    let mut handle = null_mut();
    checked(
        unsafe { BCryptGenerateKeyPair(provider.0, &mut handle, bits, 0) },
        "generate RSA key",
    )?;
    let key = RsaKey(handle);
    checked(
        unsafe { BCryptFinalizeKeyPair(key.0, 0) },
        "finalize RSA key",
    )?;
    Ok((export(&key, false)?, export(&key, true)?))
}

pub(in crate::engine::script::engine::web_crypto) fn validate(
    blob: &[u8],
    private: bool,
) -> Result<(), String> {
    let _ = import(blob, private)?;
    Ok(())
}

pub(in crate::engine::script::engine::web_crypto) fn oaep(
    decrypt: bool,
    hash_id: &str,
    blob: &[u8],
    label: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let (key, modulus) = import(blob, decrypt)?;
    if data.len() > modulus || label.len() > 65536 {
        return Err("RSA-OAEP input exceeds key or label limit".into());
    }
    let info = BCRYPT_OAEP_PADDING_INFO {
        pszAlgId: hash_name(hash_id)?,
        pbLabel: label.as_ptr().cast_mut(),
        cbLabel: label.len() as u32,
    };
    let mut output = vec![0; modulus];
    let mut written = 0;
    let status = unsafe {
        if decrypt {
            BCryptDecrypt(
                key.0,
                data.as_ptr(),
                data.len() as u32,
                (&raw const info).cast::<c_void>(),
                null_mut(),
                0,
                output.as_mut_ptr(),
                modulus as u32,
                &mut written,
                BCRYPT_PAD_OAEP,
            )
        } else {
            BCryptEncrypt(
                key.0,
                data.as_ptr(),
                data.len() as u32,
                (&raw const info).cast::<c_void>(),
                null_mut(),
                0,
                output.as_mut_ptr(),
                modulus as u32,
                &mut written,
                BCRYPT_PAD_OAEP,
            )
        }
    };
    checked(status, "RSA-OAEP")?;
    if written as usize > modulus {
        return Err("RSA provider returned an invalid length".into());
    }
    output.truncate(written as usize);
    Ok(output)
}

pub(in crate::engine::script::engine::web_crypto) fn signature(
    verify: bool,
    pss: bool,
    hash_id: &str,
    salt: u32,
    blob: &[u8],
    data: &[u8],
    signature: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let (key, modulus) = import(blob, !verify)?;
    let digest = hash(hash_id, None, data)?;
    let hash_name = hash_name(hash_id)?;
    let pkcs = BCRYPT_PKCS1_PADDING_INFO {
        pszAlgId: hash_name,
    };
    let pss_info = BCRYPT_PSS_PADDING_INFO {
        pszAlgId: hash_name,
        cbSalt: salt,
    };
    let padding = if pss {
        (&raw const pss_info).cast::<c_void>()
    } else {
        (&raw const pkcs).cast::<c_void>()
    };
    let flags = if pss {
        BCRYPT_PAD_PSS
    } else {
        BCRYPT_PAD_PKCS1
    };
    if verify {
        if signature.len() != modulus {
            return Ok(None);
        }
        let status = unsafe {
            BCryptVerifySignature(
                key.0,
                padding,
                digest.as_ptr(),
                digest.len() as u32,
                signature.as_ptr(),
                signature.len() as u32,
                flags,
            )
        };
        return Ok((status >= 0).then(Vec::new));
    }
    let mut output = vec![0; modulus];
    let mut written = 0;
    checked(
        unsafe {
            BCryptSignHash(
                key.0,
                padding,
                digest.as_ptr(),
                digest.len() as u32,
                output.as_mut_ptr(),
                modulus as u32,
                &mut written,
                flags,
            )
        },
        "sign RSA",
    )?;
    if written as usize != modulus {
        return Err("RSA signature has wrong length".into());
    }
    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsa_oaep_and_signature_roundtrip_with_tamper_rejection() {
        let (public, private) = generate(1024).unwrap();
        validate(&public, false).unwrap();
        validate(&private, true).unwrap();
        assert!(validate(&public, true).is_err());
        let label = b"context";
        let ciphertext = oaep(false, "SHA-256", &public, label, b"browser").unwrap();
        assert_eq!(
            oaep(true, "SHA-256", &private, label, &ciphertext).unwrap(),
            b"browser"
        );
        assert!(oaep(true, "SHA-256", &private, b"wrong", &ciphertext).is_err());
        let pss = signature(false, true, "SHA-256", 32, &private, b"message", &[])
            .unwrap()
            .unwrap();
        assert!(
            signature(true, true, "SHA-256", 32, &public, b"message", &pss)
                .unwrap()
                .is_some()
        );
        assert!(
            signature(true, true, "SHA-256", 32, &public, b"changed", &pss)
                .unwrap()
                .is_none()
        );
        let pkcs = signature(false, false, "SHA-256", 0, &private, b"message", &[])
            .unwrap()
            .unwrap();
        assert!(
            signature(true, false, "SHA-256", 0, &public, b"message", &pkcs)
                .unwrap()
                .is_some()
        );
        assert!(
            signature(true, false, "SHA-256", 0, &public, b"changed", &pkcs)
                .unwrap()
                .is_none()
        );
    }
}
