//! Web Crypto ECDH and ECDSA on Windows CNG's NIST-curve providers.
//! CNG ECC blobs contain an eight-byte header followed by fixed-width big-endian X, Y,
//! and (for private keys) D. We keep the blob internal; JS imports/exports Web Crypto JWK/raw.

use super::{checked, hash, open};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_ECCPRIVATE_BLOB, BCRYPT_ECCPUBLIC_BLOB, BCRYPT_ECDH_P256_ALGORITHM,
    BCRYPT_ECDH_P384_ALGORITHM, BCRYPT_ECDH_P521_ALGORITHM, BCRYPT_ECDH_PRIVATE_P256_MAGIC,
    BCRYPT_ECDH_PRIVATE_P384_MAGIC, BCRYPT_ECDH_PRIVATE_P521_MAGIC, BCRYPT_ECDH_PUBLIC_P256_MAGIC,
    BCRYPT_ECDH_PUBLIC_P384_MAGIC, BCRYPT_ECDH_PUBLIC_P521_MAGIC, BCRYPT_ECDSA_P256_ALGORITHM,
    BCRYPT_ECDSA_P384_ALGORITHM, BCRYPT_ECDSA_P521_ALGORITHM, BCRYPT_ECDSA_PRIVATE_P256_MAGIC,
    BCRYPT_ECDSA_PRIVATE_P384_MAGIC, BCRYPT_ECDSA_PRIVATE_P521_MAGIC,
    BCRYPT_ECDSA_PUBLIC_P256_MAGIC, BCRYPT_ECDSA_PUBLIC_P384_MAGIC, BCRYPT_ECDSA_PUBLIC_P521_MAGIC,
    BCRYPT_KDF_RAW_SECRET, BCRYPT_KEY_HANDLE, BCRYPT_SECRET_HANDLE, BCryptDeriveKey,
    BCryptDestroyKey, BCryptDestroySecret, BCryptExportKey, BCryptFinalizeKeyPair,
    BCryptGenerateKeyPair, BCryptImportKeyPair, BCryptSecretAgreement, BCryptSignHash,
    BCryptVerifySignature,
};

struct EcKey(BCRYPT_KEY_HANDLE);
impl Drop for EcKey {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroyKey(self.0);
        }
    }
}
struct Secret(BCRYPT_SECRET_HANDLE);
impl Drop for Secret {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroySecret(self.0);
        }
    }
}

#[derive(Clone, Copy)]
struct Curve {
    algorithm: windows_sys::core::PCWSTR,
    size: usize,
    public_magic: u32,
    private_magic: u32,
}

fn curve(family: &str, name: &str) -> Result<Curve, String> {
    let (algorithm, size, public_magic, private_magic) = match (family, name) {
        ("ECDH", "P-256") => (
            BCRYPT_ECDH_P256_ALGORITHM,
            32,
            BCRYPT_ECDH_PUBLIC_P256_MAGIC,
            BCRYPT_ECDH_PRIVATE_P256_MAGIC,
        ),
        ("ECDH", "P-384") => (
            BCRYPT_ECDH_P384_ALGORITHM,
            48,
            BCRYPT_ECDH_PUBLIC_P384_MAGIC,
            BCRYPT_ECDH_PRIVATE_P384_MAGIC,
        ),
        ("ECDH", "P-521") => (
            BCRYPT_ECDH_P521_ALGORITHM,
            66,
            BCRYPT_ECDH_PUBLIC_P521_MAGIC,
            BCRYPT_ECDH_PRIVATE_P521_MAGIC,
        ),
        ("ECDSA", "P-256") => (
            BCRYPT_ECDSA_P256_ALGORITHM,
            32,
            BCRYPT_ECDSA_PUBLIC_P256_MAGIC,
            BCRYPT_ECDSA_PRIVATE_P256_MAGIC,
        ),
        ("ECDSA", "P-384") => (
            BCRYPT_ECDSA_P384_ALGORITHM,
            48,
            BCRYPT_ECDSA_PUBLIC_P384_MAGIC,
            BCRYPT_ECDSA_PRIVATE_P384_MAGIC,
        ),
        ("ECDSA", "P-521") => (
            BCRYPT_ECDSA_P521_ALGORITHM,
            66,
            BCRYPT_ECDSA_PUBLIC_P521_MAGIC,
            BCRYPT_ECDSA_PRIVATE_P521_MAGIC,
        ),
        _ => return Err("unsupported elliptic curve".into()),
    };
    Ok(Curve {
        algorithm,
        size,
        public_magic,
        private_magic,
    })
}

