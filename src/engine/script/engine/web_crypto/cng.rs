//! BCrypt providers and owned handles. The platform provider does the cryptography; no
//! JavaScript or Rust substitute is used for hashing, derivation, or authenticated encryption.

use std::ffi::c_void;
use std::ptr::{null, null_mut};
pub(super) mod ec;
pub(super) mod kw;
pub(super) mod rsa;
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_AES_ALGORITHM, BCRYPT_ALG_HANDLE, BCRYPT_ALG_HANDLE_HMAC_FLAG,
    BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO, BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
    BCRYPT_BLOCK_PADDING, BCRYPT_CHAINING_MODE, BCRYPT_HASH_LENGTH, BCRYPT_KEY_HANDLE,
    BCRYPT_SHA1_ALGORITHM, BCRYPT_SHA256_ALGORITHM, BCRYPT_SHA384_ALGORITHM,
    BCRYPT_SHA512_ALGORITHM, BCryptCloseAlgorithmProvider, BCryptDecrypt, BCryptDeriveKeyPBKDF2,
    BCryptDestroyKey, BCryptEncrypt, BCryptGenerateSymmetricKey, BCryptGetProperty, BCryptHash,
    BCryptOpenAlgorithmProvider, BCryptSetProperty,
};

struct Algorithm(BCRYPT_ALG_HANDLE);
impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}
struct SymmetricKey(BCRYPT_KEY_HANDLE);
impl Drop for SymmetricKey {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroyKey(self.0);
        }
    }
}

