use better_web_browser::media_process::DecodedMediaFrame;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_SHA256_ALGORITHM, BCryptCloseAlgorithmProvider, BCryptHash,
    BCryptOpenAlgorithmProvider,
};

pub(super) fn decode_base64(input: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut quartet = [0_u8; 4];
    let mut count = 0;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        quartet[count] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => panic!("invalid base64 fixture byte"),
        };
        count += 1;
        if count == 4 {
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            if quartet[2] != 64 {
                output.push((quartet[1] << 4) | (quartet[2] >> 2));
            }
            if quartet[3] != 64 {
                output.push((quartet[2] << 6) | quartet[3]);
            }
            count = 0;
        }
    }
    assert_eq!(count, 0, "truncated base64 fixture");
    output
}

pub(super) fn sha256(input: &[u8]) -> [u8; 32] {
    let mut algorithm: BCRYPT_ALG_HANDLE = null_mut();
    let open_status =
        unsafe { BCryptOpenAlgorithmProvider(&mut algorithm, BCRYPT_SHA256_ALGORITHM, null(), 0) };
    assert!(
        open_status >= 0,
        "open SHA-256 provider: NTSTATUS {open_status:#x}"
    );

    let mut digest = [0_u8; 32];
    let hash_status = unsafe {
        BCryptHash(
            algorithm,
            null(),
            0,
            input.as_ptr(),
            input.len() as u32,
            digest.as_mut_ptr(),
            digest.len() as u32,
        )
    };
    let close_status = unsafe { BCryptCloseAlgorithmProvider(algorithm, 0) };
    assert!(hash_status >= 0, "hash fixture: NTSTATUS {hash_status:#x}");
    assert!(
        close_status >= 0,
        "close SHA-256 provider: NTSTATUS {close_status:#x}"
    );
    digest
}

pub(super) fn capture_frame_if_requested(frame: &DecodedMediaFrame) {
    let Some(path) = std::env::var_os("BREEZE_MEDIA_FRAME_CAPTURE") else {
        return;
    };
    let mut rgba = frame.bgra.clone();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    image::save_buffer(
        path,
        &rgba,
        frame.metadata.width,
        frame.metadata.height,
        image::ColorType::Rgba8,
    )
    .expect("save optional decoded-frame capture");
}