fn blob_kind(spec: Curve, blob: &[u8]) -> Result<bool, String> {
    if blob.len() < 8 {
        return Err("invalid EC key blob".into());
    }
    let magic = u32::from_le_bytes(blob[..4].try_into().unwrap());
    let size = u32::from_le_bytes(blob[4..8].try_into().unwrap()) as usize;
    if size != spec.size {
        return Err("wrong EC curve size".into());
    }
    if magic == spec.public_magic && blob.len() == 8 + 2 * size {
        return Ok(false);
    }
    if magic == spec.private_magic && blob.len() == 8 + 3 * size {
        return Ok(true);
    }
    Err("invalid EC key blob".into())
}

fn import(
    algorithm: windows_sys::core::PCWSTR,
    spec: Curve,
    blob: &[u8],
    private: bool,
) -> Result<EcKey, String> {
    if blob_kind(spec, blob)? != private {
        return Err("wrong EC key type".into());
    }
    let provider = open(algorithm, 0)?;
    let mut key = null_mut();
    checked(
        unsafe {
            BCryptImportKeyPair(
                provider.0,
                null_mut(),
                if private {
                    BCRYPT_ECCPRIVATE_BLOB
                } else {
                    BCRYPT_ECCPUBLIC_BLOB
                },
                &mut key,
                blob.as_ptr(),
                blob.len() as u32,
                0,
            )
        },
        "import EC key",
    )?;
    Ok(EcKey(key))
}

fn export(key: &EcKey, private: bool, maximum: usize) -> Result<Vec<u8>, String> {
    let kind = if private {
        BCRYPT_ECCPRIVATE_BLOB
    } else {
        BCRYPT_ECCPUBLIC_BLOB
    };
    let mut needed = 0;
    checked(
        unsafe { BCryptExportKey(key.0, null_mut(), kind, null_mut(), 0, &mut needed, 0) },
        "size EC key",
    )?;
    if needed as usize > maximum {
        return Err("EC key blob exceeds limit".into());
    }
    let mut bytes = vec![0; needed as usize];
    checked(
        unsafe {
            BCryptExportKey(
                key.0,
                null_mut(),
                kind,
                bytes.as_mut_ptr(),
                needed,
                &mut needed,
                0,
            )
        },
        "export EC key",
    )?;
    bytes.truncate(needed as usize);
    Ok(bytes)
}

pub(in crate::engine::script::engine::web_crypto) fn generate(
    family: &str,
    name: &str,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let spec = curve(family, name)?;
    let provider = open(spec.algorithm, 0)?;
    let mut handle = null_mut();
    checked(
        unsafe {
            BCryptGenerateKeyPair(
                provider.0,
                &mut handle,
                if spec.size == 66 {
                    521
                } else {
                    (spec.size * 8) as u32
                },
                0,
            )
        },
        "generate EC key",
    )?;
    let key = EcKey(handle);
    checked(
        unsafe { BCryptFinalizeKeyPair(key.0, 0) },
        "finalize EC key",
    )?;
    let public = export(&key, false, 8 + 2 * spec.size)?;
    let private = export(&key, true, 8 + 3 * spec.size)?;
    blob_kind(spec, &public)?;
    blob_kind(spec, &private)?;
    Ok((public, private))
}

pub(in crate::engine::script::engine::web_crypto) fn validate(
    family: &str,
    name: &str,
    blob: &[u8],
) -> Result<(), String> {
    let spec = curve(family, name)?;
    let private = blob_kind(spec, blob)?;
    let _ = import(spec.algorithm, spec, blob, private)?;
    Ok(())
}