fn checked(status: i32, operation: &'static str) -> Result<(), String> {
    if status >= 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed (NTSTATUS {status:#x})"))
    }
}

fn open(name: windows_sys::core::PCWSTR, flags: u32) -> Result<Algorithm, String> {
    let mut handle = null_mut();
    checked(
        unsafe { BCryptOpenAlgorithmProvider(&mut handle, name, null(), flags) },
        "open algorithm",
    )?;
    Ok(Algorithm(handle))
}

fn hash_name(name: &str) -> Result<windows_sys::core::PCWSTR, String> {
    match name {
        "SHA-1" => Ok(BCRYPT_SHA1_ALGORITHM),
        "SHA-256" => Ok(BCRYPT_SHA256_ALGORITHM),
        "SHA-384" => Ok(BCRYPT_SHA384_ALGORITHM),
        "SHA-512" => Ok(BCRYPT_SHA512_ALGORITHM),
        _ => Err("unsupported hash algorithm".into()),
    }
}

fn digest_length(algorithm: &Algorithm) -> Result<usize, String> {
    let mut size = 0u32;
    let mut written = 0u32;
    checked(
        unsafe {
            BCryptGetProperty(
                algorithm.0,
                BCRYPT_HASH_LENGTH,
                (&mut size as *mut u32).cast(),
                4,
                &mut written,
                0,
            )
        },
        "read hash length",
    )?;
    if written != 4 || !(1..=64).contains(&size) {
        return Err("invalid provider hash length".into());
    }
    Ok(size as usize)
}

pub(super) fn hash(name: &str, secret: Option<&[u8]>, data: &[u8]) -> Result<Vec<u8>, String> {
    let algorithm = open(
        hash_name(name)?,
        if secret.is_some() {
            BCRYPT_ALG_HANDLE_HMAC_FLAG
        } else {
            0
        },
    )?;
    let mut result = vec![0; digest_length(&algorithm)?];
    let (secret_ptr, secret_len) =
        secret.map_or((null(), 0), |secret| (secret.as_ptr(), secret.len() as u32));
    checked(
        unsafe {
            BCryptHash(
                algorithm.0,
                secret_ptr,
                secret_len,
                data.as_ptr(),
                data.len() as u32,
                result.as_mut_ptr(),
                result.len() as u32,
            )
        },
        "hash",
    )?;
    Ok(result)
}

pub(super) fn pbkdf2(
    name: &str,
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    length: usize,
) -> Result<Vec<u8>, String> {
    if iterations == 0 {
        return Err("PBKDF2 iterations must be positive".into());
    }
    let algorithm = open(hash_name(name)?, BCRYPT_ALG_HANDLE_HMAC_FLAG)?;
    let mut result = vec![0; length];
    checked(
        unsafe {
            BCryptDeriveKeyPBKDF2(
                algorithm.0,
                password.as_ptr(),
                password.len() as u32,
                salt.as_ptr(),
                salt.len() as u32,
                iterations as u64,
                result.as_mut_ptr(),
                length as u32,
                0,
            )
        },
        "derive PBKDF2",
    )?;
    Ok(result)
}

pub(super) fn hkdf(
    name: &str,
    secret: &[u8],
    salt: &[u8],
    info: &[u8],
    length: usize,
) -> Result<Vec<u8>, String> {
    let hash_length = match name {
        "SHA-1" => 20,
        "SHA-256" => 32,
        "SHA-384" => 48,
        "SHA-512" => 64,
        _ => return Err("unsupported HKDF hash".into()),
    };
    if length > hash_length * 255 {
        return Err("HKDF output exceeds 255 digest blocks".into());
    }
    // RFC 5869 extract then expand. A missing/empty salt is a zero-filled digest-size key.
    let default_salt = vec![0; hash_length];
    let prk = hash(
        name,
        Some(if salt.is_empty() { &default_salt } else { salt }),
        secret,
    )?;
    let mut output = Vec::with_capacity(length);
    let mut previous = Vec::new();
    for counter in 1..=length.div_ceil(hash_length) {
        let mut input = Vec::with_capacity(previous.len() + info.len() + 1);
        input.extend_from_slice(&previous);
        input.extend_from_slice(info);
        input.push(counter as u8);
        previous = hash(name, Some(&prk), &input)?;
        output.extend_from_slice(&previous);
    }
    output.truncate(length);
    Ok(output)
}

fn aes_key(secret: &[u8], mode: &str) -> Result<(Algorithm, SymmetricKey), String> {
    if !matches!(secret.len(), 16 | 24 | 32) {
        return Err("invalid AES key length".into());
    }
    let algorithm = open(BCRYPT_AES_ALGORITHM, 0)?;
    let mode: Vec<u16> = mode.encode_utf16().chain(std::iter::once(0)).collect();
    checked(
        unsafe {
            BCryptSetProperty(
                algorithm.0,
                BCRYPT_CHAINING_MODE,
                mode.as_ptr().cast(),
                (mode.len() * 2) as u32,
                0,
            )
        },
        "set AES mode",
    )?;
    let mut handle = null_mut();
    checked(
        unsafe {
            BCryptGenerateSymmetricKey(
                algorithm.0,
                &mut handle,
                null_mut(),
                0,
                secret.as_ptr(),
                secret.len() as u32,
                0,
            )
        },
        "import AES key",
    )?;
    Ok((algorithm, SymmetricKey(handle)))
}

pub(super) fn aes_cbc(
    decrypt: bool,
    secret: &[u8],
    iv: &[u8],
    input: &[u8],
) -> Result<Vec<u8>, String> {
    if iv.len() != 16 || decrypt && (input.is_empty() || !input.len().is_multiple_of(16)) {
        return Err("invalid AES-CBC input".into());
    }
    let (_algorithm, key) = aes_key(secret, "ChainingModeCBC")?;
    let mut iv = iv.to_vec(); // BCrypt updates the IV in-place; never mutate a caller's buffer.
    let mut output = vec![0; input.len() + if decrypt { 0 } else { 16 }];
    let mut written = 0u32;
    let status = unsafe {
        if decrypt {
            BCryptDecrypt(
                key.0,
                input.as_ptr(),
                input.len() as u32,
                null(),
                iv.as_mut_ptr(),
                16,
                output.as_mut_ptr(),
                output.len() as u32,
                &mut written,
                BCRYPT_BLOCK_PADDING,
            )
        } else {
            BCryptEncrypt(
                key.0,
                input.as_ptr(),
                input.len() as u32,
                null(),
                iv.as_mut_ptr(),
                16,
                output.as_mut_ptr(),
                output.len() as u32,
                &mut written,
                BCRYPT_BLOCK_PADDING,
            )
        }
    };
    checked(
        status,
        if decrypt {
            "decrypt AES-CBC"
        } else {
            "encrypt AES-CBC"
        },
    )?;
    if written as usize > output.len() {
        return Err("AES-CBC provider returned an invalid length".into());
    }
    output.truncate(written as usize);
    Ok(output)
}

pub(super) fn aes_ctr(
    secret: &[u8],
    initial: &[u8],
    counter_bits: u32,
    input: &[u8],
) -> Result<Vec<u8>, String> {
    if initial.len() != 16 || !(1..=128).contains(&counter_bits) {
        return Err("invalid AES-CTR counter".into());
    }
    let blocks = input.len().div_ceil(16);
    if counter_bits < 32 && blocks > (1usize << counter_bits) {
        return Err("AES-CTR counter would repeat".into());
    }
    let (_algorithm, key) = aes_key(secret, "ChainingModeECB")?;
    let mut counter = [0u8; 16];
    counter.copy_from_slice(initial);
    let mut counters = vec![0u8; blocks * 16];
    for block in counters.chunks_exact_mut(16) {
        block.copy_from_slice(&counter);
        // Increment only the rightmost `counter_bits` bits, preserving the nonce prefix.
        let mut carry = true;
        for bit in 0..counter_bits {
            if !carry {
                break;
            }
            let byte = 15 - (bit / 8) as usize;
            let mask = 1u8 << (bit % 8);
            carry = counter[byte] & mask != 0;
            counter[byte] ^= mask;
        }
    }
    let mut stream = vec![0u8; counters.len()];
    let mut written = 0u32;
    checked(
        unsafe {
            BCryptEncrypt(
                key.0,
                counters.as_ptr(),
                counters.len() as u32,
                null(),
                null_mut(),
                0,
                stream.as_mut_ptr(),
                stream.len() as u32,
                &mut written,
                0,
            )
        },
        "encrypt AES-CTR counters",
    )?;
    if written as usize != stream.len() {
        return Err("AES-CTR provider returned an unexpected length".into());
    }
    let mut result = input.to_vec();
    for (byte, mask) in result.iter_mut().zip(stream) {
        *byte ^= mask;
    }
    Ok(result)
}

// AES-GCM encrypt appends its tag; decrypt authenticates before exposing any plaintext.
pub(super) fn aes_gcm(
    decrypt: bool,
    secret: &[u8],
    nonce: &[u8],
    aad: &[u8],
    input: &[u8],
    tag_len: usize,
) -> Result<Vec<u8>, String> {
    if !matches!(secret.len(), 16 | 24 | 32)
        || nonce.is_empty()
        || nonce.len() > 65536
        || !matches!(tag_len, 4 | 8 | 12 | 13 | 14 | 15 | 16)
    {
        return Err("invalid AES-GCM parameters".into());
    }
    if decrypt && input.len() < tag_len {
        return Err("AES-GCM ciphertext is shorter than its tag".into());
    }
    let (_algorithm, key) = aes_key(secret, "ChainingModeGCM")?;
    let data_len = if decrypt {
        input.len() - tag_len
    } else {
        input.len()
    };
    let mut output = vec![0; data_len];
    let mut tag = if decrypt {
        input[data_len..].to_vec()
    } else {
        vec![0; tag_len]
    };
    let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
        pbNonce: nonce.as_ptr().cast_mut(),
        cbNonce: nonce.len() as u32,
        pbAuthData: aad.as_ptr().cast_mut(),
        cbAuthData: aad.len() as u32,
        pbTag: tag.as_mut_ptr(),
        cbTag: tag_len as u32,
        ..Default::default()
    };
    let mut written = 0u32;
    let payload = &input[..data_len];
    let status = unsafe {
        if decrypt {
            BCryptDecrypt(
                key.0,
                payload.as_ptr(),
                payload.len() as u32,
                (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),
                null_mut(),
                0,
                output.as_mut_ptr(),
                output.len() as u32,
                &mut written,
                0,
            )
        } else {
            BCryptEncrypt(
                key.0,
                payload.as_ptr(),
                payload.len() as u32,
                (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),
                null_mut(),
                0,
                output.as_mut_ptr(),
                output.len() as u32,
                &mut written,
                0,
            )
        }
    };
    checked(
        status,
        if decrypt {
            "authenticate AES-GCM"
        } else {
            "encrypt AES-GCM"
        },
    )?;
    if written as usize != data_len {
        return Err("AES-GCM provider returned an unexpected length".into());
    }
    if !decrypt {
        output.extend_from_slice(&tag);
    }
    Ok(output)
}
