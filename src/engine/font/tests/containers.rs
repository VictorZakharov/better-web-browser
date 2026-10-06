use super::super::*;
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

fn compressed_woff(data: &[u8], declared_table_length: usize) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    let payload = encoder.finish().unwrap();
    assert!(payload.len() < declared_table_length);
    let mut woff = vec![0_u8; 64];
    woff[..4].copy_from_slice(b"wOFF");
    woff[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    write_u32(&mut woff, 8, (64 + payload.len()) as u32).unwrap();
    write_u16(&mut woff, 12, 1).unwrap();
    write_u32(&mut woff, 16, (28 + align4(declared_table_length)) as u32).unwrap();
    woff[44..48].copy_from_slice(b"test");
    write_u32(&mut woff, 48, 64).unwrap();
    write_u32(&mut woff, 52, payload.len() as u32).unwrap();
    write_u32(&mut woff, 56, declared_table_length as u32).unwrap();
    woff.extend(payload);
    woff
}

#[test]
fn compressed_container_reconstructs_exactly_the_declared_table() {
    let data = vec![37; 4096];
    let output = decode_woff(&compressed_woff(&data, data.len())).unwrap();
    assert_eq!(&output[28..], data);
}

#[test]
fn oversized_inflation_is_rejected_at_the_declared_length() {
    // A small container advertises a 16 KiB table but would inflate to 8 MiB.
    // Production reads at most 16385 bytes before rejecting, not the full stream.
    let data = vec![0; 8 * 1024 * 1024];
    let woff = compressed_woff(&data, 16 * 1024);
    let error = decode_woff(&woff).unwrap_err();
    assert!(error.contains("wrong length"), "{error}");
}

#[test]
fn short_or_corrupt_compressed_tables_never_get_zero_filled_as_success() {
    let mut woff = compressed_woff(&vec![37; 1024], 4096);
    assert!(decode_woff(&woff).unwrap_err().contains("wrong length"));
    woff[64] ^= 0xff;
    assert!(decode_woff(&woff).is_err());
}
