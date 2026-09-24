//! RFC 3394 AES Key Wrap using the platform AES-ECB primitive. The 64-bit integrity
//! register is checked before releasing any unwrapped key bytes.

use super::*;

const INITIAL_VALUE: [u8; 8] = [0xa6; 8];
const MAX_KEY_DATA: usize = 64 * 1024;

fn block(key: &SymmetricKey, value: &[u8; 16], decrypt: bool) -> Result<[u8; 16], String> {
    let mut output = [0u8; 16];
    let mut written = 0u32;
    let status = unsafe {
        if decrypt {
            BCryptDecrypt(
                key.0,
                value.as_ptr(),
                16,
                null(),
                null_mut(),
                0,
                output.as_mut_ptr(),
                16,
                &mut written,
                0,
            )
        } else {
            BCryptEncrypt(
                key.0,
                value.as_ptr(),
                16,
                null(),
                null_mut(),
                0,
                output.as_mut_ptr(),
                16,
                &mut written,
                0,
            )
        }
    };
    checked(status, "AES-KW block")?;
    if written != 16 {
        return Err("AES-KW provider returned an invalid block".into());
    }
    Ok(output)
}

pub(in crate::engine::script::engine::web_crypto) fn wrap(
    secret: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    if plaintext.len() < 16 || plaintext.len() > MAX_KEY_DATA || !plaintext.len().is_multiple_of(8)
    {
        return Err("AES-KW requires at least two 64-bit plaintext blocks".into());
    }
    let (_algorithm, key) = aes_key(secret, "ChainingModeECB")?;
    let count = plaintext.len() / 8;
    let mut register = INITIAL_VALUE;
    let mut wrapped = plaintext.to_vec();
    for round in 0..6 {
        for index in 0..count {
            let mut input = [0u8; 16];
            input[..8].copy_from_slice(&register);
            input[8..].copy_from_slice(&wrapped[index * 8..index * 8 + 8]);
            let result = block(&key, &input, false)?;
            register.copy_from_slice(&result[..8]);
            let counter = (round * count + index + 1) as u64;
            for (byte, mask) in register.iter_mut().zip(counter.to_be_bytes()) {
                *byte ^= mask;
            }
            wrapped[index * 8..index * 8 + 8].copy_from_slice(&result[8..]);
        }
    }
    let mut output = Vec::with_capacity(wrapped.len() + 8);
    output.extend_from_slice(&register);
    output.extend_from_slice(&wrapped);
    Ok(output)
}

pub(in crate::engine::script::engine::web_crypto) fn unwrap(
    secret: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    if ciphertext.len() < 24
        || ciphertext.len() > MAX_KEY_DATA + 8
        || !ciphertext.len().is_multiple_of(8)
    {
        return Err("AES-KW requires at least three 64-bit ciphertext blocks".into());
    }
    let (_algorithm, key) = aes_key(secret, "ChainingModeECB")?;
    let count = ciphertext.len() / 8 - 1;
    let mut register = [0u8; 8];
    register.copy_from_slice(&ciphertext[..8]);
    let mut unwrapped = ciphertext[8..].to_vec();
    for round in (0..6).rev() {
        for index in (0..count).rev() {
            let mut input = [0u8; 16];
            input[..8].copy_from_slice(&register);
            let counter = (round * count + index + 1) as u64;
            for (byte, mask) in input[..8].iter_mut().zip(counter.to_be_bytes()) {
                *byte ^= mask;
            }
            input[8..].copy_from_slice(&unwrapped[index * 8..index * 8 + 8]);
            let result = block(&key, &input, true)?;
            register.copy_from_slice(&result[..8]);
            unwrapped[index * 8..index * 8 + 8].copy_from_slice(&result[8..]);
        }
    }
    if register != INITIAL_VALUE {
        return Err("AES-KW integrity check failed".into());
    }
    Ok(unwrapped)
}