pub(in crate::engine::script::engine::web_crypto) fn derive(
    name: &str,
    private: &[u8],
    public: &[u8],
) -> Result<Vec<u8>, String> {
    let spec = curve("ECDH", name)?;
    let private = import(spec.algorithm, spec, private, true)?;
    let public = import(spec.algorithm, spec, public, false)?;
    let mut secret = null_mut();
    checked(
        unsafe { BCryptSecretAgreement(private.0, public.0, &mut secret, 0) },
        "agree ECDH secret",
    )?;
    let secret = Secret(secret);
    let mut bytes = vec![0; spec.size];
    let mut written = 0;
    checked(
        unsafe {
            BCryptDeriveKey(
                secret.0,
                BCRYPT_KDF_RAW_SECRET,
                null(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut written,
                0,
            )
        },
        "derive raw ECDH secret",
    )?;
    if written as usize != bytes.len() {
        return Err("wrong ECDH secret length".into());
    }
    // CNG's RAW_SECRET is little-endian; Web Crypto deriveBits returns big-endian X.
    bytes.reverse();
    Ok(bytes)
}

pub(in crate::engine::script::engine::web_crypto) fn sign(
    name: &str,
    hash_name: &str,
    private: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let spec = curve("ECDSA", name)?;
    let key = import(spec.algorithm, spec, private, true)?;
    let digest = hash(hash_name, None, data)?;
    let mut signature = vec![0; spec.size * 2];
    let mut written = 0;
    checked(
        unsafe {
            BCryptSignHash(
                key.0,
                null(),
                digest.as_ptr(),
                digest.len() as u32,
                signature.as_mut_ptr(),
                signature.len() as u32,
                &mut written,
                0,
            )
        },
        "sign ECDSA",
    )?;
    if written as usize != signature.len() {
        return Err("wrong ECDSA signature length".into());
    }
    Ok(signature)
}

pub(in crate::engine::script::engine::web_crypto) fn verify(
    name: &str,
    hash_name: &str,
    public: &[u8],
    signature: &[u8],
    data: &[u8],
) -> Result<bool, String> {
    let spec = curve("ECDSA", name)?;
    let key = import(spec.algorithm, spec, public, false)?;
    if signature.len() != spec.size * 2 {
        return Ok(false);
    }
    let digest = hash(hash_name, None, data)?;
    Ok(unsafe {
        BCryptVerifySignature(
            key.0,
            null(),
            digest.as_ptr(),
            digest.len() as u32,
            signature.as_ptr(),
            signature.len() as u32,
            0,
        )
    } >= 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_nist_curves_agree_on_the_same_shared_secret() {
        for name in ["P-256", "P-384", "P-521"] {
            let (alice_public, alice_private) = generate("ECDH", name).unwrap();
            let (bob_public, bob_private) = generate("ECDH", name).unwrap();
            let first = derive(name, &alice_private, &bob_public).unwrap();
            let second = derive(name, &bob_private, &alice_public).unwrap();
            assert_eq!(first, second);
            assert_eq!(first.len(), curve("ECDH", name).unwrap().size);
            assert!(first.iter().any(|byte| *byte != 0));
            assert!(validate("ECDH", name, &alice_private).is_ok());
        }
    }

    #[test]
    fn all_nist_curves_sign_and_reject_modified_messages() {
        for (name, digest) in [
            ("P-256", "SHA-256"),
            ("P-384", "SHA-384"),
            ("P-521", "SHA-512"),
        ] {
            let (public, private) = generate("ECDSA", name).unwrap();
            let signature = sign(name, digest, &private, b"signed message").unwrap();
            assert!(verify(name, digest, &public, &signature, b"signed message").unwrap());
            assert!(!verify(name, digest, &public, &signature, b"modified message").unwrap());
            assert!(derive(name, &private, &public).is_err());
        }
    }
}
