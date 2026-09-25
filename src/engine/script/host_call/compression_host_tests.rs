use super::Codec;

fn encode(format: &str, chunks: &[&[u8]]) -> Vec<u8> {
    let mut codec = Codec::new("compress", format).unwrap();
    let mut result = Vec::new();
    for chunk in chunks {
        result.extend(codec.write(chunk).unwrap());
    }
    result.extend(codec.finish().unwrap());
    result
}

fn decode(format: &str, bytes: &[u8], chunk_size: usize) -> std::io::Result<Vec<u8>> {
    let mut codec = Codec::new("decompress", format).unwrap();
    let mut result = Vec::new();
    for chunk in bytes.chunks(chunk_size) {
        result.extend(codec.write(chunk)?);
    }
    result.extend(codec.finish()?);
    Ok(result)
}

#[test]
fn codecs_round_trip_across_independent_input_boundaries() {
    for format in ["gzip", "deflate", "deflate-raw"] {
        let compressed = encode(format, &[b"hello", b" ", b"world"]);
        assert_eq!(decode(format, &compressed, 1).unwrap(), b"hello world");
        assert_eq!(decode(format, &compressed, 7).unwrap(), b"hello world");
        assert_eq!(
            decode(format, &compressed, compressed.len()).unwrap(),
            b"hello world"
        );
    }
}

#[test]
fn gzip_rejects_bad_checksum_truncation_and_extra_member() {
    let valid = encode("gzip", &[b"test payload"]);
    let mut bad_crc = valid.clone();
    let crc_position = bad_crc.len() - 8;
    bad_crc[crc_position] ^= 0xff;
    assert!(decode("gzip", &bad_crc, 2).is_err());
    assert!(decode("gzip", &valid[..valid.len() - 1], 2).is_err());
    let mut extra = valid.clone();
    extra.extend(encode("gzip", &[b"second member"]));
    assert!(decode("gzip", &extra, 3).is_err());
}

#[test]
fn deflate_rejects_truncation_and_trailing_data() {
    for format in ["deflate", "deflate-raw"] {
        let valid = encode(format, &[b"testing 123"]);
        assert!(decode(format, &valid[..valid.len() - 2], 4).is_err());
        let mut extra = valid.clone();
        extra.push(42);
        assert!(decode(format, &extra, 4).is_err());
    }
}

#[test]
fn codecs_reject_wrong_wrappers_and_unsupported_formats() {
    assert!(Codec::new("compress", "brotli").is_none());
    assert!(Codec::new("decompress", "other").is_none());
    let gzip = encode("gzip", &[b"one"]);
    assert!(decode("deflate", &gzip, 2).is_err());
}
